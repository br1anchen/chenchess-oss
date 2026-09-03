use super::*;

#[tokio::test]
async fn retry_exhaustion_settles_the_backfill_obligation() {
    let fixture = publication_fixture_with_result(1, FixtureGameResult::RetryExhausted).await;

    let completed = fixture
        .store
        .publish(
            &fixture.address,
            &fixture.lease,
            fixture.publish_at,
            90,
            false,
        )
        .await
        .unwrap();

    assert_eq!(completed.outcome(), Some(DailyCoachingRunOutcome::NoDigest));
    assert!(!persisted_state(&fixture)
        .await
        .has_unresolved_initial_backfill());
    fixture.server.abort();
}

#[tokio::test]
async fn deadline_interruption_keeps_the_backfill_obligation_owed() {
    let fixture = publication_fixture_with_result(1, FixtureGameResult::DeadlineUnreviewed).await;

    let completed = fixture
        .store
        .publish(
            &fixture.address,
            &fixture.lease,
            fixture.publish_at,
            90,
            false,
        )
        .await
        .unwrap();

    assert_eq!(completed.outcome(), Some(DailyCoachingRunOutcome::NoDigest));
    assert_eq!(
        persisted_state(&fixture).await.connections()[0].initial_backfill(),
        crate::daily_coaching::state::InitialBackfillSnapshot::Owed(fixture.selection)
    );
    fixture.server.abort();
}

async fn persisted_state(fixture: &PublicationFixture) -> DailyCoachingDocument {
    let state_path = FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key);
    fixture
        .store
        .database
        .get_document::<DailyCoachingDocument>(
            &state_path.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await
        .unwrap()
        .unwrap()
}
