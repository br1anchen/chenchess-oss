use crate::{
    learning_plan::catalog::resources_for,
    review_session_contract::{
        DecisionExplanation, DecisionLearningTrackProjection, ExplanationPathRef, LearningTrackKey,
    },
};

use super::{knowledge::KnowledgeConcept, DecisionExplanationContractError};

pub(super) fn project(
    concept: &KnowledgeConcept,
    explanation: &DecisionExplanation,
) -> Result<DecisionLearningTrackProjection, DecisionExplanationContractError> {
    let key = match concept {
        KnowledgeConcept::Curriculum(concept) => LearningTrackKey::Curriculum { concept: *concept },
    };
    let [path] = explanation.selected_paths.as_slice() else {
        return Err(DecisionExplanationContractError::InvalidProof(
            "a projected concept must resolve exactly one selected path",
        ));
    };
    let path_ref: ExplanationPathRef = path.path_ref.clone();
    let resources = resources_for(&key)
        .map_err(|_| DecisionExplanationContractError::MissingResourceMapping)?;
    Ok(DecisionLearningTrackProjection {
        key,
        explanation_path_ref: path_ref,
        resources,
    })
}
