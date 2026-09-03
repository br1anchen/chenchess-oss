//! Identity-free hosted Language Layer capture: fingerprint + call-shape.

use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{
    evaluation_fingerprint::{
        CaptureOutcome, CaptureTrigger, EvaluationFingerprint, EvaluationFingerprintObservations,
        EvaluationStepObservation, PinVerificationVerdict,
    },
    language_layer_ledger::cost_micros_from_dollars,
    language_layer_provider::{CompletionAttempt, CompletionOutcome},
    pin_verification::ServedRoute,
};

use super::{
    model::{
        FeedbackAnchor, HostedLanguageLayerTask, LanguageLayerCallShape, QualityCaptureContent,
        QualityCaptureDraft, RecordedProseRejection, StrippedExcerpt,
    },
    RetentionPreference, ReviewFeedbackReason,
};

pub(crate) const FAILURE_EXCERPT_BOUND: usize = StrippedExcerpt::BOUND;

/// Facts taken from one settled hosted attempt. Raw provider payloads stay
/// out; an output-shaped failure leaves only a stripped excerpt.
pub(crate) struct HostedGenerationInput<'a> {
    pub fingerprint: EvaluationFingerprint,
    pub attempt: &'a CompletionAttempt,
    pub trigger: CaptureTrigger,
    pub outcome: CaptureOutcome,
    pub pin_verification: PinVerificationVerdict,
    pub served_endpoint: Option<String>,
    pub served_region: Option<String>,
    pub routed_service_tier: Option<String>,
    pub attempts: u8,
    pub task: HostedLanguageLayerTask,
    pub created_at: DateTime<Utc>,
    pub steps: Vec<EvaluationStepObservation>,
    /// Set only when the prose gate is what refused this generation.
    pub rejection: Option<RecordedProseRejection>,
}

impl HostedGenerationInput<'_> {
    pub(crate) fn with_served_route(mut self, route: Option<&ServedRoute>) -> Self {
        if let Some(route) = route {
            self.served_endpoint = route.endpoint.clone();
            self.served_region = route.region.clone();
            self.routed_service_tier = route.routed_service_tier.clone();
        }
        self
    }
}

pub(crate) fn hosted_language_layer_capture(
    input: HostedGenerationInput<'_>,
) -> QualityCaptureDraft {
    let deadline_hit = matches!(
        input.attempt.outcome,
        CompletionOutcome::TimedOut | CompletionOutcome::DeadlineExhausted
    );
    let failure_excerpt = output_shaped_excerpt(input.attempt);
    QualityCaptureDraft::hosted_language_layer(
        input.fingerprint,
        EvaluationFingerprintObservations {
            served_provider: input.attempt.served_provider.clone(),
            pin_verification: input.pin_verification,
            capture_trigger: input.trigger,
            capture_outcome: input.outcome,
            served_endpoint: input.served_endpoint,
            served_region: input.served_region,
            routed_service_tier: input.routed_service_tier,
            steps: input.steps,
        },
        LanguageLayerCallShape {
            prompt_tokens: input.attempt.prompt_tokens,
            completion_tokens: input.attempt.completion_tokens,
            reasoning_tokens: input.attempt.reasoning_tokens,
            cost_micros: cost_micros_from_dollars(input.attempt.cost),
            finish_reason: input.attempt.finish_reason.clone(),
            attempts: input.attempts,
            deadline_hit,
            created_on: input.created_at.date_naive(),
        },
        input.task,
        failure_excerpt,
        input.rejection,
        input.created_at,
    )
}

fn output_shaped_excerpt(attempt: &CompletionAttempt) -> Option<StrippedExcerpt> {
    match attempt.outcome {
        CompletionOutcome::SchemaRejected
        | CompletionOutcome::EmptyCompletion
        | CompletionOutcome::InvalidRequest => attempt
            .raw_content
            .as_deref()
            .map(strip_output_shaped_failure),
        CompletionOutcome::Completed
        | CompletionOutcome::HttpError
        | CompletionOutcome::TimedOut
        | CompletionOutcome::DeadlineExhausted
        | CompletionOutcome::TransportError
        | CompletionOutcome::RateLimited { .. } => None,
    }
}

pub(crate) fn strip_output_shaped_failure(raw: &str) -> StrippedExcerpt {
    let stripped = match serde_json::from_str::<Value>(raw) {
        Ok(value) => strip_json_strings(&value),
        Err(_) => raw.chars().filter(|ch| !ch.is_alphabetic()).collect(),
    };
    let excerpt = StrippedExcerpt::new(stripped);
    debug_assert!(excerpt.as_str().len() <= FAILURE_EXCERPT_BOUND);
    excerpt
}

fn strip_json_strings(value: &Value) -> String {
    match value {
        Value::String(_) => String::new(),
        Value::Array(values) => {
            let inner = values
                .iter()
                .map(strip_json_strings)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{inner}]")
        }
        Value::Object(fields) => {
            let inner = fields
                .iter()
                .map(|(key, value)| format!("{key}:{}", strip_json_strings(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
        other => other.to_string(),
    }
}

pub(crate) fn preference_allows_outbox(preference: &RetentionPreference) -> bool {
    preference.available && preference.enabled && !preference.disclosure_required
}

pub(crate) fn writes_without_preference(capture: &QualityCaptureDraft) -> bool {
    match &capture.content {
        QualityCaptureContent::FeedbackAnnotation { .. } => true,
        QualityCaptureContent::LanguageLayerGeneration { observations, .. } => {
            observations.capture_trigger == CaptureTrigger::FeedbackInduced
        }
        QualityCaptureContent::GameAnalysis { .. }
        | QualityCaptureContent::CoachingResponse { .. } => false,
    }
}

pub(crate) fn holds_when_preference_off(capture: &QualityCaptureDraft) -> bool {
    matches!(
        &capture.content,
        QualityCaptureContent::LanguageLayerGeneration { observations, .. }
            if observations.capture_trigger == CaptureTrigger::Preference
    )
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FeedbackInduction {
    AnnotationOnly {
        annotation: Box<QualityCaptureDraft>,
    },
    Induced {
        capture: Box<QualityCaptureDraft>,
        annotation: Box<QualityCaptureDraft>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum FeedbackInductionError {
    #[error("feedback has no hosted generation to annotate")]
    MissingGeneration,
    #[error("held generation could not be tagged feedback-induced")]
    InvalidHeldGeneration,
}

/// Preference-off submit still induces a capture. Preference-on annotates the
/// already-exported one. One payload and one withdrawal path serve both.
pub(crate) fn induce_feedback(
    exported: Option<FeedbackAnchor>,
    held: Option<QualityCaptureDraft>,
    preference: &RetentionPreference,
    reason_codes: Vec<ReviewFeedbackReason>,
    created_at: DateTime<Utc>,
) -> Result<FeedbackInduction, FeedbackInductionError> {
    if preference_allows_outbox(preference) {
        let Some(exported) = exported else {
            return Err(FeedbackInductionError::MissingGeneration);
        };
        return Ok(FeedbackInduction::AnnotationOnly {
            annotation: Box::new(QualityCaptureDraft::feedback_annotation(
                exported.capture_id,
                exported.fingerprint_digest,
                reason_codes,
                created_at,
            )),
        });
    }
    let Some(held) = held else {
        return Err(FeedbackInductionError::MissingGeneration);
    };
    let capture = held
        .with_feedback_induced_trigger()
        .map_err(|_| FeedbackInductionError::InvalidHeldGeneration)?;
    let annotation = QualityCaptureDraft::feedback_annotation(
        capture.capture_id.clone(),
        fingerprint_digest(&capture)?,
        reason_codes,
        created_at,
    );
    Ok(FeedbackInduction::Induced {
        capture: Box::new(capture),
        annotation: Box::new(annotation),
    })
}

fn fingerprint_digest(
    draft: &QualityCaptureDraft,
) -> Result<crate::review_session_contract::ArtifactDigest, FeedbackInductionError> {
    match &draft.content {
        super::model::QualityCaptureContent::LanguageLayerGeneration { fingerprint, .. } => {
            Ok(fingerprint.digest.clone())
        }
        _ => Err(FeedbackInductionError::InvalidHeldGeneration),
    }
}

/// Process-local hosted capture drafts for one Open.
///
/// Owned the same way spend is: created per Open, shared with that Open's
/// authors through `Arc`, never stored on the process-wide runtime. A
/// runtime-wide `take()` would let one Player's persist write another
/// Player's outbox. The durable capture itself still holds no Player
/// association.
pub struct HostedCaptureBuffer {
    drafts: Mutex<Vec<QualityCaptureDraft>>,
}

impl HostedCaptureBuffer {
    pub fn new() -> Self {
        Self {
            drafts: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn push(&self, draft: QualityCaptureDraft) {
        self.drafts
            .lock()
            .expect("hosted capture buffer is not poisoned")
            .push(draft);
    }

    pub(crate) fn take(&self) -> Vec<QualityCaptureDraft> {
        std::mem::take(
            &mut *self
                .drafts
                .lock()
                .expect("hosted capture buffer is not poisoned"),
        )
    }
}

impl Default for HostedCaptureBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        evaluation_fingerprint::{
            evaluation_fingerprint, CaptureOrigin, EvaluationEnvironment,
            EvaluationFingerprintAxes, EvaluationGenerationSettings, LanguageLayerAttestation,
            StructuredOutputMode,
        },
        language_layer_provider::CompletionAttempt,
        pin_record::compiled_pin_record,
        pin_verification::ServedRoute,
        quality_capture::{
            InMemoryQualityCaptureStore, QualityCaptureAppender, QualityCapturePreferenceStore,
            QualityCaptureRuntime,
        },
        review_session_contract::{
            ArtifactDigest, DeliverySurface, PlayerId, ReviewSessionEventEnvelope,
        },
        review_session_processor::{ProcessorCommandAdmission, ProcessorPrincipal},
        review_session_transport::{ReviewSessionCommandExecutor, ReviewSessionWebBinding},
    };
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;

    fn digest_fixture(byte: char) -> ArtifactDigest {
        ArtifactDigest::try_from(format!("sha256:{}", byte.to_string().repeat(64))).unwrap()
    }

    fn fingerprint() -> EvaluationFingerprint {
        evaluation_fingerprint(EvaluationFingerprintAxes {
            evaluation_contract_version: crate::evaluation_fingerprint::EVALUATION_CONTRACT_VERSION
                .to_string(),
            environment: EvaluationEnvironment::Staging,
            capture_origin: CaptureOrigin::QualityCapture,
            delivery_surface: DeliverySurface::Web,
            code_revision: "git:test".to_string(),
            pipeline_revision: "pipeline:test".to_string(),
            language_layer_attestation: LanguageLayerAttestation::Attested {
                pin: compiled_pin_record().model,
                provider_allowlist: vec!["google-vertex/global".to_string()],
                generation_settings: EvaluationGenerationSettings {
                    max_output_tokens: 512,
                    temperature: false,
                    seed: true,
                },
                structured_output_mode: StructuredOutputMode::NativeSchema,
                prompt_digest: digest_fixture('a'),
                response_schema_digest: digest_fixture('b'),
                evidence_schema_digest: digest_fixture('c'),
                coaching_profile_projection_schema_digest: digest_fixture('d'),
            },
        })
    }

    fn attempt(outcome: CompletionOutcome, raw: Option<&str>) -> CompletionAttempt {
        CompletionAttempt {
            latency: Duration::from_millis(87),
            http_status: Some(200),
            generation_id: Some("gen-secret".to_string()),
            served_model: Some("google/gemini-test".to_string()),
            served_provider: Some("Google Vertex".to_string()),
            prompt_tokens: Some(120),
            completion_tokens: Some(40),
            reasoning_tokens: None,
            cost: Some(0.002),
            finish_reason: Some("stop".to_string()),
            raw_content: raw.map(str::to_string),
            outcome,
        }
    }

    fn capture(outcome: CaptureOutcome, trigger: CaptureTrigger) -> QualityCaptureDraft {
        hosted_language_layer_capture(HostedGenerationInput {
            fingerprint: fingerprint(),
            attempt: &attempt(CompletionOutcome::Completed, Some("Keep the rook.")),
            trigger,
            outcome,
            pin_verification: PinVerificationVerdict::Passed,
            served_endpoint: Some("ep-1".to_string()),
            served_region: Some("global".to_string()),
            routed_service_tier: None,
            attempts: 1,
            task: HostedLanguageLayerTask::Comment,
            created_at: "2026-08-19T15:04:05Z".parse().unwrap(),
            steps: Vec::new(),
            rejection: None,
        })
    }

    fn player() -> PlayerId {
        PlayerId::try_from("firebase-player".to_string()).unwrap()
    }

    fn assert_exclusions(draft: &QualityCaptureDraft) {
        let serialized = serde_json::to_string(draft).unwrap();
        for forbidden in [
            "firebase-player",
            "gen-secret",
            "ll-1",
            "requestId",
            "request:",
            "15:04:05",
            "latency",
            "Keep the rook.",
            "review-session:",
            "game-import:",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "hosted capture leaked {forbidden}: {serialized}"
            );
        }
        let value = serde_json::to_value(draft).unwrap();
        assert_forbidden_keys_absent(
            &value,
            &[
                "playerId",
                "requestId",
                "generationId",
                "latency",
                "rawContent",
                "rawPayload",
                "sessionId",
                "gameImportId",
            ],
        );
        assert_eq!(value["content"]["callShape"]["createdOn"], "2026-08-19");
        assert_eq!(draft.created_at.to_rfc3339(), "2026-08-19T00:00:00+00:00");
    }

    fn assert_forbidden_keys_absent(value: &Value, forbidden: &[&str]) {
        match value {
            Value::Array(values) => {
                for value in values {
                    assert_forbidden_keys_absent(value, forbidden);
                }
            }
            Value::Object(fields) => {
                for (key, value) in fields {
                    assert!(
                        !forbidden.contains(&key.as_str()),
                        "hosted capture contains forbidden field {key}"
                    );
                    assert_forbidden_keys_absent(value, forbidden);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn preference_gate_requires_acknowledgement_and_opt_in() {
        assert!(!preference_allows_outbox(&RetentionPreference {
            available: true,
            enabled: true,
            disclosure_required: true,
            deleted_review_snapshots: 0,
        }));
        assert!(!preference_allows_outbox(&RetentionPreference {
            available: true,
            enabled: false,
            disclosure_required: false,
            deleted_review_snapshots: 0,
        }));
        assert!(preference_allows_outbox(&RetentionPreference {
            available: true,
            enabled: true,
            disclosure_required: false,
            deleted_review_snapshots: 0,
        }));
    }

    #[tokio::test]
    async fn in_memory_preference_store_still_gates_disclosure() {
        let store = InMemoryQualityCaptureStore::default();
        let unread = store.preference(&player()).await.unwrap();
        assert!(unread.disclosure_required);
        assert!(!preference_allows_outbox(&unread));
        let enabled = store.set_preference(&player(), true).await.unwrap();
        assert!(preference_allows_outbox(&enabled));
        let disabled = store.set_preference(&player(), false).await.unwrap();
        assert!(!preference_allows_outbox(&disabled));
    }

    #[test]
    fn preference_off_feedback_induces_a_capture() {
        let held = capture(CaptureOutcome::Published, CaptureTrigger::Preference);
        let capture_id = held.capture_id.clone();
        let induced = induce_feedback(
            None,
            Some(held),
            &RetentionPreference {
                available: true,
                enabled: false,
                disclosure_required: false,
                deleted_review_snapshots: 0,
            },
            vec![ReviewFeedbackReason::ExplanationNotHelpful],
            "2026-08-19T16:00:00Z".parse().unwrap(),
        )
        .unwrap();
        let FeedbackInduction::Induced {
            capture,
            annotation,
        } = induced
        else {
            panic!("preference-off feedback must induce");
        };
        assert_eq!(capture.capture_id, capture_id);
        match &capture.content {
            super::super::model::QualityCaptureContent::LanguageLayerGeneration {
                observations,
                ..
            } => assert_eq!(
                observations.capture_trigger,
                CaptureTrigger::FeedbackInduced
            ),
            _ => panic!("induced payload must stay a language-layer generation"),
        }
        match &annotation.content {
            super::super::model::QualityCaptureContent::FeedbackAnnotation {
                capture_id: annotated,
                reason_codes,
                ..
            } => {
                assert_eq!(annotated, &capture_id);
                assert_eq!(
                    reason_codes,
                    &vec![ReviewFeedbackReason::ExplanationNotHelpful]
                );
            }
            _ => panic!("annotation must stay thin"),
        }
    }

    #[test]
    fn preference_on_feedback_is_a_thin_annotation() {
        let exported = capture(CaptureOutcome::Published, CaptureTrigger::Preference);
        let capture_id = exported.capture_id.clone();
        let induced = induce_feedback(
            exported.feedback_anchor(),
            None,
            &RetentionPreference {
                available: true,
                enabled: true,
                disclosure_required: false,
                deleted_review_snapshots: 0,
            },
            vec![ReviewFeedbackReason::ExplanationUnclear],
            "2026-08-19T16:00:00Z".parse().unwrap(),
        )
        .unwrap();
        let FeedbackInduction::AnnotationOnly { annotation } = induced else {
            panic!("preference-on feedback must annotate");
        };
        match &annotation.content {
            super::super::model::QualityCaptureContent::FeedbackAnnotation {
                capture_id: annotated,
                reason_codes,
                ..
            } => {
                assert_eq!(annotated, &capture_id);
                assert_eq!(
                    reason_codes,
                    &vec![ReviewFeedbackReason::ExplanationUnclear]
                );
            }
            _ => panic!("annotation must stay thin"),
        }
    }

    #[test]
    fn failed_and_rejected_generations_use_the_same_consent() {
        for outcome in [
            CaptureOutcome::Published,
            CaptureOutcome::Rejected,
            CaptureOutcome::Failed,
            CaptureOutcome::BudgetRefused,
            CaptureOutcome::ProviderCooldown,
        ] {
            let draft = capture(outcome, CaptureTrigger::Preference);
            match &draft.content {
                super::super::model::QualityCaptureContent::LanguageLayerGeneration {
                    observations,
                    ..
                } => assert_eq!(observations.capture_outcome, outcome),
                _ => panic!("expected language-layer generation"),
            }
            assert!(preference_allows_outbox(&RetentionPreference {
                available: true,
                enabled: true,
                disclosure_required: false,
                deleted_review_snapshots: 0,
            }));
        }
    }

    #[test]
    fn hosted_capture_excludes_identity_and_raw_payloads() {
        let published = capture(CaptureOutcome::Published, CaptureTrigger::Preference);
        assert_exclusions(&published);
        let rejected = hosted_language_layer_capture(HostedGenerationInput {
            fingerprint: fingerprint(),
            attempt: &attempt(
                CompletionOutcome::SchemaRejected,
                Some(r#"{"comment":"Never store this Player sentence."}"#),
            ),
            trigger: CaptureTrigger::Preference,
            outcome: CaptureOutcome::Rejected,
            pin_verification: PinVerificationVerdict::Failed,
            served_endpoint: None,
            served_region: None,
            routed_service_tier: None,
            attempts: 2,
            task: HostedLanguageLayerTask::CoachTurn,
            created_at: "2026-08-19T15:04:05Z".parse().unwrap(),
            steps: Vec::new(),
            rejection: None,
        });
        assert_exclusions(&rejected);
        match &rejected.content {
            super::super::model::QualityCaptureContent::LanguageLayerGeneration {
                failure_excerpt,
                call_shape,
                ..
            } => {
                let excerpt = failure_excerpt.as_ref().expect("output-shaped excerpt");
                assert!(!excerpt.as_str().contains("Never store"));
                assert!(!excerpt.as_str().contains("Player"));
                assert!(excerpt.as_str().len() <= FAILURE_EXCERPT_BOUND);
                assert!(!call_shape.deadline_hit);
                assert_eq!(call_shape.attempts, 2);
                assert_eq!(call_shape.cost_micros, 2_000);
            }
            _ => panic!("expected language-layer generation"),
        }
    }

    /// The field a diagnosis reads with jq, and the only place the discipline
    /// survives narrowing. Nothing deserializes it in the service, so a shape
    /// change fails as zero matches rather than an error — hence the literal
    /// names here.
    #[test]
    fn a_prose_rejection_is_stored_with_its_marker_and_reads_back() {
        let refused = hosted_language_layer_capture(HostedGenerationInput {
            fingerprint: fingerprint(),
            attempt: &attempt(CompletionOutcome::Completed, None),
            trigger: CaptureTrigger::Preference,
            outcome: CaptureOutcome::Rejected,
            pin_verification: PinVerificationVerdict::Passed,
            served_endpoint: None,
            served_region: None,
            routed_service_tier: None,
            attempts: 2,
            task: HostedLanguageLayerTask::Comment,
            created_at: "2026-08-19T15:04:05Z".parse().unwrap(),
            steps: Vec::new(),
            rejection: Some(RecordedProseRejection::from(
                crate::critical_moment_comment::CommentProseRejection::MisplacedMarker(
                    "achievement",
                ),
            )),
        });
        let json = serde_json::to_value(&refused.content).expect("capture serializes");
        assert_eq!(json["rejection"]["discipline"], "misplacedMarker");
        assert_eq!(json["rejection"]["marker"], "achievement");
        let round_tripped: super::super::model::QualityCaptureContent =
            serde_json::from_value(json).expect("capture reads back");
        assert_eq!(round_tripped, refused.content);
    }

    /// A discipline that names no marker omits the field rather than writing
    /// null, so a jq count of one marker never picks up the disciplines that
    /// have none.
    #[test]
    fn a_markerless_discipline_omits_the_marker_field() {
        let rejection = RecordedProseRejection::from(
            crate::critical_moment_comment::CommentProseRejection::BareFigure,
        );
        let json = serde_json::to_value(&rejection).expect("rejection serializes");
        assert_eq!(json["discipline"], "bareFigure");
        assert!(json.get("marker").is_none());
    }

    /// Captures written before the field exists still read, which is the
    /// migration: every stored generation reads as no recorded discipline.
    #[test]
    fn a_capture_without_the_field_still_reads() {
        let published = capture(CaptureOutcome::Published, CaptureTrigger::Preference);
        let mut json = serde_json::to_value(&published.content).expect("capture serializes");
        json.as_object_mut()
            .expect("capture object")
            .remove("rejection");
        let read: super::super::model::QualityCaptureContent =
            serde_json::from_value(json).expect("older capture reads back");
        match read {
            super::super::model::QualityCaptureContent::LanguageLayerGeneration {
                rejection,
                ..
            } => assert!(rejection.is_none()),
            _ => panic!("expected language-layer generation"),
        }
    }

    #[test]
    fn durable_store_never_keeps_a_raw_provider_payload() {
        let transport = hosted_language_layer_capture(HostedGenerationInput {
            fingerprint: fingerprint(),
            attempt: &attempt(
                CompletionOutcome::TransportError,
                Some("upstream dump with secrets"),
            ),
            trigger: CaptureTrigger::Preference,
            outcome: CaptureOutcome::Failed,
            pin_verification: PinVerificationVerdict::Unverified,
            served_endpoint: None,
            served_region: None,
            routed_service_tier: None,
            attempts: 1,
            task: HostedLanguageLayerTask::Comment,
            created_at: "2026-08-19T15:04:05Z".parse().unwrap(),
            steps: Vec::new(),
            rejection: None,
        });
        match &transport.content {
            super::super::model::QualityCaptureContent::LanguageLayerGeneration {
                failure_excerpt,
                ..
            } => assert!(failure_excerpt.is_none()),
            _ => panic!("expected language-layer generation"),
        }
        let serialized = serde_json::to_string(&transport).unwrap();
        assert!(!serialized.contains("upstream dump"));
        assert!(!serialized.contains("gen-secret"));
    }

    #[test]
    fn served_route_sits_beside_the_fingerprint_digest() {
        let route = ServedRoute {
            endpoint: Some("ep-1".into()),
            region: Some("global".into()),
            routed_service_tier: None,
            verified_permaslug: "google/gemini-test".into(),
            verified_provider: "Google Vertex".into(),
        };
        let draft = hosted_language_layer_capture(
            HostedGenerationInput {
                fingerprint: fingerprint(),
                attempt: &attempt(CompletionOutcome::Completed, Some("Keep the rook.")),
                trigger: CaptureTrigger::Preference,
                outcome: CaptureOutcome::Published,
                pin_verification: PinVerificationVerdict::Passed,
                served_endpoint: None,
                served_region: None,
                routed_service_tier: None,
                attempts: 2,
                task: HostedLanguageLayerTask::CoachTurn,
                created_at: "2026-08-19T15:04:05Z".parse().unwrap(),
                steps: Vec::new(),
                rejection: None,
            }
            .with_served_route(Some(&route)),
        );
        match &draft.content {
            super::super::model::QualityCaptureContent::LanguageLayerGeneration {
                observations,
                call_shape,
                ..
            } => {
                assert_eq!(observations.served_endpoint.as_deref(), Some("ep-1"));
                assert_eq!(observations.served_region.as_deref(), Some("global"));
                assert_eq!(observations.routed_service_tier, None);
                assert_eq!(call_shape.attempts, 2);
            }
            _ => panic!("expected language-layer generation"),
        }
    }

    #[tokio::test]
    async fn record_feedback_induces_when_preference_is_off() {
        let store = Arc::new(InMemoryQualityCaptureStore::default());
        store.set_preference(&player(), false).await.unwrap();
        let held = capture(CaptureOutcome::Published, CaptureTrigger::Preference);
        let capture_id = held.capture_id.clone();
        QualityCaptureAppender::memory(store.clone())
            .commit_best_effort(
                &ProcessorPrincipal::Player(player()),
                std::slice::from_ref(&held),
            )
            .await;
        let runtime = QualityCaptureRuntime::in_memory(store.clone());
        runtime
            .record_feedback(&player(), vec![ReviewFeedbackReason::ExplanationNotHelpful])
            .await
            .unwrap();
        let outbox = store.recorded_outbox(&player());
        assert_eq!(outbox.len(), 2, "induced capture and thin annotation");
        match &outbox[0].content {
            super::super::model::QualityCaptureContent::LanguageLayerGeneration {
                observations,
                ..
            } => {
                assert_eq!(outbox[0].capture_id, capture_id);
                assert_eq!(
                    observations.capture_trigger,
                    CaptureTrigger::FeedbackInduced
                );
            }
            _ => panic!("preference-off submit must persist a feedback-induced capture"),
        }
        match &outbox[1].content {
            super::super::model::QualityCaptureContent::FeedbackAnnotation {
                capture_id: annotated,
                reason_codes,
                ..
            } => {
                assert_eq!(annotated, &capture_id);
                assert_eq!(
                    reason_codes,
                    &vec![ReviewFeedbackReason::ExplanationNotHelpful]
                );
            }
            _ => panic!("annotation must persist beside the induced capture"),
        }
    }

    struct UnusedExecutor;

    impl ReviewSessionCommandExecutor for UnusedExecutor {
        fn submit(
            self: Arc<Self>,
            _principal: ProcessorPrincipal,
            _admission: ProcessorCommandAdmission,
        ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
            let (_sender, receiver) = mpsc::unbounded_channel();
            receiver
        }
    }

    #[tokio::test]
    async fn preference_off_submit_loads_held_and_records_feedback_induced() {
        let store = Arc::new(InMemoryQualityCaptureStore::default());
        store.set_preference(&player(), false).await.unwrap();
        let persist = QualityCaptureAppender::memory(store.clone());
        let held = capture(CaptureOutcome::Published, CaptureTrigger::Preference);
        let capture_id = held.capture_id.clone();
        persist
            .commit_best_effort(
                &ProcessorPrincipal::Player(player()),
                std::slice::from_ref(&held),
            )
            .await;
        assert!(
            store.recorded_outbox(&player()).is_empty(),
            "preference-off generation is held, not exported"
        );
        let runtime = QualityCaptureRuntime::in_memory(store.clone());
        let binding = ReviewSessionWebBinding::new(Arc::new(UnusedExecutor))
            .with_quality_capture_runtime(Arc::new(runtime));
        binding
            .record_feedback(
                player().as_str(),
                vec![ReviewFeedbackReason::ExplanationNotHelpful],
            )
            .await
            .unwrap();
        let outbox = store.recorded_outbox(&player());
        assert_eq!(outbox.len(), 2);
        match &outbox[0].content {
            super::super::model::QualityCaptureContent::LanguageLayerGeneration {
                observations,
                ..
            } => {
                assert_eq!(outbox[0].capture_id, capture_id);
                assert_eq!(
                    observations.capture_trigger,
                    CaptureTrigger::FeedbackInduced
                );
            }
            _ => panic!("product submit must induce the held generation"),
        }
    }

    /// The consented Player is the one whose feedback is usable, and their
    /// generation is gone from the product database by the time they vote.
    #[tokio::test]
    async fn consented_submit_annotates_the_exported_generation() {
        let store = Arc::new(InMemoryQualityCaptureStore::default());
        store.set_preference(&player(), true).await.unwrap();
        let persist = QualityCaptureAppender::memory(store.clone());
        let exported = capture(CaptureOutcome::Published, CaptureTrigger::Preference);
        let capture_id = exported.capture_id.clone();
        let fingerprint_digest = exported
            .feedback_anchor()
            .expect("a language-layer generation anchors feedback")
            .fingerprint_digest;
        persist
            .commit_best_effort(
                &ProcessorPrincipal::Player(player()),
                std::slice::from_ref(&exported),
            )
            .await;
        let runtime = QualityCaptureRuntime::in_memory(store.clone());
        let binding = ReviewSessionWebBinding::new(Arc::new(UnusedExecutor))
            .with_quality_capture_runtime(Arc::new(runtime));

        binding
            .record_feedback(
                player().as_str(),
                vec![ReviewFeedbackReason::ExplanationHelpful],
            )
            .await
            .unwrap();

        let outbox = store.recorded_outbox(&player());
        assert_eq!(
            outbox.len(),
            2,
            "the exported generation plus one annotation"
        );
        match &outbox[1].content {
            super::super::model::QualityCaptureContent::FeedbackAnnotation {
                capture_id: annotated,
                fingerprint_digest: annotated_digest,
                reason_codes,
            } => {
                assert_eq!(annotated, &capture_id);
                assert_eq!(annotated_digest, &fingerprint_digest);
                assert_eq!(
                    reason_codes,
                    &vec![ReviewFeedbackReason::ExplanationHelpful]
                );
            }
            _ => panic!("consented submit must annotate the exported generation"),
        }
        assert_eq!(
            store.feedback_induced_generation_count(&player()),
            0,
            "a consented generation is already captured; feedback must not induce a second"
        );
    }

    #[tokio::test]
    async fn concurrent_players_cannot_write_each_others_outbox() {
        let store = Arc::new(InMemoryQualityCaptureStore::default());
        let player_a = PlayerId::try_from("player-a".to_string()).unwrap();
        let player_b = PlayerId::try_from("player-b".to_string()).unwrap();
        store.set_preference(&player_a, true).await.unwrap();
        store.set_preference(&player_b, true).await.unwrap();
        let persist = QualityCaptureAppender::memory(store.clone());
        let captures_a = Arc::new(HostedCaptureBuffer::new());
        let captures_b = Arc::new(HostedCaptureBuffer::new());
        let draft_a = capture(CaptureOutcome::Published, CaptureTrigger::Preference);
        let draft_b = capture(CaptureOutcome::Failed, CaptureTrigger::Preference);
        let id_a = draft_a.capture_id.clone();
        let id_b = draft_b.capture_id.clone();
        let author_a = Arc::clone(&captures_a);
        let author_b = Arc::clone(&captures_b);
        author_a.push(draft_a);
        author_b.push(draft_b);
        let persist_a = persist.clone();
        let persist_b = persist.clone();
        let owner_a = ProcessorPrincipal::Player(player_a.clone());
        let owner_b = ProcessorPrincipal::Player(player_b.clone());
        let taken_a = captures_a.take();
        let taken_b = captures_b.take();
        let ((), ()) = tokio::join!(
            persist_a.commit_best_effort(&owner_a, &taken_a),
            persist_b.commit_best_effort(&owner_b, &taken_b),
        );
        let outbox_a = store.recorded_outbox(&player_a);
        let outbox_b = store.recorded_outbox(&player_b);
        assert_eq!(outbox_a.len(), 1);
        assert_eq!(outbox_b.len(), 1);
        assert_eq!(outbox_a[0].capture_id, id_a);
        assert_eq!(outbox_b[0].capture_id, id_b);
        assert_ne!(outbox_a[0].capture_id, outbox_b[0].capture_id);
        assert!(captures_a.take().is_empty());
        assert!(captures_b.take().is_empty());
    }

    #[test]
    fn stripped_excerpt_is_bounded_and_drops_free_text() {
        let long = format!("{{\"comment\":\"{}\"}}", "word ".repeat(80));
        let excerpt = strip_output_shaped_failure(&long);
        assert!(excerpt.as_str().len() <= FAILURE_EXCERPT_BOUND);
        assert!(!excerpt.as_str().contains("word"));
        assert!(excerpt.as_str().contains("comment"));
    }
}
