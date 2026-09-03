use std::error::Error;

use reqwest::{Response, StatusCode};
use serde::Deserialize;

use super::FirestoreError;

const MAX_LOG_FIELD_CHARS: usize = 512;

pub(super) async fn require_success(
    operation: &'static str,
    response: Response,
    map_write_conflict: bool,
) -> Result<Response, FirestoreError> {
    if response.status().is_success() {
        return Ok(response);
    }
    Err(firestore_response_error(operation, response, map_write_conflict).await)
}

pub(super) fn transport_error(operation: &'static str, error: &reqwest::Error) -> FirestoreError {
    tracing::error!(
        firestore_operation = operation,
        error = %sanitized(error),
        cause = %source_chain(error),
        is_connect = error.is_connect(),
        is_request = error.is_request(),
        is_timeout = error.is_timeout(),
        http_status = ?error.status().map(|status| status.as_u16()),
        "Firestore transport failed"
    );
    FirestoreError::Transport
}

pub(super) fn invalid_response_error(
    operation: &'static str,
    error: &reqwest::Error,
) -> FirestoreError {
    tracing::error!(
        firestore_operation = operation,
        error = %sanitized(error),
        cause = %source_chain(error),
        "Firestore returned an invalid success response"
    );
    FirestoreError::InvalidDocument
}

pub(super) fn unavailable_error(operation: &'static str, error: &dyn Error) -> FirestoreError {
    tracing::error!(
        firestore_operation = operation,
        error = %sanitized(error),
        cause = %source_chain(error),
        "Firestore authentication failed"
    );
    FirestoreError::Unavailable
}

pub(super) async fn oauth_response_error(response: Response) -> FirestoreError {
    let status = response.status();
    let (details, response_body_format, body_read_error) = match response.text().await {
        Ok(body) => match serde_json::from_str::<GoogleOAuthError>(&body) {
            Ok(details) => (details, "google_oauth_error", None),
            Err(_) => (GoogleOAuthError::default(), "unrecognized", None),
        },
        Err(error) => (
            GoogleOAuthError::default(),
            "unreadable",
            Some(source_chain(&error)),
        ),
    };
    tracing::error!(
        firestore_operation = "service_account_token",
        http_status = status.as_u16(),
        oauth_error = %sanitize_text(details.error.as_deref().unwrap_or("not provided")),
        oauth_error_description = %sanitize_text(
            details.error_description.as_deref().unwrap_or("not provided")
        ),
        response_body_format,
        response_body_read_error = %body_read_error.as_deref().unwrap_or("none"),
        "Firestore service-account token request failed"
    );
    FirestoreError::Unavailable
}

async fn firestore_response_error(
    operation: &'static str,
    response: Response,
    map_write_conflict: bool,
) -> FirestoreError {
    let status = response.status();
    let (details, response_body_format, body_read_error, unrecognized_body) =
        match response.text().await {
            Ok(body) => match google_api_error(&body) {
                Some(details) => (details, "google_api_error", None, None),
                None => (
                    GoogleApiError::default(),
                    "unrecognized",
                    None,
                    Some(sanitize_text(&body)),
                ),
            },
            Err(error) => (
                GoogleApiError::default(),
                "unreadable",
                Some(source_chain(&error)),
                None,
            ),
        };
    tracing::error!(
        firestore_operation = operation,
        http_status = status.as_u16(),
        google_error_code = ?details.code,
        google_error_status = %sanitize_text(details.status.as_deref().unwrap_or("not provided")),
        google_error_message = %sanitize_text(details.message.as_deref().unwrap_or("not provided")),
        response_body_format,
        response_body_read_error = %body_read_error.as_deref().unwrap_or("none"),
        unrecognized_body = %unrecognized_body.as_deref().unwrap_or("none"),
        "Firestore API request failed"
    );
    if map_write_conflict
        && matches!(
            status,
            StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED
        )
    {
        FirestoreError::Conflict
    } else {
        FirestoreError::Unavailable
    }
}

pub(super) fn sanitized(error: &dyn Error) -> String {
    sanitize_text(&error.to_string())
}

fn source_chain(error: &dyn Error) -> String {
    let mut causes = Vec::new();
    let mut source = error.source();
    while let Some(cause) = source {
        causes.push(sanitize_text(&cause.to_string()));
        source = cause.source();
        if causes.len() == 4 {
            break;
        }
    }
    if causes.is_empty() {
        "none".to_string()
    } else {
        causes.join(": ")
    }
}

fn sanitize_text(value: &str) -> String {
    let mut sanitized = String::new();
    let mut preceding_space = false;
    for character in value.chars().take(MAX_LOG_FIELD_CHARS) {
        if character.is_control() || character.is_whitespace() {
            if !preceding_space {
                sanitized.push(' ');
                preceding_space = true;
            }
        } else {
            sanitized.push(character);
            preceding_space = false;
        }
    }
    sanitized.trim().to_string()
}

/// Firestore's document endpoints answer an error as `{"error": {...}}`, while
/// the streaming `:runQuery` endpoint wraps the same object in a one-element
/// array. Deserializing a struct envelope let serde's struct-from-sequence
/// fallback swallow the array shape into an empty envelope, which is how a
/// missing-index 400 logged every Google field as "not provided".
fn google_api_error(body: &str) -> Option<GoogleApiError> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = match &value {
        serde_json::Value::Array(rows) => rows.first()?.get("error"),
        _ => value.get("error"),
    }?;
    serde_json::from_value(error.clone()).ok()
}

#[derive(Default, Deserialize)]
struct GoogleApiError {
    code: Option<u16>,
    message: Option<String>,
    status: Option<String>,
}

#[derive(Default, Deserialize)]
struct GoogleOAuthError {
    error: Option<String>,
    error_description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_error_details_are_typed_and_log_fields_are_sanitized() {
        let details = google_api_error(
            r#"{"error":{"code":403,"message":"Permission\tdenied\nfor database","status":"PERMISSION_DENIED"}}"#,
        )
        .unwrap();

        assert_eq!(details.code, Some(403));
        assert_eq!(details.status.as_deref(), Some("PERMISSION_DENIED"));
        assert_eq!(
            sanitize_text(details.message.as_deref().unwrap()),
            "Permission denied for database"
        );
        assert_eq!(
            sanitize_text(&"x".repeat(MAX_LOG_FIELD_CHARS + 20)).len(),
            MAX_LOG_FIELD_CHARS
        );
    }

    /// The exact shape staging returned for the missing dailyCoachingRuns
    /// index on 2026-08-15, which the struct envelope logged as three
    /// "not provided" fields.
    #[test]
    fn run_query_array_envelope_yields_the_google_error() {
        let details = google_api_error(
            r#"[{"error":{"code":400,"message":"The query requires an index.","status":"FAILED_PRECONDITION"}}]"#,
        )
        .unwrap();

        assert_eq!(details.code, Some(400));
        assert_eq!(details.status.as_deref(), Some("FAILED_PRECONDITION"));
        assert_eq!(
            details.message.as_deref(),
            Some("The query requires an index.")
        );
    }

    #[test]
    fn bodies_without_an_error_object_are_unrecognized() {
        assert!(google_api_error("").is_none());
        assert!(google_api_error("{}").is_none());
        assert!(google_api_error("[]").is_none());
        assert!(google_api_error(r#"[{"readTime":"2026-08-15T00:00:00Z"}]"#).is_none());
    }
}
