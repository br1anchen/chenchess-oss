use std::{sync::Arc, time::Duration};

use reqwest::{redirect::Policy, Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::{
    valid_provider_id, DeliveryFuture, InvitationDeliveryError, InvitationDeliveryReceipt,
    InvitationDeliveryRequest, InvitationEmailDelivery,
};

const RESEND_EMAILS_ENDPOINT: &str = "https://api.resend.com/emails";
const FROM: &str = "ChenChess <invite@example.test>";
const REPLY_TO: &str = "support@example.test";
const SUBJECT: &str = "Your ChenChess beta invitation";
const JOIN_ORIGIN: &str = "https://coach.example.test/join/";
const MAX_RESPONSE_BYTES: usize = 4 * 1024;

pub(super) struct ResendInvitationDelivery {
    api_key: Arc<str>,
    client: Client,
    endpoint: Arc<str>,
}

impl ResendInvitationDelivery {
    pub(super) fn new(api_key: String) -> anyhow::Result<Self> {
        if api_key.len() > 512 || !api_key.is_ascii() {
            anyhow::bail!("BETA_RESEND_API_KEY must contain at most 512 ASCII characters");
        }
        Ok(Self {
            api_key: api_key.into(),
            client: Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .redirect(Policy::none())
                .timeout(Duration::from_secs(15))
                .build()?,
            endpoint: RESEND_EMAILS_ENDPOINT.into(),
        })
    }

    #[cfg(test)]
    fn for_test(endpoint: String, timeout: Duration) -> Self {
        Self {
            api_key: "test-resend-key".into(),
            client: Client::builder()
                .connect_timeout(timeout)
                .redirect(Policy::none())
                .timeout(timeout)
                .build()
                .unwrap(),
            endpoint: endpoint.into(),
        }
    }
}

impl InvitationEmailDelivery for ResendInvitationDelivery {
    fn deliver<'a>(&'a self, request: InvitationDeliveryRequest) -> DeliveryFuture<'a> {
        Box::pin(async move {
            let join_url = format!("{JOIN_ORIGIN}#invite={}", request.code);
            let payload = ResendEmail {
                from: FROM,
                html: format!(
                    "<p>Your ChenChess beta invitation is ready.</p><p>Invitation code: <code>{}</code></p><p><a href=\"{}\">Join the beta</a></p><p>You can copy the code if you open the invitation on another device.</p>",
                    request.code, join_url
                ),
                reply_to: REPLY_TO,
                subject: SUBJECT,
                text: format!(
                    "Your ChenChess beta invitation is ready.\n\nInvitation code: {}\n\nJoin the beta: {}\n\nYou can copy the code if you open the invitation on another device.",
                    request.code, join_url
                ),
                to: [request.email.as_str()],
            };
            let mut response = self
                .client
                .post(self.endpoint.as_ref())
                .bearer_auth(self.api_key.as_ref())
                .header(
                    "Idempotency-Key",
                    idempotency_key(&request.invitation_id, request.delivery_attempt),
                )
                .json(&payload)
                .send()
                .await
                .map_err(|_| InvitationDeliveryError::Retryable)?;
            if !response.status().is_success() {
                return Err(classify_status(response.status()));
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
            {
                return Err(InvitationDeliveryError::Rejected);
            }
            let mut body = Vec::with_capacity(
                response
                    .content_length()
                    .unwrap_or_default()
                    .min(MAX_RESPONSE_BYTES as u64) as usize,
            );
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| InvitationDeliveryError::Retryable)?
            {
                if chunk.len() > MAX_RESPONSE_BYTES - body.len() {
                    return Err(InvitationDeliveryError::Rejected);
                }
                body.extend_from_slice(&chunk);
            }
            let response: ResendEmailResponse =
                serde_json::from_slice(&body).map_err(|_| InvitationDeliveryError::Rejected)?;
            if !valid_provider_id(&response.id) {
                return Err(InvitationDeliveryError::Rejected);
            }
            Ok(InvitationDeliveryReceipt {
                provider_message_id: response.id,
            })
        })
    }
}

#[derive(Serialize)]
struct ResendEmail<'a> {
    from: &'static str,
    html: String,
    reply_to: &'static str,
    subject: &'static str,
    text: String,
    to: [&'a str; 1],
}

#[derive(Deserialize)]
struct ResendEmailResponse {
    id: String,
}

fn classify_status(status: StatusCode) -> InvitationDeliveryError {
    if status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
    {
        InvitationDeliveryError::Retryable
    } else {
        InvitationDeliveryError::Rejected
    }
}

fn idempotency_key(invitation_id: &str, delivery_attempt: u32) -> String {
    format!("beta-invitation-delivery/{invitation_id}/{delivery_attempt}")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::{
        body::{to_bytes, Body},
        extract::State,
        http::{HeaderMap, Request},
        response::IntoResponse,
        routing::post,
        Router,
    };
    use serde_json::Value;

    use super::*;

    #[derive(Default)]
    struct CapturedRequest {
        headers: HeaderMap,
        payload: Value,
    }

    #[tokio::test]
    async fn sends_the_exact_private_transactional_contract() {
        let captured = Arc::new(Mutex::new(CapturedRequest::default()));
        let application = Router::new()
            .route("/emails", post(capture_email))
            .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let endpoint = format!("http://{}/emails", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, application).await });
        let delivery = ResendInvitationDelivery::for_test(endpoint, Duration::from_secs(2));

        let receipt = delivery
            .deliver(request("secret-code-0000000000000000"))
            .await
            .unwrap();
        assert_eq!(receipt.provider_message_id, "provider-message-1");

        let captured = captured.lock().unwrap();
        assert_eq!(captured.payload["from"], FROM);
        assert_eq!(captured.payload["reply_to"], REPLY_TO);
        assert_eq!(captured.payload["to"][0], "player@example.com");
        assert!(captured.payload["text"]
            .as_str()
            .unwrap()
            .contains("Invitation code: secret-code-0000000000000000"));
        assert!(captured.payload["html"]
            .as_str()
            .unwrap()
            .contains("https://coach.example.test/join/#invite=secret-code-0000000000000000"));
        assert!(captured.payload.get("tags").is_none());
        assert!(captured.payload.get("headers").is_none());
        assert_eq!(
            captured.headers.get("idempotency-key").unwrap(),
            "beta-invitation-delivery/11111111111111111111111111111111/1"
        );
        assert_eq!(
            captured.headers.get("authorization").unwrap(),
            "Bearer test-resend-key"
        );
        assert_eq!(
            idempotency_key("1", 2),
            idempotency_key("1", 2),
            "one retry attempt must be stable"
        );
        assert_ne!(
            idempotency_key("1", 1),
            idempotency_key("1", 2),
            "a deliberate retry must not collapse into the original send"
        );
        server.abort();
    }

    #[tokio::test]
    async fn classifies_provider_rejection_and_timeout_without_response_details() {
        let rejected_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let rejected_endpoint =
            format!("http://{}/emails", rejected_listener.local_addr().unwrap());
        let rejected_server = tokio::spawn(async move {
            axum::serve(
                rejected_listener,
                Router::new().route(
                    "/emails",
                    post(|| async { (StatusCode::UNPROCESSABLE_ENTITY, "private rejection") }),
                ),
            )
            .await
        });
        let rejected =
            ResendInvitationDelivery::for_test(rejected_endpoint, Duration::from_secs(2));
        assert_eq!(
            rejected.deliver(request("secret-code")).await.unwrap_err(),
            InvitationDeliveryError::Rejected
        );
        rejected_server.abort();

        let timeout_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let timeout_endpoint = format!("http://{}/emails", timeout_listener.local_addr().unwrap());
        let timeout_server = tokio::spawn(async move {
            axum::serve(
                timeout_listener,
                Router::new().route(
                    "/emails",
                    post(|| async {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        StatusCode::OK
                    }),
                ),
            )
            .await
        });
        let timeout =
            ResendInvitationDelivery::for_test(timeout_endpoint, Duration::from_millis(20));
        assert_eq!(
            timeout.deliver(request("secret-code")).await.unwrap_err(),
            InvitationDeliveryError::Retryable
        );
        timeout_server.abort();
    }

    fn request(code: &str) -> InvitationDeliveryRequest {
        InvitationDeliveryRequest {
            delivery_attempt: 1,
            invitation_id: "1".repeat(32),
            email: super::super::super::NormalizedEmail::parse("player@example.com").unwrap(),
            code: code.to_string(),
        }
    }

    async fn capture_email(
        State(captured): State<Arc<Mutex<CapturedRequest>>>,
        request: Request<Body>,
    ) -> impl IntoResponse {
        let (parts, body) = request.into_parts();
        let payload = serde_json::from_slice(&to_bytes(body, usize::MAX).await.unwrap()).unwrap();
        *captured.lock().unwrap() = CapturedRequest {
            headers: parts.headers,
            payload,
        };
        axum::Json(serde_json::json!({ "id": "provider-message-1" }))
    }
}
