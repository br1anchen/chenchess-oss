use std::{
    fs,
    path::{Path, PathBuf},
};

use chen_chess_coach_engine::{
    evaluation_recording::ReviewSessionProviderRecording,
    review_session_board::{coordinate_text_board, graphical_board},
    review_session_contract::{
        CoachTurnId, Color, CriticalMomentId, EvidenceEntry, GameRef, ImportedGame, Piece,
        PieceRole, PositionSnapshot, RequestId, ReviewMomentSelection, ReviewSessionCoreContract,
    },
    review_session_start::{start_review_session, ReviewSessionStartError},
};
use serde::de::DeserializeOwned;

#[test]
fn pipeline_moment_starts_at_its_recorded_pre_move_position() {
    let game = game_fixture();
    let game_ref = game.game.game_ref.clone();
    let moment_id = CriticalMomentId::for_imported_game(&game_ref, 24);
    let core = start(
        game,
        ReviewMomentSelection::PipelineCriticalMoment {
            critical_moment_id: moment_id.clone(),
        },
        "pipeline",
    )
    .unwrap();
    let recorded_position = recorded_positions()
        .into_iter()
        .find(|position| {
            position.position_ref == core.imported_game.game.moves[23].before_position_ref
        })
        .expect("the provider recording contains the ply-24 Position");

    assert_eq!(core.position_snapshot, recorded_position);
    assert_eq!(core.review_moment.moment_id, moment_id);
    assert_eq!(core.review_moment.ply, 24);
    assert_eq!(
        core.review_moment
            .preceding_move
            .as_ref()
            .map(|game_move| (game_move.ply, game_move.uci.as_str())),
        Some((23, "a4b5"))
    );
    assert_eq!(core.review_moment.game_ref, game_ref);
    assert_eq!(core.evidence_packet.entries.len(), 2);
    assert!(matches!(
        &core.evidence_packet.entries[0],
        EvidenceEntry::Position { position, .. } if position == &core.position_snapshot
    ));
    assert!(matches!(
        &core.evidence_packet.entries[1],
        EvidenceEntry::Provenance { metadata, .. }
            if metadata.dependencies == vec![core.evidence_packet.entries[0].metadata().evidence_id.clone()]
    ));
    assert_eq!(core.coach_turn_context.required_evidence_refs.len(), 2);

    let wire = serde_json::to_vec(&core).unwrap();
    assert_eq!(
        serde_json::from_slice::<ReviewSessionCoreContract>(&wire).unwrap(),
        core
    );
}

#[test]
fn player_may_select_any_game_ply_regardless_of_review_side() {
    let game = game_fixture();
    assert_eq!(
        game.review_side,
        chen_chess_coach_engine::review_session_contract::ReviewSide::Black
    );

    let first_white_move = start(
        game.clone(),
        ReviewMomentSelection::PlayerSelectedMoment { ply: 1 },
        "first-white",
    )
    .unwrap();
    assert_eq!(
        first_white_move.coach_turn_context.reviewed_move.side,
        Color::White
    );
    assert!(first_white_move.review_moment.preceding_move.is_none());

    assert_eq!(
        start(
            game.clone(),
            ReviewMomentSelection::PlayerSelectedMoment { ply: 0 },
            "zero",
        )
        .unwrap_err(),
        ReviewSessionStartError::PlyOutOfRange {
            ply: 0,
            max_ply: 90,
        }
    );
    assert_eq!(
        start(
            game,
            ReviewMomentSelection::PlayerSelectedMoment { ply: 91 },
            "past-end",
        )
        .unwrap_err(),
        ReviewSessionStartError::PlyOutOfRange {
            ply: 91,
            max_ply: 90,
        }
    );
}

#[test]
fn unknown_pipeline_identity_and_tampered_import_are_rejected() {
    let game = game_fixture();
    let unknown = CriticalMomentId::try_from("review-moment:unknown:1".to_string()).unwrap();
    assert_eq!(
        start(
            game.clone(),
            ReviewMomentSelection::PipelineCriticalMoment {
                critical_moment_id: unknown,
            },
            "unknown",
        )
        .unwrap_err(),
        ReviewSessionStartError::UnknownPipelineCriticalMoment
    );

    let mut tampered = game;
    tampered.game.moves[0].uci = "a1a8".to_string();
    assert_eq!(
        start(
            tampered,
            ReviewMomentSelection::PlayerSelectedMoment { ply: 1 },
            "tampered",
        )
        .unwrap_err(),
        ReviewSessionStartError::InvalidImportedGame("a move is illegal in its recorded Position")
    );
}

#[test]
fn exact_position_facts_are_reused_across_distinct_game_occurrences() {
    let first_game = game_fixture();
    let mut second_game = first_game.clone();
    second_game.game.game_ref = GameRef::try_from(format!("sha256:{}", "0".repeat(64))).unwrap();
    let first = start(
        first_game,
        ReviewMomentSelection::PlayerSelectedMoment { ply: 24 },
        "first-occurrence",
    )
    .unwrap();
    let second = start(
        second_game,
        ReviewMomentSelection::PlayerSelectedMoment { ply: 24 },
        "second-occurrence",
    )
    .unwrap();

    assert_eq!(first.position_snapshot, second.position_snapshot);
    assert_eq!(
        first.evidence_packet.entries[0].metadata().evidence_id,
        second.evidence_packet.entries[0].metadata().evidence_id
    );
    assert_ne!(
        first.review_moment.moment_id,
        second.review_moment.moment_id
    );
    assert_ne!(first.review_moment.game_ref, second.review_moment.game_ref);
}

#[test]
fn recorded_positions_round_trip_through_graphical_and_text_boards() {
    let positions = recorded_positions();
    assert_eq!(positions.len(), 18);

    for position in positions {
        let graphical = graphical_board(&position);
        assert_eq!(graphical.position_ref, position.position_ref);
        assert_eq!(graphical.squares.len(), 64);
        assert_eq!(graphical.squares.first().unwrap().square.as_str(), "a8");
        assert_eq!(graphical.squares.last().unwrap().square.as_str(), "h1");
        let mut graphical_occupancy = graphical
            .squares
            .iter()
            .filter_map(|square| square.piece.map(|piece| (square.square.as_str(), piece)))
            .collect::<Vec<_>>();
        let mut snapshot_occupancy = position
            .occupied
            .iter()
            .map(|square| (square.square.as_str(), square.piece))
            .collect::<Vec<_>>();
        graphical_occupancy.sort_by_key(|(square, _)| *square);
        snapshot_occupancy.sort_by_key(|(square, _)| *square);
        assert_eq!(graphical_occupancy, snapshot_occupancy);

        let text = coordinate_text_board(&position);
        let rows = text.lines().take(8).collect::<Vec<_>>();
        assert_eq!(rows.len(), 8);
        for (index, square) in graphical.squares.iter().enumerate() {
            let row = rows[index / 8];
            let symbols = row
                .split_once("| ")
                .expect("board row has a coordinate label")
                .1
                .split_ascii_whitespace()
                .collect::<Vec<_>>();
            assert_eq!(
                symbols[index % 8],
                square.piece.map_or('.', piece_symbol).to_string()
            );
        }
        assert!(text.contains("    a b c d e f g h"));
        assert!(text.ends_with(match position.side_to_move {
            Color::White => "Side to move: White",
            Color::Black => "Side to move: Black",
        }));
    }
}

fn start(
    game: ImportedGame,
    selection: ReviewMomentSelection,
    suffix: &str,
) -> Result<ReviewSessionCoreContract, ReviewSessionStartError> {
    start_review_session(
        RequestId::try_from(format!("request:test:{suffix}")).unwrap(),
        CoachTurnId::try_from(format!("coach-turn:test:{suffix}")).unwrap(),
        game,
        selection,
    )
}

fn game_fixture() -> ImportedGame {
    fixture(contract_fixture_root().join("imported-game.json"))
}

fn recorded_positions() -> Vec<PositionSnapshot> {
    let recording: ReviewSessionProviderRecording =
        fixture(canonical_fixture_root().join("review-session-provider-recording.json"));
    recording
        .content
        .entries
        .into_iter()
        .filter_map(|entry| match entry {
            EvidenceEntry::Position { position, .. } => Some(position),
            _ => None,
        })
        .collect()
}

fn fixture<T: DeserializeOwned>(path: impl AsRef<Path>) -> T {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn contract_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/coach-engine-sdk/fixtures")
}

fn canonical_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/shared-assets/fixtures/Synthet1")
}

fn piece_symbol(piece: Piece) -> char {
    let symbol = match piece.role {
        PieceRole::Pawn => 'p',
        PieceRole::Knight => 'n',
        PieceRole::Bishop => 'b',
        PieceRole::Rook => 'r',
        PieceRole::Queen => 'q',
        PieceRole::King => 'k',
    };
    if piece.color == Color::White {
        symbol.to_ascii_uppercase()
    } else {
        symbol
    }
}
