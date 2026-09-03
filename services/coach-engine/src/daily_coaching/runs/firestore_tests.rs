use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::State,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

use super::*;
use crate::{
    daily_coaching::{
        configuration::DailyCoachingConfiguration, digest::FrozenDailyGameReview,
        schedule::DailyWindow, selection::SelectedDailyCoachingGame, DailyCoachingProvider,
        StoredPlayingProfileConnection,
    },
    profile_game_feed::{
        DailyGameInputSource, DailyGameReviewRequest, ProfileGameTimeControlClass,
        ProfileGameWindowEntry,
    },
    review_session_contract::{
        CanonicalGameId, GameImportId, GameReview, ImportProvenance, ImportedGame,
        OperationCompletion, PlayerId, RequestedEloProfile, RequestedReviewSide,
        ReviewSessionEvent, ReviewSessionEventEnvelope, ReviewSide,
    },
};

#[path = "firestore_tests/backfill_settlement.rs"]
mod backfill_settlement;

const TRANSACTION: &str = "fixture-transaction-1";

#[derive(Debug, Clone, PartialEq)]
enum RecordedRequest {
    Begin(String),
    Read { path: String, transaction: String },
    Commit(Value),
    CommitConflict(String),
    Rollback(String),
}

#[derive(Default)]
struct FakeFirestore {
    documents: BTreeMap<String, Value>,
    requests: Vec<RecordedRequest>,
    next_transaction: u64,
    conflict_on_next_commit: Option<ConflictInjection>,
}

struct ConflictInjection {
    document_path: String,
    replacement: Value,
}

struct PublicationFixture {
    firestore: Arc<Mutex<FakeFirestore>>,
    store: FirestoreDailyCoachingRunStore,
    address: DailyCoachingRunAddress,
    lease: DailyCoachingRunLease,
    selection: Vec<ProfileGameWindowEntry>,
    publish_at: DateTime<Utc>,
    server: tokio::task::JoinHandle<()>,
}

#[derive(Clone, Copy)]
enum FixtureGameResult {
    Reviewed,
    Terminal,
    RetryExhausted,
    DeadlineUnreviewed,
}

#[tokio::test]
async fn maximum_publication_atomically_commits_digest_cards_backfill_and_run() {
    let fixture = publication_fixture(10).await;

    let publication = fixture
        .store
        .publish(
            &fixture.address,
            &fixture.lease,
            fixture.publish_at,
            90,
            false,
        )
        .await;
    let published = match publication {
        Ok(published) => published,
        Err(error) => {
            let requests = fixture.firestore.lock().await.requests.clone();
            panic!("publication failed with {error:?} after requests {requests:#?}")
        }
    };

    assert_eq!(
        published.outcome(),
        Some(DailyCoachingRunOutcome::Published)
    );
    let expected_reads = expected_publication_reads(&fixture.address, &fixture.selection);
    let firestore = fixture.firestore.lock().await;
    let expected_prefix = std::iter::once(RecordedRequest::Begin(TRANSACTION.to_string()))
        .chain(
            expected_reads
                .into_iter()
                .map(|path| RecordedRequest::Read {
                    path,
                    transaction: TRANSACTION.to_string(),
                }),
        )
        .collect::<Vec<_>>();
    assert_eq!(firestore.requests.len(), expected_prefix.len() + 1);
    assert_eq!(
        &firestore.requests[..expected_prefix.len()],
        expected_prefix.as_slice()
    );
    let RecordedRequest::Commit(commit) = firestore.requests.last().unwrap() else {
        panic!("publication must end by committing the prepared writes")
    };
    assert_eq!(commit["transaction"], TRANSACTION);
    let writes = commit["writes"].as_array().unwrap();
    assert_eq!(writes.len(), 13);
    let expected_create_paths = std::iter::once(
        FirestoreDailyCoachingRunStore::digest_path(
            &fixture.address.owner_key,
            &fixture.address.run_id,
        )
        .join("/"),
    )
    .chain(fixture.selection.iter().map(|selected| {
        FirestoreDailyCoachingRunStore::card_path(
            &fixture.address.owner_key,
            &selected.source_identity,
        )
        .join("/")
    }))
    .collect::<Vec<_>>();
    assert_eq!(
        writes[..11]
            .iter()
            .map(written_document_path)
            .collect::<Vec<_>>(),
        expected_create_paths
    );
    assert!(writes[..11]
        .iter()
        .all(|write| write["currentDocument"]["exists"] == false));
    assert_eq!(
        written_document_path(&writes[11]),
        FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key).join("/")
    );
    assert_eq!(writes[11]["currentDocument"]["exists"], true);
    assert_eq!(
        written_document_path(writes.last().unwrap()),
        FirestoreDailyCoachingRunStore::document_path(&fixture.address).join("/")
    );
    assert_eq!(writes.last().unwrap()["currentDocument"]["exists"], true);
    drop(firestore);

    let state_path = FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key);
    let state_path = state_path.iter().map(String::as_str).collect::<Vec<_>>();
    let state = fixture
        .store
        .database
        .get_document::<DailyCoachingDocument>(&state_path)
        .await
        .unwrap()
        .unwrap();
    state.validate_for(&fixture.address.owner_key).unwrap();
    assert!(!state.has_unresolved_initial_backfill());
    fixture.server.abort();
}

#[tokio::test]
async fn stale_publication_lease_rolls_back_after_the_run_and_state_fence_reads() {
    let fixture = publication_fixture(1).await;
    let mut stale_lease = fixture.lease.clone();
    stale_lease.holder_id = "stale-holder".to_string();

    let result = fixture
        .store
        .publish(
            &fixture.address,
            &stale_lease,
            fixture.publish_at,
            90,
            false,
        )
        .await;

    let firestore = fixture.firestore.lock().await;
    assert_eq!(
        result,
        Err(DailyCoachingRunStoreError::Fenced),
        "requests: {:#?}",
        firestore.requests
    );
    assert_eq!(
        firestore.requests,
        vec![
            RecordedRequest::Begin(TRANSACTION.to_string()),
            RecordedRequest::Read {
                path: FirestoreDailyCoachingRunStore::document_path(&fixture.address).join("/"),
                transaction: TRANSACTION.to_string(),
            },
            RecordedRequest::Read {
                path: FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key)
                    .join("/"),
                transaction: TRANSACTION.to_string(),
            },
            RecordedRequest::Rollback(TRANSACTION.to_string()),
        ]
    );
    drop(firestore);
    fixture.server.abort();
}

#[tokio::test]
async fn terminal_backfill_atomically_completes_the_run_and_durable_obligation() {
    let fixture = publication_fixture_with_result(1, FixtureGameResult::Terminal).await;

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
    let firestore = fixture.firestore.lock().await;
    let RecordedRequest::Commit(commit) = firestore.requests.last().unwrap() else {
        panic!("terminal completion must commit the state and Run together")
    };
    let writes = commit["writes"].as_array().unwrap();
    assert_eq!(writes.len(), 2);
    assert_eq!(
        written_document_path(&writes[0]),
        FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key).join("/")
    );
    assert_eq!(
        written_document_path(&writes[1]),
        FirestoreDailyCoachingRunStore::document_path(&fixture.address).join("/")
    );
    drop(firestore);
    let owned_state_path = FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key);
    let state_path = owned_state_path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let state = fixture
        .store
        .database
        .get_document::<DailyCoachingDocument>(&state_path)
        .await
        .unwrap()
        .unwrap();
    state.validate_for(&fixture.address.owner_key).unwrap();
    assert!(!state.has_unresolved_initial_backfill());
    fixture.server.abort();
}

#[tokio::test]
async fn overlapping_runs_publish_one_backfill_card_and_settle_the_obligation_once() {
    let fixture = publication_fixture(1).await;
    let configuration = DailyCoachingConfiguration::standard();
    let state_path = FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key);
    let state = fixture
        .store
        .database
        .get_document::<DailyCoachingDocument>(
            &state_path.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await
        .unwrap()
        .unwrap();
    state.validate_for(&fixture.address.owner_key).unwrap();
    let second_window = DailyWindow::resolve(
        &fixture.address.owner_key,
        chrono_tz::UTC,
        NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
        &configuration,
    )
    .unwrap();
    let second_publish_at = second_window.due_at;
    let mut second_run = DailyCoachingRunDocument::claimed(
        &state,
        &second_window,
        "second-holder",
        second_publish_at,
        &configuration,
    )
    .unwrap();
    let second_address = second_run.address();
    let second_lease = second_run.lease().unwrap().clone();
    second_run
        .freeze_selection(
            &second_lease,
            vec![SelectedDailyCoachingGame {
                selected: fixture.selection[0].clone(),
                window_kind: crate::daily_coaching::digest::CoachingWindowKind::InitialBackfill,
            }],
        )
        .unwrap();
    second_run
        .record_game(
            &second_lease,
            0,
            DailyCoachingGameResult::Reviewed(frozen_review(&fixture.selection[0], 0)),
            second_publish_at,
            None,
        )
        .unwrap();
    assert!(matches!(
        fixture.store.create(second_run).await.unwrap(),
        DailyCoachingRunClaim::Created(_)
    ));
    fixture.firestore.lock().await.requests.clear();

    let first = fixture
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
    let second = fixture
        .store
        .publish(&second_address, &second_lease, second_publish_at, 90, false)
        .await
        .unwrap();

    assert_eq!(first.outcome(), Some(DailyCoachingRunOutcome::Published));
    assert_eq!(second.outcome(), Some(DailyCoachingRunOutcome::NoDigest));
    let firestore = fixture.firestore.lock().await;
    assert_eq!(
        firestore
            .documents
            .keys()
            .filter(|path| path.contains("/coachingDigests/"))
            .count(),
        1
    );
    assert_eq!(
        firestore
            .documents
            .keys()
            .filter(|path| path.contains("/digestedGames/"))
            .count(),
        1
    );
    drop(firestore);
    let state = fixture
        .store
        .database
        .get_document::<DailyCoachingDocument>(
            &state_path.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await
        .unwrap()
        .unwrap();
    state.validate_for(&fixture.address.owner_key).unwrap();
    assert!(!state.has_unresolved_initial_backfill());
    fixture.server.abort();
}

#[tokio::test]
async fn mutate_rolls_back_without_committing_when_write_preparation_fails_validation() {
    let fixture = publication_fixture(1).await;

    let result = fixture
        .store
        .mutate(&fixture.address, |run| {
            run.purge_at = run.claimed_at;
            Ok(())
        })
        .await;

    let firestore = fixture.firestore.lock().await;
    assert_eq!(result, Err(DailyCoachingRunStoreError::InvalidRecord));
    assert_eq!(
        firestore.requests,
        vec![
            RecordedRequest::Begin(TRANSACTION.to_string()),
            RecordedRequest::Read {
                path: FirestoreDailyCoachingRunStore::document_path(&fixture.address).join("/"),
                transaction: TRANSACTION.to_string(),
            },
            RecordedRequest::Rollback(TRANSACTION.to_string()),
        ]
    );
    drop(firestore);
    fixture.server.abort();
}

#[tokio::test]
async fn mutate_at_state_fence_rolls_back_without_committing_when_write_preparation_fails_validation(
) {
    let fixture = publication_fixture(1).await;

    let result = fixture
        .store
        .mutate_at_state_fence(&fixture.address, |run, _| {
            run.purge_at = run.claimed_at;
            Ok(true)
        })
        .await;

    let firestore = fixture.firestore.lock().await;
    assert_eq!(result, Err(DailyCoachingRunStoreError::InvalidRecord));
    assert_eq!(
        firestore.requests,
        vec![
            RecordedRequest::Begin(TRANSACTION.to_string()),
            RecordedRequest::Read {
                path: FirestoreDailyCoachingRunStore::document_path(&fixture.address).join("/"),
                transaction: TRANSACTION.to_string(),
            },
            RecordedRequest::Read {
                path: FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key)
                    .join("/"),
                transaction: TRANSACTION.to_string(),
            },
            RecordedRequest::Rollback(TRANSACTION.to_string()),
        ]
    );
    drop(firestore);
    fixture.server.abort();
}

#[tokio::test]
async fn backfill_update_retries_a_post_read_conflict_and_fences_the_stale_mutation() {
    let fixture = publication_fixture(1).await;
    let player = PlayerId::try_from("firebase-player-publication-boundary".to_string()).unwrap();
    let mut pending = DailyCoachingDocument::empty(fixture.address.owner_key.clone());
    pending
        .connect(
            &player,
            StoredPlayingProfileConnection::test(DailyCoachingProvider::Lichess, "PlayerOne"),
            "UTC".to_string(),
            fixture.publish_at - TimeDelta::days(1),
        )
        .unwrap();
    let mut pending_document = encoded_document(&fixture, "pending-state", &pending).await;
    let mut replacement = pending.clone();
    replacement
        .replace(
            StoredPlayingProfileConnection::test(DailyCoachingProvider::Lichess, "PlayerTwo"),
            "playerone",
        )
        .unwrap();
    assert_eq!(replacement.run_fence(), 1);
    let mut replacement_document =
        encoded_document(&fixture, "replacement-state", &replacement).await;
    let state_path =
        FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key).join("/");
    {
        let mut firestore = fixture.firestore.lock().await;
        let state_name = firestore.documents[&state_path]["name"].clone();
        pending_document["name"] = state_name.clone();
        replacement_document["name"] = state_name;
        firestore
            .documents
            .insert(state_path.clone(), pending_document);
        firestore.conflict_on_next_commit = Some(ConflictInjection {
            document_path: state_path.clone(),
            replacement: replacement_document,
        });
        firestore.requests.clear();
    }
    let run = fixture.store.read(&fixture.address).await.unwrap().unwrap();
    let connection = run.connections()[0].clone();

    let result = fixture
        .store
        .update_initial_backfill(
            &fixture.address,
            &fixture.lease,
            &connection,
            crate::daily_coaching::state::InitialBackfillMutation::Resolve(vec![fixture.selection
                [0]
            .clone()]),
        )
        .await;

    assert_eq!(
        result,
        Err(DailyCoachingRunStoreError::Fenced),
        "requests: {:#?}",
        fixture.firestore.lock().await.requests
    );
    let run_path = FirestoreDailyCoachingRunStore::document_path(&fixture.address).join("/");
    let firestore = fixture.firestore.lock().await;
    assert_eq!(
        firestore.requests,
        vec![
            RecordedRequest::Begin("fixture-transaction-1".to_string()),
            RecordedRequest::Read {
                path: run_path.clone(),
                transaction: "fixture-transaction-1".to_string(),
            },
            RecordedRequest::Read {
                path: state_path.clone(),
                transaction: "fixture-transaction-1".to_string(),
            },
            RecordedRequest::CommitConflict("fixture-transaction-1".to_string()),
            RecordedRequest::Begin("fixture-transaction-2".to_string()),
            RecordedRequest::Read {
                path: run_path,
                transaction: "fixture-transaction-2".to_string(),
            },
            RecordedRequest::Read {
                path: state_path,
                transaction: "fixture-transaction-2".to_string(),
            },
            RecordedRequest::Rollback("fixture-transaction-2".to_string()),
        ]
    );
    drop(firestore);
    let state = fixture
        .store
        .database
        .get_document::<DailyCoachingDocument>(
            &FirestoreDailyCoachingStore::document_path(&fixture.address.owner_key)
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )
        .await
        .unwrap()
        .unwrap();
    state.validate_for(&fixture.address.owner_key).unwrap();
    assert_eq!(state.connections()[0].identity_username(), "playertwo");
    assert!(matches!(
        state.connections()[0].initial_backfill(),
        crate::daily_coaching::state::InitialBackfillSnapshot::Pending { .. }
    ));
    fixture.server.abort();
}

#[tokio::test]
async fn rebuild_refuses_a_superseded_digest_whose_card_keys_are_not_publishable_paths() {
    let fixture = publication_fixture(1).await;
    fixture
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
    let digest_path = FirestoreDailyCoachingRunStore::digest_path(
        &fixture.address.owner_key,
        &fixture.address.run_id,
    )
    .join("/");
    // A card key the rebuild drops is deleted by path. A blank one is not a path.
    {
        let mut firestore = fixture.firestore.lock().await;
        firestore.documents.get_mut(&digest_path).unwrap()["fields"]["orderedCardKeys"]
            ["arrayValue"]["values"][0]["stringValue"] = Value::String("   ".to_string());
    }

    let reopened_at = fixture.publish_at + TimeDelta::hours(2);
    let reopened = fixture
        .store
        .reopen_for_regeneration(
            &fixture.address,
            "regeneration-holder",
            reopened_at,
            Duration::from_secs(300),
            reopened_at + TimeDelta::hours(4),
        )
        .await
        .unwrap();
    let lease = reopened.lease().unwrap().clone();
    fixture
        .store
        .freeze_selection(
            &fixture.address,
            &lease,
            fixture
                .selection
                .iter()
                .map(|selected| SelectedDailyCoachingGame {
                    selected: selected.clone(),
                    window_kind: crate::daily_coaching::digest::CoachingWindowKind::Daily,
                })
                .collect(),
            reopened_at,
            90,
        )
        .await
        .unwrap();
    for (index, selected) in fixture.selection.iter().enumerate() {
        fixture
            .store
            .record_game(
                &fixture.address,
                &lease,
                index,
                DailyCoachingGameResult::Reviewed(frozen_review(selected, index)),
                reopened_at,
                None,
                90,
            )
            .await
            .unwrap();
    }

    fixture.firestore.lock().await.requests.clear();
    assert_eq!(
        fixture
            .store
            .publish(&fixture.address, &lease, reopened_at, 90, false)
            .await,
        Err(DailyCoachingRunStoreError::InvalidRecord)
    );
    let firestore = fixture.firestore.lock().await;
    assert!(
        !firestore
            .requests
            .iter()
            .any(|request| matches!(request, RecordedRequest::Commit(_))),
        "the rebuild must not commit any write derived from the rejected digest"
    );
    drop(firestore);
    fixture.server.abort();
}

#[tokio::test]
async fn the_archive_omits_a_digest_its_own_summary_contract_rejects() {
    let fixture = publication_fixture(1).await;
    fixture
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
    let digest_path = FirestoreDailyCoachingRunStore::digest_path(
        &fixture.address.owner_key,
        &fixture.address.run_id,
    )
    .join("/");
    let poisoned_id = "daily-2026-08-08";

    // A digest whose summary no longer describes its own games — the state a
    // re-keying migration leaves behind when it moves the Game Import IDs the
    // summary carries but not every reference that cites them.
    {
        let mut firestore = fixture.firestore.lock().await;
        let mut poisoned = firestore.documents.get(&digest_path).unwrap().clone();
        poisoned["fields"]["digestId"]["stringValue"] = Value::String(poisoned_id.to_string());
        poisoned["fields"]["coverageDate"]["stringValue"] = Value::String("2026-08-08".to_string());
        poisoned["fields"]["gameCount"]["integerValue"] = Value::String("2".to_string());
        let poisoned_path =
            FirestoreDailyCoachingRunStore::digest_path(&fixture.address.owner_key, poisoned_id)
                .join("/");
        poisoned["name"] = Value::String(
            poisoned["name"]
                .as_str()
                .unwrap()
                .replace(&fixture.address.run_id, poisoned_id),
        );
        firestore.documents.insert(poisoned_path, poisoned);
    }

    let archive = fixture
        .store
        .archive(&fixture.address.owner_key)
        .await
        .unwrap();
    assert_eq!(
        archive
            .iter()
            .map(|digest| digest.digest_id.as_str())
            .collect::<Vec<_>>(),
        vec![fixture.address.run_id.as_str()],
        "the readable digest must survive a sibling the summary contract rejects"
    );
    fixture.server.abort();
}

#[tokio::test]
async fn digest_reads_reject_embedded_identity_that_disagrees_with_the_authenticated_path() {
    let fixture = publication_fixture(1).await;
    fixture
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
    let digest_path = FirestoreDailyCoachingRunStore::digest_path(
        &fixture.address.owner_key,
        &fixture.address.run_id,
    )
    .join("/");
    let original = fixture
        .firestore
        .lock()
        .await
        .documents
        .get(&digest_path)
        .unwrap()
        .clone();

    {
        let mut firestore = fixture.firestore.lock().await;
        firestore.documents.get_mut(&digest_path).unwrap()["fields"]["ownerKey"]["stringValue"] =
            Value::String("0".repeat(64));
    }
    assert_eq!(
        fixture
            .store
            .read_digest(&fixture.address.owner_key, &fixture.address.run_id)
            .await,
        Err(DailyCoachingRunStoreError::InvalidRecord)
    );

    {
        let mut firestore = fixture.firestore.lock().await;
        let corrupted = firestore.documents.get_mut(&digest_path).unwrap();
        *corrupted = original;
        corrupted["fields"]["digestId"]["stringValue"] =
            Value::String("daily-2026-08-08".to_string());
    }
    assert_eq!(
        fixture
            .store
            .read_digest(&fixture.address.owner_key, &fixture.address.run_id)
            .await,
        Err(DailyCoachingRunStoreError::InvalidRecord)
    );
    fixture.server.abort();
}

async fn publication_fixture(game_count: usize) -> PublicationFixture {
    publication_fixture_with_result(game_count, FixtureGameResult::Reviewed).await
}

async fn publication_fixture_with_result(
    game_count: usize,
    result: FixtureGameResult,
) -> PublicationFixture {
    let firestore = Arc::new(Mutex::new(FakeFirestore::default()));
    let application = Router::new()
        .fallback(fake_firestore_request)
        .with_state(firestore.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let server_address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, application).await;
    });
    let database =
        FirestoreDatabase::emulator("chenchess-test", server_address.to_string()).unwrap();
    let store = FirestoreDailyCoachingRunStore::new(database.clone());
    let player = PlayerId::try_from("firebase-player-publication-boundary".to_string()).unwrap();
    let owner_key = DailyCoachingOwnerKey::for_player(&player);
    let publish_at = instant("2026-08-10T03:00:00Z");
    let mut state = DailyCoachingDocument::empty(owner_key.clone());
    state
        .connect(
            &player,
            StoredPlayingProfileConnection::test(DailyCoachingProvider::Lichess, "PlayerOne"),
            "UTC".to_string(),
            publish_at - TimeDelta::days(1),
        )
        .unwrap();
    let configuration = DailyCoachingConfiguration::standard();
    let window = DailyWindow::resolve(
        &owner_key,
        chrono_tz::UTC,
        NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
        &configuration,
    )
    .unwrap();
    let mut run = DailyCoachingRunDocument::claimed(
        &state,
        &window,
        "active-holder",
        publish_at,
        &configuration,
    )
    .unwrap();
    let lease = run.lease().unwrap().clone();
    let selection = (0..game_count)
        .map(|index| selected_game(&window, index))
        .collect::<Vec<_>>();
    state
        .resolve_initial_backfill(
            state.run_fence(),
            DailyCoachingProvider::Lichess,
            "playerone",
            selection.iter().take(5).cloned().collect(),
        )
        .unwrap();
    run.freeze_selection(
        &lease,
        selection
            .iter()
            .enumerate()
            .map(|(index, selected)| SelectedDailyCoachingGame {
                selected: selected.clone(),
                window_kind: if index < 5 {
                    crate::daily_coaching::digest::CoachingWindowKind::InitialBackfill
                } else {
                    crate::daily_coaching::digest::CoachingWindowKind::Daily
                },
            })
            .collect(),
    )
    .unwrap();
    for (index, selected) in selection.iter().enumerate() {
        let result = match result {
            FixtureGameResult::Reviewed => {
                DailyCoachingGameResult::Reviewed(frozen_review(selected, index))
            }
            FixtureGameResult::Terminal => DailyCoachingGameResult::Terminal,
            FixtureGameResult::RetryExhausted => {
                DailyCoachingGameResult::RetryExhausted { attempted: true }
            }
            FixtureGameResult::DeadlineUnreviewed => DailyCoachingGameResult::UnfinishedAtDeadline,
        };
        run.record_game(&lease, index, result, publish_at, None)
            .unwrap();
    }
    let serialized_run = serde_json::to_value(&run).unwrap();
    let direct_round_trip =
        serde_json::from_value::<DailyCoachingRunDocument>(serialized_run.clone()).unwrap_or_else(
            |error| panic!("direct Run JSON round-trip failed with {error}: {serialized_run:#}"),
        );
    assert_eq!(direct_round_trip, run);
    let address = run.address();
    create_document_at(
        &database,
        &FirestoreDailyCoachingStore::document_path(&owner_key),
        &state,
    )
    .await;
    let state_path = FirestoreDailyCoachingStore::document_path(&owner_key);
    let state_path = state_path.iter().map(String::as_str).collect::<Vec<_>>();
    let stored_state = database
        .get_document::<DailyCoachingDocument>(&state_path)
        .await
        .unwrap()
        .unwrap();
    stored_state.validate_for(&owner_key).unwrap();
    assert_eq!(stored_state, state);
    let expected_run = run.clone();
    assert!(matches!(
        store.create(run).await.unwrap(),
        DailyCoachingRunClaim::Created(_)
    ));
    let run_path = FirestoreDailyCoachingRunStore::document_path(&address);
    let run_path = run_path.iter().map(String::as_str).collect::<Vec<_>>();
    let stored_run = database
        .get_document::<DailyCoachingRunDocument>(&run_path)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_run, expected_run);
    firestore.lock().await.requests.clear();
    PublicationFixture {
        firestore,
        store,
        address,
        lease,
        selection,
        publish_at,
        server,
    }
}

async fn create_document_at<T: Serialize>(
    database: &FirestoreDatabase,
    document_path: &[String],
    document: &T,
) {
    let (document_id, collection_path) = document_path.split_last().unwrap();
    let collection_path = collection_path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    database
        .create_document(&collection_path, document_id, document, &[])
        .await
        .unwrap();
}

async fn encoded_document<T: Serialize>(
    fixture: &PublicationFixture,
    document_id: &str,
    document: &T,
) -> Value {
    let document_path = ["fixtureDocuments".to_string(), document_id.to_string()];
    create_document_at(&fixture.store.database, &document_path, document).await;
    fixture
        .firestore
        .lock()
        .await
        .documents
        .remove(&document_path.join("/"))
        .unwrap()
}

fn expected_publication_reads(
    address: &DailyCoachingRunAddress,
    selection: &[ProfileGameWindowEntry],
) -> Vec<String> {
    std::iter::once(FirestoreDailyCoachingRunStore::document_path(address).join("/"))
        .chain(std::iter::once(
            FirestoreDailyCoachingStore::document_path(&address.owner_key).join("/"),
        ))
        .chain(selection.iter().map(|selected| {
            FirestoreDailyCoachingRunStore::card_path(&address.owner_key, &selected.source_identity)
                .join("/")
        }))
        .chain(std::iter::once(
            FirestoreDailyCoachingRunStore::digest_path(&address.owner_key, &address.run_id)
                .join("/"),
        ))
        .collect()
}

fn selected_game(window: &DailyWindow, index: usize) -> ProfileGameWindowEntry {
    let game_id = format!("85SQH9d{index}");
    let ended_at_unix_milliseconds =
        u64::try_from((window.ends_at - TimeDelta::hours(1)).timestamp_millis()).unwrap();
    ProfileGameWindowEntry {
        source_identity: ProfileGameSourceIdentity::lichess(game_id.clone()),
        source_profile: "https://lichess.org/@/PlayerOne".to_string(),
        review_request: DailyGameReviewRequest {
            source: DailyGameInputSource::LichessUrl {
                url: format!("https://lichess.org/{game_id}"),
            },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::Black,
            },
            elo_profile: RequestedEloProfile::FromImportedMetadata,
            ended_at_unix_milliseconds: Some(ended_at_unix_milliseconds),
        },
        ended_at_unix_milliseconds,
        time_control_raw: "600+0".to_string(),
        time_control_class: ProfileGameTimeControlClass::Rapid,
        expected_clock_seconds: Some(600),
        played_plies: 90,
    }
}

fn frozen_review(selected: &ProfileGameWindowEntry, index: usize) -> FrozenDailyGameReview {
    let mut imported: ImportedGame = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
    )))
    .unwrap();
    let ImportProvenance::Lichess {
        canonical_game_id,
        side_qualified_url,
        canonical_url,
        ..
    } = &mut imported.provenance
    else {
        panic!("the imported Game fixture must use Lichess provenance")
    };
    let game_id = &selected.source_identity.game_id;
    *canonical_game_id = CanonicalGameId::try_from(game_id.clone()).unwrap();
    *side_qualified_url = format!("https://lichess.org/{game_id}0000/black");
    *canonical_url = format!("https://lichess.org/{game_id}");
    FrozenDailyGameReview::capture(
        selected,
        GameImportId::try_from(format!("game-import:daily:boundary-{index}")).unwrap(),
        &imported,
        &fixture_review(),
    )
    .unwrap()
}

fn fixture_review() -> GameReview {
    let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/events.json"
    )))
    .unwrap();
    events
        .into_iter()
        .find_map(|envelope| match envelope.event {
            ReviewSessionEvent::Completed { result } => match *result {
                OperationCompletion::GameImported { review, .. } => Some(*review),
                _ => None,
            },
            _ => None,
        })
        .unwrap()
}

fn instant(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn written_document_path(write: &Value) -> String {
    write["update"]["name"]
        .as_str()
        .unwrap()
        .split("/documents/")
        .nth(1)
        .unwrap()
        .to_string()
}

async fn fake_firestore_request(
    State(state): State<Arc<Mutex<FakeFirestore>>>,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> Response {
    // A transactional read arrives as `:batchGet`, which is the documented
    // transactional read and the one the Firebase emulator answers.
    if method == Method::POST && uri.path().ends_with("documents:batchGet") {
        let request: Value = serde_json::from_slice(&body).unwrap();
        let name = request["documents"][0].as_str().unwrap().to_string();
        let document_path = name.split("/documents/").nth(1).unwrap().to_string();
        let mut state = state.lock().await;
        state.requests.push(RecordedRequest::Read {
            path: document_path.clone(),
            transaction: request["transaction"].as_str().unwrap().to_string(),
        });
        return match state.documents.get(&document_path) {
            Some(document) => {
                Json(serde_json::json!([{ "found": document.clone() }])).into_response()
            }
            None => Json(serde_json::json!([{ "missing": name }])).into_response(),
        };
    }
    let request_path = uri.path();
    if method == Method::POST && request_path.ends_with("documents:beginTransaction") {
        let mut state = state.lock().await;
        state.next_transaction += 1;
        let transaction = format!("fixture-transaction-{}", state.next_transaction);
        state
            .requests
            .push(RecordedRequest::Begin(transaction.clone()));
        return Json(serde_json::json!({ "transaction": transaction })).into_response();
    }
    if method == Method::POST && request_path.ends_with("documents:commit") {
        let commit: Value = serde_json::from_slice(&body).unwrap();
        let mut state = state.lock().await;
        if let Some(conflict) = state.conflict_on_next_commit.take() {
            state
                .documents
                .insert(conflict.document_path, conflict.replacement);
            state.requests.push(RecordedRequest::CommitConflict(
                commit["transaction"].as_str().unwrap().to_string(),
            ));
            return StatusCode::CONFLICT.into_response();
        }
        let writes = commit["writes"].as_array().unwrap();
        for write in writes {
            let path = written_document_path(write);
            match write["currentDocument"]["exists"].as_bool() {
                Some(false) if state.documents.contains_key(&path) => {
                    return StatusCode::CONFLICT.into_response();
                }
                Some(true) if !state.documents.contains_key(&path) => {
                    return StatusCode::CONFLICT.into_response();
                }
                _ => {}
            }
        }
        for write in writes {
            let path = written_document_path(write);
            state.documents.insert(path, write["update"].clone());
        }
        state.requests.push(RecordedRequest::Commit(commit));
        return StatusCode::OK.into_response();
    }
    if method == Method::POST && request_path.ends_with("documents:rollback") {
        let rollback: Value = serde_json::from_slice(&body).unwrap();
        let transaction = rollback["transaction"].as_str().unwrap().to_string();
        state
            .lock()
            .await
            .requests
            .push(RecordedRequest::Rollback(transaction));
        return StatusCode::OK.into_response();
    }
    if method == Method::GET && uri.query().is_some_and(|query| query.contains("pageSize=")) {
        let collection_prefix = format!(
            "{}/",
            request_path.split("/documents/").nth(1).unwrap_or_default()
        );
        let state = state.lock().await;
        let documents = state
            .documents
            .iter()
            .filter_map(|(path, document)| {
                path.strip_prefix(&collection_prefix)
                    .filter(|document_id| !document_id.contains('/'))
                    .map(|_| document.clone())
            })
            .collect::<Vec<_>>();
        return Json(serde_json::json!({ "documents": documents })).into_response();
    }
    if method == Method::GET {
        let document_path = request_path
            .split("/documents/")
            .nth(1)
            .unwrap_or_default()
            .to_string();
        let transaction = uri
            .query()
            .and_then(|query| query.strip_prefix("transaction="))
            .map(str::to_string);
        let mut state = state.lock().await;
        if let Some(transaction) = transaction {
            state.requests.push(RecordedRequest::Read {
                path: document_path.clone(),
                transaction,
            });
        }
        return state
            .documents
            .get(&document_path)
            .cloned()
            .map(Json)
            .map(IntoResponse::into_response)
            .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response());
    }
    if method == Method::POST {
        let Some(document_id) = uri
            .query()
            .and_then(|query| query.strip_prefix("documentId="))
        else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let Some(collection_path) = request_path.split("/documents/").nth(1) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let document_path = format!("{collection_path}/{document_id}");
        let mut document: Value = serde_json::from_slice(&body).unwrap();
        document["name"] = Value::String(format!(
            "{}/{}",
            request_path.trim_start_matches("/v1/"),
            document_id
        ));
        let mut state = state.lock().await;
        if state.documents.contains_key(&document_path) {
            return StatusCode::CONFLICT.into_response();
        }
        state.documents.insert(document_path, document);
        return StatusCode::OK.into_response();
    }
    StatusCode::BAD_REQUEST.into_response()
}
