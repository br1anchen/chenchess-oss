//! Pin Verification match and deadline policy.
//!
//! #373: the bake-off and
//! the hosted comment runtime share one match over
//! [`LanguageLayerProvider::verify_generation`]. The match is telemetry: a
//! mismatch alerts and is recorded on the capture and ledger. It does not
//! discard paid output. The harness may keep `unverified` for bookkeeping
//! outages.

use std::time::Duration;

use serde::Serialize;

use crate::evaluation_fingerprint::PinVerificationVerdict;
use crate::language_layer_provider::{LanguageLayerProvider, PinVerification};

/// OpenRouter reports the provider as a display name (`Amazon Bedrock`,
/// `Google`) while the pin declares a family slug (`amazon-bedrock`,
/// `google-vertex`). The mapping is not derivable — `Google` serves Vertex —
/// so it is written down, and an unrecognised name is returned unchanged so it
/// fails Pin Verification rather than passing by accident.
pub fn provider_family(display_name: &str) -> &str {
    match display_name {
        "Amazon Bedrock" => "amazon-bedrock",
        "Google" | "Google Vertex" => "google-vertex",
        "Google AI Studio" => "google-ai-studio",
        "Azure" => "azure",
        "Anthropic" => "anthropic",
        "OpenAI" => "openai",
        other => other,
    }
}

pub fn pinned_provider_family(provider_only: &str) -> &str {
    provider_only.split('/').next().unwrap_or_default()
}

/// A null `routed_service_tier` means the declared default was served. The
/// field is recorded; a name is not invented.
pub fn recorded_service_tier(routed: Option<String>) -> Option<String> {
    routed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinVerificationStrictness {
    Runtime,
    Harness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinVerificationFailure {
    DeadlineMissed,
    VerifyError,
    MissingIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedRoute {
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub routed_service_tier: Option<String>,
    pub verified_permaslug: String,
    pub verified_provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinMismatchReport {
    pub pinned_model: String,
    pub pinned_provider_family: String,
    pub observed_permaslug: Option<String>,
    pub observed_provider: Option<String>,
    pub observed_provider_family: Option<String>,
    pub served_endpoint: Option<String>,
    pub served_region: Option<String>,
    pub routed_service_tier: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinVerificationCause {
    Mismatched,
    VerifyError,
    MissingIdentity,
    DeadlineMissed,
}

impl PinVerificationCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mismatched => "mismatched",
            Self::VerifyError => "verifyError",
            Self::MissingIdentity => "missingIdentity",
            Self::DeadlineMissed => "deadlineMissed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinVerificationJudgement {
    Passed(ServedRoute),
    Mismatched(PinMismatchReport),
    Failed(PinVerificationFailure),
    Unverified,
    NotApplicable,
}

impl PinVerificationJudgement {
    pub fn as_harness_label(&self) -> &'static str {
        match self {
            Self::Passed(_) => "passed",
            Self::Mismatched(_) => "failed",
            Self::Unverified | Self::Failed(_) => "unverified",
            Self::NotApplicable => "notApplicable",
        }
    }

    pub fn as_verdict(&self) -> PinVerificationVerdict {
        match self {
            Self::Passed(_) => PinVerificationVerdict::Passed,
            Self::Mismatched(_) => PinVerificationVerdict::Failed,
            Self::Unverified => PinVerificationVerdict::Unverified,
            Self::Failed(_) => PinVerificationVerdict::Failed,
            Self::NotApplicable => PinVerificationVerdict::NotApplicable,
        }
    }

    pub fn pin_mismatched(&self) -> bool {
        matches!(self, Self::Mismatched(_))
    }

    pub fn served_route(&self) -> Option<&ServedRoute> {
        match self {
            Self::Passed(route) => Some(route),
            _ => None,
        }
    }

    pub fn cause(&self) -> Option<PinVerificationCause> {
        match self {
            Self::Mismatched(_) => Some(PinVerificationCause::Mismatched),
            Self::Failed(PinVerificationFailure::VerifyError) => {
                Some(PinVerificationCause::VerifyError)
            }
            Self::Failed(PinVerificationFailure::MissingIdentity) => {
                Some(PinVerificationCause::MissingIdentity)
            }
            Self::Failed(PinVerificationFailure::DeadlineMissed) => {
                Some(PinVerificationCause::DeadlineMissed)
            }
            Self::Passed(_) | Self::Unverified | Self::NotApplicable => None,
        }
    }

    pub fn mismatch_report(&self) -> Option<&PinMismatchReport> {
        match self {
            Self::Mismatched(report) => Some(report),
            _ => None,
        }
    }
}

pub fn verify_deadline_exhausted(remaining: Duration) -> bool {
    remaining.is_zero()
}

/// OpenRouter writes a generation record shortly *after* the completion it
/// describes returns, and Pin Verification asks for it immediately — so the
/// first read reliably 404s on a record that exists a moment later. Staging
/// verified nothing at all for this reason: every capture ever recorded read
/// `failed`, and the logged detail was `404 Generation not found` each time.
///
/// Only that one shape is retried. An unreadable key or a provider outage is
/// answered once and taken at its word, because repeating it would spend the
/// Player's deadline to reach the same verdict.
const VERIFY_NOT_FOUND_BACKOFF: [Duration; 2] =
    [Duration::from_millis(150), Duration::from_millis(500)];

fn parse_is_generation_pending(error: &str) -> bool {
    error.contains("returned 404")
}

pub async fn verify_generation_within_deadline(
    provider: &LanguageLayerProvider,
    generation_id: &str,
    remaining: Duration,
) -> Result<PinVerification, PinVerificationFailure> {
    if verify_deadline_exhausted(remaining) {
        return Err(PinVerificationFailure::DeadlineMissed);
    }
    // The whole retry sequence shares the one deadline: a slow provider can
    // still only cost what a single call could.
    tokio::time::timeout(
        remaining,
        verify_generation_past_the_race(provider, generation_id),
    )
    .await
    .map_err(|_| PinVerificationFailure::DeadlineMissed)
}

async fn verify_generation_past_the_race(
    provider: &LanguageLayerProvider,
    generation_id: &str,
) -> PinVerification {
    let mut verification = provider.verify_generation(generation_id).await;
    for backoff in VERIFY_NOT_FOUND_BACKOFF {
        match verification.error.as_deref() {
            Some(error) if parse_is_generation_pending(error) => {}
            _ => return verification,
        }
        tokio::time::sleep(backoff).await;
        verification = provider.verify_generation(generation_id).await;
    }
    verification
}

pub fn judge_completed_verification(
    result: Result<PinVerification, PinVerificationFailure>,
    expected_model: &str,
    provider_only: &str,
    attempt_completed: bool,
    strictness: PinVerificationStrictness,
) -> PinVerificationJudgement {
    let pin = match result {
        Err(failure) => {
            // A mismatch alerts with its whole report; these three verdicts read
            // the same on a capture and said nothing about which one they were.
            tracing::warn!(
                event = "coach_pin_verification_failure",
                cause = ?failure,
                detail = tracing::field::Empty,
                "pin verification did not complete; the served route stays unrecorded"
            );
            return unverified_or_fail(strictness, failure);
        }
        Ok(pin) if pin.error.is_some() => {
            tracing::warn!(
                event = "coach_pin_verification_failure",
                cause = ?PinVerificationFailure::VerifyError,
                detail = pin.error.as_deref(),
                "pin verification call failed; the served route stays unrecorded"
            );
            return unverified_or_fail(strictness, PinVerificationFailure::VerifyError);
        }
        Ok(pin) => pin,
    };
    match (
        pin.verified_permaslug.as_deref(),
        pin.verified_provider.as_deref(),
    ) {
        (Some(permaslug), Some(provider)) => {
            let family = pinned_provider_family(provider_only);
            let observed_family = provider_family(provider);
            if permaslug == expected_model && observed_family == family {
                PinVerificationJudgement::Passed(ServedRoute {
                    endpoint: pin.served_endpoint_id,
                    region: pin.served_region,
                    routed_service_tier: recorded_service_tier(pin.routed_service_tier),
                    verified_permaslug: permaslug.to_string(),
                    verified_provider: provider.to_string(),
                })
            } else {
                PinVerificationJudgement::Mismatched(PinMismatchReport {
                    pinned_model: expected_model.to_string(),
                    pinned_provider_family: family.to_string(),
                    observed_permaslug: Some(permaslug.to_string()),
                    observed_provider: Some(provider.to_string()),
                    observed_provider_family: Some(observed_family.to_string()),
                    served_endpoint: pin.served_endpoint_id,
                    served_region: pin.served_region,
                    routed_service_tier: recorded_service_tier(pin.routed_service_tier),
                })
            }
        }
        _ if attempt_completed => {
            unverified_or_fail(strictness, PinVerificationFailure::MissingIdentity)
        }
        _ => PinVerificationJudgement::NotApplicable,
    }
}

fn unverified_or_fail(
    strictness: PinVerificationStrictness,
    failure: PinVerificationFailure,
) -> PinVerificationJudgement {
    match strictness {
        PinVerificationStrictness::Runtime => PinVerificationJudgement::Failed(failure),
        PinVerificationStrictness::Harness => PinVerificationJudgement::Unverified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "google/gemini-3.5-flash-lite-20260721";
    const TAG: &str = "google-vertex/global";

    fn pin(permaslug: Option<&str>, provider: Option<&str>, tier: Option<&str>) -> PinVerification {
        PinVerification {
            verified_permaslug: permaslug.map(str::to_string),
            verified_provider: provider.map(str::to_string),
            served_endpoint_id: Some("ep-1".into()),
            served_region: Some("global".into()),
            routed_service_tier: tier.map(str::to_string),
            prompt_tokens: None,
            completion_tokens: None,
            cost: None,
            error: None,
        }
    }

    #[test]
    fn only_a_pending_generation_record_is_worth_asking_again() {
        // The staging failure, verbatim from the logged detail.
        assert!(parse_is_generation_pending(
            "https://openrouter.ai/api/v1/generation?id=gen-1 returned 404 Not Found: {\"error\":{\"message\":\"Generation gen-1 not found\",\"code\":404}}"
        ));
        // An unreadable key or a provider outage reaches the same verdict
        // however many times it is asked, so the deadline is not spent on it.
        assert!(!parse_is_generation_pending(
            "https://openrouter.ai/api/v1/generation?id=gen-1 returned 401 Unauthorized: {}"
        ));
        assert!(!parse_is_generation_pending(
            "https://openrouter.ai/api/v1/generation?id=gen-1 returned 503 Service Unavailable: {}"
        ));
        // A generation id that merely contains the digits is not a status.
        assert!(!parse_is_generation_pending(
            "https://openrouter.ai/api/v1/generation?id=gen-404-abc returned invalid JSON: expected value"
        ));
    }

    fn judge(
        verification: PinVerification,
        completed: bool,
        strictness: PinVerificationStrictness,
    ) -> PinVerificationJudgement {
        judge_completed_verification(Ok(verification), MODEL, TAG, completed, strictness)
    }

    #[test]
    fn matching_permaslug_and_family_passes() {
        let judgement = judge(
            pin(Some(MODEL), Some("Google Vertex"), None),
            true,
            PinVerificationStrictness::Runtime,
        );
        match &judgement {
            PinVerificationJudgement::Passed(route) => {
                assert_eq!(route.verified_permaslug, MODEL);
                assert_eq!(route.verified_provider, "Google Vertex");
                assert_eq!(route.routed_service_tier, None);
                assert_eq!(route.endpoint.as_deref(), Some("ep-1"));
                assert_eq!(route.region.as_deref(), Some("global"));
            }
            other => panic!("expected pass, got {other:?}"),
        }
        assert!(!judgement.pin_mismatched());
        assert_eq!(judgement.as_harness_label(), "passed");
    }

    #[test]
    fn google_display_name_is_vertex() {
        let judgement = judge(
            pin(Some(MODEL), Some("Google"), None),
            true,
            PinVerificationStrictness::Runtime,
        );
        assert!(matches!(judgement, PinVerificationJudgement::Passed(_)));
    }

    #[test]
    fn model_mismatch_fails_and_keeps_both_identities() {
        let judgement = judge(
            pin(Some("other/model"), Some("Google Vertex"), Some("priority")),
            true,
            PinVerificationStrictness::Runtime,
        );
        match &judgement {
            PinVerificationJudgement::Mismatched(report) => {
                assert_eq!(report.pinned_model, MODEL);
                assert_eq!(report.pinned_provider_family, "google-vertex");
                assert_eq!(report.observed_permaslug.as_deref(), Some("other/model"));
                assert_eq!(report.observed_provider.as_deref(), Some("Google Vertex"));
                assert_eq!(
                    report.observed_provider_family.as_deref(),
                    Some("google-vertex")
                );
                assert_eq!(report.served_endpoint.as_deref(), Some("ep-1"));
                assert_eq!(report.served_region.as_deref(), Some("global"));
                assert_eq!(report.routed_service_tier.as_deref(), Some("priority"));
            }
            other => panic!("expected mismatch, got {other:?}"),
        }
        assert!(judgement.pin_mismatched());
        assert_eq!(judgement.cause(), Some(PinVerificationCause::Mismatched));
        assert_eq!(judgement.as_verdict(), PinVerificationVerdict::Failed);
    }

    #[test]
    fn provider_family_mismatch_fails() {
        let judgement = judge(
            pin(Some(MODEL), Some("Amazon Bedrock"), None),
            true,
            PinVerificationStrictness::Runtime,
        );
        match judgement {
            PinVerificationJudgement::Mismatched(report) => {
                assert_eq!(
                    report.observed_provider_family.as_deref(),
                    Some("amazon-bedrock")
                );
                assert_eq!(report.pinned_provider_family, "google-vertex");
            }
            other => panic!("expected family mismatch, got {other:?}"),
        }
    }

    #[test]
    fn unrecognised_provider_is_returned_unchanged_and_fails() {
        assert_eq!(provider_family("Mystery Cloud"), "Mystery Cloud");
        let judgement = judge(
            pin(Some(MODEL), Some("Mystery Cloud"), None),
            true,
            PinVerificationStrictness::Runtime,
        );
        assert!(matches!(judgement, PinVerificationJudgement::Mismatched(_)));
    }

    #[test]
    fn missing_identity_on_a_completed_attempt_is_unverified_in_the_harness_and_fails_at_runtime() {
        let missing = pin(None, None, None);
        assert_eq!(
            judge(missing.clone(), true, PinVerificationStrictness::Harness),
            PinVerificationJudgement::Unverified
        );
        assert_eq!(
            judge(missing, true, PinVerificationStrictness::Runtime),
            PinVerificationJudgement::Failed(PinVerificationFailure::MissingIdentity)
        );
        assert_eq!(
            judge_completed_verification(
                Ok(pin(None, None, None)),
                MODEL,
                TAG,
                true,
                PinVerificationStrictness::Runtime,
            )
            .cause(),
            Some(PinVerificationCause::MissingIdentity)
        );
    }

    #[test]
    fn missing_identity_on_an_incomplete_attempt_is_not_applicable() {
        assert_eq!(
            judge(
                pin(None, None, None),
                false,
                PinVerificationStrictness::Runtime
            ),
            PinVerificationJudgement::NotApplicable
        );
    }

    #[test]
    fn verify_error_is_unverified_in_the_harness_and_fails_at_runtime() {
        let mut errored = pin(Some(MODEL), Some("Google Vertex"), None);
        errored.error = Some("generation lookup 503".into());
        assert_eq!(
            judge(errored.clone(), true, PinVerificationStrictness::Harness),
            PinVerificationJudgement::Unverified
        );
        assert_eq!(
            judge(errored, true, PinVerificationStrictness::Runtime),
            PinVerificationJudgement::Failed(PinVerificationFailure::VerifyError)
        );
    }

    #[test]
    fn deadline_miss_and_timeout_are_verify_failures() {
        assert!(verify_deadline_exhausted(Duration::ZERO));
        assert!(!verify_deadline_exhausted(Duration::from_millis(1)));
        let judgement = judge_completed_verification(
            Err(PinVerificationFailure::DeadlineMissed),
            MODEL,
            TAG,
            true,
            PinVerificationStrictness::Runtime,
        );
        assert_eq!(
            judgement,
            PinVerificationJudgement::Failed(PinVerificationFailure::DeadlineMissed)
        );
        assert_eq!(
            judgement.cause(),
            Some(PinVerificationCause::DeadlineMissed)
        );
        assert_eq!(
            judge_completed_verification(
                Err(PinVerificationFailure::DeadlineMissed),
                MODEL,
                TAG,
                true,
                PinVerificationStrictness::Harness,
            ),
            PinVerificationJudgement::Unverified
        );
    }

    #[test]
    fn null_routed_service_tier_is_recorded_as_declared_default() {
        let judgement = judge(
            pin(Some(MODEL), Some("Google Vertex"), None),
            true,
            PinVerificationStrictness::Runtime,
        );
        let route = judgement.served_route().expect("passed");
        assert_eq!(recorded_service_tier(None), None);
        assert_eq!(route.routed_service_tier, None);
        let value = serde_json::json!({
            "servedEndpoint": route.endpoint,
            "servedRegion": route.region,
            "routedServiceTier": route.routed_service_tier,
        });
        assert_eq!(value["routedServiceTier"], serde_json::Value::Null);
        assert!(value
            .as_object()
            .expect("object")
            .contains_key("routedServiceTier"));
        assert_ne!(value["routedServiceTier"], serde_json::json!("default"));
    }

    #[test]
    fn a_zero_deadline_does_not_call_the_provider() {
        let provider = LanguageLayerProvider::from_client(reqwest::Client::new(), "test");
        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(verify_generation_within_deadline(
                &provider,
                "gen-never",
                Duration::ZERO,
            ));
        assert_eq!(outcome.err(), Some(PinVerificationFailure::DeadlineMissed));
    }

    #[test]
    fn a_404_lookup_is_a_verify_error_not_a_mismatch() {
        let missing = PinVerification {
            error: Some("generation lookup 404".into()),
            ..PinVerification::default()
        };
        let judgement = judge_completed_verification(
            Ok(missing),
            MODEL,
            TAG,
            true,
            PinVerificationStrictness::Runtime,
        );
        assert_eq!(
            judgement,
            PinVerificationJudgement::Failed(PinVerificationFailure::VerifyError)
        );
        assert!(!judgement.pin_mismatched());
        assert_eq!(judgement.cause(), Some(PinVerificationCause::VerifyError));
    }

    #[test]
    fn cause_names_mismatch_and_verify_failures() {
        assert_eq!(
            judge(
                pin(Some("other/model"), Some("Google Vertex"), None),
                true,
                PinVerificationStrictness::Runtime,
            )
            .cause(),
            Some(PinVerificationCause::Mismatched)
        );
        assert_eq!(
            PinVerificationJudgement::Failed(PinVerificationFailure::VerifyError).cause(),
            Some(PinVerificationCause::VerifyError)
        );
        assert_eq!(
            PinVerificationJudgement::Failed(PinVerificationFailure::MissingIdentity).cause(),
            Some(PinVerificationCause::MissingIdentity)
        );
        assert_eq!(
            PinVerificationJudgement::Failed(PinVerificationFailure::DeadlineMissed).cause(),
            Some(PinVerificationCause::DeadlineMissed)
        );
        assert_eq!(PinVerificationJudgement::NotApplicable.cause(), None);
    }
}
