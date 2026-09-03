use std::{sync::Arc, time::Duration};

use reqwest::{redirect::Policy, Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::{
    valid_resend_provider_id, DigestEmailDelivery, DigestEmailDeliveryError, DigestEmailReceipt,
    DigestEmailRequest, EmailDeliveryFuture,
};

const RESEND_EMAILS_ENDPOINT: &str = "https://api.resend.com/emails";
const FROM: &str = "ChenChess <coach@example.test>";
const REPLY_TO: &str = "support@example.test";
const MAX_RESPONSE_BYTES: usize = 4 * 1024;

pub(super) struct ResendDigestEmailDelivery {
    api_key: Arc<str>,
    client: Client,
    endpoint: Arc<str>,
}

impl ResendDigestEmailDelivery {
    pub(super) fn new(api_key: String) -> anyhow::Result<Self> {
        if api_key.is_empty() || api_key.len() > 512 || !api_key.is_ascii() {
            anyhow::bail!("DAILY_COACHING_RESEND_API_KEY must contain 1-512 ASCII characters");
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
    fn for_test(endpoint: String) -> Self {
        Self {
            api_key: "test-resend-key".into(),
            client: Client::builder()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            endpoint: endpoint.into(),
        }
    }
}

impl DigestEmailDelivery for ResendDigestEmailDelivery {
    fn deliver<'a>(&'a self, request: DigestEmailRequest) -> EmailDeliveryFuture<'a> {
        Box::pin(async move {
            let idempotency_key = delivery_idempotency_key(&request);
            let payload = ResendEmail {
                from: FROM,
                headers: ResendHeaders {
                    list_unsubscribe: format!("<{}>", request.unsubscribe_url),
                    list_unsubscribe_post: "List-Unsubscribe=One-Click",
                },
                html: request.rendered.html,
                reply_to: REPLY_TO,
                subject: request.rendered.subject,
                tags: [
                    ResendTag {
                        name: "coaching_owner",
                        value: request.owner_key.as_str(),
                    },
                    ResendTag {
                        name: "delivery_id",
                        value: &request.delivery_id,
                    },
                ],
                text: request.rendered.text,
                to: [request.recipient.as_str()],
            };
            let mut response = self
                .client
                .post(self.endpoint.as_ref())
                .bearer_auth(self.api_key.as_ref())
                .header("Idempotency-Key", idempotency_key)
                .json(&payload)
                .send()
                .await
                .map_err(|_| DigestEmailDeliveryError::Retryable)?;
            if !response.status().is_success() {
                return Err(classify_status(response.status()));
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
            {
                return Err(DigestEmailDeliveryError::Retryable);
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
                .map_err(|_| DigestEmailDeliveryError::Retryable)?
            {
                if chunk.len() > MAX_RESPONSE_BYTES - body.len() {
                    return Err(DigestEmailDeliveryError::Retryable);
                }
                body.extend_from_slice(&chunk);
            }
            let response: ResendEmailResponse =
                serde_json::from_slice(&body).map_err(|_| DigestEmailDeliveryError::Retryable)?;
            if !valid_resend_provider_id(&response.id) {
                return Err(DigestEmailDeliveryError::Retryable);
            }
            Ok(DigestEmailReceipt {
                provider_message_id: response.id,
            })
        })
    }
}

fn delivery_idempotency_key(request: &DigestEmailRequest) -> String {
    let kind = if request.delivery_id.starts_with("daily-") {
        "digest"
    } else {
        "profile-unavailable"
    };
    format!(
        "daily-coaching-{kind}/{}/{}",
        request.owner_key.as_str(),
        request.delivery_id
    )
}

#[derive(Serialize)]
struct ResendEmail<'a> {
    from: &'static str,
    headers: ResendHeaders,
    html: String,
    reply_to: &'static str,
    subject: String,
    tags: [ResendTag<'a>; 2],
    text: String,
    to: [&'a str; 1],
}

#[derive(Serialize)]
struct ResendHeaders {
    #[serde(rename = "List-Unsubscribe")]
    list_unsubscribe: String,
    #[serde(rename = "List-Unsubscribe-Post")]
    list_unsubscribe_post: &'static str,
}

#[derive(Serialize)]
struct ResendTag<'a> {
    name: &'static str,
    value: &'a str,
}

#[derive(Deserialize)]
struct ResendEmailResponse {
    id: String,
}

fn classify_status(status: StatusCode) -> DigestEmailDeliveryError {
    if status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::CONFLICT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
    {
        DigestEmailDeliveryError::Retryable
    } else {
        DigestEmailDeliveryError::Rejected
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::{
        body::{to_bytes, Body},
        extract::State,
        http::{HeaderMap, Request},
        routing::post,
        Router,
    };
    use serde_json::Value;

    use super::*;
    use crate::{
        beta_access::NormalizedEmail,
        daily_coaching::{delivery::RenderedDigestEmail, DailyCoachingOwnerKey},
        review_session_contract::PlayerId,
    };

    #[derive(Default)]
    struct Captured {
        headers: HeaderMap,
        payload: Value,
    }

    #[tokio::test]
    async fn sends_one_click_headers_and_private_owner_tags() {
        let captured = Arc::new(Mutex::new(Captured::default()));
        let app = Router::new()
            .route("/emails", post(capture))
            .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let endpoint = format!("http://{}/emails", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let delivery = ResendDigestEmailDelivery::for_test(endpoint);
        let owner_key = DailyCoachingOwnerKey::for_player(
            &PlayerId::try_from("resend-player".to_string()).unwrap(),
        );

        delivery
            .deliver(DigestEmailRequest {
                delivery_id: "daily-2026-08-10".to_string(),
                owner_key: owner_key.clone(),
                recipient: NormalizedEmail::parse("player@example.com").unwrap(),
                rendered: RenderedDigestEmail {
                    subject: "Digest".to_string(),
                    text: "Text".to_string(),
                    html: "<p>HTML</p>".to_string(),
                },
                unsubscribe_url: "https://coach.example.test/unsubscribe?token=opaque".to_string(),
            })
            .await
            .unwrap();

        let captured = captured.lock().unwrap();
        assert_eq!(captured.payload["to"][0], "player@example.com");
        assert_eq!(
            captured.payload["headers"]["List-Unsubscribe"],
            "<https://coach.example.test/unsubscribe?token=opaque>"
        );
        assert_eq!(
            captured.payload["headers"]["List-Unsubscribe-Post"],
            "List-Unsubscribe=One-Click"
        );
        assert_eq!(captured.payload["tags"][0]["name"], "coaching_owner");
        assert_eq!(captured.payload["tags"][0]["value"], owner_key.as_str());
        assert_eq!(captured.payload["tags"][1]["name"], "delivery_id");
        assert_eq!(
            captured
                .headers
                .get("idempotency-key")
                .unwrap()
                .to_str()
                .unwrap(),
            format!(
                "daily-coaching-digest/{}/daily-2026-08-10",
                owner_key.as_str()
            )
        );
        server.abort();
    }

    #[test]
    fn concurrent_idempotent_request_remains_retryable() {
        assert_eq!(
            classify_status(StatusCode::CONFLICT),
            DigestEmailDeliveryError::Retryable
        );
        assert_eq!(
            classify_status(StatusCode::UNPROCESSABLE_ENTITY),
            DigestEmailDeliveryError::Rejected
        );
    }

    async fn capture(
        State(captured): State<Arc<Mutex<Captured>>>,
        request: Request<Body>,
    ) -> axum::Json<Value> {
        let (parts, body) = request.into_parts();
        let payload = serde_json::from_slice(&to_bytes(body, usize::MAX).await.unwrap()).unwrap();
        *captured.lock().unwrap() = Captured {
            headers: parts.headers,
            payload,
        };
        axum::Json(serde_json::json!({ "id": "provider-message-1" }))
    }

    #[test]
    fn a_rebuilt_digest_send_never_reuses_the_original_idempotency_key() {
        let owner_key = crate::daily_coaching::DailyCoachingOwnerKey::for_player(
            &crate::review_session_contract::PlayerId::try_from("player-1".to_string()).unwrap(),
        );
        let request = |delivery_id: &str| DigestEmailRequest {
            delivery_id: delivery_id.to_string(),
            owner_key: owner_key.clone(),
            recipient: NormalizedEmail::parse("player@example.com").unwrap(),
            rendered: RenderedDigestEmail {
                subject: "Digest".to_string(),
                text: "Text".to_string(),
                html: "<p>HTML</p>".to_string(),
            },
            unsubscribe_url: "https://coach.example.test/unsubscribe?token=opaque".to_string(),
        };

        let original = delivery_idempotency_key(&request("daily-2026-08-10"));
        let rebuilt = delivery_idempotency_key(&request("daily-2026-08-10-r1"));

        assert_ne!(
            original, rebuilt,
            "a regenerated digest must reach the provider instead of collapsing into the original"
        );
        // Both still route as digest mail rather than the profile-unavailable notice.
        assert!(original.starts_with("daily-coaching-digest/"));
        assert!(rebuilt.starts_with("daily-coaching-digest/"));
    }
}
