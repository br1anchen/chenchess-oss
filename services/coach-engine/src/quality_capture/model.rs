use chrono::{DateTime, Months, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use chrono::NaiveDate;

use crate::{
    critical_moment_comment::{
        CommentProseRejection, CriticalMomentCommentAuthoringProvenance, ProseRejectionDiscipline,
    },
    evaluation_fingerprint::{EvaluationFingerprint, EvaluationFingerprintObservations},
    game_import_store::GameImportRecord,
    projected_plan::ProjectedPlanProvenance,
    review_session_contract::{
        ArtifactDigest, CanonicalGameMove, CompletedGameOutcome, CriticalMomentComment, EloRating,
        EvidenceProvenance, GameReview, ImportedGame, PositionRef, ReviewMomentCommentFacts,
        ReviewMomentSelection, ReviewSessionCoreContract, ReviewSide,
    },
};

pub(super) const QUALITY_CAPTURE_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct QualityCaptureId(String);

impl QualityCaptureId {
    fn new() -> Self {
        Self(format!("quality-capture:{}", Uuid::new_v4().simple()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Call-shape facts a Quality Capture may retain. Latency and wall-clock
/// timestamps are excluded on purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LanguageLayerCallShape {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cost_micros: i64,
    pub finish_reason: Option<String>,
    pub attempts: u8,
    pub deadline_hit: bool,
    pub created_on: NaiveDate,
}

/// Bounded, free-text-stripped excerpt of an output-shaped failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct StrippedExcerpt(String);

impl StrippedExcerpt {
    pub(super) const BOUND: usize = 160;

    pub(super) fn new(stripped: String) -> Self {
        let mut excerpt = stripped;
        excerpt.truncate(Self::BOUND);
        Self(excerpt)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum HostedLanguageLayerTask {
    Comment,
    CoachTurn,
    HostTurn,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QualityCaptureDraft {
    pub(crate) schema_version: u8,
    pub(crate) capture_id: QualityCaptureId,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) purge_at: DateTime<Utc>,
    pub(crate) case_key: ArtifactDigest,
    pub(crate) content_digest: ArtifactDigest,
    pub(crate) content: QualityCaptureContent,
}

impl QualityCaptureDraft {
    pub(crate) fn game_analysis(
        record: &GameImportRecord,
    ) -> Result<Self, QualityCaptureBuildError> {
        let engine = record
            .engine_provenance
            .clone()
            .and_then(crate::provider_provenance::stockfish)
            .ok_or(QualityCaptureBuildError)?;
        let normalized_game = QualityNormalizedGame::from(&record.imported_game);
        let content = QualityCaptureContent::GameAnalysis {
            normalized_game,
            review_side: record.imported_game.review_side,
            resolved_elo: record.imported_game.elo_profile.rating,
            result: Box::new(record.frozen_review.clone()),
            reproducibility: GameAnalysisReproducibility { engine },
        };
        Ok(Self::new(record.created_at, content))
    }

    pub(crate) fn coaching_response(
        core: &ReviewSessionCoreContract,
        facts: ReviewMomentCommentFacts,
        generated_response: CriticalMomentComment,
        authoring_provenance: CriticalMomentCommentAuthoringProvenance,
        projected_plan_provenance: Option<ProjectedPlanProvenance>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, QualityCaptureBuildError> {
        if !matches!(
            core.review_moment.selection,
            ReviewMomentSelection::PipelineCriticalMoment { .. }
        ) || core.review_moment.ply != facts.moment().ply
            || !facts.is_well_formed()
            || !authoring_provenance.is_valid_for(&generated_response)
        {
            return Err(QualityCaptureBuildError);
        }
        let normalized_game = QualityNormalizedGame::from(&core.imported_game);
        let normalized_game_digest = canonical_digest(&normalized_game);
        let ply = core.review_moment.ply;
        let content = QualityCaptureContent::CoachingResponse {
            parent_game_digest: normalized_game_digest.clone(),
            ply,
            position_ref: core.position_snapshot.position_ref.clone(),
            action: CoachingActionCategory::from(&facts),
            grounding_facts: Box::new(facts),
            generated_response,
            reproducibility: Box::new(CoachingResponseReproducibility {
                authoring: authoring_provenance,
                projected_plan: projected_plan_provenance,
            }),
        };
        Ok(Self::new(created_at, content))
    }

    pub(crate) fn hosted_language_layer(
        fingerprint: EvaluationFingerprint,
        observations: EvaluationFingerprintObservations,
        call_shape: LanguageLayerCallShape,
        task: HostedLanguageLayerTask,
        failure_excerpt: Option<StrippedExcerpt>,
        rejection: Option<RecordedProseRejection>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self::new(
            created_at,
            QualityCaptureContent::LanguageLayerGeneration {
                fingerprint: Box::new(fingerprint),
                observations,
                call_shape,
                task,
                failure_excerpt,
                rejection,
            },
        )
    }

    /// What a Review Feedback Report needs to point at one generation.
    ///
    /// Consented generations leave the product database as soon as they
    /// export, so feedback cannot be anchored by re-reading their content. A
    /// capture reference and the Evaluation Fingerprint digest are the whole
    /// join, and neither identifies the Player.
    pub(crate) fn feedback_anchor(&self) -> Option<FeedbackAnchor> {
        let QualityCaptureContent::LanguageLayerGeneration { fingerprint, .. } = &self.content
        else {
            return None;
        };
        Some(FeedbackAnchor {
            capture_id: self.capture_id.clone(),
            fingerprint_digest: fingerprint.digest.clone(),
        })
    }

    pub(crate) fn feedback_annotation(
        capture_id: QualityCaptureId,
        fingerprint_digest: ArtifactDigest,
        reason_codes: Vec<ReviewFeedbackReason>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self::new(
            created_at,
            QualityCaptureContent::FeedbackAnnotation {
                capture_id,
                fingerprint_digest,
                reason_codes,
            },
        )
    }

    pub(crate) fn with_feedback_induced_trigger(self) -> Result<Self, QualityCaptureBuildError> {
        let QualityCaptureContent::LanguageLayerGeneration {
            fingerprint,
            mut observations,
            call_shape,
            task,
            failure_excerpt,
            rejection,
        } = self.content
        else {
            return Err(QualityCaptureBuildError);
        };
        observations.capture_trigger =
            crate::evaluation_fingerprint::CaptureTrigger::FeedbackInduced;
        let mut induced = Self::new(
            self.created_at,
            QualityCaptureContent::LanguageLayerGeneration {
                fingerprint,
                observations,
                call_shape,
                task,
                failure_excerpt,
                rejection,
            },
        );
        induced.capture_id = self.capture_id;
        Ok(induced)
    }

    fn new(created_at: DateTime<Utc>, content: QualityCaptureContent) -> Self {
        let created_at = created_at
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time")
            .and_utc();
        let purge_at = created_at
            .checked_add_months(Months::new(12))
            .expect("a quality capture timestamp can advance by 12 months");
        let case_key = content.expected_case_key();
        let content_digest = canonical_digest(&CaptureDigestMaterial {
            case_key: &case_key,
            content: &content,
        });
        Self {
            schema_version: QUALITY_CAPTURE_SCHEMA_VERSION,
            capture_id: QualityCaptureId::new(),
            created_at,
            purge_at,
            case_key,
            content_digest,
            content,
        }
    }

    pub(crate) fn has_valid_shape(&self) -> bool {
        Self::material_has_valid_shape(
            self.schema_version,
            self.created_at,
            self.purge_at,
            &self.case_key,
            &self.content_digest,
            &self.content,
        )
    }

    pub(super) fn material_has_valid_shape(
        schema_version: u8,
        created_at: DateTime<Utc>,
        purge_at: DateTime<Utc>,
        case_key: &ArtifactDigest,
        content_digest: &ArtifactDigest,
        content: &QualityCaptureContent,
    ) -> bool {
        schema_version == QUALITY_CAPTURE_SCHEMA_VERSION
            && created_at
                .checked_add_months(Months::new(12))
                .is_some_and(|expected| expected == purge_at)
            && case_key == &content.expected_case_key()
            && content_digest == &canonical_digest(&CaptureDigestMaterial { case_key, content })
            && content.failure_excerpt_within_bound()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum QualityCaptureContent {
    GameAnalysis {
        normalized_game: QualityNormalizedGame,
        review_side: ReviewSide,
        resolved_elo: EloRating,
        result: Box<GameReview>,
        reproducibility: GameAnalysisReproducibility,
    },
    CoachingResponse {
        parent_game_digest: ArtifactDigest,
        ply: u16,
        position_ref: PositionRef,
        action: CoachingActionCategory,
        grounding_facts: Box<ReviewMomentCommentFacts>,
        generated_response: CriticalMomentComment,
        reproducibility: Box<CoachingResponseReproducibility>,
    },
    LanguageLayerGeneration {
        fingerprint: Box<EvaluationFingerprint>,
        observations: EvaluationFingerprintObservations,
        call_shape: LanguageLayerCallShape,
        task: HostedLanguageLayerTask,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failure_excerpt: Option<StrippedExcerpt>,
        /// Why the gate refused this generation, when it refused it for prose.
        ///
        /// Task vocabulary, so it sits beside `task` rather than on the shared
        /// `observations`, which describe the generation identity every hosted
        /// call has. Absent on captures written before this field, and on any
        /// generation the prose gate never judged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rejection: Option<RecordedProseRejection>,
    },
    FeedbackAnnotation {
        capture_id: QualityCaptureId,
        fingerprint_digest: ArtifactDigest,
        reason_codes: Vec<ReviewFeedbackReason>,
    },
}

/// Why the prose gate refused one generation, as stored.
///
/// Discipline and marker on separate axes: the diagnosis this exists for asks
/// "which rule" and "which marker" separately, and a prompt edit that makes one
/// marker unwritable shows up as a discipline count until the marker is beside
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RecordedProseRejection {
    pub(crate) discipline: ProseRejectionDiscipline,
    /// Absent for the disciplines that name no marker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) marker: Option<String>,
}

impl From<CommentProseRejection> for RecordedProseRejection {
    fn from(rejection: CommentProseRejection) -> Self {
        Self {
            discipline: rejection.discipline(),
            marker: rejection.marker().map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FeedbackAnchor {
    pub(crate) capture_id: QualityCaptureId,
    pub(crate) fingerprint_digest: ArtifactDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewFeedbackReason {
    ExplanationHelpful,
    ExplanationNotHelpful,
    ExplanationIncorrect,
    ExplanationUnclear,
    ShouldSelect,
}

impl QualityCaptureContent {
    fn expected_case_key(&self) -> ArtifactDigest {
        match self {
            Self::GameAnalysis {
                normalized_game, ..
            } => case_key(
                canonical_digest(normalized_game),
                QualityTaskKind::GameAnalysis,
                0,
            ),
            Self::CoachingResponse {
                parent_game_digest,
                ply,
                ..
            } => case_key(
                parent_game_digest.clone(),
                QualityTaskKind::CoachingResponse,
                *ply,
            ),
            Self::LanguageLayerGeneration {
                fingerprint,
                observations,
                call_shape,
                task,
                ..
            } => canonical_digest(&LanguageLayerCaseKeyMaterial {
                fingerprint_digest: fingerprint.digest.clone(),
                task: *task,
                created_on: call_shape.created_on,
                capture_outcome: observations.capture_outcome,
            }),
            Self::FeedbackAnnotation { capture_id, .. } => {
                canonical_digest(&FeedbackCaseKeyMaterial {
                    capture_id: capture_id.clone(),
                    task_kind: QualityTaskKind::FeedbackAnnotation,
                })
            }
        }
    }

    fn failure_excerpt_within_bound(&self) -> bool {
        match self {
            Self::LanguageLayerGeneration {
                failure_excerpt: Some(excerpt),
                ..
            } => excerpt.as_str().len() <= StrippedExcerpt::BOUND,
            Self::LanguageLayerGeneration {
                failure_excerpt: None,
                ..
            }
            | Self::GameAnalysis { .. }
            | Self::CoachingResponse { .. }
            | Self::FeedbackAnnotation { .. } => true,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LanguageLayerCaseKeyMaterial {
    fingerprint_digest: ArtifactDigest,
    task: HostedLanguageLayerTask,
    created_on: NaiveDate,
    capture_outcome: crate::evaluation_fingerprint::CaptureOutcome,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FeedbackCaseKeyMaterial {
    capture_id: QualityCaptureId,
    task_kind: QualityTaskKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QualityNormalizedGame {
    outcome: CompletedGameOutcome,
    moves: Vec<CanonicalGameMove>,
    final_position_ref: PositionRef,
}

impl From<&ImportedGame> for QualityNormalizedGame {
    fn from(imported: &ImportedGame) -> Self {
        Self {
            outcome: imported.game.outcome,
            moves: imported.game.moves.clone(),
            final_position_ref: imported.game.final_position_ref.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GameAnalysisReproducibility {
    engine: EvidenceProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CoachingResponseReproducibility {
    authoring: CriticalMomentCommentAuthoringProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    projected_plan: Option<ProjectedPlanProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CoachingActionCategory {
    PositiveHighlight,
    ImprovementOpportunity,
    NeutralExplanation,
}

impl From<&ReviewMomentCommentFacts> for CoachingActionCategory {
    fn from(facts: &ReviewMomentCommentFacts) -> Self {
        match facts {
            ReviewMomentCommentFacts::Positive { .. } => Self::PositiveHighlight,
            ReviewMomentCommentFacts::Improvement { .. } => Self::ImprovementOpportunity,
            ReviewMomentCommentFacts::Neutral { .. } => Self::NeutralExplanation,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum QualityTaskKind {
    GameAnalysis,
    CoachingResponse,
    FeedbackAnnotation,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseKeyMaterial {
    normalized_game_digest: ArtifactDigest,
    task_kind: QualityTaskKind,
    ply: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureDigestMaterial<'a> {
    case_key: &'a ArtifactDigest,
    content: &'a QualityCaptureContent,
}

fn case_key(
    normalized_game_digest: ArtifactDigest,
    task_kind: QualityTaskKind,
    ply: u16,
) -> ArtifactDigest {
    canonical_digest(&CaseKeyMaterial {
        normalized_game_digest,
        task_kind,
        ply,
    })
}

fn canonical_digest(value: &impl Serialize) -> ArtifactDigest {
    let encoded = serde_json_canonicalizer::to_vec(value)
        .expect("quality capture values have an infallible canonical representation");
    let digest = Sha256::digest(encoded);
    ArtifactDigest::try_from(format!("sha256:{digest:x}"))
        .expect("a SHA-256 quality capture digest is valid")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("quality capture content is inconsistent with the completed business result")]
pub(crate) struct QualityCaptureBuildError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        critical_moment_comment::grounding_ledger_for,
        engine_analysis::EngineProvenance,
        review_session_contract::{
            GameImportId, ImportedGame, OperationCompletion, PlayerId, ReviewSessionEvent,
            ReviewSessionEventEnvelope,
        },
        review_session_processor::ProcessorPrincipal,
    };

    #[test]
    fn both_capture_variants_are_identity_free_and_expire_after_twelve_months() {
        let created_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = fixture_import(created_at);
        let analysis = QualityCaptureDraft::game_analysis(&imported).unwrap();
        let mut differently_sourced = imported.clone();
        differently_sourced.imported_game.game.game_ref =
            crate::review_session_contract::GameRef::try_from(format!("sha256:{}", "f".repeat(64)))
                .unwrap();
        let differently_sourced_analysis =
            QualityCaptureDraft::game_analysis(&differently_sourced).unwrap();
        let core: ReviewSessionCoreContract = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/core-contract.json"
        )))
        .unwrap();
        let facts = ReviewMomentCommentFacts::try_from_presented_moment(
            imported
                .frozen_review
                .critical_moments
                .iter()
                .find(|moment| moment.ply == core.review_moment.ply)
                .expect("the fixture core names an imported critical moment")
                .clone(),
        )
        .unwrap();
        let comment = CriticalMomentComment {
            text: "Keep the rook active and meet the threat.".to_string(),
        };
        let mut authoring =
            CriticalMomentCommentAuthoringProvenance::hosted(grounding_ledger_for(&facts), false);
        authoring.served_endpoint = Some("ep-1".to_string());
        authoring.served_region = Some("global".to_string());
        authoring.routed_service_tier = None;
        let coaching = QualityCaptureDraft::coaching_response(
            &core, facts, comment, authoring, None, created_at,
        )
        .unwrap();
        let captured = serde_json::to_value(&coaching).unwrap();
        assert_eq!(
            captured["content"]["reproducibility"]["authoring"]["routedServiceTier"],
            serde_json::Value::Null
        );
        assert_eq!(
            captured["content"]["reproducibility"]["authoring"]["servedEndpoint"],
            "ep-1"
        );
        assert_eq!(
            captured["content"]["reproducibility"]["authoring"]["servedRegion"],
            "global"
        );
        assert!(
            captured["content"]["reproducibility"]["authoring"]
                .as_object()
                .unwrap()
                .contains_key("routedServiceTier"),
            "null tier must be recorded as declared default, not dropped"
        );

        assert!(analysis.has_valid_shape());
        assert!(coaching.has_valid_shape());
        assert_ne!(analysis.case_key, coaching.case_key);
        assert_eq!(
            analysis.case_key, differently_sourced_analysis.case_key,
            "case identity must not depend on the source PGN digest"
        );
        let created_on = created_at
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        assert_eq!(
            analysis.purge_at,
            created_on.checked_add_months(Months::new(12)).unwrap()
        );
        for serialized in [
            serde_json::to_string(&analysis).unwrap(),
            serde_json::to_string(&coaching).unwrap(),
        ] {
            for forbidden in [
                "firebase-player",
                "review-session:",
                "request:",
                "https://lichess.org",
                "synthetic-white",
                "synthetic-white",
                "original pasted PGN",
                imported.imported_game.game.game_ref.as_str(),
            ] {
                assert!(
                    !serialized.contains(forbidden),
                    "capture contains {forbidden}"
                );
            }
            let value: serde_json::Value = serde_json::from_str(&serialized).unwrap();
            assert_forbidden_keys_absent(
                &value,
                &[
                    "gameRef",
                    "requestId",
                    "sessionId",
                    "importProvenance",
                    "providerTimings",
                    "timing",
                    "trace",
                    "white",
                    "black",
                    "event",
                    "site",
                    "url",
                    "pgn",
                ],
            );
        }
    }

    fn assert_forbidden_keys_absent(value: &serde_json::Value, forbidden: &[&str]) {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    assert_forbidden_keys_absent(value, forbidden);
                }
            }
            serde_json::Value::Object(fields) => {
                for (key, value) in fields {
                    assert!(
                        !forbidden.contains(&key.as_str()),
                        "quality capture contains forbidden field {key}"
                    );
                    assert_forbidden_keys_absent(value, forbidden);
                }
            }
            _ => {}
        }
    }

    fn fixture_import(created_at: DateTime<Utc>) -> GameImportRecord {
        let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/events.json"
        )))
        .unwrap();
        let review = events
            .into_iter()
            .find_map(|event| match event.event {
                ReviewSessionEvent::Completed { result } => match *result {
                    OperationCompletion::GameImported { review, .. } => Some(*review),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let snapshot: ImportedGame = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
        )))
        .unwrap();
        GameImportRecord::new(
            GameImportId::try_from("game-import:quality-fixture".to_string()).unwrap(),
            ProcessorPrincipal::Player(PlayerId::try_from("firebase-player".to_string()).unwrap()),
            snapshot,
            review,
            Vec::new(),
            Some(EngineProvenance {
                version: "Stockfish 18".to_string(),
                binary_sha256: "a".repeat(64),
                depth: 16,
                threads: 1,
                hash_mib: 16,
            }),
            created_at,
        )
    }
}
