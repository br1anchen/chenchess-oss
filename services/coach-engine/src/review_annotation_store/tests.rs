use super::*;
use crate::review_session_contract::{ArtifactDigest, CriticalMomentGroundingLedger, PlayerId};

fn player(seed: &str) -> ProcessorPrincipal {
    ProcessorPrincipal::Player(PlayerId::try_from(format!("firebase-player-{seed}")).unwrap())
}

fn address(seed: &str) -> ReviewAnnotationAddress {
    ReviewAnnotationAddress {
        owner: player(seed),
        game_import_id: GameImportId::try_from(format!(
            "game-import:{}:{}",
            "a".repeat(64),
            "b".repeat(32)
        ))
        .unwrap(),
    }
}

fn moment(seed: &str) -> CriticalMomentId {
    CriticalMomentId::try_from(format!("moment:{seed}")).unwrap()
}

fn key(seed: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(format!("idempotency-key:test:{seed}")).unwrap()
}

fn annotation(
    moment_seed: &str,
    key_seed: &str,
    text: &str,
    published_at: &str,
) -> ReviewMomentAnnotation {
    ReviewMomentAnnotation {
        moment_id: moment(moment_seed),
        idempotency_key: key(key_seed),
        comment: CriticalMomentComment {
            text: text.to_string(),
        },
        authoring_provenance: CriticalMomentCommentAuthoringProvenance::hosted_authored(
            CriticalMomentGroundingLedger {
                facts_ref: ArtifactDigest::try_from(format!("sha256:{}", "c".repeat(64))).unwrap(),
                factual_claims: Vec::new(),
            },
            1,
        ),
        published_at: published_at.parse().unwrap(),
    }
}

#[tokio::test]
async fn one_idempotency_key_writes_one_annotation_however_often_it_is_replayed() {
    let store = InMemoryReviewAnnotationStore::default();
    let address = address("replay");

    let first = store
        .append(
            &address,
            annotation("1", "once", "first text", "2026-08-09T10:00:00Z"),
        )
        .await
        .unwrap();
    let replayed = store
        .append(
            &address,
            annotation("1", "once", "rewritten text", "2026-08-09T11:00:00Z"),
        )
        .await
        .unwrap();

    assert_eq!(replayed, first);
    let stored = store.read(&address).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(
        stored
            .active(&moment("1"))
            .map(|active| active.comment.text.as_str()),
        Some("first text")
    );
}

#[tokio::test]
async fn a_distinct_key_appends_and_the_newest_annotation_is_active() {
    let store = InMemoryReviewAnnotationStore::default();
    let address = address("append");

    store
        .append(
            &address,
            annotation("1", "first", "earlier", "2026-08-09T10:00:00Z"),
        )
        .await
        .unwrap();
    store
        .append(
            &address,
            annotation("1", "second", "later", "2026-08-09T12:00:00Z"),
        )
        .await
        .unwrap();

    let stored = store.read(&address).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(
        stored
            .active(&moment("1"))
            .map(|active| active.comment.text.as_str()),
        Some("later")
    );
    // Nothing was overwritten: the earlier key still replays its own comment.
    assert_eq!(
        stored
            .for_key(&moment("1"), &key("first"))
            .map(|earlier| earlier.comment.text.as_str()),
        Some("earlier")
    );
}

#[tokio::test]
async fn annotations_written_in_the_same_instant_order_deterministically() {
    let store = InMemoryReviewAnnotationStore::default();
    let address = address("concurrent");
    let instant = "2026-08-09T10:00:00Z";

    store
        .append(&address, annotation("1", "zulu", "zulu text", instant))
        .await
        .unwrap();
    store
        .append(&address, annotation("1", "alpha", "alpha text", instant))
        .await
        .unwrap();

    assert_eq!(
        store
            .read(&address)
            .await
            .unwrap()
            .active(&moment("1"))
            .map(|active| active.comment.text.as_str()),
        Some("zulu text")
    );
}

#[tokio::test]
async fn each_review_moment_and_player_addresses_its_own_annotations() {
    let store = InMemoryReviewAnnotationStore::default();
    let owned = address("owner");
    store
        .append(
            &owned,
            annotation("1", "only", "mine", "2026-08-09T10:00:00Z"),
        )
        .await
        .unwrap();

    let stored = store.read(&owned).await.unwrap();
    assert!(stored.active(&moment("2")).is_none());
    assert!(store
        .read(&address("other-owner"))
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_loaded_log_answers_from_its_snapshot_and_adopts_what_it_publishes() {
    let store = Arc::new(InMemoryReviewAnnotationStore::default());
    let address = address("log");
    store
        .append(
            &address,
            annotation("1", "earlier", "earlier", "2026-08-09T10:00:00Z"),
        )
        .await
        .unwrap();

    let log = ReviewAnnotationLog::load(store.clone(), address.clone())
        .await
        .unwrap();
    assert_eq!(
        log.active(&moment("1"))
            .await
            .map(|active| active.comment.text),
        Some("earlier".to_string())
    );

    let published = log
        .publish(annotation("1", "later", "later", "2026-08-09T12:00:00Z"))
        .await
        .unwrap();
    assert_eq!(published.comment.text, "later");
    assert_eq!(
        log.active(&moment("1"))
            .await
            .map(|active| active.comment.text),
        Some("later".to_string())
    );

    // A log opened after another conversation published sees that comment; the
    // one already open deliberately does not reconcile mid-conversation.
    store
        .append(
            &address,
            annotation("1", "elsewhere", "elsewhere", "2026-08-09T14:00:00Z"),
        )
        .await
        .unwrap();
    assert_eq!(
        log.active(&moment("1"))
            .await
            .map(|active| active.comment.text),
        Some("later".to_string())
    );
    assert_eq!(
        ReviewAnnotationLog::load(store, address)
            .await
            .unwrap()
            .active(&moment("1"))
            .await
            .map(|active| active.comment.text),
        Some("elsewhere".to_string())
    );
}

#[test]
fn a_stored_annotation_records_no_review_session() {
    let encoded = serde_json_canonicalizer::to_string(&ReviewAnnotationDocument::from_annotation(
        &annotation("1", "durable", "durable text", "2026-08-09T10:00:00Z"),
    ))
    .unwrap();

    assert!(!encoded.contains("review-session"));
    assert!(!encoded.contains("sessionId"));
    assert!(!encoded.contains("revision"));
    assert!(!encoded.contains("purgeAt"));
}

#[test]
fn the_durable_layout_is_a_review_scoped_subtree_of_the_owning_player() {
    let owned = address("layout");
    let path = annotation_path(
        &owned,
        &annotation("1", "layout", "", "2026-08-09T10:00:00Z"),
    )
    .unwrap();

    assert_eq!(path.len(), 6);
    assert_eq!(path[2], REVIEW_ANNOTATIONS_COLLECTION);
    assert_eq!(path[4], REVIEW_ANNOTATION_COMMENTS_COLLECTION);
    assert!(
        [&path[1], &path[3], &path[5]]
            .iter()
            .all(|segment| segment.len() == 64
                && segment.bytes().all(|byte| byte.is_ascii_hexdigit()))
    );
    // A different Review Moment is a different document in the same review.
    assert_ne!(
        path[5],
        annotation_path(
            &owned,
            &annotation("2", "layout", "", "2026-08-09T10:00:00Z")
        )
        .unwrap()[5]
    );
}

#[test]
fn local_coach_annotations_have_no_durable_subtree() {
    assert!(matches!(
        review_annotations_path(&ReviewAnnotationAddress {
            owner: ProcessorPrincipal::LocalCoach,
            ..address("local")
        }),
        Err(ReviewAnnotationStoreError::Configuration(_))
    ));
}

/// Erasure is structural: the annotation path is built from the very document
/// account deletion removes recursively.
#[test]
fn the_annotation_subtree_is_the_one_account_deletion_removes() {
    let player_id = PlayerId::try_from("firebase-player-erasure".to_string()).unwrap();
    let path = review_annotations_path(&ReviewAnnotationAddress {
        owner: ProcessorPrincipal::Player(player_id.clone()),
        ..address("erasure")
    })
    .unwrap();

    assert_eq!(
        path[..2],
        crate::account_deletion::application_data_document_path(&player_id)
    );
}
