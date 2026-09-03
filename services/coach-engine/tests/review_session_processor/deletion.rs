use std::collections::BTreeSet;

use chen_chess_coach_engine::{
    digested_games::{DigestedGameFuture, DigestedGameIndex, NoDigestedGames},
    imported_games::ImportedGameReviewSide,
    review_annotation_store::{
        InMemoryReviewAnnotationStore, ReviewAnnotationAddress, ReviewAnnotationStore,
        ReviewAnnotationStoreFuture, ReviewAnnotations, ReviewMomentAnnotation,
    },
    review_share::{InMemoryReviewShareStore, ReviewShareAddress, ReviewShareRuntime},
    reviewed_games::ReviewedGameKey,
};

use super::*;

#[tokio::test]
async fn deleting_a_game_takes_every_elo_profile_it_was_reviewed_at() {
    let (game_imports, processor) = deletable_processor(Arc::new(NoDigestedGames));
    let owner = player("deletes-own-game");
    let first = import_at_rating(&processor, &owner, "delete-1200", 1200).await;
    let second = import_at_rating(&processor, &owner, "delete-1500", 1500).await;
    assert_ne!(first, second);
    assert_eq!(imported_card_count(&game_imports, &owner).await, 1);

    let deleted = delete_game_import(&processor, &owner, "delete-both", first.clone()).await;

    assert_eq!(
        deleted,
        DeleteOutcome::Deleted {
            game_import_id: first,
            deleted_import_count: 2,
        }
    );
    assert_eq!(imported_card_count(&game_imports, &owner).await, 0);
    assert!(game_imports
        .list_game_import_records(&owner)
        .await
        .unwrap()
        .is_empty());
    // The second Elo's review is gone as well, not merely unlisted.
    assert!(matches!(
        game_imports.find(&owner, &second).await.unwrap(),
        GameImportLookup::NotFound
    ));
}

#[tokio::test]
async fn a_deleted_game_stops_answering_from_a_resident_review_session() {
    let (_, processor) = deletable_processor(Arc::new(NoDigestedGames));
    let owner = player("deletes-resident-game");
    let game_import_id = import_at_rating(&processor, &owner, "resident", 1200).await;
    submit(
        &processor,
        owner.clone(),
        envelope_for(
            &owner,
            "resident-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
    )
    .await;

    delete_game_import(
        &processor,
        &owner,
        "resident-delete",
        game_import_id.clone(),
    )
    .await;

    let restarted = submit(
        &processor,
        owner.clone(),
        envelope_for(
            &owner,
            "resident-restart",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        ),
    )
    .await;
    assert!(restarted.iter().any(|event| matches!(
        &event.event,
        ReviewSessionEvent::Rejected { reason, .. }
            if *reason == CommandRejectionReason::UnknownGameImport
    )));
}

#[tokio::test]
async fn a_digested_game_and_another_players_game_are_both_refused() {
    let (game_imports, processor) = deletable_processor(Arc::new(DigestedReviewedGame {
        canonical_source_key: "lichess:Synthet1".to_string(),
    }));
    let owner = player("cannot-delete-digested");
    let game_import_id = import_at_rating(&processor, &owner, "digested", 1200).await;

    let refused = delete_game_import(
        &processor,
        &owner,
        "digested-delete",
        game_import_id.clone(),
    )
    .await;
    assert_eq!(
        refused,
        DeleteOutcome::Rejected(CommandRejectionReason::DigestedGameImport)
    );
    assert_eq!(imported_card_count(&game_imports, &owner).await, 1);

    let stranger = player("owns-nothing");
    let refused_stranger =
        delete_game_import(&processor, &stranger, "stranger-delete", game_import_id).await;
    assert_eq!(
        refused_stranger,
        DeleteOutcome::Rejected(CommandRejectionReason::UnknownGameImport)
    );
    assert_eq!(imported_card_count(&game_imports, &owner).await, 1);
}

#[tokio::test]
async fn a_deleted_game_leaves_no_comments_and_no_resolvable_link_behind() {
    let annotations = Arc::new(RecordingAnnotationStore::default());
    let shares = Arc::new(InMemoryReviewShareStore::default());
    let (_, _, recording) = processor(false);
    let built = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(support::RecordingEngine::new(&recording)),
            Arc::new(support::RecordingHuman::new(&recording, false)),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(Arc::new(InMemoryGameImportStore::default()))
        .with_review_annotation_store(annotations.clone())
        .with_review_share_store(shares.clone()),
    );
    let owner = player("deletes-with-residue");
    let ProcessorPrincipal::Player(player_id) = owner.clone() else {
        unreachable!("the fixture principal is a Player")
    };
    let game_import_id = import_at_rating(&built, &owner, "residue", 1200).await;

    // A link the Player minted for this review, and one for a review they keep.
    let links = ReviewShareRuntime::new(shares);
    let now = chrono::Utc::now();
    links
        .mint(
            &player_id,
            ReviewShareAddress {
                game_import_id: game_import_id.clone(),
                review_moment_id: CriticalMomentId::try_from("critical-moment:residue".to_string())
                    .unwrap(),
                sequence_kind: None,
            },
            now,
        )
        .await
        .unwrap();

    delete_game_import(&built, &owner, "residue-delete", game_import_id.clone()).await;

    /* A re-import lands at the same address, so a comment or a link left
    behind would attach itself to a review the Player never published it
    against. */
    assert_eq!(
        annotations.deleted.lock().unwrap().as_slice(),
        &[game_import_id]
    );
    assert!(links.outstanding(&player_id, now).await.unwrap().is_empty());
}

#[derive(Default)]
struct RecordingAnnotationStore {
    inner: InMemoryReviewAnnotationStore,
    deleted: std::sync::Mutex<Vec<GameImportId>>,
}

impl ReviewAnnotationStore for RecordingAnnotationStore {
    fn append<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
        annotation: ReviewMomentAnnotation,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewMomentAnnotation> {
        self.inner.append(address, annotation)
    }

    fn read<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewAnnotations> {
        self.inner.read(address)
    }

    fn delete<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ()> {
        self.deleted
            .lock()
            .expect("the recording annotation store is not poisoned")
            .push(address.game_import_id.clone());
        self.inner.delete(address)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum DeleteOutcome {
    Deleted {
        game_import_id: GameImportId,
        deleted_import_count: u16,
    },
    Rejected(CommandRejectionReason),
}

async fn delete_game_import(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    owner: &ProcessorPrincipal,
    label: &str,
    game_import_id: GameImportId,
) -> DeleteOutcome {
    let events = submit(
        processor,
        owner.clone(),
        web_envelope(
            label,
            ReviewSessionCommand::DeleteGameImport { game_import_id },
        ),
    )
    .await;
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::GameImportDeleted {
                    game_import_id,
                    deleted_import_count,
                } => Some(DeleteOutcome::Deleted {
                    game_import_id: game_import_id.clone(),
                    deleted_import_count: *deleted_import_count,
                }),
                _ => None,
            },
            ReviewSessionEvent::Rejected { reason, .. } => Some(DeleteOutcome::Rejected(*reason)),
            _ => None,
        })
        .expect("a delete answers with a completion or a rejection")
}

/// The delete is web-only, so its envelope cannot come from `envelope_for`,
/// which speaks for the Coach App.
fn web_envelope(label: &str, command: ReviewSessionCommand) -> ReviewSessionCommandEnvelope {
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from(format!("request:processor:{label}")).unwrap(),
        operation_id: OperationId::try_from(format!("operation:processor:{label}")).unwrap(),
        surface: DeliverySurface::Web,
        command,
    }
}

async fn import_at_rating(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    owner: &ProcessorPrincipal,
    label: &str,
    rating: u16,
) -> GameImportId {
    let imported = submit(
        processor,
        owner.clone(),
        envelope_for(
            owner,
            label,
            ReviewSessionCommand::ImportGame {
                source: GameInputSource::LichessUrl {
                    url: "https://lichess.org/Synthet1Demo/black".to_string(),
                },
                review_side: RequestedReviewSide::FromQualifiedUrl,
                elo_profile: RequestedEloProfile::PlayerProvided {
                    rating: EloRating::try_from(rating).unwrap(),
                },
            },
        ),
    )
    .await;
    imported.iter().find_map(imported_game).unwrap()
}

async fn imported_card_count(
    game_imports: &Arc<InMemoryGameImportStore>,
    owner: &ProcessorPrincipal,
) -> usize {
    game_imports
        .list_imported_game_cards(owner)
        .await
        .unwrap()
        .len()
}

fn deletable_processor(
    digested: Arc<dyn DigestedGameIndex>,
) -> (
    Arc<InMemoryGameImportStore>,
    Arc<ReviewSessionProcessor<CapturedLichess>>,
) {
    let (_, _, recording) = processor(false);
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let built = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(support::RecordingEngine::new(&recording)),
            Arc::new(support::RecordingHuman::new(&recording, false)),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(game_imports.clone())
        .with_digested_games(digested),
    );
    (game_imports, built)
}

/// Answers with the one Game these tests import, digested as Black — the side
/// the reviewed Lichess URL qualifies.
struct DigestedReviewedGame {
    canonical_source_key: String,
}

impl DigestedGameIndex for DigestedReviewedGame {
    fn digested_games<'a>(
        &'a self,
        _owner: &'a PlayerId,
    ) -> DigestedGameFuture<'a, BTreeSet<ReviewedGameKey>> {
        Box::pin(async {
            Ok(BTreeSet::from([ReviewedGameKey {
                canonical_source_key: self.canonical_source_key.clone(),
                review_side: ImportedGameReviewSide::Black,
            }]))
        })
    }
}

fn player(seed: &str) -> ProcessorPrincipal {
    ProcessorPrincipal::Player(PlayerId::try_from(format!("player:{seed}")).unwrap())
}
