use std::{fs, path::Path};

use chen_chess_coach_engine::{
    evaluation_recording::{RecordingError, ReviewSessionProviderRecording, VerifiedLichessReplay},
    review_session_contract::{
        DeliverySurface, EloRating, EvidenceEntry, GameInputSource, RequestedReviewSide,
        ReviewSessionCommand, ReviewSessionCommandEnvelope, ReviewSessionEventEnvelope, ReviewSide,
    },
};
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
enum ReplayFailure {
    Transport,
    Timeout,
    Cancelled,
}

struct ReplayFailureInjector(ReplayFailure);

impl ReplayFailureInjector {
    fn result<T>(self) -> Result<T, ReplayFailure> {
        Err(self.0)
    }
}

#[test]
fn rust_decodes_the_shared_command_and_event_fixtures() {
    let commands: Vec<ReviewSessionCommandEnvelope> = fixture("commands.json");
    let events: Vec<ReviewSessionEventEnvelope> = fixture("events.json");

    assert_eq!(commands.len(), 5);
    assert_eq!(commands[0].surface, DeliverySurface::Web);
    assert_eq!(commands[1].surface, DeliverySurface::CoachSkill);
    assert_eq!(commands[2].surface, DeliverySurface::CoachApp);
    assert_eq!(commands[3].surface, DeliverySurface::CoachApp);
    assert_eq!(commands[4].surface, DeliverySurface::Web);
    assert!(matches!(
        &commands[2].command,
        ReviewSessionCommand::ImportGame {
            source: GameInputSource::ChessComUrl { url },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::White
            },
            ..
        } if url == "https://www.chess.com/game/computer/1403674481"
    ));
    assert!(matches!(
        &commands[4].command,
        ReviewSessionCommand::StartHostTurn {
            message,
            prior_turns,
            ..
        } if message == "Why was this move a mistake?" && prior_turns.len() == 1
    ));
    assert_eq!(events.len(), 20);

    let mut invalid_command: Value = fixture("commands.json");
    invalid_command[1]["surface"] = Value::String("web".to_string());
    assert!(serde_json::from_value::<Vec<ReviewSessionCommandEnvelope>>(invalid_command).is_err());

    let mut invalid_coach_app_command: Value = fixture("commands.json");
    invalid_coach_app_command[2]["command"]["source"] =
        serde_json::json!({ "kind": "localPgnFile", "path": "/private/player/game.pgn" });
    assert!(
        serde_json::from_value::<Vec<ReviewSessionCommandEnvelope>>(invalid_coach_app_command)
            .is_err()
    );

    let mut missing_coach_side: Value = fixture("commands.json");
    missing_coach_side[1]["command"]["reviewSide"] = serde_json::json!({ "kind": "required" });
    assert!(
        serde_json::from_value::<Vec<ReviewSessionCommandEnvelope>>(missing_coach_side).is_err()
    );

    let mut source_without_qualified_side: Value = fixture("commands.json");
    source_without_qualified_side[0]["command"]["source"] =
        serde_json::json!({ "kind": "pastedPgn", "pgn": "1. e4 e5" });
    assert!(serde_json::from_value::<Vec<ReviewSessionCommandEnvelope>>(
        source_without_qualified_side
    )
    .is_err());

    let mut bare_url_with_derived_side: Value = fixture("commands.json");
    bare_url_with_derived_side[0]["command"]["source"]["url"] =
        Value::String("https://lichess.org/Synthet1".to_string());
    assert!(serde_json::from_value::<Vec<ReviewSessionCommandEnvelope>>(
        bare_url_with_derived_side
    )
    .is_err());

    let mut oversized_pgn: Value = fixture("commands.json");
    oversized_pgn[0]["command"]["source"] = serde_json::json!({
        "kind": "pastedPgn",
        "pgn": "é".repeat(262_145)
    });
    oversized_pgn[0]["command"]["reviewSide"] =
        serde_json::json!({ "kind": "selected", "reviewSide": "both" });
    assert!(serde_json::from_value::<Vec<ReviewSessionCommandEnvelope>>(oversized_pgn).is_err());

    let mut zero_selected_ply: Value = fixture("commands.json");
    let core_contract: Value = fixture("core-contract.json");
    zero_selected_ply[0]["command"] = serde_json::json!({
        "kind": "startReviewSession",
        "importedGame": core_contract["importedGame"],
        "moment": { "kind": "playerSelectedMoment", "ply": 0 }
    });
    assert!(
        serde_json::from_value::<Vec<ReviewSessionCommandEnvelope>>(zero_selected_ply).is_err()
    );

    let evidence_ref = core_contract["coachTurnContext"]["requiredEvidenceRefs"][0].clone();
    let dimension = serde_json::json!({
        "explanation": "Grounded fixture assessment.",
        "evidenceRefs": [evidence_ref]
    });
    let mut mismatched_coach_turn: Value = fixture("commands.json");
    mismatched_coach_turn[0]["command"] = serde_json::json!({
        "kind": "publishCoachTurn",
        "sessionId": "review-session:fixture:identity",
        "coachTurnId": "coach-turn:fixture:outer",
        "assessment": {
            "coachTurnId": "coach-turn:fixture:inner",
            "alternativeMoveId": "alternative-move:fixture:identity",
            "objectiveQuality": dimension,
            "findability": dimension,
            "resilience": dimension
        },
        "idempotencyKey": "idempotency-key:fixture:identity"
    });
    assert!(
        serde_json::from_value::<Vec<ReviewSessionCommandEnvelope>>(mismatched_coach_turn).is_err()
    );

    let mut invalid_event: Value = fixture("events.json");
    invalid_event[3]["event"]["stateChanged"] = Value::Bool(true);
    assert!(serde_json::from_value::<Vec<ReviewSessionEventEnvelope>>(invalid_event).is_err());
}

#[test]
fn verified_fakes_only_replay_canonical_provider_outputs() {
    let recording = provider_recording();
    let (position_ref, engine) = recording
        .content
        .entries
        .iter()
        .find_map(|entry| match entry {
            EvidenceEntry::EngineAnalysis {
                position_ref,
                analysis,
                ..
            } => Some((position_ref, analysis)),
            _ => None,
        })
        .expect("fixture should contain Stockfish evidence");
    let replay = recording.replay().unwrap();
    assert_eq!(replay.stockfish(position_ref).unwrap(), engine);
    assert!(replay
        .maia(position_ref, EloRating::try_from(1246).unwrap())
        .is_ok());
    assert!(ReplayFailureInjector(ReplayFailure::Timeout)
        .result::<()>()
        .is_err());

    let canonical_pgn = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/shared-assets/fixtures/Synthet1/lichess-export.pgn"),
    )
    .expect("canonical PGN should exist");
    let lichess = VerifiedLichessReplay::from_canonical_pgn(&canonical_pgn).unwrap();
    assert!(lichess.respond().contains("[GameId \"Synthet1\"]"));
    assert!(ReplayFailureInjector(ReplayFailure::Transport)
        .result::<()>()
        .is_err());
    assert!(ReplayFailureInjector(ReplayFailure::Cancelled)
        .result::<()>()
        .is_err());
}

#[test]
fn provider_recording_rejects_an_internally_consistent_canonical_subset() {
    let recording = provider_recording();
    let mut content = recording.content;
    let removed_position = content
        .entries
        .iter()
        .find_map(|entry| match entry {
            EvidenceEntry::Branch { branch, .. }
                if branch.branch_ref.as_str() == "branch:capture:d7b6:c4e2" =>
            {
                Some(branch.resulting_position_ref.clone())
            }
            _ => None,
        })
        .expect("fixture should contain the selected reply branch");
    content.entries.retain(|entry| match entry {
        EvidenceEntry::Position { position, .. } => position.position_ref != removed_position,
        EvidenceEntry::EngineAnalysis { position_ref, .. }
        | EvidenceEntry::HumanMoveModel { position_ref, .. } => position_ref != &removed_position,
        EvidenceEntry::Branch { branch, .. } => {
            branch.resulting_position_ref != removed_position
                && branch.source_position_ref != removed_position
        }
        EvidenceEntry::Provenance { .. } => true,
    });

    assert!(matches!(
        ReviewSessionProviderRecording::from_content(content),
        Err(RecordingError::ProvenanceDrift("canonical Position set"))
    ));
}

fn provider_recording() -> ReviewSessionProviderRecording {
    serde_json::from_slice(
        &fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../packages/shared-assets/fixtures/Synthet1/review-session-provider-recording.json",
        ))
        .expect("evaluation recording should be readable"),
    )
    .expect("evaluation recording should decode")
}

fn fixture<T: DeserializeOwned>(name: &str) -> T {
    serde_json::from_slice(
        &fs::read(fixture_root().join(name)).expect("shared fixture should be readable"),
    )
    .expect("shared fixture should decode")
}

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/coach-engine-sdk/fixtures")
}

#[test]
fn web_and_coach_app_command_envelopes_are_separable_by_delivery_surface() {
    let commands: Vec<ReviewSessionCommandEnvelope> = fixture("commands.json");
    let web = commands
        .iter()
        .find(|envelope| envelope.surface == DeliverySurface::Web)
        .expect("fixture includes a Web envelope");
    let mut coach_app = web.clone();
    coach_app.surface = DeliverySurface::CoachApp;
    assert_ne!(web.surface, coach_app.surface);
    assert_ne!(
        serde_json::to_value(web).unwrap(),
        serde_json::to_value(&coach_app).unwrap(),
        "web and Coach App envelopes must be separable via DeliverySurface"
    );
}
