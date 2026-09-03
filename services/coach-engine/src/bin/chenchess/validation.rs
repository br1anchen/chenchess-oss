use std::path::Path;

use chen_chess_coach_engine::{
    review_session_contract::{
        CoachTurnTarget, CriticalMomentId, GameImportId, GameReview, ImportedGame,
        OperationCompletion, ReviewMomentAuthoringReadiness, ReviewMomentSelection,
        ReviewSessionCoreContract, ReviewSessionEvent, ReviewSessionEventEnvelope,
    },
    review_validation::{validate_game_review, validate_grounded_game_review, DraftGameReview},
};
use serde::Serialize;

use super::{read_text, write_json, CliError};

pub(super) fn review(
    event_path: &Path,
    start_event_path: &Path,
    draft_path: Option<&Path>,
) -> Result<(), CliError> {
    eprintln!("validating Draft Game Review");
    let (game_import_id, review) = read_review(event_path)?;
    let draft_json = read_text(draft_path, "Draft Game Review")?;
    let draft: DraftGameReview = serde_json::from_str(&draft_json)
        .map_err(|error| CliError::Validation(format!("invalid Draft Game Review: {error}")))?;
    let expected_plies = review
        .critical_moments
        .iter()
        .map(|moment| usize::from(moment.ply))
        .collect::<Vec<_>>();
    validate_game_review(&draft, &expected_plies)
        .map_err(|error| CliError::Validation(error.to_string()))?;
    let review_moment_plies = validate_review_start(start_event_path, &game_import_id, &review)?;
    validate_authority_order(&review_moment_plies)?;
    validate_grounded_game_review(&draft, &review)
        .map_err(|error| CliError::Validation(error.to_string()))?;
    write_json(&ValidationResult::valid("gameReviewValidation"))
}

fn validate_review_start(
    path: &Path,
    imported_game_import_id: &GameImportId,
    imported_review: &GameReview,
) -> Result<Vec<u16>, CliError> {
    let envelope = read_event(path)?;
    let ReviewSessionEvent::Completed { result } = envelope.event else {
        return Err(CliError::Validation(
            "Review Session start event is not completed".to_string(),
        ));
    };
    let OperationCompletion::ReviewSessionStarted {
        game_import_id,
        review,
        imported_game,
        review_moments,
        ..
    } = *result
    else {
        return Err(CliError::Validation(
            "Review Session start event has the wrong completion kind".to_string(),
        ));
    };
    let review_moments = review_moments
        .into_iter()
        .map(|moment| match moment.authoring {
            ReviewMomentAuthoringReadiness::Prepared { core } => Ok(*core),
            ReviewMomentAuthoringReadiness::Pending => Err(CliError::Validation(
                "Coach Skill Review Session start is not a complete prepared batch".to_string(),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_review_start_binding(
        imported_game_import_id,
        &game_import_id,
        imported_review,
        &review,
        &imported_game,
        &review_moments,
    )?;
    Ok(review_start_authorities(&review_moments))
}

fn validate_review_start_binding(
    imported_game_import_id: &GameImportId,
    started_game_import_id: &GameImportId,
    imported_review: &GameReview,
    started_review: &GameReview,
    imported_game: &ImportedGame,
    review_moments: &[ReviewSessionCoreContract],
) -> Result<(), CliError> {
    if started_game_import_id != imported_game_import_id {
        return Err(CliError::Validation(
            "Review Session start event belongs to a different Game import".to_string(),
        ));
    }
    if review_identity(started_review) != review_identity(imported_review) {
        return Err(CliError::Validation(
            "Review Session start event has a different Game Review identity".to_string(),
        ));
    }
    if review_moments.len() != imported_review.critical_moments.len() {
        return Err(CliError::Validation(
            "Review Session start authority does not cover the imported Game Review exactly"
                .to_string(),
        ));
    }
    for (core, critical_moment) in review_moments.iter().zip(&imported_review.critical_moments) {
        if critical_moment.critical_moment_id
            != CriticalMomentId::for_imported_game(
                &imported_game.game.game_ref,
                critical_moment.ply,
            )
            || core.imported_game != *imported_game
            || core.review_moment.moment_id != critical_moment.critical_moment_id
            || core.review_moment.ply != critical_moment.ply
            || !matches!(
                &core.review_moment.selection,
                ReviewMomentSelection::PipelineCriticalMoment { critical_moment_id }
                    if critical_moment_id == &critical_moment.critical_moment_id
            )
            || !has_valid_start_context(core)
        {
            return Err(CliError::Validation(
                "Review Session start authority does not match the imported Game Review moments"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

fn has_valid_start_context(core: &ReviewSessionCoreContract) -> bool {
    let context = &core.coach_turn_context;
    let imported_target = matches!(
        &context.target,
        CoachTurnTarget::ImportedGameMove {
            critical_moment_id,
            ply,
            uci,
        } if critical_moment_id == &core.review_moment.moment_id
            && ply == &core.review_moment.ply
            && uci == &context.reviewed_move.played_move_uci
    );
    imported_target
}

fn review_identity(review: &GameReview) -> Vec<(&CriticalMomentId, u16)> {
    review
        .critical_moments
        .iter()
        .map(|moment| (&moment.critical_moment_id, moment.ply))
        .collect()
}

fn review_start_authorities(review_moments: &[ReviewSessionCoreContract]) -> Vec<u16> {
    review_moments
        .iter()
        .map(|core| core.review_moment.ply)
        .collect()
}

fn validate_authority_order(authorities: &[u16]) -> Result<(), CliError> {
    for pair in authorities.windows(2) {
        let previous = pair[0];
        let current = pair[1];
        if previous > current {
            return Err(CliError::Validation(format!(
                "Draft Game Review start authority is out of Game order: ply {current} follows ply {previous}"
            )));
        }
    }
    Ok(())
}

fn read_review(path: &Path) -> Result<(GameImportId, GameReview), CliError> {
    let envelope = read_event(path)?;
    match envelope.event {
        ReviewSessionEvent::Completed { result } => match *result {
            OperationCompletion::GameImported {
                game_import_id,
                review,
                ..
            } => Ok((game_import_id, *review)),
            _ => Err(CliError::InvalidInput(
                "Review Session event is not a Game import completion".to_string(),
            )),
        },
        _ => Err(CliError::InvalidInput(
            "Review Session event is not a Game import completion".to_string(),
        )),
    }
}

fn read_event(path: &Path) -> Result<ReviewSessionEventEnvelope, CliError> {
    let json = std::fs::read_to_string(path).map_err(|error| {
        CliError::InvalidInput(format!(
            "failed to read Review Session event {}: {error}",
            path.display()
        ))
    })?;
    serde_json::from_str(&json)
        .map_err(|error| CliError::InvalidInput(format!("invalid Review Session event: {error}")))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationResult {
    kind: &'static str,
    valid: bool,
}

impl ValidationResult {
    fn valid(kind: &'static str) -> Self {
        Self { kind, valid: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_import() -> (GameImportId, GameReview) {
        let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/events.json"
        )))
        .unwrap();
        events
            .into_iter()
            .find_map(|event| match event.event {
                ReviewSessionEvent::Completed { result } => match *result {
                    OperationCompletion::GameImported {
                        game_import_id,
                        review,
                        ..
                    } => Some((game_import_id, *review)),
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    }

    fn fixture_core() -> ReviewSessionCoreContract {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/core-contract.json"
        )))
        .unwrap()
    }

    #[test]
    fn one_atomic_start_supplies_authority_for_every_chronological_moment() {
        let first = fixture_core();
        let mut second = first.clone();
        second.review_moment.ply = 3;

        let authorities = review_start_authorities(&[first, second]);

        assert_eq!(authorities, vec![1, 3]);
    }

    #[test]
    fn zero_moment_start_supplies_no_authority() {
        assert!(review_start_authorities(&[]).is_empty());
    }

    #[test]
    fn out_of_order_start_authority_is_rejected() {
        let authorities = [22, 10];

        assert!(matches!(
            validate_authority_order(&authorities),
            Err(CliError::Validation(message)) if message.contains("out of Game order")
        ));
    }

    #[test]
    fn start_authority_must_belong_to_the_imported_review() {
        let (game_import_id, review) = fixture_import();
        let core = fixture_core();
        let imported_game = core.imported_game.clone();
        assert!(validate_review_start_binding(
            &game_import_id,
            &game_import_id,
            &review,
            &review,
            &imported_game,
            std::slice::from_ref(&core)
        )
        .is_ok());

        let mut presented_review = review.clone();
        presented_review
            .summary
            .push_str(" with presentation metadata");
        assert!(validate_review_start_binding(
            &game_import_id,
            &game_import_id,
            &review,
            &presented_review,
            &imported_game,
            std::slice::from_ref(&core)
        )
        .is_ok());

        let mut different_review = review.clone();
        different_review.critical_moments[0].ply += 1;
        assert!(matches!(
            validate_review_start_binding(
                &game_import_id,
                &game_import_id,
                &review,
                &different_review,
                &imported_game,
                std::slice::from_ref(&core)
            ),
            Err(CliError::Validation(message)) if message.contains("different Game Review identity")
        ));

        let mut different_core = core;
        different_core.review_moment.ply += 1;
        assert!(matches!(
            validate_review_start_binding(
                &game_import_id,
                &game_import_id,
                &review,
                &review,
                &imported_game,
                &[different_core]
            ),
            Err(CliError::Validation(message)) if message.contains("does not match")
        ));

        let mut player_selected_core = fixture_core();
        player_selected_core.review_moment.selection =
            ReviewMomentSelection::PlayerSelectedMoment {
                ply: player_selected_core.review_moment.ply,
            };
        assert!(matches!(
            validate_review_start_binding(
                &game_import_id,
                &game_import_id,
                &review,
                &review,
                &imported_game,
                &[player_selected_core]
            ),
            Err(CliError::Validation(message)) if message.contains("does not match")
        ));
    }

    #[test]
    fn zero_moment_start_authority_must_belong_to_the_game_import() {
        let (game_import_id, mut review) = fixture_import();
        review.critical_moments.clear();
        let imported_game = fixture_core().imported_game;
        let other_game_import_id =
            GameImportId::try_from("game-import:fixture:other".to_string()).unwrap();

        assert!(matches!(
            validate_review_start_binding(
                &game_import_id,
                &other_game_import_id,
                &review,
                &review,
                &imported_game,
                &[]
            ),
            Err(CliError::Validation(message)) if message.contains("different Game import")
        ));
    }
}
