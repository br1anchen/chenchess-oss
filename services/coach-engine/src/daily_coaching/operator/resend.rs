use std::{sync::Arc, time::Duration};

use reqwest::{redirect::Policy, Client};
use serde::{Deserialize, Serialize};

use super::{
    OperatorDeliveryFuture, OperatorDigestDelivery, OperatorDigestEmail, OperatorDigestError,
};
use crate::daily_coaching::valid_resend_provider_id;

const RESEND_EMAILS_ENDPOINT: &str = "https://api.resend.com/emails";
const FROM: &str = "ChenChess <coach@example.test>";
const REPLY_TO: &str = "support@example.test";
const MAX_RESPONSE_BYTES: usize = 4 * 1024;

pub(super) struct ResendOperatorDigestDelivery {
    api_key: Arc<str>,
    client: Client,
}

impl ResendOperatorDigestDelivery {
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
        })
    }
}

impl OperatorDigestDelivery for ResendOperatorDigestDelivery {
    fn deliver<'a>(&'a self, request: OperatorDigestEmail) -> OperatorDeliveryFuture<'a> {
        Box::pin(async move {
            let payload = ResendEmail {
                from: FROM,
                html: request.rendered.html,
                reply_to: REPLY_TO,
                subject: request.rendered.subject,
                tags: [ResendTag {
                    name: "operator_digest",
                    value: &request.digest_id,
                }],
                text: request.rendered.text,
                to: [request.recipient.as_str()],
            };
            let mut response = self
                .client
                .post(RESEND_EMAILS_ENDPOINT)
                .bearer_auth(self.api_key.as_ref())
                .header(
                    "Idempotency-Key",
                    format!("daily-coaching-operator/{}", request.digest_id),
                )
                .json(&payload)
                .send()
                .await
                .map_err(|_| OperatorDigestError::Delivery)?;
            if !response.status().is_success() {
                return Err(OperatorDigestError::Delivery);
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
            {
                return Err(OperatorDigestError::Delivery);
            }
            let mut body = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| OperatorDigestError::Delivery)?
            {
                if chunk.len() > MAX_RESPONSE_BYTES - body.len() {
                    return Err(OperatorDigestError::Delivery);
                }
                body.extend_from_slice(&chunk);
            }
            let response: ResendEmailResponse =
                serde_json::from_slice(&body).map_err(|_| OperatorDigestError::Delivery)?;
            if !valid_resend_provider_id(&response.id) {
                return Err(OperatorDigestError::Delivery);
            }
            Ok(response.id)
        })
    }
}

#[derive(Serialize)]
struct ResendEmail<'a> {
    from: &'static str,
    html: String,
    reply_to: &'static str,
    subject: String,
    tags: [ResendTag<'a>; 1],
    text: String,
    to: [&'a str; 1],
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
