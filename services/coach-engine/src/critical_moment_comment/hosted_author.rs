//! Hosted Review Moment Comment author.
//!
//! #372: `complete()` is
//! the named port. #373
//! records Pin Verification beside the Grounding Gate. Pin mismatch does
//! not discard hosted prose.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::critical_moment_comment::{
    compiled_comment_prompt_digest, compiled_comment_schema_digest, grounding_ledger_for,
    CommentProseRejection, CriticalMomentCommentAuthor, CriticalMomentCommentAuthorInput,
};
use crate::evaluation_fingerprint::{
    CaptureOutcome, CaptureTrigger, EvaluationFingerprint, PinVerificationVerdict,
};
use crate::language_layer_ledger::{
    begin_hosted_attempt, finish_hosted_attempt, AttemptContext, CeilingAlert, DenialReason,
    HostedAttemptStart, HostedTask, LanguageLayerAdmissionConfig, LanguageLayerLedger,
    OpenHostedAttempt, PinMismatchAlert, ProviderConcurrency, ReviewSessionSpend,
    TracingCeilingAlert, TracingPinMismatchAlert,
};
use crate::language_layer_prompt::{
    comment_response_schema, compile_comment_prompt, CoachingProfileProjection,
};
use crate::language_layer_provider::{
    ChatMessage, CompletionAttempt, CompletionOutcome, CompletionRequest, DeterminismControls,
    LanguageLayerProvider, PinnedGenerationContract,
};
use crate::pin_record::PinRecord;
use crate::pin_verification::{
    judge_completed_verification, verify_generation_within_deadline, PinVerificationFailure,
    PinVerificationJudgement, PinVerificationStrictness,
};
use crate::quality_capture::{
    hosted_language_layer_capture, HostedCaptureBuffer, HostedGenerationInput,
    HostedLanguageLayerTask, RecordedProseRejection,
};
use crate::review_session_contract::{
    CriticalMomentCommentDraft, CriticalMomentCommentGenerationContract,
    CriticalMomentExplainerCandidate, CriticalMomentGenerationRandomness,
    CriticalMomentGenerationSettings, CriticalMomentIntentAuthoringContext, PlayerId,
    ProviderUnavailableReason, ReviewMomentCommentFacts,
};

/// Process-wide hosted comment pieces. Player, session spend, and the capture
/// buffer are attached per Open so one author never shares another Player's
/// meter or outbox.
pub struct HostedCommentRuntime {
    pub(crate) provider: Arc<LanguageLayerProvider>,
    pub(crate) pin: PinRecord,
    pub(crate) fingerprint: EvaluationFingerprint,
    pub(crate) ledger: Arc<dyn LanguageLayerLedger>,
    pub(crate) concurrency: Arc<ProviderConcurrency>,
    pub(crate) config: LanguageLayerAdmissionConfig,
    pub(crate) ceiling_alert: Arc<dyn CeilingAlert>,
    pub(crate) pin_mismatch_alert: Arc<dyn PinMismatchAlert>,
}

impl HostedCommentRuntime {
    pub fn new(
        provider: Arc<LanguageLayerProvider>,
        pin: PinRecord,
        fingerprint: EvaluationFingerprint,
        ledger: Arc<dyn LanguageLayerLedger>,
        concurrency: Arc<ProviderConcurrency>,
        config: LanguageLayerAdmissionConfig,
    ) -> Self {
        Self {
            provider,
            pin,
            fingerprint,
            ledger,
            concurrency,
            config,
            ceiling_alert: Arc::new(TracingCeilingAlert),
            pin_mismatch_alert: Arc::new(TracingPinMismatchAlert),
        }
    }

    pub fn with_ceiling_alert(mut self, ceiling_alert: Arc<dyn CeilingAlert>) -> Self {
        self.ceiling_alert = ceiling_alert;
        self
    }

    pub fn with_pin_mismatch_alert(
        mut self,
        pin_mismatch_alert: Arc<dyn PinMismatchAlert>,
    ) -> Self {
        self.pin_mismatch_alert = pin_mismatch_alert;
        self
    }

    pub fn author(
        self: &Arc<Self>,
        player_id: PlayerId,
        session: Arc<ReviewSessionSpend>,
        profile: CoachingProfileProjection,
        captures: Arc<HostedCaptureBuffer>,
    ) -> HostedCommentAuthor {
        HostedCommentAuthor {
            runtime: Arc::clone(self),
            player_id,
            session,
            profile,
            captures,
        }
    }
}

pub struct HostedCommentAuthor {
    runtime: Arc<HostedCommentRuntime>,
    player_id: PlayerId,
    session: Arc<ReviewSessionSpend>,
    profile: CoachingProfileProjection,
    captures: Arc<HostedCaptureBuffer>,
}

impl HostedCommentAuthor {
    fn record_capture(&self, input: HostedGenerationInput<'_>) {
        self.captures.push(hosted_language_layer_capture(input));
    }

    fn generation_contract_from_pin(pin: &PinRecord) -> CriticalMomentCommentGenerationContract {
        CriticalMomentCommentGenerationContract {
            code_revision: format!("chen-chess-coach-engine/{}", env!("CARGO_PKG_VERSION")),
            candidate: CriticalMomentExplainerCandidate::new(
                pin.endpoint_tag.clone(),
                pin.model.clone(),
                pin.catalogue_slug.clone(),
                compiled_comment_prompt_digest(),
                compiled_comment_schema_digest(),
            ),
            settings: CriticalMomentGenerationSettings {
                randomness: CriticalMomentGenerationRandomness::LowestSupported,
                stable_seed: pin.determinism.seed.then_some(0),
                seed_supported: pin.determinism.seed,
                max_output_tokens: u16::try_from(pin.max_tokens).unwrap_or(u16::MAX),
            },
        }
    }
}

/// Bake-off `build_request` for a comment, with the Task Contract deadline.
pub fn completion_request_for_comment(
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
    profile: &CoachingProfileProjection,
    pin: &PinRecord,
    remaining_deadline: Duration,
) -> CompletionRequest {
    let prompt = compile_comment_prompt(facts, intent, profile);
    CompletionRequest {
        contract: PinnedGenerationContract {
            model: pin.model.clone(),
            provider_only: pin.endpoint_tag.clone(),
            max_tokens: pin.max_tokens,
            determinism: DeterminismControls {
                temperature: pin.determinism.temperature,
                seed: pin.determinism.seed,
            },
        },
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: prompt.system,
            },
            ChatMessage {
                role: "user".into(),
                content: prompt.user,
            },
        ],
        schema_name: "review_moment_comment".to_string(),
        schema: comment_response_schema(),
        remaining_deadline,
    }
}

pub fn parse_comment_draft(
    facts: &ReviewMomentCommentFacts,
    raw_content: &str,
) -> Option<CriticalMomentCommentDraft> {
    let value: Value = serde_json::from_str(raw_content).ok()?;
    let text = value["comment"]
        .as_str()
        .or_else(|| value["text"].as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())?
        .to_string();
    Some(CriticalMomentCommentDraft {
        text,
        grounding_ledger: grounding_ledger_for(facts),
    })
}

pub struct AuthoredComment {
    pub draft: CriticalMomentCommentDraft,
    pin: PinCheck,
    generation: Option<HostedGenerationSnapshot>,
    pin_mismatch_alert: Arc<dyn PinMismatchAlert>,
}

pub(crate) struct HostedGenerationSnapshot {
    fingerprint: EvaluationFingerprint,
    attempt: CompletionAttempt,
}

enum PinCheck {
    NotRequired,
    Ready(Box<PinVerificationJudgement>),
    Pending(Box<PendingPinCheck>),
}

struct PendingPinCheck {
    provider: Arc<LanguageLayerProvider>,
    generation_id: Option<String>,
    remaining: Duration,
    expected_model: String,
    provider_only: String,
    completed: bool,
    open: OpenHostedAttempt,
    player_id: PlayerId,
    remaining_deadline: Duration,
    as_of: DateTime<Utc>,
    fingerprint_digest: String,
    cancelled: bool,
    in_flight_cancelled: bool,
    ledger: Arc<dyn LanguageLayerLedger>,
    session: Arc<ReviewSessionSpend>,
    ceiling_alert: Arc<dyn CeilingAlert>,
}

impl AuthoredComment {
    pub fn without_pin_check(draft: CriticalMomentCommentDraft) -> Self {
        Self {
            draft,
            pin: PinCheck::NotRequired,
            generation: None,
            pin_mismatch_alert: Arc::new(TracingPinMismatchAlert),
        }
    }

    pub fn with_pin_judgement(
        draft: CriticalMomentCommentDraft,
        judgement: PinVerificationJudgement,
    ) -> Self {
        Self {
            draft,
            pin: PinCheck::Ready(Box::new(judgement)),
            generation: None,
            pin_mismatch_alert: Arc::new(TracingPinMismatchAlert),
        }
    }

    pub(crate) fn record_capture(
        &mut self,
        outcome: CaptureOutcome,
        pin: &PinVerificationJudgement,
        attempts: u8,
        rejection: Option<CommentProseRejection>,
        created_at: DateTime<Utc>,
    ) -> Option<crate::quality_capture::QualityCaptureDraft> {
        let generation = self.generation.take()?;
        Some(hosted_language_layer_capture(
            HostedGenerationInput {
                fingerprint: generation.fingerprint,
                attempt: &generation.attempt,
                trigger: CaptureTrigger::Preference,
                outcome,
                pin_verification: pin.as_verdict(),
                served_endpoint: None,
                served_region: None,
                routed_service_tier: None,
                attempts,
                task: HostedLanguageLayerTask::Comment,
                created_at,
                steps: Vec::new(),
                rejection: rejection.map(RecordedProseRejection::from),
            }
            .with_served_route(pin.served_route()),
        ))
    }

    pub(crate) async fn verify_pin(&self) -> PinVerificationJudgement {
        let judgement = match &self.pin {
            PinCheck::NotRequired => PinVerificationJudgement::NotApplicable,
            PinCheck::Ready(judgement) => judgement.as_ref().clone(),
            PinCheck::Pending(pending) => pending.verify().await,
        };
        if let PinVerificationJudgement::Mismatched(report) = &judgement {
            self.pin_mismatch_alert.pin_mismatched(report);
        }
        judgement
    }

    pub(crate) async fn finish(
        self,
        judgement: &PinVerificationJudgement,
    ) -> Result<(), crate::language_layer_ledger::LedgerError> {
        let PinCheck::Pending(pending) = self.pin else {
            return Ok(());
        };
        let context = AttemptContext {
            player_id: pending.player_id.clone(),
            task: HostedTask::Comment,
            remaining_deadline: pending.remaining_deadline,
            as_of: pending.as_of,
            fingerprint_digest: pending.fingerprint_digest.clone(),
            cancelled: pending.cancelled,
            in_flight_cancelled: pending.in_flight_cancelled,
            pin: judgement.clone(),
        };
        finish_hosted_attempt(
            &context,
            pending.ledger.as_ref(),
            pending.session.as_ref(),
            pending.ceiling_alert.as_ref(),
            pending.open.attempt,
            pending.open.provider_cooldown,
        )
        .await
        .map(|_| ())
    }
}

impl PendingPinCheck {
    async fn verify(&self) -> PinVerificationJudgement {
        let Some(generation_id) = self.generation_id.as_deref() else {
            return judge_completed_verification(
                Err(PinVerificationFailure::MissingIdentity),
                &self.expected_model,
                &self.provider_only,
                self.completed,
                PinVerificationStrictness::Runtime,
            );
        };
        let result = verify_generation_within_deadline(
            self.provider.as_ref(),
            generation_id,
            self.remaining,
        )
        .await;
        judge_completed_verification(
            result,
            &self.expected_model,
            &self.provider_only,
            self.completed,
            PinVerificationStrictness::Runtime,
        )
    }
}

fn usable_completed_draft(
    facts: &ReviewMomentCommentFacts,
    attempt: &CompletionAttempt,
) -> Option<CriticalMomentCommentDraft> {
    if attempt.outcome != CompletionOutcome::Completed {
        return None;
    }
    parse_comment_draft(facts, attempt.raw_content.as_deref()?)
}

impl CriticalMomentCommentAuthor for HostedCommentAuthor {
    fn generation_contract(&self) -> CriticalMomentCommentGenerationContract {
        HostedCommentAuthor::generation_contract_from_pin(&self.runtime.pin)
    }

    fn author(
        &self,
        input: CriticalMomentCommentAuthorInput,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<AuthoredComment, ProviderUnavailableReason>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move { self.author_once(input).await })
    }
}

impl HostedCommentAuthor {
    async fn author_once(
        &self,
        input: CriticalMomentCommentAuthorInput,
    ) -> Result<AuthoredComment, ProviderUnavailableReason> {
        let remaining = self.runtime.config.comment_authoring_deadline;
        let request = completion_request_for_comment(
            input.facts(),
            input.intent(),
            &self.profile,
            &self.runtime.pin,
            remaining,
        );
        let as_of = Utc::now();
        let context = AttemptContext {
            player_id: self.player_id.clone(),
            task: HostedTask::Comment,
            remaining_deadline: remaining,
            as_of,
            fingerprint_digest: self.runtime.fingerprint.digest.as_str().to_string(),
            cancelled: false,
            in_flight_cancelled: false,
            pin: PinVerificationJudgement::Unverified,
        };
        let start = begin_hosted_attempt(
            &context,
            &self.runtime.config,
            self.runtime.ledger.as_ref(),
            self.session.as_ref(),
            self.runtime.concurrency.as_ref(),
            self.runtime.ceiling_alert.as_ref(),
            || async {
                match self.runtime.provider.complete(&request).await {
                    Ok(attempt) => attempt,
                    Err(error) => CompletionAttempt {
                        latency: Duration::ZERO,
                        http_status: None,
                        generation_id: None,
                        served_model: None,
                        served_provider: None,
                        prompt_tokens: None,
                        completion_tokens: None,
                        reasoning_tokens: None,
                        cost: None,
                        finish_reason: None,
                        raw_content: Some(error.to_string()),
                        outcome: CompletionOutcome::InvalidRequest,
                    },
                }
            },
        )
        .await
        .map_err(|_| ProviderUnavailableReason::LanguageLayer)?;

        match start {
            HostedAttemptStart::Open(open) => {
                let draft = usable_completed_draft(input.facts(), &open.attempt);
                if let Some(draft) = draft {
                    let leftover = remaining.saturating_sub(open.attempt.latency);
                    return Ok(AuthoredComment {
                        draft,
                        generation: Some(HostedGenerationSnapshot {
                            fingerprint: self.runtime.fingerprint.clone(),
                            attempt: open.attempt.clone(),
                        }),
                        pin: PinCheck::Pending(Box::new(PendingPinCheck {
                            provider: Arc::clone(&self.runtime.provider),
                            generation_id: open.attempt.generation_id.clone(),
                            remaining: leftover,
                            expected_model: self.runtime.pin.model.clone(),
                            provider_only: self.runtime.pin.endpoint_tag.clone(),
                            completed: true,
                            open,
                            player_id: self.player_id.clone(),
                            remaining_deadline: remaining,
                            as_of,
                            fingerprint_digest: self
                                .runtime
                                .fingerprint
                                .digest
                                .as_str()
                                .to_string(),
                            cancelled: false,
                            in_flight_cancelled: false,
                            ledger: Arc::clone(&self.runtime.ledger),
                            session: Arc::clone(&self.session),
                            ceiling_alert: Arc::clone(&self.runtime.ceiling_alert),
                        })),
                        pin_mismatch_alert: Arc::clone(&self.runtime.pin_mismatch_alert),
                    });
                }
                self.record_capture(HostedGenerationInput {
                    fingerprint: self.runtime.fingerprint.clone(),
                    attempt: &open.attempt,
                    trigger: CaptureTrigger::Preference,
                    outcome: CaptureOutcome::Failed,
                    pin_verification: PinVerificationVerdict::Unverified,
                    served_endpoint: None,
                    served_region: None,
                    routed_service_tier: None,
                    attempts: 1,
                    task: HostedLanguageLayerTask::Comment,
                    created_at: as_of,
                    steps: Vec::new(),
                    rejection: None,
                });
                finish_hosted_attempt(
                    &context,
                    self.runtime.ledger.as_ref(),
                    self.session.as_ref(),
                    &TracingCeilingAlert,
                    open.attempt,
                    open.provider_cooldown,
                )
                .await
                .map_err(|_| ProviderUnavailableReason::LanguageLayer)?;
                Err(ProviderUnavailableReason::LanguageLayer)
            }
            denied => {
                let outcome = match denied {
                    HostedAttemptStart::Denied {
                        reason: DenialReason::ProviderCooldown,
                        ..
                    } => CaptureOutcome::ProviderCooldown,
                    _ => CaptureOutcome::BudgetRefused,
                };
                self.record_capture(HostedGenerationInput {
                    fingerprint: self.runtime.fingerprint.clone(),
                    attempt: &CompletionAttempt {
                        latency: Duration::ZERO,
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
                        outcome: CompletionOutcome::InvalidRequest,
                    },
                    trigger: CaptureTrigger::Preference,
                    outcome,
                    pin_verification: PinVerificationVerdict::NotApplicable,
                    served_endpoint: None,
                    served_region: None,
                    routed_service_tier: None,
                    attempts: 1,
                    task: HostedLanguageLayerTask::Comment,
                    created_at: as_of,
                    steps: Vec::new(),
                    rejection: None,
                });
                Err(ProviderUnavailableReason::LanguageLayer)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation_fingerprint::{CaptureOutcome, EvaluationEnvironment};
    use crate::language_layer_ledger::{HostedAttemptOutcome, MemoryLanguageLayerLedger};
    use crate::language_layer_provider::PinVerification;
    use crate::pin_record::{compiled_pin_record, fingerprint_from_pin};
    use crate::pin_verification::{PinMismatchReport, ServedRoute};
    use crate::review_session_contract::ArtifactDigest;

    fn pin_verification(permaslug: &str, provider: &str) -> PinVerification {
        PinVerification {
            verified_permaslug: Some(permaslug.to_string()),
            verified_provider: Some(provider.to_string()),
            served_endpoint_id: Some("ep-hosted".into()),
            served_region: Some("global".into()),
            routed_service_tier: None,
            prompt_tokens: None,
            completion_tokens: None,
            cost: None,
            error: None,
        }
    }

    #[test]
    fn record_capture_copies_served_route_and_attempt_count() {
        let pin = compiled_pin_record();
        let attempt = CompletionAttempt {
            latency: Duration::from_millis(8),
            http_status: Some(200),
            generation_id: Some("gen-hosted".into()),
            served_model: Some(pin.model.clone()),
            served_provider: Some("Google Vertex".into()),
            prompt_tokens: Some(10),
            completion_tokens: Some(4),
            reasoning_tokens: None,
            cost: Some(0.001),
            finish_reason: Some("stop".into()),
            raw_content: Some("{}".into()),
            outcome: CompletionOutcome::Completed,
        };
        let mut authored = AuthoredComment {
            draft: CriticalMomentCommentDraft {
                text: "discarded hosted prose".into(),
                grounding_ledger: crate::review_session_contract::CriticalMomentGroundingLedger {
                    facts_ref: ArtifactDigest::try_from(format!("sha256:{}", "a".repeat(64)))
                        .unwrap(),
                    factual_claims: Vec::new(),
                },
            },
            pin: PinCheck::NotRequired,
            generation: Some(HostedGenerationSnapshot {
                fingerprint: fingerprint_from_pin(&pin, EvaluationEnvironment::Staging),
                attempt,
            }),
            pin_mismatch_alert: Arc::new(TracingPinMismatchAlert),
        };
        let route = ServedRoute {
            endpoint: Some("ep-hosted".into()),
            region: Some("global".into()),
            routed_service_tier: None,
            verified_permaslug: pin.model.clone(),
            verified_provider: "Google Vertex".into(),
        };
        let capture = authored
            .record_capture(
                CaptureOutcome::Published,
                &PinVerificationJudgement::Passed(route),
                2,
                None,
                "2026-08-19T15:04:05Z".parse().unwrap(),
            )
            .expect("generation snapshot");
        let value = serde_json::to_value(&capture).unwrap();
        assert_eq!(value["content"]["callShape"]["attempts"], 2);
        assert_eq!(
            value["content"]["observations"]["servedEndpoint"],
            "ep-hosted"
        );
        assert_eq!(value["content"]["observations"]["servedRegion"], "global");
        assert_eq!(
            value["content"]["observations"]["pinVerification"],
            "passed"
        );
    }

    #[test]
    fn a_ready_mismatch_is_still_a_mismatch() {
        let pin = compiled_pin_record();
        let judgement = judge_completed_verification(
            Ok(pin_verification("other/model", "Google Vertex")),
            &pin.model,
            &pin.endpoint_tag,
            true,
            PinVerificationStrictness::Runtime,
        );
        assert!(judgement.pin_mismatched());
        let authored = AuthoredComment::with_pin_judgement(
            CriticalMomentCommentDraft {
                text: "discarded hosted prose".into(),
                grounding_ledger: crate::review_session_contract::CriticalMomentGroundingLedger {
                    facts_ref: ArtifactDigest::try_from(format!("sha256:{}", "a".repeat(64)))
                        .unwrap(),
                    factual_claims: Vec::new(),
                },
            },
            judgement,
        );
        let ready = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(authored.verify_pin());
        assert!(ready.pin_mismatched());
    }

    #[test]
    fn finish_records_pin_mismatched_on_the_ledger() {
        let ledger = MemoryLanguageLayerLedger::new();
        let session = ReviewSessionSpend::new();
        let pin = compiled_pin_record();
        let attempt = CompletionAttempt {
            latency: Duration::from_millis(8),
            http_status: Some(200),
            generation_id: Some("gen-hosted".into()),
            served_model: Some(pin.model.clone()),
            served_provider: Some("Google Vertex".into()),
            prompt_tokens: Some(10),
            completion_tokens: Some(4),
            reasoning_tokens: None,
            cost: Some(0.001),
            finish_reason: Some("stop".into()),
            raw_content: Some("{}".into()),
            outcome: CompletionOutcome::Completed,
        };
        let authored = AuthoredComment {
            draft: CriticalMomentCommentDraft {
                text: "discarded hosted prose".into(),
                grounding_ledger: crate::review_session_contract::CriticalMomentGroundingLedger {
                    facts_ref: ArtifactDigest::try_from(format!("sha256:{}", "a".repeat(64)))
                        .unwrap(),
                    factual_claims: Vec::new(),
                },
            },
            pin: PinCheck::Pending(Box::new(PendingPinCheck {
                provider: Arc::new(LanguageLayerProvider::from_client(
                    reqwest::Client::new(),
                    "test",
                )),
                generation_id: Some("gen-hosted".into()),
                remaining: Duration::from_secs(1),
                expected_model: pin.model.clone(),
                provider_only: pin.endpoint_tag.clone(),
                completed: true,
                open: OpenHostedAttempt {
                    attempt,
                    provider_cooldown: None,
                },
                player_id: PlayerId::try_from("player-373".to_string()).unwrap(),
                remaining_deadline: Duration::from_secs(10),
                as_of: Utc::now(),
                fingerprint_digest: "sha256:test".into(),
                cancelled: false,
                in_flight_cancelled: false,
                ledger: Arc::new(ledger),
                session: Arc::new(session),
                ceiling_alert: Arc::new(TracingCeilingAlert),
            })),
            generation: None,
            pin_mismatch_alert: Arc::new(TracingPinMismatchAlert),
        };
        let judgement = PinVerificationJudgement::Mismatched(PinMismatchReport {
            pinned_model: pin.model,
            pinned_provider_family: "google-vertex".into(),
            observed_permaslug: Some("other/model".into()),
            observed_provider: Some("Amazon Bedrock".into()),
            observed_provider_family: Some("amazon-bedrock".into()),
            served_endpoint: Some("ep-hosted".into()),
            served_region: Some("global".into()),
            routed_service_tier: None,
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let PinCheck::Pending(pending) = &authored.pin else {
            panic!("pending");
        };
        let ledger = Arc::clone(&pending.ledger);
        runtime
            .block_on(authored.finish(&judgement))
            .expect("settle");
        let records = runtime.block_on(ledger.records()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].error_class, None);
        assert_eq!(records[0].pin_verification, PinVerificationVerdict::Failed);
        assert_eq!(
            records[0].pin_cause,
            Some(crate::pin_verification::PinVerificationCause::Mismatched)
        );
        assert!(records[0].cost_micros > 0);
        let _ = HostedAttemptOutcome::Settled {
            record: records[0].clone(),
            attempt: None,
        };
    }

    #[test]
    fn an_unparseable_completed_attempt_still_finishes_the_ledger() {
        let ledger = MemoryLanguageLayerLedger::new();
        let session = ReviewSessionSpend::new();
        let attempt = CompletionAttempt {
            latency: Duration::from_millis(8),
            http_status: Some(200),
            generation_id: Some("gen-unparseable".into()),
            served_model: None,
            served_provider: None,
            prompt_tokens: Some(10),
            completion_tokens: Some(4),
            reasoning_tokens: None,
            cost: Some(0.001),
            finish_reason: Some("stop".into()),
            raw_content: Some("{}".into()),
            outcome: CompletionOutcome::Completed,
        };
        let context = AttemptContext {
            player_id: PlayerId::try_from("player-373".to_string()).unwrap(),
            task: HostedTask::Comment,
            remaining_deadline: Duration::from_secs(10),
            as_of: Utc::now(),
            fingerprint_digest: "sha256:test".into(),
            cancelled: false,
            in_flight_cancelled: false,
            pin: PinVerificationJudgement::Unverified,
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime
            .block_on(finish_hosted_attempt(
                &context,
                &ledger,
                &session,
                &TracingCeilingAlert,
                attempt,
                None,
            ))
            .expect("settle");
        let records = runtime.block_on(ledger.records()).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].cost_micros > 0);
        assert_eq!(records[0].error_class, None);
        assert_eq!(
            records[0].pin_verification,
            PinVerificationVerdict::Unverified
        );
    }
}
