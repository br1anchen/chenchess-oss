use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    canonical_sha256, ArtifactDigest, GameReviewCriticalMoment, GameReviewMomentClassification,
};

/// The only facts boundary accepted by Review Moment comment authoring.
///
/// Keeping the classification tag beside its source moment means every
/// downstream consumer can use one typed dispatch instead of attempting to
/// infer a teaching mode from optional fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReviewMomentCommentFacts {
    Positive { moment: GameReviewCriticalMoment },
    Improvement { moment: GameReviewCriticalMoment },
    Neutral { moment: GameReviewCriticalMoment },
}

impl ReviewMomentCommentFacts {
    pub fn try_from_moment(
        moment: GameReviewCriticalMoment,
    ) -> Result<Self, ReviewMomentCommentFactsError> {
        if moment.comment.is_some() {
            return Err(ReviewMomentCommentFactsError::AlreadyAuthored);
        }
        if !moment.classification.is_well_formed() {
            return Err(ReviewMomentCommentFactsError::MalformedClassification);
        }
        Ok(match &moment.classification {
            GameReviewMomentClassification::PositiveHighlight { .. } => Self::Positive { moment },
            GameReviewMomentClassification::ImprovementOpportunity { .. } => {
                Self::Improvement { moment }
            }
            GameReviewMomentClassification::Neutral { .. } => Self::Neutral { moment },
        })
    }

    /// Presentation validation uses the imported facts after a comment has
    /// already been attached. Authoring still uses `try_from_moment`, which
    /// rejects that state to prevent re-authoring a Player-visible comment.
    pub fn try_from_presented_moment(
        mut moment: GameReviewCriticalMoment,
    ) -> Result<Self, ReviewMomentCommentFactsError> {
        moment.comment = None;
        Self::try_from_moment(moment)
    }

    pub fn moment(&self) -> &GameReviewCriticalMoment {
        match self {
            Self::Positive { moment } | Self::Improvement { moment } | Self::Neutral { moment } => {
                moment
            }
        }
    }

    pub fn is_well_formed(&self) -> bool {
        (match self {
            Self::Positive { moment } => matches!(
                moment.classification,
                GameReviewMomentClassification::PositiveHighlight { .. }
            ),
            Self::Improvement { moment } => matches!(
                moment.classification,
                GameReviewMomentClassification::ImprovementOpportunity { .. }
            ),
            Self::Neutral { moment } => matches!(
                moment.classification,
                GameReviewMomentClassification::Neutral { .. }
            ),
        }) && self.moment().comment.is_none()
            && self.moment().classification.is_well_formed()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReviewMomentCommentFactsError {
    #[error("Review Moment facts already contain an authored comment")]
    AlreadyAuthored,
    #[error("Review Moment classification facts are malformed")]
    MalformedClassification,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriticalMomentComment {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriticalMomentCommentDraft {
    pub text: String,
    pub grounding_ledger: CriticalMomentGroundingLedger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriticalMomentGroundingLedger {
    pub facts_ref: ArtifactDigest,
    pub factual_claims: Vec<CriticalMomentFactualClaim>,
}

/// Complete immutable authority a host Language Layer needs to author one
/// Review Moment comment without recreating the Review Engine's grounding policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewMomentCommentAuthoringContext {
    pub facts: ReviewMomentCommentFacts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<CriticalMomentIntentAuthoringContext>,
    pub required_grounding_ledger: CriticalMomentGroundingLedger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriticalMomentIntentAuthoringContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<IntentEnrichment>,
    pub instructions: CriticalMomentIntentAuthoringInstructions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentEnrichment {
    pub projected_plan_san: Vec<String>,
    pub objective_counterplay_san: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriticalMomentIntentAuthoringInstructions {
    pub hypothesis: String,
    pub counterplay: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum CriticalMomentFactualClaim {
    PositiveGrade,
    PlayedMove,
    /// How often players at the Player's rating choose the played move. Long
    /// present in the facts, first expressible in a comment through
    /// `{playedPopularity}`.
    PlayedPopularity,
    PositiveAchievement,
    PositiveDifficulty,
    PositiveTakeaway,
    ImprovementOutcome,
    ImprovementConsequence,
    ImprovementCorrection,
    ImprovementDecisionCue,
    /// The moment's teaching theme, in the same words the safe rendering uses.
    /// Long present in the facts as an enum spelling the comment could only
    /// parrot; first expressible on this path through `{takeaway}`.
    ImprovementTakeaway,
    NeutralReason,
    NeutralObservation,
    /// What the opponent's best reply does about the move just played. Long
    /// present in the facts as a line the comment was told not to transcribe;
    /// first expressible through `{opponentResource}`.
    OpponentResource,
    /// What the tactical line settles ahead by, once the piece it won has been
    /// paid for. Long present in the facts as a payoff variant no rendering
    /// reached — the credited one never sits first in the achievement list,
    /// and the missed one belonged to a line nothing narrated; first
    /// expressible through `{materialVerdict}`.
    MaterialVerdict,
    /// The enemy piece a move takes or newly hits. Long present in the facts
    /// as the played move's effects, of which `{achievement}` reads only the
    /// captures, and absent for the better move entirely; first expressible
    /// through `{moveTarget}`.
    MoveTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriticalMomentCommentGenerationContract {
    pub code_revision: String,
    pub candidate: CriticalMomentExplainerCandidate,
    pub settings: CriticalMomentGenerationSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriticalMomentExplainerCandidate {
    pub candidate_ref: ArtifactDigest,
    pub provider: String,
    pub model: String,
    pub model_revision: String,
    pub prompt_digest: ArtifactDigest,
    pub response_schema_digest: ArtifactDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriticalMomentGenerationSettings {
    pub randomness: CriticalMomentGenerationRandomness,
    pub stable_seed: Option<u32>,
    pub seed_supported: bool,
    pub max_output_tokens: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum CriticalMomentGenerationRandomness {
    LowestSupported,
}

impl CriticalMomentCommentGenerationContract {
    pub fn is_reproducible(&self) -> bool {
        !self.code_revision.trim().is_empty()
            && !self.candidate.provider.trim().is_empty()
            && !self.candidate.model.trim().is_empty()
            && !self.candidate.model_revision.trim().is_empty()
            && self.candidate.candidate_ref == self.candidate.computed_ref()
            && self.settings.max_output_tokens > 0
            && matches!(
                (self.settings.seed_supported, self.settings.stable_seed),
                (true, Some(_)) | (false, None)
            )
    }
}

impl CriticalMomentExplainerCandidate {
    pub fn new(
        provider: String,
        model: String,
        model_revision: String,
        prompt_digest: ArtifactDigest,
        response_schema_digest: ArtifactDigest,
    ) -> Self {
        let mut candidate = Self {
            candidate_ref: ArtifactDigest::try_from(format!("sha256:{}", "0".repeat(64)))
                .expect("zero SHA-256 is a valid placeholder"),
            provider,
            model,
            model_revision,
            prompt_digest,
            response_schema_digest,
        };
        candidate.candidate_ref = candidate.computed_ref();
        candidate
    }

    fn computed_ref(&self) -> ArtifactDigest {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct CandidateIdentity<'a> {
            provider: &'a str,
            model: &'a str,
            model_revision: &'a str,
            prompt_digest: &'a ArtifactDigest,
            response_schema_digest: &'a ArtifactDigest,
        }
        ArtifactDigest::try_from(canonical_sha256(&CandidateIdentity {
            provider: &self.provider,
            model: &self.model,
            model_revision: &self.model_revision,
            prompt_digest: &self.prompt_digest,
            response_schema_digest: &self.response_schema_digest,
        }))
        .expect("canonical Explainer Candidate identity has a valid digest")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CriticalMomentCommentGenerationOutcome {
    Authored {
        attempts: u8,
    },
    SafeRendered {
        attempts: u8,
        reason: CriticalMomentGroundingRejection,
        /// Whether an open has already re-authored this moment under the
        /// prompt it carries, and landed on a rendering again.
        ///
        /// A safe rendering is stamped with the compiled digests, so without a
        /// mark of its own it reads as current forever and the Player keeps the
        /// template for the life of the prompt. This is what lets an open retry
        /// once without retrying on every open after. It clears when the digests
        /// move: an edited prompt is a fresh chance, not the same one taken
        /// twice.
        ///
        /// Absent from provenance written before the field existed, which is
        /// the migration -- every stored fallback reads as not-yet-retried and
        /// is owed exactly one attempt.
        ///
        /// What this bounds is repeated *gate rejection*: a moment whose facts
        /// the model cannot write about lands on a rendering, is marked, and is
        /// left alone. It does not bound an open that never gets as far as
        /// writing -- an authoring error, a rejected retry, or a store that
        /// will not take the mutation all leave the stored mark untouched, so
        /// the next open tries again. That is the wanted behaviour for a
        /// passing failure and the wrong one for a permanent one; nothing
        /// currently tells them apart.
        #[serde(default)]
        retried: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum CriticalMomentGroundingRejection {
    /// Retired: prose that never guesses is given the guess rather than
    /// refused, so no gate emits this any more. Staging records from before
    /// that change carry it, so it stays readable.
    MissingUncertainty,
    AuthoritativeIntent,
    MultipleIntentClaims,
    InternalIntentDisclosure,
    ChangedReference,
    MissingFactualClaim,
    ChangedFact,
    InvalidGenerationContract,
    AlreadyAuthoredFacts,
    InvalidClassificationFacts,
    MissingRequiredLiteral,
    MultiParagraph,
    UnexpectedIntentHypothesis,
    ProviderUnavailable,
}
