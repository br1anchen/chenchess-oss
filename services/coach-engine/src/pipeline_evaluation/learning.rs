use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    decision_explanation::validate_decision_explanation,
    review_session_contract::{
        CurriculumLearningConcept, GameReview, LearningTrackKey, LearningTrackSupport,
        LearningTrackSupportBasis,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LearningMatchPurpose {
    Improvement,
    Reinforcement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LearningMatchDisposition {
    Selected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConceptMigrationDisposition {
    DirectlyProvable,
    ScopedRecognition,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningMatchEvaluation {
    pub critical_moment_id: String,
    pub ply: u16,
    pub idea_key: String,
    pub purpose: LearningMatchPurpose,
    pub disposition: LearningMatchDisposition,
    pub concept_disposition: ConceptMigrationDisposition,
    pub proof_family: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("could not evaluate frozen Decision Explanations: {0}")]
pub struct LearningMatchEvaluationError(String);

/// Reads the current durable proof model from a regenerated ladder review.
/// Older review shapes fail rather than invoking a compatibility decoder.
pub fn evaluate_frozen_learning_review(
    review: &Value,
) -> Result<Vec<LearningMatchEvaluation>, LearningMatchEvaluationError> {
    let review = serde_json::from_value::<GameReview>(review.clone())
        .map_err(|error| LearningMatchEvaluationError(error.to_string()))?;
    evaluate_frozen_game_review(&review)
}

/// Evaluates an already-decoded current Game Review without repeating its JSON boundary.
pub fn evaluate_frozen_game_review(
    review: &GameReview,
) -> Result<Vec<LearningMatchEvaluation>, LearningMatchEvaluationError> {
    let mut matches = Vec::new();
    for moment in &review.critical_moments {
        let selected_paths = match &moment.decision_explanation {
            Some(explanation) => {
                validate_decision_explanation(explanation)
                    .map_err(|error| LearningMatchEvaluationError(error.to_string()))?;
                explanation
                    .selected_paths
                    .iter()
                    .map(|path| &path.path_ref)
                    .collect::<std::collections::BTreeSet<_>>()
            }
            None => std::collections::BTreeSet::new(),
        };
        for track in &moment.learning_material.tracks {
            let LearningTrackKey::Curriculum { concept } = track.key else {
                continue;
            };
            let [support] = track.support.as_slice() else {
                return Err(LearningMatchEvaluationError(
                    "moment-local chess-concept track must have exactly one support".to_string(),
                ));
            };
            let (critical_moment_id, ply, purpose, basis) = match support {
                LearningTrackSupport::Improvement {
                    critical_moment_id,
                    ply,
                    basis,
                    ..
                } => (
                    critical_moment_id,
                    *ply,
                    LearningMatchPurpose::Improvement,
                    basis,
                ),
                LearningTrackSupport::Reinforcement {
                    critical_moment_id,
                    ply,
                    basis,
                    ..
                } => (
                    critical_moment_id,
                    *ply,
                    LearningMatchPurpose::Reinforcement,
                    basis,
                ),
            };
            let LearningTrackSupportBasis::DecisionExplanation {
                explanation_path_ref,
            } = basis
            else {
                return Err(LearningMatchEvaluationError(
                    "chess-concept track uses non-Decision support".to_string(),
                ));
            };
            if !selected_paths.contains(explanation_path_ref) {
                return Err(LearningMatchEvaluationError(
                    "chess-concept support references no persisted Explanation Path".to_string(),
                ));
            }
            let idea_key = serialized_string(&concept)?;
            matches.push(LearningMatchEvaluation {
                critical_moment_id: critical_moment_id.as_str().to_string(),
                ply,
                concept_disposition: migration_disposition(concept),
                idea_key,
                purpose,
                disposition: LearningMatchDisposition::Selected,
                proof_family: "decisionExplanation".to_string(),
            });
        }
    }
    Ok(matches)
}

pub fn migration_disposition_for_idea(idea_key: &str) -> Option<ConceptMigrationDisposition> {
    serde_json::from_value::<CurriculumLearningConcept>(Value::String(idea_key.to_string()))
        .map(migration_disposition)
        .ok()
}

fn migration_disposition(concept: CurriculumLearningConcept) -> ConceptMigrationDisposition {
    use CurriculumLearningConcept::*;

    match concept {
        Zugzwang | DefensiveMove | QuietMove | Equality | Advantage | CrushingAdvantage => {
            ConceptMigrationDisposition::ScopedRecognition
        }
        PieceCheckmates
        | CheckmatePatterns
        | KnightAndBishopMate
        | Pin
        | Skewer
        | Fork
        | HangingPiece
        | DiscoveredAttack
        | DoubleCheck
        | OverloadedPiece
        | Intermezzo
        | XRayAttack
        | Interference
        | GreekGift
        | Deflection
        | Attraction
        | Underpromotion
        | Desperado
        | CounterCheck
        | CapturingDefender
        | Clearance
        | KeySquares
        | Opposition
        | SeventhRankRookPawn
        | PassiveRookDefense
        | Lucena
        | Philidor
        | IntermediateRookEndings
        | PracticalRookEndings
        | AdvancedPawn
        | AttackingF2F7
        | ExposedKing
        | KingsideAttack
        | QueensideAttack
        | Sacrifice
        | TrappedPiece
        | CollinearMove
        | DiscoveredCheck
        | AnastasiaMate
        | ArabianMate
        | BackRankMate
        | BalestraMate
        | BlindSwineMate
        | BodenMate
        | CornerMate
        | DoubleBishopMate
        | DovetailMate
        | EpauletteMate
        | HookMate
        | KillBoxMate
        | PillsburysMate
        | MorphysMate
        | OperaMate
        | SwallowstailMate
        | TriangleMate
        | VukovicMate
        | SmotheredMate
        | Castling
        | EnPassant
        | Promotion
        | RookEndgame
        | BishopEndgame
        | PawnEndgame
        | KnightEndgame
        | QueenEndgame
        | QueenAndRookEndgame
        | Checkmate => ConceptMigrationDisposition::DirectlyProvable,
    }
}

fn serialized_string<T: Serialize>(value: &T) -> Result<String, LearningMatchEvaluationError> {
    serde_json::to_value(value)
        .map_err(|error| LearningMatchEvaluationError(error.to_string()))?
        .as_str()
        .map(ToString::to_string)
        .ok_or_else(|| LearningMatchEvaluationError("concept key is not a string".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_frozen_review_shapes_are_not_translated() {
        let legacy = serde_json::json!({
            "criticalMoments": [],
            "learningPlan": {
                "selectionPolicyVersion": "learning-plan-selection/v3",
                "resourceCatalogVersion": "learning-resources/2026-08-03",
                "tracks": []
            }
        });

        assert!(evaluate_frozen_learning_review(&legacy).is_err());
    }

    #[test]
    fn amended_scoped_concepts_have_an_explicit_disposition() {
        for idea in [
            "zugzwang",
            "defensiveMove",
            "quietMove",
            "equality",
            "advantage",
            "crushingAdvantage",
        ] {
            assert_eq!(
                migration_disposition_for_idea(idea),
                Some(ConceptMigrationDisposition::ScopedRecognition)
            );
        }
        assert_eq!(
            migration_disposition_for_idea("fork"),
            Some(ConceptMigrationDisposition::DirectlyProvable)
        );
        assert_eq!(migration_disposition_for_idea("unknownConcept"), None);
    }
}
