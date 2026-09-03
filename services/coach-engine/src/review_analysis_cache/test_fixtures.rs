use chrono::{DateTime, Utc};

use crate::{
    game_import_store::GameImportRecord,
    review_durability::game_import_id,
    review_session_contract::{
        EloRating, GameInputSource, OperationCompletion, PlayerId, ReviewSessionEvent,
        ReviewSessionEventEnvelope, ReviewSide,
    },
    review_session_game_identity::ReviewSessionGameIdentity,
    review_session_processor::ProcessorPrincipal,
};

pub(crate) fn fixture_player(id: &str) -> ProcessorPrincipal {
    ProcessorPrincipal::Player(PlayerId::try_from(id.to_string()).unwrap())
}

/// The one Game every checkpoint fixture reviews.
///
/// Digest-derived rather than a literal, so the fixture Game Import ID carries a
/// real review key and the analysis-cache address behaves as it does in
/// production — including two Players resolving to one cache entry.
pub(crate) fn fixture_identity() -> ReviewSessionGameIdentity {
    ReviewSessionGameIdentity::from_request(
        &GameInputSource::LichessUrl {
            url: "https://lichess.org/Synthet1".to_string(),
        },
        ReviewSide::Black,
        EloRating::try_from(1450).unwrap(),
    )
    .unwrap()
}

pub(crate) fn fixture_import_owned_by(
    owner: ProcessorPrincipal,
    created_at: DateTime<Utc>,
) -> GameImportRecord {
    let mut imported = fixture_import(created_at);
    imported.game_import_id = game_import_id(&owner, &fixture_identity());
    imported.owner = owner;
    imported
}

pub(crate) fn fixture_import(created_at: DateTime<Utc>) -> GameImportRecord {
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
    let imported_game = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
    )))
    .unwrap();
    let owner = fixture_player("firebase-player-a");
    GameImportRecord::new(
        game_import_id(&owner, &fixture_identity()),
        owner,
        imported_game,
        review,
        Vec::new(),
        None,
        created_at,
    )
}

/// A review the pipeline flagged nothing in — a valid review with no entries.
pub(super) fn fixture_empty_import(created_at: DateTime<Utc>) -> GameImportRecord {
    let mut imported = fixture_import(created_at);
    imported.frozen_review.critical_moments.clear();
    imported
}
