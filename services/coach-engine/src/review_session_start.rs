use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position};

use crate::review_session_contract::*;

const POSITION_PRODUCER: &str = "review-session-position/v1";
const IMPORT_PROVENANCE_PRODUCER: &str = "game-import-provenance-verifier/v1";

/// Resolves where a Review Moment sits in a Game and what the board looked like.
///
/// This is the whole of a Review Moment's deterministic identity: the ply, the
/// selection that named it, and the Position the Player faced. It depends on
/// the imported Game alone, so a session-free read reconstructs exactly what a
/// Review Session start reconstructs, with no authoring machinery in between.
/// Reconstructs the Position a Review Moment sits at, from its ply alone.
///
/// A reader of one Review Moment already knows which moment it wants, so how
/// that moment came to be selected is not part of the question. Bounds
/// checking still runs, because a ply outside the Game names no Position.
pub(crate) fn review_moment_position(
    imported_game: &ImportedGame,
    ply: u16,
) -> Result<PositionSnapshot, ReviewSessionStartError> {
    let (move_index, _) = resolve_moment(
        &imported_game.game,
        &ReviewMomentSelection::PlayerSelectedMoment { ply },
    )?;
    let (position_snapshot, _) = reconstruct_selected_position(imported_game, move_index)?;
    Ok(position_snapshot)
}

pub(crate) fn resolve_review_moment_position(
    imported_game: &ImportedGame,
    selection: ReviewMomentSelection,
) -> Result<(ReviewMomentOccurrence, PositionSnapshot), ReviewSessionStartError> {
    let game = &imported_game.game;
    let (move_index, moment_id) = resolve_moment(game, &selection)?;
    let reviewed_move = game
        .moves
        .get(move_index)
        .expect("a resolved move index is present");
    let (position_snapshot, _) = reconstruct_selected_position(imported_game, move_index)?;
    Ok((
        ReviewMomentOccurrence {
            moment_id,
            ply: reviewed_move.ply,
            preceding_move: move_index
                .checked_sub(1)
                .and_then(|index| game.moves.get(index))
                .cloned(),
            selection,
            game_ref: game.game_ref.clone(),
        },
        position_snapshot,
    ))
}

pub fn start_review_session(
    request_id: RequestId,
    coach_turn_id: CoachTurnId,
    imported_game: ImportedGame,
    selection: ReviewMomentSelection,
) -> Result<ReviewSessionCoreContract, ReviewSessionStartError> {
    let (review_moment, position_snapshot) =
        resolve_review_moment_position(&imported_game, selection)?;
    let reviewed_move = imported_game
        .game
        .moves
        .iter()
        .find(|game_move| game_move.ply == review_moment.ply)
        .expect("a resolved Review Moment names a move of its Game");
    let (evidence_packet, required_evidence_refs) =
        initial_evidence_packet(&imported_game, &position_snapshot);

    Ok(ReviewSessionCoreContract {
        request_id,
        coach_turn_context: CoachTurnContext {
            coach_turn_id,
            reviewed_move: ReviewedMoveAnchor {
                critical_moment_id: review_moment.moment_id.clone(),
                ply: reviewed_move.ply,
                side: reviewed_move.side,
                position_ref: position_snapshot.position_ref.clone(),
                played_move_uci: reviewed_move.uci.clone(),
            },
            selected_position_ref: position_snapshot.position_ref.clone(),
            target: CoachTurnTarget::ImportedGameMove {
                critical_moment_id: review_moment.moment_id.clone(),
                ply: reviewed_move.ply,
                uci: reviewed_move.uci.clone(),
            },
            required_evidence_refs,
        },
        review_moment,
        imported_game,
        position_snapshot,
        evidence_packet,
    })
}

fn resolve_moment(
    game: &CanonicalCompletedGame,
    selection: &ReviewMomentSelection,
) -> Result<(usize, CriticalMomentId), ReviewSessionStartError> {
    let max_ply = game.moves.last().map_or(0, |game_move| game_move.ply);
    match selection {
        ReviewMomentSelection::PipelineCriticalMoment { critical_moment_id } => game
            .moves
            .iter()
            .position(|game_move| {
                CriticalMomentId::for_imported_game(&game.game_ref, game_move.ply)
                    == *critical_moment_id
            })
            .map(|index| (index, critical_moment_id.clone()))
            .ok_or(ReviewSessionStartError::UnknownPipelineCriticalMoment),
        ReviewMomentSelection::PlayerSelectedMoment { ply } => {
            let index = ply
                .checked_sub(1)
                .map(usize::from)
                .ok_or(ReviewSessionStartError::PlyOutOfRange { ply: *ply, max_ply })?;
            if index >= game.moves.len() {
                return Err(ReviewSessionStartError::PlyOutOfRange { ply: *ply, max_ply });
            }
            Ok((
                index,
                CriticalMomentId::for_imported_game(&game.game_ref, *ply),
            ))
        }
    }
}

pub(crate) fn reconstruct_selected_position(
    imported_game: &ImportedGame,
    selected_index: usize,
) -> Result<(PositionSnapshot, Vec<String>), ReviewSessionStartError> {
    let game = &imported_game.game;
    let mut chess: Chess = Fen::from_ascii(STANDARD_STARTING_FEN.as_bytes())
        .expect("the standard starting FEN is valid")
        .into_position(CastlingMode::Standard)
        .expect("the standard starting Position is legal");
    let mut history = Vec::<String>::with_capacity(selected_index + 1);
    let mut current = build_position_snapshot(STANDARD_STARTING_FEN, &[]).map_err(|_| {
        ReviewSessionStartError::InvalidImportedGame(
            "the starting Position could not be reconstructed",
        )
    })?;

    for (index, game_move) in game.moves.iter().take(selected_index + 1).enumerate() {
        if game_move.before_position_ref != current.position_ref {
            return Err(ReviewSessionStartError::InvalidImportedGame(
                "move does not match its pre-move Position",
            ));
        }
        let selected = (index == selected_index).then(|| current.clone());

        let chess_move = UciMove::from_ascii(game_move.uci.as_bytes())
            .map_err(|_| ReviewSessionStartError::InvalidImportedGame("a move has malformed UCI"))?
            .to_move(&chess)
            .map_err(|_| {
                ReviewSessionStartError::InvalidImportedGame(
                    "a move is illegal in its recorded Position",
                )
            })?;
        chess.play_unchecked(&chess_move);
        history.push(current.fen);

        let after_fen = Fen::from_position(chess.clone(), EnPassantMode::Legal).to_string();
        let preceding = history.iter().map(String::as_str).collect::<Vec<_>>();
        current = build_position_snapshot(&after_fen, &preceding).map_err(|_| {
            ReviewSessionStartError::InvalidImportedGame(
                "a post-move Position could not be reconstructed",
            )
        })?;
        if game_move.after_position_ref != current.position_ref {
            return Err(ReviewSessionStartError::InvalidImportedGame(
                "move does not match its post-move Position",
            ));
        }
        if let Some(selected) = selected {
            if index + 1 == game.moves.len() && current.position_ref != game.final_position_ref {
                return Err(ReviewSessionStartError::InvalidImportedGame(
                    "final Position does not match the canonical Game",
                ));
            }
            return Ok((selected, history));
        }
    }

    Err(ReviewSessionStartError::InvalidImportedGame(
        "selected move is absent from the canonical Game",
    ))
}

pub(crate) fn reconstruct_derived_position(
    imported_game: &ImportedGame,
    root_ply: u16,
    uci_path: &[String],
) -> Result<PositionSnapshot, ReviewSessionStartError> {
    let root_index =
        root_ply
            .checked_sub(1)
            .map(usize::from)
            .ok_or(ReviewSessionStartError::PlyOutOfRange {
                ply: root_ply,
                max_ply: imported_game
                    .game
                    .moves
                    .last()
                    .map_or(0, |game_move| game_move.ply),
            })?;
    let (mut current, mut history) = reconstruct_selected_position(imported_game, root_index)?;
    for uci in uci_path {
        let mut position: Chess = Fen::from_ascii(current.fen.as_bytes())
            .map_err(|_| {
                ReviewSessionStartError::InvalidImportedGame("a derived Position has malformed FEN")
            })?
            .into_position(CastlingMode::Standard)
            .map_err(|_| {
                ReviewSessionStartError::InvalidImportedGame("a derived Position is not legal")
            })?;
        let chess_move = UciMove::from_ascii(uci.as_bytes())
            .map_err(|_| {
                ReviewSessionStartError::InvalidImportedGame(
                    "a derived Position path has malformed UCI",
                )
            })?
            .to_move(&position)
            .map_err(|_| {
                ReviewSessionStartError::InvalidImportedGame(
                    "a derived Position path contains an illegal move",
                )
            })?;
        position.play_unchecked(&chess_move);
        let fen = Fen::from_position(position, EnPassantMode::Legal).to_string();
        let preceding = history.iter().map(String::as_str).collect::<Vec<_>>();
        current = build_position_snapshot(&fen, &preceding).map_err(|_| {
            ReviewSessionStartError::InvalidImportedGame(
                "a derived Position could not be reconstructed",
            )
        })?;
        history.push(fen);
    }
    Ok(current)
}

fn initial_evidence_packet(
    imported_game: &ImportedGame,
    position: &PositionSnapshot,
) -> (ReviewSessionEvidencePacket, Vec<EvidenceId>) {
    let position_entry = EvidenceEntry::position(
        EvidenceMetadata::derived(POSITION_PRODUCER, Vec::new()),
        position.clone(),
    );
    let position_evidence_id = position_entry.metadata().evidence_id.clone();
    let provenance_entry = EvidenceEntry::provenance(
        EvidenceMetadata::derived(
            IMPORT_PROVENANCE_PRODUCER,
            vec![position_evidence_id.clone()],
        ),
        ProvenanceFact::Import {
            source: imported_game.provenance.clone(),
        },
    );
    let provenance_evidence_id = provenance_entry.metadata().evidence_id.clone();
    let packet = ReviewSessionEvidencePacket {
        entries: vec![position_entry, provenance_entry],
    };
    (packet, vec![position_evidence_id, provenance_evidence_id])
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReviewSessionStartError {
    #[error("invalid imported Game: {0}")]
    InvalidImportedGame(&'static str),
    #[error("pipeline Critical Moment is not part of the imported Game")]
    UnknownPipelineCriticalMoment,
    #[error("selected ply {ply} is outside the imported Game (1-{max_ply})")]
    PlyOutOfRange { ply: u16, max_ply: u16 },
}
