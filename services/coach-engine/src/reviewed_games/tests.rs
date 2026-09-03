use std::sync::Arc;

use crate::{
    game_import_store::InMemoryGameImportStore,
    profile_game_feed::{
        ProfileValidationError, ProfileValidationFuture, PublicChessProfile, PublicProfileValidator,
    },
    review_session_contract::{
        OperationCompletion, ReviewSessionEvent, ReviewSessionEventEnvelope,
    },
};

use super::*;

struct UnusedProfileValidator;

impl PublicProfileValidator for UnusedProfileValidator {
    fn validate<'a>(&'a self, _profile: &'a PublicChessProfile) -> ProfileValidationFuture<'a> {
        Box::pin(async { Err(ProfileValidationError::ProfileNotFound) })
    }
}

#[test]
fn opening_names_fold_case_and_diacritics() {
    assert_eq!(fold_diacritics("Défense Française"), "defense francaise");
}

#[test]
fn rejects_invalid_ranges_but_accepts_empty_filters() {
    assert!(SearchFilters::parse(&ReviewedGameSearchRequest::default()).is_ok());
    assert!(SearchFilters::parse(&ReviewedGameSearchRequest {
        played_from: Some("2026-08-12".to_string()),
        played_to: Some("2026-08-11".to_string()),
        ..Default::default()
    })
    .is_err());
    assert!(SearchFilters::parse(&ReviewedGameSearchRequest {
        opponent_rating_min: Some(99),
        ..Default::default()
    })
    .is_err());
}

#[test]
fn filters_are_anded_with_accent_folded_openings_and_exact_usernames() {
    let filters = SearchFilters::parse(&ReviewedGameSearchRequest {
        played_from: Some("2026-08-01".to_string()),
        played_to: Some("2026-08-12".to_string()),
        provider: Some(ImportedGameProvider::Lichess),
        opening_eco_prefix: Some("c4".to_string()),
        opening_name: Some("defense francaise".to_string()),
        outcome: Some(ImportedGameOutcome::Win),
        review_side: Some(ImportedGameReviewSide::White),
        time_control_class: Some(ImportedGameTimeControlClass::Rapid),
        opponent_name: Some("exacthandle".to_string()),
        opponent_rating_min: Some(1_700),
        opponent_rating_max: Some(1_900),
    })
    .unwrap();
    let card = fixture_card(0);
    assert!(filters.matches(&card));

    let substring = SearchFilters::parse(&ReviewedGameSearchRequest {
        opponent_name: Some("handle".to_string()),
        ..Default::default()
    })
    .unwrap();
    assert!(!substring.matches(&card));
}

fn resolved_fixture(card: MergedCard) -> ResolvedCard {
    let projected = project_card(card.clone(), Vec::new());
    ResolvedCard { card, projected }
}

#[test]
fn search_caps_twenty_newest_cards_and_reports_the_oldest_boundary() {
    let resolved = (0..21)
        .map(|index| {
            let mut card = fixture_card(index);
            card.ended_at = DateTime::from_timestamp(1_786_531_600 + index, 0);
            card.imported_at = card.ended_at.unwrap();
            resolved_fixture(card)
        })
        .collect::<Vec<_>>();
    let filters = SearchFilters::parse(&ReviewedGameSearchRequest::default()).unwrap();
    let cover = coverage(&resolved).unwrap();
    let selected = select_matches(resolved, &filters).unwrap();

    assert_eq!(cover.reviewed_game_count, 21);
    assert_eq!(selected.total_match_count, 21);
    assert_eq!(selected.games.len(), REVIEWED_GAME_SEARCH_LIMIT);
    assert!(selected
        .games
        .windows(2)
        .all(|pair| pair[0].ended_at > pair[1].ended_at));
    assert_eq!(
        selected.oldest_returned_at,
        selected.games.last().and_then(|game| game.ended_at.clone())
    );
}

#[test]
fn imported_projection_wins_while_preserving_digest_identity() {
    let digested = fixture_card(1);
    let key = digested.identity_key();
    let mut imported = digested.clone();
    imported.game_import_id =
        GameImportId::try_from("game-import:fixture:imported-winner".to_string()).unwrap();
    imported.imported = true;
    imported.digested = false;
    imported.digest_id = None;
    imported.digest_date = None;
    let mut merged = BTreeMap::from([(key.clone(), digested)]);

    merge_imported_card(&mut merged, imported);

    let card = merged.get(&key).unwrap();
    assert!(card.imported && card.digested);
    assert_eq!(
        card.game_import_id.as_str(),
        "game-import:fixture:imported-winner"
    );
    assert_eq!(card.digest_id.as_deref(), Some("daily-2026-08-11"));
}

#[test]
fn kind_namespaced_chess_com_ids_remain_distinct_in_search() {
    let mut live = fixture_card(1);
    live.canonical_source_key = "chessCom:live:42".to_string();
    let mut daily = fixture_card(2);
    daily.canonical_source_key = "chessCom:daily:42".to_string();
    let resolved = vec![resolved_fixture(live), resolved_fixture(daily)];
    let filters = SearchFilters::parse(&ReviewedGameSearchRequest::default()).unwrap();

    let selected = select_matches(resolved, &filters).unwrap();

    assert_eq!(selected.games.len(), 2);
}

#[tokio::test]
async fn search_includes_game_import_records_without_imported_cards() {
    let daily_coaching = DailyCoachingRuntime::in_memory(Arc::new(UnusedProfileValidator), "UTC");
    let imported_games = ImportedGamesRuntime::in_memory();
    let record = fixture_import_record("2026-07-26T10:00:00Z", "reviewed-search");
    let player_id = match &record.owner {
        ProcessorPrincipal::Player(player_id) => player_id.clone(),
        ProcessorPrincipal::LocalCoach => panic!("the fixture record is Player-owned"),
    };
    imported_games.store().create(record).await.unwrap();

    let result = search_reviewed_games(
        &daily_coaching,
        &imported_games,
        &player_id,
        ReviewedGameSearchRequest::default(),
    )
    .await
    .unwrap();

    assert_eq!(result.coverage.reviewed_game_count, 1);
    assert_eq!(result.coverage.earliest_played_at, None);
    assert_eq!(result.coverage.latest_played_at, None);
    assert_eq!(result.games.len(), 1);
    assert!(result.games[0].imported);
    assert!(!result.games[0].digested);
    assert_eq!(
        result.games[0].opponent_name.as_deref(),
        Some("synthetic-white")
    );
    assert_eq!(result.games[0].ended_at, None);
    assert_eq!(
        result.games[0].time_control_class,
        Some(ImportedGameTimeControlClass::Rapid)
    );
}

#[test]
fn game_import_records_fill_gaps_without_overwriting_imported_cards() {
    let record = fixture_import_record("2026-07-26T10:00:00Z", "reviewed-search");
    let card = ImportedGameCard::new(
        record.game_import_id.clone(),
        &record.imported_game,
        r#"[Date "2026.04.28"]
[Time "10:07:47"]
[TimeControl "600+5"]

1. e4 e5 2. Nf3 Nc6 *"#,
        0,
        record.created_at,
    )
    .unwrap();
    let card_ended_at = card.ended_at();
    let merged = merge_cards(Vec::new(), vec![card], std::slice::from_ref(&record));
    assert_eq!(merged.len(), 1);
    let merged = merged.into_values().next().unwrap();
    assert_eq!(merged.ended_at, Some(card_ended_at));
    assert_ne!(merged.ended_at, Some(record.created_at));

    let record_only = merge_cards(Vec::new(), Vec::new(), std::slice::from_ref(&record));
    assert_eq!(record_only.len(), 1);
    let record_only = record_only.into_values().next().unwrap();
    assert_eq!(record_only.ended_at, None);
    assert_eq!(record_only.imported_at, record.created_at);
    assert_eq!(
        record_only.time_control_class,
        Some(ImportedGameTimeControlClass::Rapid)
    );

    let played_on = SearchFilters::parse(&ReviewedGameSearchRequest {
        played_from: Some("2026-07-01".to_string()),
        played_to: Some("2026-07-31".to_string()),
        ..Default::default()
    })
    .unwrap();
    assert!(!played_on.matches(&record_only));
    let rapid = SearchFilters::parse(&ReviewedGameSearchRequest {
        time_control_class: Some(ImportedGameTimeControlClass::Rapid),
        ..Default::default()
    })
    .unwrap();
    assert!(rapid.matches(&record_only));
}

#[tokio::test]
async fn resolve_skips_a_missing_game_import_instead_of_failing_search() {
    let store = Arc::new(InMemoryGameImportStore::default());
    let missing = fixture_card(3);
    let present = fixture_import_record("2026-07-26T10:00:00Z", "reviewed-search");
    let player = present.owner.clone();
    store.create(present.clone()).await.unwrap();
    let listed = BTreeMap::from([(present.game_import_id.clone(), present.clone())]);
    let mut present_card = fixture_card(4);
    present_card.game_import_id = present.game_import_id.clone();
    present_card.learning_path_count = 0;

    let cards = BTreeMap::from([
        (missing.identity_key(), missing),
        (present_card.identity_key(), present_card),
    ]);
    let resolved = resolve_records(cards, player, store, listed).await;

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].card.game_import_id, present.game_import_id);
}

#[test]
fn the_newest_import_of_one_game_is_the_one_searched() {
    // The same Game reviewed at three Elo Profiles is three Game Import
    // records and one entry, and the entry is the latest of them however the
    // store happened to list them.
    let older = fixture_import_record("2026-07-26T10:00:00Z", "elo-1000");
    let newest = fixture_import_record("2026-07-26T10:48:00Z", "elo-1300");
    let middle = fixture_import_record("2026-07-26T10:45:00Z", "elo-1246");

    for listing in [
        vec![older.clone(), middle.clone(), newest.clone()],
        vec![newest.clone(), older.clone(), middle.clone()],
        vec![middle.clone(), newest.clone(), older.clone()],
    ] {
        let merged = merge_cards(Vec::new(), Vec::new(), &listing);
        assert_eq!(merged.len(), 1);
        let entry = merged.into_values().next().unwrap();
        assert_eq!(entry.game_import_id, newest.game_import_id);
        assert_eq!(entry.imported_at, newest.created_at);
    }
}

fn fixture_import_record(created_at: &str, variant: &str) -> GameImportRecord {
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
    let snapshot = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
    )))
    .unwrap();
    GameImportRecord::new(
        GameImportId::try_from(format!("game-import:fixture:{variant}")).unwrap(),
        ProcessorPrincipal::Player(PlayerId::try_from("firebase-player-a".to_string()).unwrap()),
        snapshot,
        review,
        Vec::new(),
        None,
        created_at.parse().unwrap(),
    )
}

fn fixture_card(index: i64) -> MergedCard {
    MergedCard {
        reviewed_game_key: format!("reviewed-key-{index:02}"),
        canonical_source_key: format!("lichess:fixture-{index:02}"),
        game_import_id: GameImportId::try_from(format!("game-import:fixture:search-{index}"))
            .unwrap(),
        provider: ImportedGameProvider::Lichess,
        review_side: ImportedGameReviewSide::White,
        outcome: Some(ImportedGameOutcome::Win),
        opening: Some(ImportedGameOpening {
            eco: "C42".to_string(),
            name: "Défense Française".to_string(),
        }),
        opponent_name: Some("ExactHandle".to_string()),
        opponent_rating: Some(1_800),
        ended_at: Some("2026-08-10T12:00:00Z".parse().unwrap()),
        imported_at: "2026-08-10T12:00:00Z".parse().unwrap(),
        time_control_class: Some(ImportedGameTimeControlClass::Rapid),
        learning_path_count: 1,
        digested: true,
        imported: true,
        digest_id: Some("daily-2026-08-11".to_string()),
        digest_date: Some("2026-08-11".to_string()),
    }
}
