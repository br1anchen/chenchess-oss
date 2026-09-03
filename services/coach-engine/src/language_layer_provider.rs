//! The Language Layer provider port.
//!
//! #368 and ADR 0051: one
//! named port is the only way a hosted completion is issued. The OpenRouter
//! adapter lives here; the bake-off harness calls it. Nothing partial crosses
//! the port. Request invariants live in the adapter, not at call sites.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::retry_after::{parse_retry_after, retry_delay_from_error_body};

pub const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";

/// After the task deadline, keep the in-flight send this long so headers
/// already on the wire can still yield a generation id. Never fall through
/// to the client timeout — that wait is additive.
pub const IN_FLIGHT_GRACE: Duration = Duration::from_millis(250);

/// The pin the adapter must honour on every completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedGenerationContract {
    pub model: String,
    pub provider_only: String,
    pub max_tokens: u32,
    pub determinism: DeterminismControls,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeterminismControls {
    pub temperature: bool,
    pub seed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub contract: PinnedGenerationContract,
    pub messages: Vec<ChatMessage>,
    pub schema_name: String,
    pub schema: Value,
    pub remaining_deadline: Duration,
}

/// One buffered attempt. Usage is populated whenever the provider billed,
/// including timed-out and cancelled attempts that still returned a body.
#[derive(Debug, Clone)]
pub struct CompletionAttempt {
    pub latency: Duration,
    pub http_status: Option<u16>,
    pub generation_id: Option<String>,
    pub served_model: Option<String>,
    pub served_provider: Option<String>,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cost: Option<f64>,
    pub finish_reason: Option<String>,
    pub raw_content: Option<String>,
    pub outcome: CompletionOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionOutcome {
    Completed,
    EmptyCompletion,
    HttpError,
    InvalidRequest,
    SchemaRejected,
    TimedOut,
    DeadlineExhausted,
    TransportError,
    /// HTTP 429. `retry_after` is the validated header or RetryInfo delay,
    /// or `None` when both are missing or unusable; callers then use the
    /// configured 1 s floor. `source` says which signal produced the delay.
    RateLimited {
        retry_after: Option<Duration>,
        source: RateLimitDelaySource,
    },
}

/// Where a 429's honoured wait came from. Validated at the HTTP boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDelaySource {
    Header,
    RetryInfo,
    Unspecified,
}

impl RateLimitDelaySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::RetryInfo => "retryInfo",
            Self::Unspecified => "floor",
        }
    }
}

impl CompletionOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::EmptyCompletion => "emptyCompletion",
            Self::HttpError => "httpError",
            Self::InvalidRequest => "invalidRequest",
            Self::SchemaRejected => "schemaRejected",
            Self::TimedOut => "timedOut",
            Self::DeadlineExhausted => "deadlineExhausted",
            Self::TransportError => "transportError",
            Self::RateLimited { .. } => "rateLimited",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PinVerification {
    pub verified_permaslug: Option<String>,
    pub verified_provider: Option<String>,
    pub served_endpoint_id: Option<String>,
    pub served_region: Option<String>,
    pub routed_service_tier: Option<String>,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub cost: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PostureReport {
    pub key_readable: bool,
    pub pinned_endpoint_on_zdr: bool,
    pub filtered_model_count: Option<usize>,
    pub public_model_count: Option<usize>,
    pub catalogue_is_narrowed: Option<bool>,
    pub limit_remaining: Value,
    pub is_free_tier: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    #[error("the provider allowlist must carry the full endpoint tag, not a family name")]
    ProviderTagNotFull,
    #[error("the model slug must not carry a variant suffix")]
    ModelHasVariantSuffix,
    #[error("Structured Output Mode requires a nativeSchema response format")]
    MissingSchema,
    #[error("a completion request must carry at least one message")]
    EmptyMessages,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PostureError {
    #[error("could not construct the OpenRouter client: {0}")]
    Client(String),
    #[error("the pinned endpoint {tag} is missing from the live ZDR list")]
    PinnedEndpointNotOnZdr { tag: String },
    #[error("OpenRouter posture check failed: {0}")]
    Transport(String),
    #[error("the OpenRouter account key is unreadable: {0}")]
    KeyUnreadable(String),
}

/// The named Language Layer provider port. Today the only adapter is OpenRouter.
pub struct LanguageLayerProvider {
    client: reqwest::Client,
    api_key: String,
    base: String,
}

impl LanguageLayerProvider {
    pub fn new(api_key: impl Into<String>) -> Result<Self, PostureError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| PostureError::Client(error.to_string()))?;
        Ok(Self {
            client,
            api_key: api_key.into(),
            base: OPENROUTER_BASE.to_string(),
        })
    }

    pub fn from_client(client: reqwest::Client, api_key: impl Into<String>) -> Self {
        Self {
            client,
            api_key: api_key.into(),
            base: OPENROUTER_BASE.to_string(),
        }
    }

    pub fn from_client_at(
        client: reqwest::Client,
        api_key: impl Into<String>,
        base: impl Into<String>,
    ) -> Self {
        Self {
            client,
            api_key: api_key.into(),
            base: base.into(),
        }
    }

    /// One buffered structured completion. The adapter may stream internally
    /// for abort; nothing partial is returned.
    pub async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionAttempt, RequestError> {
        if request.remaining_deadline.is_zero() {
            return Ok(blank_attempt(
                Duration::ZERO,
                CompletionOutcome::DeadlineExhausted,
            ));
        }
        let body = completion_request_body(request)?;
        let started = Instant::now();
        let send = self
            .client
            .post(format!("{}/chat/completions", self.base))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send();
        tokio::pin!(send);
        let remaining = request.remaining_deadline;
        let response = tokio::select! {
            result = &mut send => match result {
                Ok(response) => response,
                Err(error) => {
                    let mut attempt =
                        blank_attempt(started.elapsed(), CompletionOutcome::TransportError);
                    attempt.raw_content = Some(error.to_string());
                    return Ok(attempt);
                }
            },
            _ = tokio::time::sleep(remaining) => {
                return Ok(match tokio::time::timeout(IN_FLIGHT_GRACE, send).await {
                    Ok(Ok(response)) => self.timed_out_from_response(response, started).await,
                    Ok(Err(_)) | Err(_) => {
                        blank_attempt(started.elapsed(), CompletionOutcome::TimedOut)
                    }
                });
            }
        };
        let generation_id = generation_id_from(&response);
        let leftover = remaining.saturating_sub(started.elapsed());
        let attempt = match tokio::time::timeout(leftover, read_completion(response, started)).await
        {
            Err(_) => {
                let mut attempt = blank_attempt(started.elapsed(), CompletionOutcome::TimedOut);
                attempt.generation_id = generation_id;
                attempt
            }
            Ok(attempt) => attempt,
        };
        Ok(self.with_billed_usage(attempt).await)
    }

    async fn timed_out_from_response(
        &self,
        response: reqwest::Response,
        started: Instant,
    ) -> CompletionAttempt {
        let mut attempt = blank_attempt(started.elapsed(), CompletionOutcome::TimedOut);
        attempt.http_status = Some(response.status().as_u16());
        attempt.generation_id = generation_id_from(&response);
        drop(response);
        self.with_billed_usage(attempt).await
    }

    async fn with_billed_usage(&self, mut attempt: CompletionAttempt) -> CompletionAttempt {
        let Some(generation_id) = attempt.generation_id.clone() else {
            return attempt;
        };
        if attempt.cost.is_some() || attempt.prompt_tokens.is_some() {
            return attempt;
        }
        if !matches!(
            attempt.outcome,
            CompletionOutcome::TimedOut | CompletionOutcome::DeadlineExhausted
        ) {
            return attempt;
        }
        let pin = self.verify_generation(&generation_id).await;
        if attempt.prompt_tokens.is_none() {
            attempt.prompt_tokens = pin.prompt_tokens;
        }
        if attempt.completion_tokens.is_none() {
            attempt.completion_tokens = pin.completion_tokens;
        }
        if attempt.cost.is_none() {
            attempt.cost = pin.cost;
        }
        attempt
    }

    /// Pin Verification is a second authenticated call: live completions do
    /// not carry the served route.
    pub async fn verify_generation(&self, generation_id: &str) -> PinVerification {
        let url = format!("{}/generation?id={generation_id}", self.base);
        match fetch_json(&self.client, &url, Some(&self.api_key)).await {
            Ok(value) => {
                let data = &value["data"];
                let (prompt_tokens, completion_tokens, cost) = usage_from_generation(data);
                PinVerification {
                    verified_permaslug: data["model"].as_str().map(str::to_string),
                    verified_provider: data["provider_name"].as_str().map(str::to_string),
                    served_endpoint_id: data["provider_responses"][0]["endpoint_id"]
                        .as_str()
                        .map(str::to_string),
                    served_region: data["data_region"].as_str().map(str::to_string),
                    routed_service_tier: data["provider_responses"][0]["routed_service_tier"]
                        .as_str()
                        .map(str::to_string),
                    prompt_tokens,
                    completion_tokens,
                    cost,
                    error: None,
                }
            }
            Err(error) => PinVerification {
                error: Some(truncate(&error, 200)),
                ..PinVerification::default()
            },
        }
    }

    /// Boot assertion: the live ZDR list still contains every pinned tag, and
    /// the account key is readable. Either failing refuses hosted serving.
    pub async fn assert_posture(
        &self,
        pins: &[(String, String)],
    ) -> Result<PostureReport, PostureError> {
        let key_info = fetch_json(
            &self.client,
            &format!("{}/key", self.base),
            Some(&self.api_key),
        )
        .await
        .map_err(PostureError::KeyUnreadable)?;

        let zdr = fetch_json(&self.client, &format!("{}/endpoints/zdr", self.base), None)
            .await
            .map_err(PostureError::Transport)?;
        let endpoints = zdr["data"]
            .as_array()
            .ok_or_else(|| PostureError::Transport("the ZDR listing has no data array".into()))?;
        for (catalogue_slug, tag) in pins {
            let listed = endpoints.iter().any(|endpoint| {
                endpoint["model_id"].as_str() == Some(catalogue_slug.as_str())
                    && endpoint["tag"].as_str() == Some(tag.as_str())
            });
            if !listed {
                return Err(PostureError::PinnedEndpointNotOnZdr { tag: tag.clone() });
            }
        }

        let filtered = fetch_json(
            &self.client,
            &format!("{}/models/user", self.base),
            Some(&self.api_key),
        )
        .await;
        let public = fetch_json(&self.client, &format!("{}/models", self.base), None).await;
        let count = |value: &Result<Value, String>| {
            value
                .as_ref()
                .ok()
                .and_then(|value| value["data"].as_array().map(Vec::len))
        };
        let filtered_model_count = count(&filtered);
        let public_model_count = count(&public);
        Ok(PostureReport {
            key_readable: true,
            pinned_endpoint_on_zdr: true,
            filtered_model_count,
            public_model_count,
            catalogue_is_narrowed: match (filtered_model_count, public_model_count) {
                (Some(filtered), Some(public)) => Some(filtered < public),
                _ => None,
            },
            limit_remaining: key_info["data"]["limit_remaining"].clone(),
            is_free_tier: key_info["data"]["is_free_tier"].clone(),
        })
    }
}

/// Builds the OpenRouter body and refuses anything that contradicts the pin.
pub fn completion_request_body(request: &CompletionRequest) -> Result<Value, RequestError> {
    let contract = &request.contract;
    if !contract.provider_only.contains('/') {
        return Err(RequestError::ProviderTagNotFull);
    }
    if contract.model.contains(':') {
        return Err(RequestError::ModelHasVariantSuffix);
    }
    if request.schema_name.is_empty() || request.schema.is_null() {
        return Err(RequestError::MissingSchema);
    }
    if request.messages.is_empty() {
        return Err(RequestError::EmptyMessages);
    }

    let mut body = json!({
        "model": contract.model,
        "messages": request.messages.iter().map(|message| {
            json!({ "role": message.role, "content": message.content })
        }).collect::<Vec<_>>(),
        "max_tokens": contract.max_tokens,
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": request.schema_name,
                "strict": true,
                "schema": request.schema,
            }
        },
        "provider": {
            "only": [contract.provider_only],
            "allow_fallbacks": false,
            "require_parameters": true,
            "data_collection": "deny",
            "zdr": true,
        },
    });
    if contract.determinism.temperature {
        body["temperature"] = json!(0);
    }
    if contract.determinism.seed {
        body["seed"] = json!(0);
    }
    debug_assert!(body.get("models").is_none());
    debug_assert!(body.get("route").is_none());
    Ok(body)
}

async fn read_completion(response: reqwest::Response, started: Instant) -> CompletionAttempt {
    let status = response.status().as_u16();
    let generation_id = generation_id_from(&response);
    let header_retry_after = retry_after_from_headers(response.headers(), Utc::now());
    let text = response.text().await.unwrap_or_default();
    let latency = started.elapsed();
    let parsed = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    let usage = &parsed["usage"];
    let usage_fields = (
        usage["prompt_tokens"].as_u64(),
        usage["completion_tokens"].as_u64(),
        usage["completion_tokens_details"]["reasoning_tokens"].as_u64(),
        usage["cost"].as_f64(),
    );

    if status != 200 {
        let outcome = wire_error_outcome(status, &parsed, &text, header_retry_after);
        if outcome == CompletionOutcome::SchemaRejected {
            tracing::error!(
                status,
                "Language Layer nativeSchema rejected; refusing to retry without the schema"
            );
        }
        return CompletionAttempt {
            latency,
            http_status: Some(status),
            generation_id,
            served_model: None,
            served_provider: None,
            prompt_tokens: usage_fields.0,
            completion_tokens: usage_fields.1,
            reasoning_tokens: usage_fields.2,
            cost: usage_fields.3,
            finish_reason: None,
            raw_content: Some(truncate(&text, 600)),
            outcome,
        };
    }

    let choice = &parsed["choices"][0];
    let content = choice["message"]["content"].as_str().map(str::to_string);
    CompletionAttempt {
        latency,
        http_status: Some(status),
        generation_id: generation_id.or_else(|| parsed["id"].as_str().map(str::to_string)),
        served_model: parsed["model"].as_str().map(str::to_string),
        served_provider: parsed["provider"].as_str().map(str::to_string),
        prompt_tokens: usage_fields.0,
        completion_tokens: usage_fields.1,
        reasoning_tokens: usage_fields.2,
        cost: usage_fields.3,
        finish_reason: choice["finish_reason"].as_str().map(str::to_string),
        outcome: if content.is_some() {
            CompletionOutcome::Completed
        } else {
            CompletionOutcome::EmptyCompletion
        },
        raw_content: content,
    }
}

fn usage_from_generation(data: &Value) -> (Option<u64>, Option<u64>, Option<f64>) {
    (
        data["tokens_prompt"]
            .as_u64()
            .or_else(|| data["native_tokens_prompt"].as_u64()),
        data["tokens_completion"]
            .as_u64()
            .or_else(|| data["native_tokens_completion"].as_u64()),
        data["total_cost"]
            .as_f64()
            .or_else(|| data["usage"].as_f64()),
    )
}

fn generation_id_from(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get("x-generation-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn retry_after_from_headers(
    headers: &reqwest::header::HeaderMap,
    now: DateTime<Utc>,
) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_retry_after(value, now))
}

fn openrouter_error_code(parsed: &Value) -> Option<&str> {
    parsed["error"]["code"].as_str()
}

fn openrouter_error_type(parsed: &Value) -> Option<&str> {
    parsed["error"]["metadata"]["error_type"]
        .as_str()
        .or_else(|| parsed["error_type"].as_str())
}

fn is_pin_reject(parsed: &Value) -> bool {
    // JSON taxonomy only. Numeric 404/503 echo the status line and are outages.
    const PIN: &[&str] = &["invalid_request", "no_endpoints", "no-endpoints"];
    [openrouter_error_code(parsed), openrouter_error_type(parsed)]
        .into_iter()
        .flatten()
        .any(|code| PIN.contains(&code))
}

fn wire_error_outcome(
    status: u16,
    parsed: &Value,
    body: &str,
    header_retry_after: Option<Duration>,
) -> CompletionOutcome {
    if status == 429 {
        return rate_limited_outcome(header_retry_after, parsed);
    }
    if is_schema_rejection(status, parsed, body) {
        return CompletionOutcome::SchemaRejected;
    }
    if is_pin_reject(parsed) {
        return CompletionOutcome::InvalidRequest;
    }
    CompletionOutcome::HttpError
}

fn rate_limited_outcome(header_retry_after: Option<Duration>, parsed: &Value) -> CompletionOutcome {
    let usable_header = header_retry_after.filter(|wait| !wait.is_zero());
    if let Some(retry_after) = usable_header {
        return CompletionOutcome::RateLimited {
            retry_after: Some(retry_after),
            source: RateLimitDelaySource::Header,
        };
    }
    if let Some(retry_after) = retry_delay_from_error_body(parsed) {
        return CompletionOutcome::RateLimited {
            retry_after: Some(retry_after),
            source: RateLimitDelaySource::RetryInfo,
        };
    }
    CompletionOutcome::RateLimited {
        retry_after: None,
        source: RateLimitDelaySource::Unspecified,
    }
}

fn is_schema_rejection(status: u16, parsed: &Value, body: &str) -> bool {
    if is_pin_reject(parsed) || !(400..500).contains(&status) {
        return false;
    }
    let lowered = body.to_ascii_lowercase();
    lowered.contains("response_format")
        || lowered.contains("json_schema")
        || lowered.contains("structured output")
        || lowered.contains("nativeschema")
}

fn blank_attempt(latency: Duration, outcome: CompletionOutcome) -> CompletionAttempt {
    CompletionAttempt {
        latency,
        http_status: None,
        generation_id: None,
        served_model: None,
        served_provider: None,
        prompt_tokens: None,
        completion_tokens: None,
        reasoning_tokens: None,
        cost: None,
        finish_reason: None,
        raw_content: None,
        outcome,
    }
}

pub async fn fetch_json(
    client: &reqwest::Client,
    url: &str,
    key: Option<&str>,
) -> Result<Value, String> {
    let mut request = client.get(url);
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("{url} returned {status}: {}", truncate(&body, 300)));
    }
    serde_json::from_str(&body).map_err(|error| format!("{url} returned invalid JSON: {error}"))
}

fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        value.to_string()
    } else {
        format!("{}…", &value[..limit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> CompletionRequest {
        CompletionRequest {
            contract: PinnedGenerationContract {
                model: "google/gemini-3.5-flash-lite-20260721".into(),
                provider_only: "google-vertex/global".into(),
                max_tokens: 512,
                determinism: DeterminismControls {
                    temperature: true,
                    seed: false,
                },
            },
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: "sys".into(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: "hi".into(),
                },
            ],
            schema_name: "review_moment_comment".into(),
            schema: json!({"type": "object"}),
            remaining_deadline: Duration::from_secs(20),
        }
    }

    #[test]
    fn the_adapter_owns_every_posture_invariant() {
        let body = completion_request_body(&sample_request()).expect("valid pin");
        assert_eq!(body["provider"]["only"], json!(["google-vertex/global"]));
        assert_eq!(body["provider"]["allow_fallbacks"], json!(false));
        assert_eq!(body["provider"]["require_parameters"], json!(true));
        assert_eq!(body["provider"]["zdr"], json!(true));
        assert_eq!(body["provider"]["data_collection"], json!("deny"));
        assert!(body.get("models").is_none());
        assert!(body.get("route").is_none());
        assert_eq!(body["response_format"]["type"], json!("json_schema"));
        assert_eq!(body["temperature"], json!(0));
        assert!(body.get("seed").is_none());
    }

    #[test]
    fn a_family_name_is_not_a_pin() {
        let mut request = sample_request();
        request.contract.provider_only = "google".into();
        assert_eq!(
            completion_request_body(&request),
            Err(RequestError::ProviderTagNotFull)
        );
    }

    #[test]
    fn a_variant_suffix_is_refused() {
        let mut request = sample_request();
        request.contract.model = "google/gemini-3.5-flash-lite:free".into();
        assert_eq!(
            completion_request_body(&request),
            Err(RequestError::ModelHasVariantSuffix)
        );
    }

    #[test]
    fn a_missing_schema_is_refused() {
        let mut request = sample_request();
        request.schema = Value::Null;
        assert_eq!(
            completion_request_body(&request),
            Err(RequestError::MissingSchema)
        );
    }

    #[test]
    fn schema_rejection_is_its_own_reason() {
        let schema = serde_json::from_str(
            r#"{"error":{"code":400,"message":"response_format json_schema is not supported"}}"#,
        )
        .unwrap();
        assert_eq!(
            wire_error_outcome(
                400,
                &schema,
                "response_format json_schema is not supported",
                None
            ),
            CompletionOutcome::SchemaRejected
        );
        assert_eq!(
            wire_error_outcome(429, &json!({}), "rate limited", None),
            CompletionOutcome::RateLimited {
                retry_after: None,
                source: RateLimitDelaySource::Unspecified,
            }
        );
        assert_eq!(
            wire_error_outcome(
                429,
                &json!({}),
                "rate limit exceeded for json_schema response_format",
                None
            ),
            CompletionOutcome::RateLimited {
                retry_after: None,
                source: RateLimitDelaySource::Unspecified,
            }
        );
        assert_eq!(
            CompletionOutcome::RateLimited {
                retry_after: None,
                source: RateLimitDelaySource::Unspecified,
            }
            .as_str(),
            "rateLimited"
        );
    }

    #[test]
    fn a_wire_pin_reject_is_classified_by_error_code() {
        let pin = json!({"error": {"code": "no_endpoints", "message": "ignored"}});
        assert_eq!(
            wire_error_outcome(404, &pin, "ignored", None),
            CompletionOutcome::InvalidRequest
        );
        let typed = json!({"error": {"code": 400, "metadata": {"error_type": "invalid_request"}}});
        assert_eq!(
            wire_error_outcome(400, &typed, "ignored", None),
            CompletionOutcome::InvalidRequest
        );
        let status_echo = json!({"error": {"code": 404, "message": "ignored"}});
        assert_eq!(
            wire_error_outcome(404, &status_echo, "ignored", None),
            CompletionOutcome::HttpError
        );
        let outage = json!({"error": {"code": 503, "message": "ignored"}});
        assert_eq!(
            wire_error_outcome(503, &outage, "ignored", None),
            CompletionOutcome::HttpError
        );
        let english_only = json!({"error": {"code": 400, "message": "no endpoints found"}});
        assert_eq!(
            wire_error_outcome(400, &english_only, "no endpoints found", None),
            CompletionOutcome::HttpError
        );
    }

    #[test]
    fn a_zero_deadline_is_not_attempted() {
        let provider = LanguageLayerProvider::from_client(reqwest::Client::new(), "test");
        let mut request = sample_request();
        request.remaining_deadline = Duration::ZERO;
        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(provider.complete(&request));
        assert_eq!(
            outcome.expect("deadline is not a request error").outcome,
            CompletionOutcome::DeadlineExhausted
        );
    }

    #[test]
    fn a_bad_pin_is_a_request_error_not_an_http_error() {
        let provider = LanguageLayerProvider::from_client(reqwest::Client::new(), "test");
        let mut request = sample_request();
        request.contract.provider_only = "google".into();
        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(provider.complete(&request));
        assert_eq!(outcome.err(), Some(RequestError::ProviderTagNotFull));
    }

    #[test]
    fn generation_records_still_carry_usage_after_a_timeout() {
        let data = json!({
            "tokens_prompt": 12,
            "tokens_completion": 4,
            "total_cost": 0.0012
        });
        assert_eq!(
            usage_from_generation(&data),
            (Some(12), Some(4), Some(0.0012))
        );
    }

    #[test]
    fn a_timeout_keeps_the_in_flight_generation_id() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 2048];
            let _ = socket.read(&mut buf);
            std::thread::sleep(Duration::from_millis(120));
            let body = "{}";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nx-generation-id: gen-keep\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes());
        });

        let provider = LanguageLayerProvider::from_client_at(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .expect("client"),
            "test",
            format!("http://{addr}"),
        );
        let mut request = sample_request();
        request.remaining_deadline = Duration::from_millis(40);
        let attempt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(provider.complete(&request))
            .expect("timeout is not a request error");
        assert_eq!(attempt.outcome, CompletionOutcome::TimedOut);
        assert_eq!(attempt.generation_id.as_deref(), Some("gen-keep"));
    }

    #[test]
    fn a_late_send_is_cut_off_after_in_flight_grace() {
        use std::io::Read;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 2048];
            let _ = socket.read(&mut buf);
            std::thread::sleep(Duration::from_secs(2));
        });

        let provider = LanguageLayerProvider::from_client_at(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("client"),
            "test",
            format!("http://{addr}"),
        );
        let mut request = sample_request();
        request.remaining_deadline = Duration::from_millis(20);
        let started = Instant::now();
        let attempt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(provider.complete(&request))
            .expect("timeout is not a request error");
        let elapsed = started.elapsed();
        assert_eq!(attempt.outcome, CompletionOutcome::TimedOut);
        assert!(attempt.generation_id.is_none());
        assert!(
            elapsed < Duration::from_millis(800),
            "in-flight grace must not fall through to the client timeout, took {elapsed:?}"
        );
    }

    #[test]
    fn a_429_is_rate_limited_and_status_wins_over_a_429_body_code() {
        use std::io::{Read, Write};

        fn serve(status_line: &str, extra_headers: &str, body: &str) -> String {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("addr");
            let status_line = status_line.to_string();
            let extra_headers = extra_headers.to_string();
            let body = body.to_string();
            std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().expect("accept");
                let mut buf = [0u8; 2048];
                let _ = socket.read(&mut buf);
                let response = format!(
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes());
            });
            format!("http://{addr}")
        }

        fn complete_against(base: String) -> CompletionAttempt {
            let provider = LanguageLayerProvider::from_client_at(
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(2))
                    .build()
                    .expect("client"),
                "test",
                base,
            );
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime")
                .block_on(provider.complete(&sample_request()))
                .expect("HTTP error is not a request error")
        }

        let limited = complete_against(serve(
            "HTTP/1.1 429 Too Many Requests",
            "Retry-After: 7\r\n",
            r#"{"error":{"code":429,"message":"rate limited"}}"#,
        ));
        assert_eq!(
            limited.outcome,
            CompletionOutcome::RateLimited {
                retry_after: Some(Duration::from_secs(7)),
                source: RateLimitDelaySource::Header,
            }
        );
        assert_eq!(limited.http_status, Some(429));

        let missing = complete_against(serve(
            "HTTP/1.1 429 Too Many Requests",
            "",
            r#"{"error":{"code":429,"message":"rate limited"}}"#,
        ));
        assert_eq!(
            missing.outcome,
            CompletionOutcome::RateLimited {
                retry_after: None,
                source: RateLimitDelaySource::Unspecified,
            }
        );

        let retry_info = complete_against(serve(
            "HTTP/1.1 429 Too Many Requests",
            "",
            r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED","details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"8s"}]}}"#,
        ));
        assert_eq!(
            retry_info.outcome,
            CompletionOutcome::RateLimited {
                retry_after: Some(Duration::from_secs(8)),
                source: RateLimitDelaySource::RetryInfo,
            }
        );

        let status_wins = complete_against(serve(
            "HTTP/1.1 503 Service Unavailable",
            "",
            r#"{"error":{"code":429,"message":"rate limited"}}"#,
        ));
        assert_eq!(status_wins.outcome, CompletionOutcome::HttpError);
        assert_eq!(status_wins.http_status, Some(503));

        let outage = complete_against(serve(
            "HTTP/1.1 503 Service Unavailable",
            "",
            r#"{"error":{"code":503,"message":"service unavailable"}}"#,
        ));
        assert_eq!(outage.outcome, CompletionOutcome::HttpError);
        assert_eq!(outage.http_status, Some(503));
    }
}
