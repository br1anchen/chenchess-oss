use super::*;
use crate::daily_coaching::digest::CoachingWindowKind;

#[tokio::test]
async fn publishes_backfill_provenance_and_completes_the_durable_obligation() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player_with_pending_backfill(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let selected = ProfileGameFeed::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo"],
    )))
    .eligible_games_in_window(
        "https://lichess.org/@/PlayerOne",
        window.starts_at,
        window.ends_at,
    )
    .await
    .unwrap()
    .pop()
    .unwrap();
    state_store
        .resolve_initial_backfill(
            &owner(),
            0,
            DailyCoachingProvider::Lichess,
            "playerone".to_string(),
            vec![selected],
        )
        .await
        .unwrap();
    let lifecycle = lifecycle_with_reviewer(
        state_store.clone(),
        run_store.clone(),
        Arc::new(EmptyWindowClient),
        Arc::new(ScriptedReviewer::new([reviewed_result(
            "Synthet1",
            "game-import:daily:backfill",
        )])),
    );

    let report = lifecycle.tick(window.due_at).await.unwrap();
    let (_, cards) = run_store
        .read_digest(&owner(), &run_address(window.coverage_date).run_id)
        .await
        .unwrap()
        .unwrap();
    let state = state_store.read(&owner()).await.unwrap();

    assert_eq!(report.published, 1);
    assert_eq!(cards[0].window_kind, CoachingWindowKind::InitialBackfill);
    assert!(!state.has_unresolved_initial_backfill());
}
