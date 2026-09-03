use crate::{
    chess_literal_grounding::ChessLiteralGrounding,
    review_session_contract::{
        EnPassantState, PlayerPlanEvaluation, PlayerPlanEvaluationContext,
        PlayerPlanEvaluationDraft,
    },
};

pub(crate) fn admit(
    context: &PlayerPlanEvaluationContext,
    draft: PlayerPlanEvaluationDraft,
) -> Option<PlayerPlanEvaluation> {
    if draft.facts_ref != context.facts_ref
        || draft.text.trim().is_empty()
        || draft.text.contains('\n')
        || exposes_internal_details(&draft.text)
    {
        return None;
    }

    let facts = &context.facts;
    let mut grounding = ChessLiteralGrounding::empty();
    grounding.allow(&facts.reviewed_move_san);
    grounding.allow_all(&facts.objective_counterplay_san);
    for occupied in &facts.position_snapshot.occupied {
        grounding.allow(occupied.square.as_str());
    }
    if let EnPassantState::Available { square } = &facts.position_snapshot.en_passant {
        grounding.allow(square.as_str());
    }
    grounding
        .validate(&draft.text)
        .then_some(PlayerPlanEvaluation { text: draft.text })
}

fn exposes_internal_details(text: &str) -> bool {
    let lowercase = text.to_ascii_lowercase();
    [
        "facts_ref",
        "factsref",
        "position_ref",
        "positionref",
        "evidence",
        "provider",
        "stockfish",
        "trace",
    ]
    .iter()
    .any(|marker| lowercase.contains(marker))
}
