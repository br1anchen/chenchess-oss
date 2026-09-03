use std::{future::Future, pin::Pin, sync::LazyLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::review_session_contract::{
    json_text_bytes_within_limit, ArtifactDigest, CriticalMomentComment,
    CriticalMomentCommentDraft, CriticalMomentCommentGenerationContract,
    CriticalMomentCommentGenerationOutcome, CriticalMomentExplainerCandidate,
    CriticalMomentFactualClaim, CriticalMomentGenerationRandomness,
    CriticalMomentGenerationSettings, CriticalMomentGroundingLedger,
    CriticalMomentGroundingRejection, CriticalMomentIntentAuthoringContext,
    CriticalMomentIntentAuthoringInstructions, GameReviewCriticalMoment,
    GameReviewMomentClassification, GameReviewPlayedMoveEffect, IntentEnrichment,
    PositiveHighlightAchievement, ProviderUnavailableReason, ReviewMomentCommentFacts,
    ReviewSessionLimits,
};

use crate::chess_literal_grounding::ChessLiteralGrounding;
use crate::language_layer_markers::{append_sentence, MarkerViolation, MarkerVocabulary};
use crate::language_layer_prompt::{
    comment_prompt_digest, comment_schema_digest, CoachingProfileProjection,
};
use crate::pin_verification::ServedRoute;

#[path = "critical_moment_comment/fact_shape.rs"]
mod fact_shape;
#[path = "critical_moment_comment/hosted_author.rs"]
mod hosted_author;
#[path = "critical_moment_comment/learning_grounding.rs"]
mod learning_grounding;
#[path = "critical_moment_comment/rendering.rs"]
mod rendering;
pub use fact_shape::{
    AchievementKind, FactShape, FactShapeId, GradeKind, ImprovementOutcomeKind, MarkerFormKind,
    MarkerSlot, NeutralReasonKind, PayoffKind, PlayedOutcomeKind, ResidualKind, ShapeAxis,
    ShapeDiscriminants,
};
pub use hosted_author::{
    completion_request_for_comment, parse_comment_draft, AuthoredComment, HostedCommentAuthor,
    HostedCommentRuntime,
};
use rendering::{
    achievement_sentence, decision_cue_clause, improvement_correction_marker_text,
    improvement_correction_text, material_verdict_text, move_target_text, neutral_reason_text,
    opponent_resource_text, played_outcome_clause, played_outcome_marker_text,
    played_outcome_sentence, played_popularity_text, positive_achievement_text,
    positive_difficulty_text, positive_grade_marker_text, positive_grade_text,
    residual_consequence_text, safe_intent_sentence, takeaway_marker_text, teaching_takeaway,
};

/* Both digests hash a compiled-in template, so they are the same for the life
of the process. Staleness asks for them once per stored comment, which is once
per Review Moment across a whole Game when web artifacts are authored eagerly,
and the schema digest canonicalises and hashes a freshly built JSON document
every time it is asked. */
static COMPILED_COMMENT_PROMPT_DIGEST: LazyLock<ArtifactDigest> =
    LazyLock::new(|| compiled_digest(comment_prompt_digest()));
static COMPILED_COMMENT_SCHEMA_DIGEST: LazyLock<ArtifactDigest> =
    LazyLock::new(|| compiled_digest(comment_schema_digest()));

/// The comment prompt template digest this build compiles.
pub(crate) fn compiled_comment_prompt_digest() -> ArtifactDigest {
    COMPILED_COMMENT_PROMPT_DIGEST.clone()
}

/// The comment response schema digest this build compiles.
pub(crate) fn compiled_comment_schema_digest() -> ArtifactDigest {
    COMPILED_COMMENT_SCHEMA_DIGEST.clone()
}

fn compiled_digest(value: String) -> ArtifactDigest {
    ArtifactDigest::try_from(value).expect("compiled digest is a valid ArtifactDigest")
}

/// Why a stored comment is not the last word on a Review Moment.
///
/// Absent means serve what is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reauthor {
    /// The prompt this was written from no longer compiles. `authored` says
    /// whether the stored text is real prose, which a fresh fallback must not
    /// replace -- an outage would otherwise cost the Player real coaching.
    PromptEdited { authored: bool },
    /// A safe rendering written from the compiled prompt that has not been
    /// retried. What replaces it is marked retried, so this ends after one.
    RetryFallback,
}

/// Immutable Language Layer input. The tagged facts, optional ephemeral intent
/// authoring context, and contract remain byte-identical on retry.
#[derive(Debug, Clone, PartialEq)]
pub struct CriticalMomentCommentAuthorInput {
    facts: ReviewMomentCommentFacts,
    intent: Option<CriticalMomentIntentAuthoringContext>,
    generation_contract: CriticalMomentCommentGenerationContract,
}

impl CriticalMomentCommentAuthorInput {
    pub fn try_new(
        facts: ReviewMomentCommentFacts,
        intent: Option<CriticalMomentIntentAuthoringContext>,
        generation_contract: CriticalMomentCommentGenerationContract,
    ) -> Result<Self, CriticalMomentCommentAuthorInputError> {
        if !facts.is_well_formed() {
            return Err(CriticalMomentCommentAuthorInputError::InvalidClassificationFacts);
        }
        if matches!(facts, ReviewMomentCommentFacts::Neutral { .. }) && intent.is_some() {
            return Err(CriticalMomentCommentAuthorInputError::InvalidClassificationFacts);
        }
        if !generation_contract.is_reproducible() {
            return Err(CriticalMomentCommentAuthorInputError::InvalidGenerationContract);
        }
        Ok(Self {
            facts,
            intent,
            generation_contract,
        })
    }

    pub fn facts(&self) -> &ReviewMomentCommentFacts {
        &self.facts
    }
    pub fn moment(&self) -> &GameReviewCriticalMoment {
        self.facts.moment()
    }
    pub fn intent(&self) -> Option<&CriticalMomentIntentAuthoringContext> {
        self.intent.as_ref()
    }
    pub fn generation_contract(&self) -> &CriticalMomentCommentGenerationContract {
        &self.generation_contract
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CriticalMomentCommentAuthorInputError {
    #[error("Review Moment classification facts are invalid")]
    InvalidClassificationFacts,
    #[error("Review Moment generation contract is not reproducible")]
    InvalidGenerationContract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CriticalMomentCommentAdmissionError {
    #[error("Review Moment classification facts are invalid")]
    InvalidClassificationFacts,
    #[error("Review Moment generation contract is not reproducible")]
    InvalidGenerationContract,
}

pub trait CriticalMomentCommentAuthor: Send + Sync {
    fn generation_contract(&self) -> CriticalMomentCommentGenerationContract;
    fn author<'a>(
        &'a self,
        input: CriticalMomentCommentAuthorInput,
    ) -> Pin<Box<dyn Future<Output = Result<AuthoredComment, ProviderUnavailableReason>> + Send + 'a>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroundedCriticalMomentComment {
    pub comment: CriticalMomentComment,
    pub authoring_provenance: CriticalMomentCommentAuthoringProvenance,
    pub(crate) quality_captures: Vec<crate::quality_capture::QualityCaptureDraft>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriticalMomentCommentAuthoringProvenance {
    pub generation_contract: CriticalMomentCommentGenerationContract,
    pub grounding_ledger: CriticalMomentGroundingLedger,
    pub outcome: CriticalMomentCommentGenerationOutcome,
    /// The Coaching Profile Projection that shaped this published comment.
    /// Cold-start is stored only when that is the profile that authored it.
    #[serde(default)]
    pub coaching_profile_projection: CoachingProfileProjection,
    #[serde(default)]
    pub served_endpoint: Option<String>,
    #[serde(default)]
    pub served_region: Option<String>,
    /// None means the declared default was served.
    #[serde(default)]
    pub routed_service_tier: Option<String>,
}

impl CriticalMomentCommentAuthoringProvenance {
    pub(crate) fn hosted_generation_contract() -> CriticalMomentCommentGenerationContract {
        fn digest(byte: char) -> ArtifactDigest {
            ArtifactDigest::try_from(format!("sha256:{}", byte.to_string().repeat(64)))
                .expect("fixed hosted-comment digest is valid")
        }

        CriticalMomentCommentGenerationContract {
            code_revision: "hosted-review-moment-comment-v1".to_string(),
            candidate: CriticalMomentExplainerCandidate::new(
                "coach-app-host".to_string(),
                "submitted-comment".to_string(),
                "v1".to_string(),
                digest('a'),
                digest('b'),
            ),
            settings: CriticalMomentGenerationSettings {
                randomness: CriticalMomentGenerationRandomness::LowestSupported,
                stable_seed: Some(0),
                seed_supported: true,
                max_output_tokens: 1,
            },
        }
    }

    /// Whether the Coach App's host model submitted this comment, as opposed
    /// to the engine's hosted Language Layer authoring it for the web.
    pub(crate) fn is_host_submitted(&self) -> bool {
        self.generation_contract.code_revision == Self::hosted_generation_contract().code_revision
    }

    /// Whether this comment was authored against a comment prompt the engine
    /// no longer compiles.
    ///
    /// The prompt and response-schema digests the candidate already carries
    /// are the artifact's version: editing either template moves them, so a
    /// stored comment disagreeing with the compiled pair is prose from a
    /// prompt that no longer exists. The pin is deliberately not consulted —
    /// a model or endpoint change does not discard hosted prose, only a
    /// change to what the engine asked for does.
    ///
    /// Host-submitted prose is the Coach App's text rather than the engine's
    /// and is never stale against a template it was not written from.
    pub(crate) fn is_stale_web_artifact(&self) -> bool {
        if self.is_host_submitted() {
            return false;
        }
        let candidate = &self.generation_contract.candidate;
        candidate.prompt_digest != *COMPILED_COMMENT_PROMPT_DIGEST
            || candidate.response_schema_digest != *COMPILED_COMMENT_SCHEMA_DIGEST
    }

    /// Why an open that can author should replace what is stored, if it should.
    ///
    /// One question with one answer, because the two reasons need different
    /// consequences and reading them through separate predicates let them
    /// disagree: a prompt edit may not clobber real prose with a rendering,
    /// and a retry has to mark what it writes so the next open stops.
    pub(crate) fn reauthor(&self) -> Option<Reauthor> {
        if self.is_host_submitted() {
            return None;
        }
        if self.is_stale_web_artifact() {
            return Some(Reauthor::PromptEdited {
                authored: matches!(
                    self.outcome,
                    CriticalMomentCommentGenerationOutcome::Authored { .. }
                ),
            });
        }
        matches!(
            self.outcome,
            CriticalMomentCommentGenerationOutcome::SafeRendered { retried: false, .. }
        )
        .then_some(Reauthor::RetryFallback)
    }

    pub(crate) fn hosted_authored(
        grounding_ledger: CriticalMomentGroundingLedger,
        attempts: u8,
    ) -> Self {
        Self {
            generation_contract: Self::hosted_generation_contract(),
            grounding_ledger,
            outcome: CriticalMomentCommentGenerationOutcome::Authored { attempts },
            coaching_profile_projection: CoachingProfileProjection::cold_start(),
            served_endpoint: None,
            served_region: None,
            routed_service_tier: None,
        }
    }

    pub(crate) fn hosted_safe_rendered(
        grounding_ledger: CriticalMomentGroundingLedger,
        reason: CriticalMomentGroundingRejection,
        retried: bool,
    ) -> Self {
        Self {
            generation_contract: Self::hosted_generation_contract(),
            grounding_ledger,
            outcome: CriticalMomentCommentGenerationOutcome::SafeRendered {
                attempts: 2,
                reason,
                retried,
            },
            coaching_profile_projection: CoachingProfileProjection::cold_start(),
            served_endpoint: None,
            served_region: None,
            routed_service_tier: None,
        }
    }

    /// A Coach App submits prose, but the Review Engine remains the canonical admission
    /// boundary. Retain that admission receipt without storing host identity.
    pub fn hosted(grounding_ledger: CriticalMomentGroundingLedger, safe_rendered: bool) -> Self {
        if safe_rendered {
            Self::hosted_safe_rendered(
                grounding_ledger,
                CriticalMomentGroundingRejection::ChangedFact,
                false,
            )
        } else {
            Self::hosted_authored(grounding_ledger, 1)
        }
    }

    pub fn with_coaching_profile(mut self, profile: CoachingProfileProjection) -> Self {
        self.coaching_profile_projection = profile;
        self
    }

    pub fn with_served_route(mut self, route: ServedRoute) -> Self {
        self.served_endpoint = route.endpoint;
        self.served_region = route.region;
        self.routed_service_tier = route.routed_service_tier;
        self
    }

    pub fn is_valid_for(&self, comment: &CriticalMomentComment) -> bool {
        let valid_attempts = match self.outcome {
            CriticalMomentCommentGenerationOutcome::Authored { attempts } => {
                (1..=2).contains(&attempts)
            }
            CriticalMomentCommentGenerationOutcome::SafeRendered { attempts, .. } => attempts == 2,
        };
        valid_attempts
            && self.generation_contract.is_reproducible()
            && !comment.text.trim().is_empty()
            && !self.grounding_ledger.factual_claims.is_empty()
    }
}

/// The Web-owned grounding gate. Invalid tagged facts fail closed before any
/// prose is requested or rendered; valid facts retry once and then use the
/// same kind policy for deterministic safe rendering.
///
/// `retrying` says whether this open is itself the one retry a stored rendering
/// is owed. A rendering written here inherits that mark, so the next open stops
/// rather than re-authoring forever.
pub async fn author_grounded_comment(
    author: &dyn CriticalMomentCommentAuthor,
    facts: ReviewMomentCommentFacts,
    intent: Option<CriticalMomentIntentAuthoringContext>,
    retrying: bool,
) -> Result<GroundedCriticalMomentComment, CriticalMomentCommentAdmissionError> {
    let generation_contract = author.generation_contract();
    let input = CriticalMomentCommentAuthorInput::try_new(
        facts.clone(),
        intent.clone(),
        generation_contract.clone(),
    )
    .map_err(|error| match error {
        CriticalMomentCommentAuthorInputError::InvalidClassificationFacts => {
            CriticalMomentCommentAdmissionError::InvalidClassificationFacts
        }
        CriticalMomentCommentAuthorInputError::InvalidGenerationContract => {
            CriticalMomentCommentAdmissionError::InvalidGenerationContract
        }
    })?;

    let mut last_rejection = CriticalMomentGroundingRejection::ProviderUnavailable;
    // The prose discipline behind `last_rejection`, kept because the wire enum
    // collapses ten of them into `ChangedFact` and the fallback event reads as
    // an invented fact when it was a misplaced marker.
    let mut last_prose_rejection: Option<CommentProseRejection> = None;
    let mut quality_captures = Vec::new();
    for attempts in 1..=2 {
        match author.author(input.clone()).await {
            Ok(mut authored) => {
                let draft = authored.draft.clone();
                let (pin, ground) = tokio::join!(authored.verify_pin(), async {
                    diagnose_draft(&input, &draft)
                });
                let mut attempt_rejection = None;
                let outcome = match &ground {
                    Ok(_) => crate::evaluation_fingerprint::CaptureOutcome::Published,
                    Err(rejection) => {
                        last_rejection = rejection.into_wire();
                        last_prose_rejection = rejection.prose();
                        attempt_rejection = rejection.prose();
                        crate::evaluation_fingerprint::CaptureOutcome::Rejected
                    }
                };
                if let Some(capture) = authored.record_capture(
                    outcome,
                    &pin,
                    attempts,
                    attempt_rejection,
                    chrono::Utc::now(),
                ) {
                    quality_captures.push(capture);
                }
                if authored.finish(&pin).await.is_err() {
                    last_rejection = CriticalMomentGroundingRejection::ProviderUnavailable;
                    last_prose_rejection = None;
                    continue;
                }
                let Ok(grounded_comment) = ground else {
                    continue;
                };
                let mut published = grounded(
                    grounded_comment.comment,
                    generation_contract,
                    grounded_comment.grounding_ledger,
                    CriticalMomentCommentGenerationOutcome::Authored { attempts },
                );
                if let Some(route) = pin.served_route() {
                    published.authoring_provenance = published
                        .authoring_provenance
                        .with_served_route(route.clone());
                }
                published.quality_captures = quality_captures;
                tracing::info!(
                    event = "coach_hosted_comment_authoring_completion",
                    status = "published",
                    attempts,
                    "hosted Review Moment comment published"
                );
                return Ok(published);
            }
            Err(_) => {
                last_rejection = CriticalMomentGroundingRejection::ProviderUnavailable;
                last_prose_rejection = None;
            }
        }
    }
    // The fallback is an `Ok`, so nothing downstream can tell it from authored
    // prose. Counting this event against the published one is the fallback
    // rate, without reading the Quality Outbox.
    tracing::warn!(
        event = "coach_hosted_comment_authoring_completion",
        status = "safe_rendered",
        reason = ?last_rejection,
        // `reason` is the Player-facing narrowing and reads as `ChangedFact`
        // for any of ten prose disciplines. This is the one to diagnose from.
        prose_rejection = ?last_prose_rejection,
        "hosted Review Moment comment fell back to the safe rendering"
    );
    let mut rendered = safe_rendered(
        &facts,
        intent,
        generation_contract,
        last_rejection,
        retrying,
    );
    rendered.quality_captures = quality_captures;
    Ok(rendered)
}

/// A comment that passed the gate, in the only form the Player may see.
///
/// The ledger travels with it because it is *derived* from the markers the
/// draft used, not restated from the facts: these are the claims this comment
/// actually asserts.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundedCommentDraft {
    pub comment: CriticalMomentComment,
    pub grounding_ledger: CriticalMomentGroundingLedger,
}

pub fn ground_draft(
    input: &CriticalMomentCommentAuthorInput,
    draft: &CriticalMomentCommentDraft,
) -> Result<GroundedCommentDraft, CriticalMomentGroundingRejection> {
    diagnose_draft(input, draft).map_err(DraftRejection::into_wire)
}

/// Why one hosted draft was refused, at the width each gate resolved it.
///
/// The two gates do not lose the same amount on the way out. A ledger failure
/// is already exact in the wire enum — `ChangedReference` and
/// `MissingFactualClaim` say what happened — so there is no wider form to
/// keep. The prose gate collapses ten disciplines into `ChangedFact`, which is
/// how a misplaced marker came to be recorded as the model inventing a fact.
/// So this keeps prose at full width and leaves the ledger where it already
/// is, rather than inventing a wide form for reasons that do not have one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftRejection {
    Ledger(CriticalMomentGroundingRejection),
    Prose(CommentProseRejection),
}

impl DraftRejection {
    pub fn into_wire(self) -> CriticalMomentGroundingRejection {
        match self {
            Self::Ledger(rejection) => rejection,
            Self::Prose(rejection) => rejection.into_wire(),
        }
    }

    /// The prose discipline, when the prose gate is what refused.
    pub fn prose(self) -> Option<CommentProseRejection> {
        match self {
            Self::Ledger(_) => None,
            Self::Prose(rejection) => Some(rejection),
        }
    }
}

/// [`ground_draft`] at the width the gates resolve, for the authoring path that
/// records why a generation was refused.
pub fn diagnose_draft(
    input: &CriticalMomentCommentAuthorInput,
    draft: &CriticalMomentCommentDraft,
) -> Result<GroundedCommentDraft, DraftRejection> {
    validate_hosted_grounding_ledger(input.facts(), &draft.grounding_ledger)
        .map_err(DraftRejection::Ledger)?;
    diagnose_hosted_comment_text(input.facts(), input.intent(), &draft.text)
        .inspect_err(log_grounding_rejection)
        .map_err(DraftRejection::Prose)
}

/// Operator telemetry at the gate's own width, emitted once per refusal on
/// whichever path reached the gate.
fn log_grounding_rejection(rejection: &CommentProseRejection) {
    tracing::warn!(
        event = "coach_hosted_comment_grounding_rejection",
        reason = ?rejection,
        "hosted Review Moment comment failed the grounding gate"
    );
}

/// The Coach App boundary validates a submitted draft against authority that
/// remains in the Review Session. Ledger failures reject the command; prose
/// failures produce deterministic canonical rendering, never host prose.
pub fn admit_hosted_review_moment_comment(
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
    draft: &CriticalMomentCommentDraft,
) -> Result<CriticalMomentComment, CriticalMomentGroundingRejection> {
    if !facts.is_well_formed() {
        return Err(CriticalMomentGroundingRejection::ChangedReference);
    }
    validate_hosted_grounding_ledger(facts, &draft.grounding_ledger)?;
    match ground_hosted_comment_text(facts, intent, &draft.text) {
        Ok(grounded) => Ok(grounded.comment),
        Err(_) => Ok(safely_rendered_comment(facts, intent.cloned())),
    }
}

pub(crate) fn validate_hosted_grounding_ledger(
    facts: &ReviewMomentCommentFacts,
    ledger: &CriticalMomentGroundingLedger,
) -> Result<(), CriticalMomentGroundingRejection> {
    let policy = CommentFactsPolicy::for_facts(facts);
    if ledger.facts_ref != artifact_digest(facts) {
        return Err(CriticalMomentGroundingRejection::ChangedReference);
    }
    if ledger.factual_claims.is_empty() {
        return Err(CriticalMomentGroundingRejection::MissingFactualClaim);
    }
    if ledger.factual_claims != policy.claims {
        return Err(CriticalMomentGroundingRejection::ChangedReference);
    }
    Ok(())
}

/// The whole comment gate, in order.
///
/// 1. Parse markers; unknown or repeated markers reject.
/// 2. Every required marker present, otherwise the claim is missing.
/// 3. No figure written in the model's own words.
/// 4. Chess literals against the widened allowlist, judged on what the model
///    wrote rather than on what we substituted into it.
/// 5. Substitute.
/// 6. The existing post-substitution checks. A surviving brace here means
///    substitution failed, and that comment must never ship.
pub(crate) fn ground_hosted_comment_text(
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
    text: &str,
) -> Result<GroundedCommentDraft, CriticalMomentGroundingRejection> {
    // The wire enum below collapses distinct disciplines into one Player-facing
    // outcome, so which discipline a candidate actually loses is visible only
    // in the event this logs and, on the authoring path, on the Quality
    // Capture [`diagnose_draft`] feeds.
    diagnose_hosted_comment_text(facts, intent, text)
        .inspect_err(log_grounding_rejection)
        .map_err(CommentProseRejection::into_wire)
}

/// Why a draft failed, at the resolution a per-candidate reliability metric
/// needs.
///
/// `CriticalMomentGroundingRejection` is the wire enum a Player-facing surface
/// reports, and it deliberately collapses most of this: whether the model
/// invented a marker, repeated one, or wrote a bare figure, the Player sees the
/// safe rendering either way. A bake-off comparing candidates is asking exactly
/// the question the collapse discards — cheap models separate on *which*
/// discipline they lose — so the gate resolves at this width and narrows on the
/// way out. There is one code path; only the error type differs.
///
/// The three marker disciplines name the marker they lost, because the
/// discipline alone is not actionable. A prompt edit that makes one required
/// marker unwritable reads here as an undifferentiated `MissingRequiredMarker`
/// run, and telling which rule did it meant correlating prompt digests against
/// publish rates after the fact. `UnknownMarker` names nothing: the offending
/// text is the model's rather than ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommentProseRejection {
    TooLong,
    MultiParagraph,
    UnknownMarker,
    RepeatedMarker(&'static str),
    MissingRequiredMarker(&'static str),
    BareFigure,
    MisplacedMarker(&'static str),
    ClaimOutsideFacts,
    MissingRequiredClaim,
    UngroundedChessLiteral,
    InternalVocabulary,
    InternalIdentifier,
    ForbiddenNeutralLiteral,
    AuthoritativeIntent,
    InternalIntentDisclosure,
    MultipleIntentClaims,
    UnexpectedIntentHypothesis,
    LearningResource,
}

/// One gate discipline, without the marker it names.
///
/// The durable record wants the discipline and the marker on separate axes:
/// "which rule did this prompt break" and "which marker did it break it on"
/// are different questions, and a variant that carries its payload groups
/// cleanly for neither. [`CommentProseRejection`] cannot be the stored form
/// itself — it is `Copy` over `&'static str`, so it serializes and never
/// deserializes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProseRejectionDiscipline {
    TooLong,
    MultiParagraph,
    UnknownMarker,
    RepeatedMarker,
    MissingRequiredMarker,
    BareFigure,
    MisplacedMarker,
    ClaimOutsideFacts,
    MissingRequiredClaim,
    UngroundedChessLiteral,
    InternalVocabulary,
    InternalIdentifier,
    ForbiddenNeutralLiteral,
    AuthoritativeIntent,
    InternalIntentDisclosure,
    /// Retired: the runtime now writes the guess the model left out, so
    /// nothing produces this. It stays because it is the *stored* form —
    /// checked-in prose-regression records and staging's own rejection counts
    /// carry `missingUncertainty`, and they still have to read back.
    MissingUncertainty,
    MultipleIntentClaims,
    UnexpectedIntentHypothesis,
    LearningResource,
}

impl CommentProseRejection {
    /// The rule that refused, with the marker stripped off.
    pub fn discipline(self) -> ProseRejectionDiscipline {
        match self {
            Self::TooLong => ProseRejectionDiscipline::TooLong,
            Self::MultiParagraph => ProseRejectionDiscipline::MultiParagraph,
            Self::UnknownMarker => ProseRejectionDiscipline::UnknownMarker,
            Self::RepeatedMarker(_) => ProseRejectionDiscipline::RepeatedMarker,
            Self::MissingRequiredMarker(_) => ProseRejectionDiscipline::MissingRequiredMarker,
            Self::BareFigure => ProseRejectionDiscipline::BareFigure,
            Self::MisplacedMarker(_) => ProseRejectionDiscipline::MisplacedMarker,
            Self::ClaimOutsideFacts => ProseRejectionDiscipline::ClaimOutsideFacts,
            Self::MissingRequiredClaim => ProseRejectionDiscipline::MissingRequiredClaim,
            Self::UngroundedChessLiteral => ProseRejectionDiscipline::UngroundedChessLiteral,
            Self::InternalVocabulary => ProseRejectionDiscipline::InternalVocabulary,
            Self::InternalIdentifier => ProseRejectionDiscipline::InternalIdentifier,
            Self::ForbiddenNeutralLiteral => ProseRejectionDiscipline::ForbiddenNeutralLiteral,
            Self::AuthoritativeIntent => ProseRejectionDiscipline::AuthoritativeIntent,
            Self::InternalIntentDisclosure => ProseRejectionDiscipline::InternalIntentDisclosure,
            Self::MultipleIntentClaims => ProseRejectionDiscipline::MultipleIntentClaims,
            Self::UnexpectedIntentHypothesis => {
                ProseRejectionDiscipline::UnexpectedIntentHypothesis
            }
            Self::LearningResource => ProseRejectionDiscipline::LearningResource,
        }
    }

    /// The marker this discipline names, when it names one.
    ///
    /// `UnknownMarker` names nothing on purpose: the offending text is the
    /// model's rather than ours.
    pub fn marker(self) -> Option<&'static str> {
        match self {
            Self::RepeatedMarker(marker)
            | Self::MissingRequiredMarker(marker)
            | Self::MisplacedMarker(marker) => Some(marker),
            Self::TooLong
            | Self::MultiParagraph
            | Self::UnknownMarker
            | Self::BareFigure
            | Self::ClaimOutsideFacts
            | Self::MissingRequiredClaim
            | Self::UngroundedChessLiteral
            | Self::InternalVocabulary
            | Self::InternalIdentifier
            | Self::ForbiddenNeutralLiteral
            | Self::AuthoritativeIntent
            | Self::InternalIntentDisclosure
            | Self::MultipleIntentClaims
            | Self::UnexpectedIntentHypothesis
            | Self::LearningResource => None,
        }
    }

    pub fn into_wire(self) -> CriticalMomentGroundingRejection {
        match self {
            Self::MultiParagraph => CriticalMomentGroundingRejection::MultiParagraph,
            Self::MissingRequiredMarker(_) | Self::MissingRequiredClaim => {
                CriticalMomentGroundingRejection::MissingFactualClaim
            }
            Self::ClaimOutsideFacts => CriticalMomentGroundingRejection::ChangedReference,
            Self::AuthoritativeIntent => CriticalMomentGroundingRejection::AuthoritativeIntent,
            Self::InternalIntentDisclosure => {
                CriticalMomentGroundingRejection::InternalIntentDisclosure
            }
            Self::MultipleIntentClaims => CriticalMomentGroundingRejection::MultipleIntentClaims,
            Self::UnexpectedIntentHypothesis => {
                CriticalMomentGroundingRejection::UnexpectedIntentHypothesis
            }
            Self::TooLong
            | Self::UnknownMarker
            | Self::RepeatedMarker(_)
            | Self::BareFigure
            | Self::MisplacedMarker(_)
            | Self::UngroundedChessLiteral
            | Self::InternalVocabulary
            | Self::InternalIdentifier
            | Self::ForbiddenNeutralLiteral
            | Self::LearningResource => CriticalMomentGroundingRejection::ChangedFact,
        }
    }
}

/// The comment gate, resolving its failures at full width. See
/// [`ground_hosted_comment_text`] for the narrowed form every Player-facing
/// seam uses.
pub fn diagnose_hosted_comment_text(
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
    text: &str,
) -> Result<GroundedCommentDraft, CommentProseRejection> {
    if !json_text_bytes_within_limit(text, ReviewSessionLimits::V1.max_player_message_bytes) {
        return Err(CommentProseRejection::TooLong);
    }
    if text.contains('\n') || text.contains('\r') {
        return Err(CommentProseRejection::MultiParagraph);
    }
    let policy = CommentFactsPolicy::for_facts(facts);
    let authored = policy.markers.ground(text).map_err(marker_rejection)?;
    let grounding_ledger = policy.ledger_from(facts, &authored.markers)?;
    if !chess_literal_grounding_for(facts, intent).validate(&authored.authored) {
        return Err(CommentProseRejection::UngroundedChessLiteral);
    }

    let text = authored.text;
    if contains_internal_player_facing_text(&text) {
        return Err(CommentProseRejection::InternalVocabulary);
    }
    if contains_internal_identifier(&text) {
        return Err(CommentProseRejection::InternalIdentifier);
    }
    if policy
        .forbidden_literals
        .iter()
        .any(|literal| text.to_ascii_lowercase().contains(literal))
    {
        return Err(CommentProseRejection::ForbiddenNeutralLiteral);
    }
    let text = present_intent(text, facts, intent)?;
    learning_grounding::validate(&text, facts)
        .map_err(|_| CommentProseRejection::LearningResource)?;
    Ok(GroundedCommentDraft {
        comment: CriticalMomentComment { text },
        grounding_ledger,
    })
}

fn marker_rejection(violation: MarkerViolation) -> CommentProseRejection {
    match violation {
        MarkerViolation::MissingRequiredMarker(marker) => {
            CommentProseRejection::MissingRequiredMarker(marker)
        }
        MarkerViolation::UnknownMarker => CommentProseRejection::UnknownMarker,
        MarkerViolation::RepeatedMarker(marker) => CommentProseRejection::RepeatedMarker(marker),
        MarkerViolation::BareFigure => CommentProseRejection::BareFigure,
        MarkerViolation::MisplacedMarker(marker) => CommentProseRejection::MisplacedMarker(marker),
    }
}

/// Coach Skill admission validates the ordered complete Draft Game Review
/// against the same policy the Web gate uses, and returns the same substituted
/// comment, so no surface can publish a draft's raw marker form.
pub fn validate_review_moment_comment(
    text: &str,
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
) -> Result<CriticalMomentComment, CriticalMomentGroundingRejection> {
    ground_hosted_comment_text(facts, intent, text).map(|grounded| grounded.comment)
}

pub fn validate_emitted_comment(
    comment: &CriticalMomentComment,
) -> Result<(), CriticalMomentGroundingRejection> {
    if comment.text.trim().is_empty() || exposes_internal_intent_presentation(&comment.text) {
        Err(CriticalMomentGroundingRejection::ChangedFact)
    } else {
        Ok(())
    }
}

/// Whether a recorded ledger is one this moment's facts could have produced.
///
/// Equality with the canonical set is the wrong test now that the ledger is
/// derived from the markers a comment used: two admissible comments about the
/// same moment legitimately assert different optional claims. What must hold is
/// that every required claim is there and no claim is invented — which is also
/// what the safe rendering satisfies, since it asserts all of them.
pub fn admissible_grounding_ledger(
    facts: &ReviewMomentCommentFacts,
    ledger: &CriticalMomentGroundingLedger,
) -> bool {
    let policy = CommentFactsPolicy::for_facts(facts);
    ledger.facts_ref == artifact_digest(facts)
        && !ledger.factual_claims.is_empty()
        && ledger
            .factual_claims
            .iter()
            .all(|claim| policy.claims.contains(claim))
        && policy
            .required_claims()
            .iter()
            .all(|claim| ledger.factual_claims.contains(claim))
}

pub fn grounding_ledger_for(facts: &ReviewMomentCommentFacts) -> CriticalMomentGroundingLedger {
    let policy = CommentFactsPolicy::for_facts(facts);
    CriticalMomentGroundingLedger {
        facts_ref: artifact_digest(facts),
        factual_claims: policy.claims,
    }
}

pub fn intent_authoring_context_for(
    facts: &ReviewMomentCommentFacts,
    enrichment: Option<IntentEnrichment>,
) -> Option<CriticalMomentIntentAuthoringContext> {
    let instructions = match facts {
        ReviewMomentCommentFacts::Improvement { .. } => {
            CriticalMomentIntentAuthoringInstructions {
                hypothesis: "State exactly one plausible plan in a single hedged sentence that also names it as a plan. Describe the plan by what the pieces do — which piece, where it goes, what it threatens or defends — not by reciting the move list. When enrichment is unavailable, infer a reasonable possibility from the played move and grounded facts.".to_string(),
                counterplay: "Contrast Objective Counterplay as the strongest response that may disrupt the Projected Plan. Describe it the same way — the responding piece and what it threatens, not the move list.".to_string(),
            }
        }
        ReviewMomentCommentFacts::Positive { .. } => {
            CriticalMomentIntentAuthoringInstructions {
                hypothesis: "State exactly one plausible plan in a single hedged sentence that also names it as a plan. Describe the plan by what the pieces do — which piece, where it goes, what it threatens or defends — not by reciting the move list. Connect it to the grounded achievement; when enrichment is unavailable, infer a reasonable possibility from the played move and grounded facts.".to_string(),
                counterplay: "Describe Objective Counterplay only as strongest defense. Preserve the grounded achievement and never imply that the Player missed or miscalculated it.".to_string(),
            }
        }
        ReviewMomentCommentFacts::Neutral { .. } => return None,
    };
    Some(CriticalMomentIntentAuthoringContext {
        enrichment,
        instructions,
    })
}

pub(crate) struct CommentFactsPolicy {
    /// Which moment path this policy came from. Carried because one marker
    /// name now spans two paths: `{takeaway}` asserts the positive takeaway on
    /// one and the improvement takeaway on the other, and the ledger has to
    /// say which.
    path: CommentPath,
    /// Every claim this moment's facts support. A comment may assert any
    /// subset that includes the required ones; it may never assert a claim
    /// outside it.
    ///
    /// Derived from the vocabulary rather than accumulated beside it. A marker
    /// *is* the claim it asserts — [`marker_claim`] is that mapping and the
    /// only one — so a hand-kept list beside the vocabulary is the same fact
    /// written twice, and the failure mode is silent: a marker offered without
    /// its claim pushed is a comment refused for `ClaimOutsideFacts` after the
    /// model did everything right.
    claims: Vec<CriticalMomentFactualClaim>,
    pub(crate) markers: MarkerVocabulary,
    safe_sentences: Vec<String>,
    forbidden_literals: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommentPath {
    Positive,
    Improvement,
    Neutral,
}

/// One marker, one claim. Two markers may assert the same claim — naming the
/// better move and stating where it leaves the evaluation are one correction —
/// and the ledger is a set, so that collapses as it should.
fn marker_claim(path: CommentPath, marker: &str) -> CriticalMomentFactualClaim {
    match marker {
        "takeaway" if path == CommentPath::Improvement => {
            CriticalMomentFactualClaim::ImprovementTakeaway
        }
        "playedMove" => CriticalMomentFactualClaim::PlayedMove,
        "playedPopularity" => CriticalMomentFactualClaim::PlayedPopularity,
        "opponentResource" => CriticalMomentFactualClaim::OpponentResource,
        "materialVerdict" => CriticalMomentFactualClaim::MaterialVerdict,
        "moveTarget" => CriticalMomentFactualClaim::MoveTarget,
        "playedEval" => CriticalMomentFactualClaim::ImprovementOutcome,
        "betterMove" | "bestEval" => CriticalMomentFactualClaim::ImprovementCorrection,
        "consequence" => CriticalMomentFactualClaim::ImprovementConsequence,
        "decisionCue" => CriticalMomentFactualClaim::ImprovementDecisionCue,
        "grade" => CriticalMomentFactualClaim::PositiveGrade,
        "achievement" => CriticalMomentFactualClaim::PositiveAchievement,
        "difficulty" => CriticalMomentFactualClaim::PositiveDifficulty,
        "takeaway" => CriticalMomentFactualClaim::PositiveTakeaway,
        "reason" => CriticalMomentFactualClaim::NeutralReason,
        "observation" => CriticalMomentFactualClaim::NeutralObservation,
        _ => unreachable!("every marker in a vocabulary carries a claim"),
    }
}

/// The claims a set of markers asserts, in one canonical order.
///
/// Sorted and deduped because a claim set is a set: two markers may assert one
/// claim, and `validate_hosted_grounding_ledger` compares ledgers for equality.
/// Every producer of a claim list goes through here, so the order a Coach App
/// echoes back is the order the gate expects.
fn claims_asserted_by(
    path: CommentPath,
    markers: impl Iterator<Item = &'static str>,
) -> Vec<CriticalMomentFactualClaim> {
    let mut claims = markers
        .map(|marker| marker_claim(path, marker))
        .collect::<Vec<_>>();
    claims.sort_unstable();
    claims.dedup();
    claims
}

impl CommentFactsPolicy {
    /// One path's policy, with its claims read off the vocabulary it just
    /// built. The three arms of [`Self::for_facts`] differ in what they offer
    /// and agree on everything after it, so this is where they converge.
    fn assembled(
        path: CommentPath,
        markers: MarkerVocabulary,
        safe_sentences: Vec<String>,
        forbidden_literals: Vec<&'static str>,
    ) -> Self {
        let claims = claims_asserted_by(path, markers.entries().map(|(marker, _, _)| marker));
        Self {
            path,
            claims,
            markers,
            safe_sentences,
            forbidden_literals,
        }
    }

    /// The ledger the comment earned: the claims its markers assert.
    ///
    /// This is the check the old ledger could not make. It compared a computed
    /// set against a computed set, so the model asserted nothing checkable;
    /// the markers used are an assertion, and they must land inside the claims
    /// the facts support.
    fn required_claims(&self) -> Vec<CriticalMomentFactualClaim> {
        claims_asserted_by(self.path, self.markers.required_markers().iter().copied())
    }

    fn ledger_from(
        &self,
        facts: &ReviewMomentCommentFacts,
        markers: &[&'static str],
    ) -> Result<CriticalMomentGroundingLedger, CommentProseRejection> {
        let factual_claims = claims_asserted_by(self.path, markers.iter().copied());
        if factual_claims.is_empty() {
            return Err(CommentProseRejection::MissingRequiredClaim);
        }
        if factual_claims
            .iter()
            .any(|claim| !self.claims.contains(claim))
        {
            return Err(CommentProseRejection::ClaimOutsideFacts);
        }
        if self
            .required_claims()
            .iter()
            .any(|claim| !factual_claims.contains(claim))
        {
            return Err(CommentProseRejection::MissingRequiredClaim);
        }
        Ok(CriticalMomentGroundingLedger {
            facts_ref: artifact_digest(facts),
            factual_claims,
        })
    }

    pub(crate) fn for_facts(facts: &ReviewMomentCommentFacts) -> Self {
        let moment = facts.moment();
        match facts {
            ReviewMomentCommentFacts::Positive { .. } => {
                let GameReviewMomentClassification::PositiveHighlight {
                    qualification,
                    grade,
                } = &moment.classification
                else {
                    unreachable!("tagged facts were validated")
                };
                let achievement = positive_achievement_text(&qualification.achievements[0]);
                let grade_value = *grade;
                let grade = positive_grade_text(grade_value);
                let difficulty = positive_difficulty_text(qualification, grade_value);
                let takeaway = teaching_takeaway(moment);
                let opening = format!("{grade}: {} {achievement}.", moment.played_san);
                let outcome = played_outcome_sentence(moment);
                let mut safe_sentences = vec![opening, difficulty.sentence.clone(), outcome];
                let mut markers = MarkerVocabulary::default();
                markers.require_literal("playedMove", moment.played_san.clone());
                markers.require("grade", positive_grade_marker_text(grade_value));
                markers.require_own_sentence("achievement", achievement_sentence(&achievement));
                markers.require_shaped("difficulty", difficulty.sentence, difficulty.clause);
                if let Some(takeaway) = takeaway {
                    markers.offer("takeaway", takeaway_marker_text(&takeaway));
                    safe_sentences.push(takeaway);
                }
                markers.offer_available("playedPopularity", played_popularity_text(moment));
                offer_tactical_facts(&mut markers, moment);
                Self::assembled(CommentPath::Positive, markers, safe_sentences, vec![])
            }
            ReviewMomentCommentFacts::Improvement { .. } => {
                let GameReviewMomentClassification::ImprovementOpportunity { correction } =
                    &moment.classification
                else {
                    unreachable!("tagged facts were validated")
                };
                let consequence = residual_consequence_text(moment.residual_outcome.classification);
                let opening = format!(
                    "Improvement: After {}, the evaluation is {} — {}; {consequence}.",
                    moment.played_san,
                    moment.display.played_evaluation.score,
                    moment.display.played_evaluation.label
                );
                let correction_text = improvement_correction_text(moment, correction);
                let cue = format!(
                    "Before committing here, calculate {} first.",
                    correction.better_move_san
                );
                let mut markers = MarkerVocabulary::default();
                markers.require_literal("playedMove", moment.played_san.clone());
                markers.require("playedEval", played_outcome_marker_text(moment));
                markers.require_literal("betterMove", correction.better_move_san.clone());
                markers.require(
                    "bestEval",
                    improvement_correction_marker_text(moment, correction),
                );
                markers.require("consequence", consequence);
                markers.require_shaped(
                    "decisionCue",
                    cue.clone(),
                    decision_cue_clause(&correction.better_move_san),
                );
                markers.offer_available("playedPopularity", played_popularity_text(moment));
                offer_tactical_facts(&mut markers, moment);
                let mut safe_sentences = vec![opening, correction_text, cue];
                if let Some(takeaway) = teaching_takeaway(moment) {
                    markers.offer("takeaway", takeaway_marker_text(&takeaway));
                    safe_sentences.push(takeaway);
                }
                Self::assembled(CommentPath::Improvement, markers, safe_sentences, vec![])
            }
            ReviewMomentCommentFacts::Neutral { .. } => {
                let GameReviewMomentClassification::Neutral { reasons } = &moment.classification
                else {
                    unreachable!("tagged facts were validated")
                };
                let reasons = reasons
                    .iter()
                    .map(|reason| neutral_reason_text(*reason))
                    .collect::<Vec<_>>()
                    .join(" and ");
                let observation = played_outcome_sentence(moment);
                let observation_clause = played_outcome_clause(moment);
                let opening = format!("Neutral: {}.", moment.played_san);
                let mut markers = MarkerVocabulary::default();
                markers.require_literal("playedMove", moment.played_san.clone());
                markers.require("reason", reasons.clone());
                markers.require_shaped("observation", observation.clone(), observation_clause);
                // No `offer_tactical_facts` here; the reason is on that function.
                markers.offer_available("playedPopularity", played_popularity_text(moment));
                Self::assembled(
                    CommentPath::Neutral,
                    markers,
                    vec![
                        opening,
                        format!("This move is neutral because {reasons}."),
                        observation,
                    ],
                    vec![
                        "better move",
                        "correction",
                        "lesson",
                        "takeaway",
                        "decision cue",
                        "great move",
                        "good move",
                    ],
                )
            }
        }
    }
}

/// What the moment's tactical line says beyond the move that opened it: what
/// the opponent can answer with, what the line is finally worth, and what the
/// move the Player is told about takes or hits.
///
/// The three are what separate the two tactical paths from Neutral, so it is
/// one call rather than three gated the same way. None is offered on the
/// Neutral path: that moment earns one line and its marker set is deliberately
/// the smallest of the three, so an optional clause inviting a second sentence
/// works against the only length rule it has -- and a Neutral moment has no
/// tactical line for a verdict to be about. The vocabulary carries the absence:
/// an unoffered marker is an unknown marker, not an empty substitution.
fn offer_tactical_facts(markers: &mut MarkerVocabulary, moment: &GameReviewCriticalMoment) {
    markers.offer_available("opponentResource", opponent_resource_text(moment));
    markers.offer_available("materialVerdict", material_verdict_text(moment));
    markers.offer_available("moveTarget", move_target_text(moment));
}

fn safe_rendered(
    facts: &ReviewMomentCommentFacts,
    intent: Option<CriticalMomentIntentAuthoringContext>,
    generation_contract: CriticalMomentCommentGenerationContract,
    reason: CriticalMomentGroundingRejection,
    retried: bool,
) -> GroundedCriticalMomentComment {
    let ledger = grounding_ledger_for(facts);
    grounded(
        safely_rendered_comment(facts, intent),
        generation_contract,
        ledger,
        CriticalMomentCommentGenerationOutcome::SafeRendered {
            attempts: 2,
            reason,
            retried,
        },
    )
}

pub(crate) fn safely_rendered_comment(
    facts: &ReviewMomentCommentFacts,
    intent: Option<CriticalMomentIntentAuthoringContext>,
) -> CriticalMomentComment {
    let mut sentences = CommentFactsPolicy::for_facts(facts).safe_sentences;
    if let Some(intent) = intent {
        sentences.insert(1, safe_intent_sentence(facts, &intent));
    }
    CriticalMomentComment {
        text: sentences.join(" "),
    }
}

fn grounded(
    comment: CriticalMomentComment,
    generation_contract: CriticalMomentCommentGenerationContract,
    grounding_ledger: CriticalMomentGroundingLedger,
    outcome: CriticalMomentCommentGenerationOutcome,
) -> GroundedCriticalMomentComment {
    GroundedCriticalMomentComment {
        comment,
        authoring_provenance: CriticalMomentCommentAuthoringProvenance {
            generation_contract,
            grounding_ledger,
            outcome,
            coaching_profile_projection: CoachingProfileProjection::cold_start(),
            served_endpoint: None,
            served_region: None,
            routed_service_tier: None,
        },
        quality_captures: Vec::new(),
    }
}

/// The intent guess, checked where the model wrote one and supplied where it
/// did not.
///
/// Guessing wrong is the failure this guards, so asserting a purpose and
/// guessing twice both still refuse. Guessing *not at all* is a different
/// thing: every fact is grounded, nothing is overclaimed, and the only
/// shortfall is a sentence the runtime can write itself — the same string
/// [`safe_intent_sentence`] gives the deterministic rendering. Refusing here
/// sent the whole moment to that rendering, so the Player lost a paragraph of
/// the model's prose to gain a sentence they were getting either way.
///
/// Writing it instead is how every other fact reaches the Player: the runtime
/// renders, and the model is only ever asked for the words around it. The
/// guess asserts no claim, so the grounding ledger does not move.
fn present_intent(
    text: String,
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
) -> Result<String, CommentProseRejection> {
    if asserts_authoritative_intent(&text) {
        return Err(CommentProseRejection::AuthoritativeIntent);
    }
    if exposes_internal_intent_presentation(&text) {
        return Err(CommentProseRejection::InternalIntentDisclosure);
    }
    let hypothesis_count = text
        .split(['.', '!', '?', ';'])
        .filter(|sentence| is_uncertain_intent_sentence(sentence))
        .count();
    match (intent, hypothesis_count) {
        (Some(intent), 0) => Ok(append_sentence(text, &safe_intent_sentence(facts, intent))),
        (Some(_), 1) | (None, 0) => Ok(text),
        (Some(_), _) => Err(CommentProseRejection::MultipleIntentClaims),
        (None, _) => Err(CommentProseRejection::UnexpectedIntentHypothesis),
    }
}

fn is_uncertain_intent_sentence(sentence: &str) -> bool {
    let lowercase = sentence.to_ascii_lowercase();
    let uncertain = [
        "best guess",
        "may have",
        "might have",
        "perhaps",
        "possibly",
        "likely",
    ]
    .iter()
    .any(|marker| lowercase.contains(marker));
    let intent = ["aim", "plan", "idea", "intend", "expect"]
        .iter()
        .any(|marker| lowercase.contains(marker));
    uncertain && intent
}

/// The allowlist this moment grounds, gate and prompt sharing one derivation.
///
/// The prompt shows the model exactly this list, so building it twice would
/// mean the model could be offered a literal the gate then rejects.
pub(crate) fn chess_literal_grounding_for(
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
) -> ChessLiteralGrounding {
    let mut grounding = review_moment_chess_literals(facts.moment());
    if let Some(enrichment) = intent.and_then(|context| context.enrichment.as_ref()) {
        grounding.allow_moves_san(&enrichment.projected_plan_san);
        grounding.allow_moves_san(&enrichment.objective_counterplay_san);
    }
    grounding
}

/// Every move and square this moment's facts ground, projected deliberately.
///
/// The projection is a prompt input — the model is shown exactly this list —
/// so its shape joins the prompt digest. It adds chess facts to what the model
/// sees and no Player data, so the minimization rule is untouched.
pub(crate) fn review_moment_chess_literals(
    moment: &GameReviewCriticalMoment,
) -> ChessLiteralGrounding {
    let mut grounding = ChessLiteralGrounding::empty();
    grounding.allow_move_san(&moment.played_san);
    grounding.allow_uci_squares(&moment.objective.played_move_uci);
    grounding.allow_uci_squares(&moment.objective.best_move_uci);
    // The principal variation is stored as UCI, which no coach speaks and the
    // gate separately bans. Its SAN lives on the display lines, so the engine
    // line becomes quotable for the first time.
    grounding.allow_uci_squares_all(&moment.objective.principal_variation);
    if let Some(lines) = &moment.objective.lines {
        for line_move in lines.best.iter().chain(&lines.refutation) {
            grounding.allow_move_san(&line_move.san);
        }
        // The square an opponent's reply hits is a square no line has to land
        // on, so it reaches the allowlist through the fact that names it and no
        // other way — and only the fact the Player is actually told, so the
        // permission and the claim have the same width.
        if let Some(resource) = lines.opponent_resource() {
            allow_effect_square(&mut grounding, resource.does);
        }
    }
    // The square a target stands on, by the same rule. A better move's target
    // is a square its line need not land on, so it reaches the allowlist only
    // through the fact the Player is told; the played move's is admitted again
    // below with the rest of its effects, and a set does not mind.
    if let Some(target) = moment.move_target() {
        grounding.allow_square(target.square());
    }
    if let Some(mechanism) = &moment.mechanism {
        for line_move in &mechanism.moves {
            grounding.allow_move_san(&line_move.san);
        }
    }
    grounding.allow_uci_squares(&moment.human.most_likely_move_uci);
    allow_effect_squares(&mut grounding, &moment.effects);
    match &moment.classification {
        GameReviewMomentClassification::ImprovementOpportunity { correction } => {
            grounding.allow_move_san(&correction.better_move_san);
            grounding.allow_uci_squares(&correction.better_move_uci);
        }
        GameReviewMomentClassification::PositiveHighlight { qualification, .. } => {
            for achievement in &qualification.achievements {
                match achievement {
                    PositiveHighlightAchievement::CapturedPiece { square, .. } => {
                        grounding.allow_square(square)
                    }
                    PositiveHighlightAchievement::AdvancedPassedPawn { to_square } => {
                        grounding.allow_square(to_square)
                    }
                    PositiveHighlightAchievement::CompletedCheckmate
                    | PositiveHighlightAchievement::TacticalPayoff { .. } => {}
                }
            }
        }
        GameReviewMomentClassification::Neutral { .. } => {}
    }
    grounding
}

/// Admits the squares a derived effect names. Every effect that names one is a
/// fact about a piece standing there, so permitting the square and stating the
/// fact are the same act.
fn allow_effect_squares(
    grounding: &mut ChessLiteralGrounding,
    effects: &[GameReviewPlayedMoveEffect],
) {
    for effect in effects {
        allow_effect_square(grounding, effect);
    }
}

fn allow_effect_square(grounding: &mut ChessLiteralGrounding, effect: &GameReviewPlayedMoveEffect) {
    match effect {
        GameReviewPlayedMoveEffect::CapturedPiece { square, .. }
        | GameReviewPlayedMoveEffect::AttackedPiece { square, .. } => {
            grounding.allow_square(square)
        }
        GameReviewPlayedMoveEffect::AdvancedPassedPawn { to_square } => {
            grounding.allow_square(to_square)
        }
        GameReviewPlayedMoveEffect::AllowsQueenExchange => {}
    }
}

/// Internal vocabulary that must never reach a Player.
///
/// A surviving brace is the load-bearing one: after substitution it means
/// substitution failed, and that comment must not ship. The human move model
/// is the other: Maia is an internal name, the Player-facing phrasing is
/// "players at your rating", and naming the model is a rejection rather than a
/// style miss.
pub(crate) fn contains_internal_player_facing_text(text: &str) -> bool {
    let lowercase = text.to_ascii_lowercase();
    text.contains('{')
        || text.contains('}')
        || lowercase.contains("grounded correction")
        || [
            "human model",
            "move model",
            "maia",
            "human-likely",
            "human likely",
        ]
        .iter()
        .any(|phrase| lowercase.contains(phrase))
        || contains_analyzed_score(text)
        || contains_raw_uci(text)
}

/// Machine spellings the projection hands the model as reasoning input, and
/// which it must never repeat back.
///
/// The facts carry ~30 of them across eleven paths — `occupyTheCenter`,
/// `advantageLost`, `lowProbabilityRelativeToTopMove` — and no other gate sees
/// them: they are not notation, not figures, and not the human-model
/// vocabulary. Rather than enumerate the enums, which is the drift a single
/// derivation exists to avoid, this keys on the *shape* of an identifier: a
/// lowercase letter immediately followed by an uppercase one. Nothing to
/// maintain, and a variant added tomorrow is caught the day it appears — as is
/// one the model invents but the facts never carried.
///
/// Verified against every SAN token the corpus produces: `Nxd4`, `R2xf5`,
/// `O-O-O` and `c1=Q+` all pass, because notation never puts a capital after a
/// lowercase letter. Substituted marker renderings pass for the same reason.
///
/// URL tokens are exempt, and not as a convenience: resource ids are camelCase
/// by convention (`lichess.org/training/hangingPiece`), and reproducing an
/// exact `LEARNING_MATERIAL` line is the one place a comment may carry a URL at
/// all. Nothing is lost by skipping them, because every URL is separately held
/// to being one of the lines the facts admit.
pub(crate) fn contains_internal_identifier(text: &str) -> bool {
    text.split_whitespace()
        .filter(|token| !token.contains("://"))
        .any(|token| {
            token
                .as_bytes()
                .windows(2)
                .any(|pair| pair[0].is_ascii_lowercase() && pair[1].is_ascii_uppercase())
        })
}

fn contains_analyzed_score(text: &str) -> bool {
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    tokens.windows(2).any(|pair| {
        let score = pair[1].trim_matches(|character: char| {
            !character.is_ascii_alphanumeric() && !matches!(character, '+' | '-' | '#' | '.')
        });
        pair[0]
            .trim_matches(|character: char| !character.is_ascii_alphabetic())
            .eq_ignore_ascii_case("analyzed")
            && score.chars().next().is_some_and(|character| {
                character.is_ascii_digit() || matches!(character, '+' | '-' | '#')
            })
    })
}

fn contains_raw_uci(text: &str) -> bool {
    text.split_whitespace()
        .map(|token| token.trim_matches(|character: char| !character.is_ascii_alphanumeric()))
        .any(is_raw_uci)
}

fn is_raw_uci(token: &str) -> bool {
    let bytes = token.as_bytes();
    matches!(bytes.len(), 4 | 5)
        && matches!(bytes[0], b'a'..=b'h')
        && matches!(bytes[1], b'1'..=b'8')
        && matches!(bytes[2], b'a'..=b'h')
        && matches!(bytes[3], b'1'..=b'8')
        && (bytes.len() == 4 || matches!(bytes[4], b'q' | b'r' | b'b' | b'n'))
}

fn asserts_authoritative_intent(text: &str) -> bool {
    let lowercase = text.to_ascii_lowercase();
    [
        "you intended ",
        "you wanted to ",
        "you were trying to ",
        "your intent is ",
        "your intent was ",
        "your plan was ",
        "you definitely ",
        "you clearly ",
    ]
    .iter()
    .any(|phrase| lowercase.contains(phrase))
}

fn exposes_internal_intent_presentation(text: &str) -> bool {
    [
        "Intent Hypothesis",
        "Intent Selection Trace",
        "intentSelectionTrace",
        "sha256:",
        "Was that your idea",
        "What were you trying to achieve",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

pub(crate) fn artifact_digest(value: &impl Serialize) -> ArtifactDigest {
    let bytes = serde_json_canonicalizer::to_vec(value)
        .expect("Review Moment grounding values have a canonical JSON form");
    ArtifactDigest::try_from(format!("sha256:{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 is a valid artifact digest")
}

#[cfg(test)]
mod rejection_serialization_tests {
    use super::*;

    /// The bake-off record's `rejection` field is read by jq, never
    /// deserialized, so a shape change fails as zero matches rather than as an
    /// error. Both shapes are locked here, and `RECORD_VERSION` moves with them.
    #[test]
    fn a_marker_rejection_serializes_beside_the_marker_it_names() {
        assert_eq!(
            serde_json::to_value(CommentProseRejection::MissingRequiredMarker("playedMove"))
                .expect("a rejection serializes"),
            serde_json::json!({ "missingRequiredMarker": "playedMove" })
        );
        assert_eq!(
            serde_json::to_value(CommentProseRejection::BareFigure)
                .expect("a rejection serializes"),
            serde_json::json!("bareFigure")
        );
    }
}

#[cfg(test)]
mod staleness_tests {
    use super::*;

    fn digest_of(seed: &str) -> ArtifactDigest {
        ArtifactDigest::try_from(format!("sha256:{:x}", Sha256::digest(seed.as_bytes())))
            .expect("SHA-256 is a valid artifact digest")
    }

    fn grounding_ledger() -> CriticalMomentGroundingLedger {
        CriticalMomentGroundingLedger {
            facts_ref: digest_of("facts"),
            factual_claims: Vec::new(),
        }
    }

    /// Provenance shaped like the engine's own hosted Language Layer writes it,
    /// with the two template digests left to the caller.
    fn engine_authored(
        prompt_digest: ArtifactDigest,
        response_schema_digest: ArtifactDigest,
    ) -> CriticalMomentCommentAuthoringProvenance {
        CriticalMomentCommentAuthoringProvenance {
            generation_contract: CriticalMomentCommentGenerationContract {
                code_revision: format!("chen-chess-coach-engine/{}", env!("CARGO_PKG_VERSION")),
                candidate: CriticalMomentExplainerCandidate::new(
                    "openrouter".to_string(),
                    "test-model".to_string(),
                    "test-catalogue".to_string(),
                    prompt_digest,
                    response_schema_digest,
                ),
                settings: CriticalMomentGenerationSettings {
                    randomness: CriticalMomentGenerationRandomness::LowestSupported,
                    stable_seed: Some(0),
                    seed_supported: true,
                    max_output_tokens: 512,
                },
            },
            grounding_ledger: grounding_ledger(),
            outcome: CriticalMomentCommentGenerationOutcome::Authored { attempts: 1 },
            coaching_profile_projection: CoachingProfileProjection::cold_start(),
            served_endpoint: None,
            served_region: None,
            routed_service_tier: None,
        }
    }

    /* Each of these asserts the contrast rather than the digest comparison on
    its own: prose is current until exactly one digest moves. Asserting only
    that the compiled pair reads as current would restate the predicate, and
    asserting only that a moved digest reads as stale would still pass if
    everything read as stale — which is the expensive failure, because it
    rewrites every comment on every open forever. */
    #[test]
    fn a_web_artifact_is_current_until_the_comment_prompt_moves() {
        let current = engine_authored(
            compiled_comment_prompt_digest(),
            compiled_comment_schema_digest(),
        );
        let edited = engine_authored(
            digest_of("the comment prompt as it read before the edit"),
            compiled_comment_schema_digest(),
        );

        assert!(!current.is_stale_web_artifact());
        assert!(edited.is_stale_web_artifact());
    }

    #[test]
    fn a_web_artifact_is_current_until_the_response_schema_moves() {
        let current = engine_authored(
            compiled_comment_prompt_digest(),
            compiled_comment_schema_digest(),
        );
        let edited = engine_authored(
            compiled_comment_prompt_digest(),
            digest_of("the response schema as it read before the edit"),
        );

        assert!(!current.is_stale_web_artifact());
        assert!(edited.is_stale_web_artifact());
    }

    /// The Coach App's own prose carries fixed placeholder digests that never
    /// match the compiled pair, so the host-submitted guard is what keeps it
    /// from being regenerated on every open.
    #[test]
    fn host_submitted_prose_is_never_stale_against_the_engine_template() {
        let provenance =
            CriticalMomentCommentAuthoringProvenance::hosted_authored(grounding_ledger(), 1);

        assert!(provenance.is_host_submitted());
        assert!(!provenance.is_stale_web_artifact());
    }
}
