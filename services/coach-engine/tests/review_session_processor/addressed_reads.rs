use super::*;

/// The snapshot is a rendering input, so what matters is that it names the same
/// Review Moments, in ply order, at the same Positions a Review Session start
/// would have produced — without one existing.
#[tokio::test]
async fn a_snapshot_read_reconstructs_the_review_moments_a_session_start_would_admit() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("snapshot-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let started = submit(
        &processor,
        principal.clone(),
        envelope(
            "snapshot-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
    )
    .await;
    let session_moments = started
        .iter()
        .find_map(session_review_moments)
        .expect("the Review Session start admits its Review Moments");

    let read = submit(
        &processor,
        principal,
        envelope(
            "snapshot-read",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (read_id, review_moments, review) =
        snapshot(&read).expect("the snapshot address answers with a snapshot");

    assert_eq!(read_id, &game_import_id);
    assert!(!review_moments.is_empty());
    assert_eq!(
        review_moments
            .iter()
            .map(|moment| moment.review_moment.moment_id.clone())
            .collect::<Vec<_>>(),
        session_moments
            .iter()
            .map(|moment| moment.review_moment.moment_id.clone())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        review_moments
            .iter()
            .map(|moment| moment.position_snapshot.position_ref.clone())
            .collect::<Vec<_>>(),
        session_moments
            .iter()
            .map(|moment| moment.position_snapshot.position_ref.clone())
            .collect::<Vec<_>>(),
    );
    let plies = review_moments
        .iter()
        .map(|moment| moment.review_moment.ply)
        .collect::<Vec<_>>();
    assert!(
        plies.windows(2).all(|window| window[0] < window[1]),
        "snapshot Review Moments must be ordered by ply: {plies:?}"
    );
    for (index, moment) in review_moments.iter().enumerate() {
        assert!(
            review
                .critical_moments
                .iter()
                .any(|critical| critical.critical_moment_id == moment.review_moment.moment_id),
            "snapshot Review Moment {index} has no Game Review entry"
        );
        assert!(
            matches!(moment.authoring, ReviewMomentAuthoringReadiness::Pending),
            "a snapshot renders; it does not prepare authoring"
        );
    }
}

/// An immutable snapshot that answers differently on a second read is not one,
/// and a read that leaves a Review Session behind is not a read.
#[tokio::test]
async fn reading_the_same_address_twice_answers_identically_and_starts_nothing() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("repeat-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();

    let mut answers = Vec::new();
    for label in ["repeat-read-one", "repeat-read-two"] {
        let events = submit(
            &processor,
            principal.clone(),
            envelope(
                label,
                ReviewSessionCommand::ReadGameReviewSnapshot {
                    game_import_id: game_import_id.clone(),
                    known_content_digest: None,
                },
            ),
        )
        .await;
        assert!(
            events.iter().all(
                |event| !matches!(&event.event, ReviewSessionEvent::Completed { result }
                if matches!(
                    result.as_ref(),
                    OperationCompletion::ReviewSessionStarted { .. }
                ))
            ),
            "a snapshot read must not create a Review Session"
        );
        let (_, review_moments, review) = snapshot(&events).expect("both reads answer");
        answers.push((review_moments.clone(), review.clone()));
    }
    assert_eq!(answers[0], answers[1]);
}

#[tokio::test]
async fn an_unaddressed_or_unowned_game_review_has_no_snapshot() {
    let (processor, _, _) = processor(false);
    let owner = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-snapshot-owner".to_string()).unwrap(),
    );
    let imported = submit(
        &processor,
        owner.clone(),
        envelope_for(&owner, "snapshot-owned-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();

    let other = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-snapshot-other".to_string()).unwrap(),
    );
    let foreign = submit(
        &processor,
        other.clone(),
        envelope_for(
            &other,
            "snapshot-cross-player",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;

    let missing = submit(
        &processor,
        other.clone(),
        envelope_for(
            &other,
            "snapshot-unknown",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: GameImportId::try_from("game-import:absent:absent".to_string())
                    .unwrap(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    assert_eq!(
        foreign.last().map(|event| &event.event),
        missing.last().map(|event| &event.event)
    );
    assert!(matches!(
        missing.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            operation: OperationKind::GameReviewOpen,
            reason: CommandRejectionReason::UnknownGameImport,
            recovery: RejectionRecovery::CorrectInput,
        })
    ));
}

#[allow(clippy::type_complexity)]
/// A client cache can hold a review's bytes but cannot date them: the review
/// is immutable, its derivation is not, and neither is the comment template its
/// prose came from. Offering the digest back is how it revalidates.
#[tokio::test]
async fn a_snapshot_read_revalidates_against_the_digest_it_answered_with() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("digest-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();

    let first = submit(
        &processor,
        principal.clone(),
        envelope(
            "digest-first",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let digest = snapshot_digest(&first).expect("a snapshot read names its content digest");

    let revalidated = submit(
        &processor,
        principal.clone(),
        envelope(
            "digest-revalidate",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: Some(digest.clone()),
            },
        ),
    )
    .await;

    assert_eq!(
        unchanged_digest(&revalidated),
        Some(&digest),
        "an offered digest that still matches must answer unchanged"
    );
    assert!(
        snapshot(&revalidated).is_none(),
        "an unchanged answer must not carry the review it declined to resend"
    );

    // A digest the engine never issued is a miss, not an error: a cache holding
    // bytes from an older build gets the payload rather than a refusal.
    let stale = submit(
        &processor,
        principal,
        envelope(
            "digest-stale",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: Some(
                    ReviewContentDigest::try_from(format!("sha256:{}", "4".repeat(64)))
                        .expect("the fixture digest is valid"),
                ),
            },
        ),
    )
    .await;

    let (stale_id, _, _) = snapshot(&stale).expect("a stale digest is answered with the review");
    assert_eq!(stale_id, &game_import_id);
    assert_eq!(
        snapshot_digest(&stale).as_ref(),
        Some(&digest),
        "the digest a miss answers with is the one a cache should store"
    );
}

/// A Review Moment Comment is the one part of a review a later build rewrites,
/// and the wire comment carries no identity, so the validator has to compare
/// the answer rather than the inputs that produced it. The case that proves it:
/// prose published after a caller cached the moment. Nothing the engine could
/// fold from inputs moves between these two reads — the compiled template
/// digests are identical either side — so an input-folded validator would
/// wrongly answer unchanged and strand the caller on an empty comment.
#[tokio::test]
async fn a_published_comment_breaks_a_moment_detail_the_caller_already_held() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-detail-digest".to_string()).unwrap(),
    );
    let imported = submit(
        &processor,
        principal.clone(),
        envelope_for(&principal, "detail-digest-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let unread = open_addressed(
        &processor,
        &principal,
        "detail-digest-unread",
        &game_import_id,
        ReviewMomentReference::Next {
            after_review_moment_id: None,
            classification: None,
        },
    )
    .await
    .expect("the first Critical Moment opens");
    assert_eq!(unread.comment, None, "nothing published yet is absence");
    let moment_id = unread.review_moment_id.clone();

    let detail_command = |known| ReviewSessionCommand::ReadReviewMomentDetail {
        game_import_id: game_import_id.clone(),
        review_moment_id: moment_id.clone(),
        known_content_digest: known,
    };

    let first = submit(
        &processor,
        principal.clone(),
        envelope_for(&principal, "detail-digest-first", detail_command(None)),
    )
    .await;
    let digest = detail_digest(&first).expect("a moment detail names its content digest");

    let revalidated = submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "detail-digest-again",
            detail_command(Some(digest.clone())),
        ),
    )
    .await;
    assert_eq!(
        detail_unchanged_digest(&revalidated),
        Some(&digest),
        "an unchanged moment must answer without resending its detail"
    );
    assert!(
        detail(&revalidated).is_none(),
        "an unchanged answer must not carry the detail it declined to resend"
    );

    submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "detail-digest-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
    )
    .await;
    let opened_events = submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "detail-digest-open",
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection: ReviewMomentSelection::PipelineCriticalMoment {
                    critical_moment_id: moment_id.clone(),
                },
                idempotency_key: idempotency_key("detail-digest-open"),
            },
        ),
    )
    .await;
    // The first open safe-renders prose but does not publish it, and the
    // detail read answers from the published store, so the Coach App's publish
    // is the step that makes a comment durable.
    let (text, ledger) = opened_events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewMomentOpened {
                    comment: Some(comment),
                    authoring_context: Some(context),
                    ..
                } => Some((
                    comment.text.clone(),
                    context.required_grounding_ledger.clone(),
                )),
                _ => None,
            },
            _ => None,
        })
        .expect("the first open safe-renders a comment with a grounding ledger");
    submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "detail-digest-publish",
            ReviewSessionCommand::PublishReviewMomentComment {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment_id.clone(),
                text,
                grounding_ledger: ledger,
                idempotency_key: idempotency_key("detail-digest-publish"),
            },
        ),
    )
    .await;

    let after_comment = submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "detail-digest-after",
            detail_command(Some(digest.clone())),
        ),
    )
    .await;
    let answered = detail(&after_comment)
        .expect("a moment whose comment was published must be resent, not revalidated");
    assert!(
        answered.comment.is_some(),
        "the resent detail must carry the comment the caller could not have held"
    );
    assert_ne!(
        detail_digest(&after_comment).as_ref(),
        Some(&digest),
        "publishing prose must move the digest a cache compares against"
    );
}

fn detail(events: &[ReviewSessionEventEnvelope]) -> Option<&GroundedReviewMomentDetail> {
    events.iter().find_map(|event| match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewMomentDetailRead { detail, .. } => Some(detail.as_ref()),
            _ => None,
        },
        _ => None,
    })
}

fn detail_digest(events: &[ReviewSessionEventEnvelope]) -> Option<ReviewContentDigest> {
    events.iter().find_map(|event| match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewMomentDetailRead { content_digest, .. } => {
                Some(content_digest.clone())
            }
            _ => None,
        },
        _ => None,
    })
}

fn detail_unchanged_digest(events: &[ReviewSessionEventEnvelope]) -> Option<&ReviewContentDigest> {
    events.iter().find_map(|event| match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewMomentDetailUnchanged { content_digest, .. } => {
                Some(content_digest)
            }
            _ => None,
        },
        _ => None,
    })
}

fn snapshot_digest(events: &[ReviewSessionEventEnvelope]) -> Option<ReviewContentDigest> {
    events.iter().find_map(|event| match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::GameReviewSnapshotRead { content_digest, .. } => {
                Some(content_digest.clone())
            }
            _ => None,
        },
        _ => None,
    })
}

fn unchanged_digest(events: &[ReviewSessionEventEnvelope]) -> Option<&ReviewContentDigest> {
    events.iter().find_map(|event| match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::GameReviewSnapshotUnchanged { content_digest, .. } => {
                Some(content_digest)
            }
            _ => None,
        },
        _ => None,
    })
}

fn snapshot(
    events: &[ReviewSessionEventEnvelope],
) -> Option<(&GameImportId, &Vec<ReviewSessionMoment>, &GameReview)> {
    events.iter().find_map(|event| match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::GameReviewSnapshotRead {
                game_import_id,
                review,
                review_moments,
                ..
            } => Some((game_import_id, review_moments, review.as_ref())),
            _ => None,
        },
        _ => None,
    })
}

fn session_review_moments(event: &ReviewSessionEventEnvelope) -> Option<&Vec<ReviewSessionMoment>> {
    match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewSessionStarted { review_moments, .. } => {
                Some(review_moments)
            }
            _ => None,
        },
        _ => None,
    }
}

/// A moment read answers the moment and nothing that contains it, from the same
/// address the snapshot named it at.
#[tokio::test]
async fn a_moment_read_grounds_the_moment_the_snapshot_named() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("moment-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "moment-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, review_moments, review) = snapshot(&read).expect("the review has a snapshot");
    let named = review_moments
        .first()
        .expect("the snapshot names at least one Review Moment");

    let detailed = submit(
        &processor,
        principal,
        envelope(
            "moment-detail",
            ReviewSessionCommand::ReadReviewMomentDetail {
                game_import_id: game_import_id.clone(),
                review_moment_id: named.review_moment.moment_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let detail = moment_detail(&detailed).expect("the moment address answers");

    assert_eq!(detail.game_import_id, game_import_id);
    assert_eq!(detail.review_moment_id, named.review_moment.moment_id);
    assert_eq!(detail.ply, named.review_moment.ply);
    assert_eq!(detail.continuation.fen, named.position_snapshot.fen);
    let critical = review
        .critical_moments
        .iter()
        .find(|moment| moment.critical_moment_id == named.review_moment.moment_id)
        .expect("the named moment is in the review");
    // The continuations a surface plays out are the ones the Game Review froze,
    // not anything re-derived from candidate evidence.
    assert_eq!(detail.objective_lines, critical.objective.lines);
}

/// A proven moment reaches a reader as names and moves, not as content hashes.
///
/// This is the whole point of the moment address: the stored proof is a graph of
/// sha256 references, and what a host model receives has to be speakable.
#[tokio::test]
async fn a_proven_moment_read_carries_its_proof_resolved_into_names_and_moves() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("proven-detail-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "proven-detail-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, _, review) = snapshot(&read).expect("the review has a snapshot");
    let proven = review
        .critical_moments
        .iter()
        .find(|moment| moment.decision_explanation.is_some())
        .expect("this fixture review proves at least one Review Moment");

    let detailed = submit(
        &processor,
        principal,
        envelope(
            "proven-detail-read",
            ReviewSessionCommand::ReadReviewMomentDetail {
                game_import_id,
                review_moment_id: proven.critical_moment_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;

    let detail = moment_detail(&detailed).expect("the moment address answers");
    assert_eq!(
        detail.explanation_ref, proven.decision_explanation_ref,
        "a grounded moment still addresses its audit copy"
    );
    let grounded = detail
        .explanation
        .as_ref()
        .expect("a proven moment delivers its proof grounded");
    assert_eq!(
        grounded.capability,
        proven
            .decision_explanation
            .as_ref()
            .expect("the proven moment carries its aggregate")
            .capability,
        "the capability that governs what may be claimed survives grounding"
    );
    assert!(!grounded.paths.is_empty());
    for path in &grounded.paths {
        // Every one of these is a sha256 reference in the stored aggregate.
        assert!(!path.candidate.san.is_empty());
        assert!(!path.causal_step.san.is_empty());
        assert!(!path.payoff_step.san.is_empty());
    }
    assert!(grounded.candidates.iter().all(|candidate| candidate
        .retained_variation
        .iter()
        .all(|san| !san.is_empty())));
}

/// The proof is dropped from every rendering payload on the way out.
#[tokio::test]
async fn a_delivered_game_review_carries_proof_references_and_never_proof() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("audit-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();

    let read = submit(
        &processor,
        principal,
        envelope(
            "audit-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id,
                known_content_digest: None,
            },
        ),
    )
    .await;

    let delivered_moments = delivered_result(read.last().unwrap())
        .pointer("/review/criticalMoments")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("a delivered snapshot carries its Critical Moments");
    assert!(!delivered_moments.is_empty());
    assert!(delivered_moments
        .iter()
        .all(|moment| moment.get("decisionExplanation").is_none()));
}

/// The audit address is the one address a stored proof leaves by, so a moment
/// that has one must answer with it — whole, and unchanged by delivery.
#[tokio::test]
async fn the_audit_address_answers_a_proven_moment_with_its_stored_proof() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("audit-proven-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "audit-proven-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, _, review) = snapshot(&read).expect("the review has a snapshot");
    let proven = review
        .critical_moments
        .iter()
        .find(|moment| moment.decision_explanation.is_some())
        .expect("this fixture review proves at least one Review Moment");
    let stored = proven
        .decision_explanation
        .clone()
        .expect("the proven moment carries its aggregate");

    let audited = submit(
        &processor,
        principal,
        envelope(
            "audit-proven-read",
            ReviewSessionCommand::ReadReviewMomentExplanation {
                game_import_id,
                review_moment_id: proven.critical_moment_id.clone(),
            },
        ),
    )
    .await;

    let delivered = delivered_result(audited.last().expect("the audit read answers"));
    assert_eq!(
        delivered.get("explanation"),
        Some(&serde_json::to_value(&stored).unwrap()),
        "the audit address delivers the stored aggregate verbatim"
    );
}

#[tokio::test]
async fn a_moment_outside_the_review_is_addressable_but_not_readable() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("absent-moment-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();

    let missing = submit(
        &processor,
        principal,
        envelope(
            "absent-moment-read",
            ReviewSessionCommand::ReadReviewMomentDetail {
                game_import_id,
                review_moment_id: CriticalMomentId::try_from("review-moment:absent:1".to_string())
                    .unwrap(),
                known_content_digest: None,
            },
        ),
    )
    .await;

    assert!(matches!(
        missing.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            operation: OperationKind::ReviewMomentOpen,
            reason: CommandRejectionReason::UnknownMoment,
            recovery: RejectionRecovery::CorrectInput,
        })
    ));
}

/// A Review Moment ID is a pure function of the Game and the ply, so every
/// legal ply in the Game spells a well-formed address. Only the ones the frozen
/// review named may answer: the alternative is an address whose meaning depends
/// on what a Player has clicked on since, which is what these addresses exist
/// to rule out.
#[tokio::test]
async fn a_legal_ply_the_review_never_named_is_not_addressable() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("unnamed-ply-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "unnamed-ply-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, review_moments, _) = snapshot(&read).expect("the review has a snapshot");
    let named = review_moments
        .iter()
        .map(|moment| moment.review_moment.ply)
        .collect::<Vec<_>>();
    let game_ref = review_moments[0].review_moment.game_ref.clone();
    let unnamed = (1..=named.iter().copied().max().unwrap())
        .find(|ply| !named.contains(ply))
        .expect("the fixture Game has a ply the review did not flag");

    let refused = submit(
        &processor,
        principal,
        envelope(
            "unnamed-ply-read",
            ReviewSessionCommand::ReadReviewMomentDetail {
                game_import_id,
                review_moment_id: CriticalMomentId::for_imported_game(&game_ref, unnamed),
                known_content_digest: None,
            },
        ),
    )
    .await;

    assert!(
        matches!(
            refused.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Rejected {
                operation: OperationKind::ReviewMomentOpen,
                reason: CommandRejectionReason::UnknownMoment,
                recovery: RejectionRecovery::CorrectInput,
            })
        ),
        "ply {unnamed} is a real move but not a Critical Moment: {refused:#?}"
    );
}

/// The three ways a Player names a moment have to reach the same moment.
///
/// Asking for a Critical Moment by its ID and asking for it by its bare ply are
/// the same question, so they may not answer with two different groundings —
/// and the pipeline's proof is the one that has to survive, because the
/// Player-Selected fallback proves strictly less.
#[tokio::test]
async fn a_critical_moment_opens_the_same_way_whether_named_by_id_or_by_ply() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("reference-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "reference-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, review_moments, _) = snapshot(&read).expect("the review has a snapshot");
    let named = review_moments
        .first()
        .expect("the snapshot names at least one Review Moment");

    let by_id = open_addressed(
        &processor,
        &principal,
        "reference-by-id",
        &game_import_id,
        ReviewMomentReference::Critical {
            review_moment_id: named.review_moment.moment_id.clone(),
        },
    )
    .await
    .expect("a Critical Moment reference opens");
    let by_ply = open_addressed(
        &processor,
        &principal,
        "reference-by-ply",
        &game_import_id,
        ReviewMomentReference::Ply {
            ply: named.review_moment.ply,
        },
    )
    .await
    .expect("the same moment opens by its ply");

    assert_eq!(by_id.review_moment_id, named.review_moment.moment_id);
    assert_eq!(by_id.ply, named.review_moment.ply);
    assert_eq!(by_id.game_import_id, game_import_id);
    assert_eq!(by_id, by_ply);
}

/// The moment the selector never flagged is the one requirement 4.4 exists for.
#[tokio::test]
async fn a_ply_the_review_never_flagged_opens_as_a_player_selected_moment() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("bare-ply-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "bare-ply-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, review_moments, _) = snapshot(&read).expect("the review has a snapshot");
    let flagged = review_moments
        .iter()
        .map(|moment| moment.review_moment.ply)
        .collect::<Vec<_>>();
    let unflagged = (1..=flagged.iter().copied().max().unwrap())
        .find(|ply| !flagged.contains(ply))
        .expect("the fixture Game has a ply the review did not flag");

    let opened = open_addressed(
        &processor,
        &principal,
        "bare-ply-open",
        &game_import_id,
        ReviewMomentReference::Ply { ply: unflagged },
    )
    .await
    .expect("a ply outside the Critical Moment set opens");

    assert_eq!(opened.ply, unflagged);
    assert_eq!(opened.game_import_id, game_import_id);
    // A moment the frozen review never named, which is what makes this the
    // Player-Selected branch rather than a Critical Moment reached by its ply.
    // The same address the resource read refuses, because that address must
    // answer the same forever and this one is the Player asking a live question.
    assert!(!flagged.contains(&opened.ply));
    assert!(
        !review_moments
            .iter()
            .any(|moment| moment.review_moment.moment_id == opened.review_moment_id),
        "a bare-ply open must resolve outside the frozen Critical Moment set"
    );

    // Requirement 4.4 then 4.6: the Player opens a move nothing flagged and asks
    // what comes next. Anchoring the step on the Critical Moment set alone would
    // refuse that pair, and the instruction block tells the model to make it.
    let stepped = open_addressed(
        &processor,
        &principal,
        "bare-ply-next",
        &game_import_id,
        ReviewMomentReference::Next {
            after_review_moment_id: Some(opened.review_moment_id.clone()),
            classification: None,
        },
    )
    .await
    .expect("a Player-Selected Moment can be stepped onward from");

    assert!(stepped.ply > opened.ply);
    assert_eq!(
        stepped.ply,
        flagged
            .iter()
            .copied()
            .filter(|ply| *ply > opened.ply)
            .min()
            .expect("the unflagged ply precedes at least one Critical Moment"),
        "the step lands on the next frozen Critical Moment, not the next ply"
    );
}

/// "Show me the next Critical Moment" walks the ply order and then stops.
#[tokio::test]
async fn next_steps_through_the_ordered_critical_moments_and_ends_at_the_last() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("next-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "next-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, review_moments, _) = snapshot(&read).expect("the review has a snapshot");
    let ordered = review_moments
        .iter()
        .map(|moment| moment.review_moment.moment_id.clone())
        .collect::<Vec<_>>();
    assert!(
        ordered.len() > 1,
        "this fixture needs at least two Critical Moments to step between"
    );

    let first = open_addressed(
        &processor,
        &principal,
        "next-first",
        &game_import_id,
        ReviewMomentReference::Next {
            after_review_moment_id: None,
            classification: None,
        },
    )
    .await
    .expect("no current moment opens the first one");
    assert_eq!(first.review_moment_id, ordered[0]);

    let mut walked = vec![first.review_moment_id.clone()];
    for (index, current) in ordered.iter().enumerate().take(ordered.len() - 1) {
        let stepped = open_addressed(
            &processor,
            &principal,
            &format!("next-step-{index}"),
            &game_import_id,
            ReviewMomentReference::Next {
                after_review_moment_id: Some(current.clone()),
                classification: None,
            },
        )
        .await
        .expect("each Critical Moment has a successor until the last");
        walked.push(stepped.review_moment_id);
    }
    assert_eq!(walked, ordered);

    let past_the_end = submit(
        &processor,
        principal,
        envelope(
            "next-past-the-end",
            ReviewSessionCommand::OpenAddressedReviewMoment {
                game_import_id,
                reference: ReviewMomentReference::Next {
                    after_review_moment_id: ordered.last().cloned(),
                    classification: None,
                },
            },
        ),
    )
    .await;
    assert!(
        matches!(
            past_the_end.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Rejected {
                operation: OperationKind::ReviewMomentOpen,
                reason: CommandRejectionReason::UnknownMoment,
                recovery: RejectionRecovery::CorrectInput,
            })
        ),
        "the last Critical Moment has no successor: {past_the_end:#?}"
    );
}

/// “Next moment that can be improved” is a forward query, not model-side search.
#[tokio::test]
async fn next_improvement_skips_other_classifications_and_keeps_the_full_order_as_its_anchor() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("next-improvement-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "next-improvement-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, review_moments, review) = snapshot(&read).expect("the review has a snapshot");
    let ordered = review_moments
        .iter()
        .map(|moment| {
            let ReviewMomentSelection::PipelineCriticalMoment { critical_moment_id } =
                &moment.review_moment.selection
            else {
                panic!("a frozen snapshot contains only automatic moments");
            };
            let reviewed = review
                .critical_moments
                .iter()
                .find(|candidate| candidate.critical_moment_id == *critical_moment_id)
                .expect("the frozen review owns every snapshot moment");
            (
                moment.review_moment.moment_id.clone(),
                matches!(
                    reviewed.classification,
                    GameReviewMomentClassification::ImprovementOpportunity { .. }
                ),
            )
        })
        .collect::<Vec<_>>();
    let first_improvement = ordered
        .iter()
        .position(|(_, improvement)| *improvement)
        .expect("the fixture needs an Improvement Opportunity");

    let first = open_addressed(
        &processor,
        &principal,
        "next-improvement-first",
        &game_import_id,
        ReviewMomentReference::Next {
            after_review_moment_id: None,
            classification: Some(ReviewMomentReferenceClassification::ImprovementOpportunity),
        },
    )
    .await
    .expect("a bare filtered next opens the first Improvement Opportunity");
    assert_eq!(first.review_moment_id, ordered[first_improvement].0);

    let (anchor, next_improvement) = (0..ordered.len() - 1)
        .find_map(|anchor| {
            let next_improvement = ordered[anchor + 1..]
                .iter()
                .position(|(_, improvement)| *improvement)?
                + anchor
                + 1;
            (next_improvement > anchor + 1).then_some((anchor, next_improvement))
        })
        .expect("the fixture needs a non-improvement between an anchor and an improvement");
    let chronological = open_addressed(
        &processor,
        &principal,
        "next-improvement-chronological",
        &game_import_id,
        ReviewMomentReference::Next {
            after_review_moment_id: Some(ordered[anchor].0.clone()),
            classification: None,
        },
    )
    .await
    .expect("the anchor has a chronological successor");
    let filtered = open_addressed(
        &processor,
        &principal,
        "next-improvement-filtered",
        &game_import_id,
        ReviewMomentReference::Next {
            after_review_moment_id: Some(ordered[anchor].0.clone()),
            classification: Some(ReviewMomentReferenceClassification::ImprovementOpportunity),
        },
    )
    .await
    .expect("the anchor has a later Improvement Opportunity");
    assert_eq!(chronological.review_moment_id, ordered[anchor + 1].0);
    assert_eq!(filtered.review_moment_id, ordered[next_improvement].0);

    let last_improvement = ordered
        .iter()
        .rposition(|(_, improvement)| *improvement)
        .expect("the fixture needs an Improvement Opportunity");
    let past_the_end = submit(
        &processor,
        principal,
        envelope(
            "next-improvement-past-the-end",
            ReviewSessionCommand::OpenAddressedReviewMoment {
                game_import_id,
                reference: ReviewMomentReference::Next {
                    after_review_moment_id: Some(ordered[last_improvement].0.clone()),
                    classification: Some(
                        ReviewMomentReferenceClassification::ImprovementOpportunity,
                    ),
                },
            },
        ),
    )
    .await;
    assert!(matches!(
        past_the_end.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            operation: OperationKind::ReviewMomentOpen,
            reason: CommandRejectionReason::UnknownMoment,
            recovery: RejectionRecovery::CorrectInput,
        })
    ));
}

/// A misrouted Review Moment becomes a visible correction, not a broken widget.
#[tokio::test]
async fn a_moment_that_does_not_belong_to_the_game_import_is_refused_by_type() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("misroute-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();

    let refused = submit(
        &processor,
        principal,
        envelope(
            "misroute-open",
            ReviewSessionCommand::OpenAddressedReviewMoment {
                game_import_id,
                reference: ReviewMomentReference::Critical {
                    review_moment_id: CriticalMomentId::try_from(
                        "review-moment:another-review:1".to_string(),
                    )
                    .unwrap(),
                },
            },
        ),
    )
    .await;

    assert!(
        matches!(
            refused.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Rejected {
                operation: OperationKind::ReviewMomentOpen,
                reason: CommandRejectionReason::UnknownMoment,
                recovery: RejectionRecovery::CorrectInput,
            })
        ),
        "a Review Moment of another review names nothing here: {refused:#?}"
    );
}

/// A ply outside the Game names no Position, so it is refused rather than clamped.
#[tokio::test]
async fn a_ply_outside_the_game_is_refused() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("out-of-range-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();

    let refused = submit(
        &processor,
        principal,
        envelope(
            "out-of-range-open",
            ReviewSessionCommand::OpenAddressedReviewMoment {
                game_import_id,
                reference: ReviewMomentReference::Ply { ply: u16::MAX },
            },
        ),
    )
    .await;

    assert!(
        matches!(
            refused.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Rejected {
                operation: OperationKind::ReviewMomentOpen,
                reason: CommandRejectionReason::UnknownMoment,
                recovery: RejectionRecovery::CorrectInput,
            })
        ),
        "{refused:#?}"
    );
}

/// The open is a read: it may not leave a Review Session behind, and the
/// address has to answer the same way the second time.
#[tokio::test]
async fn opening_an_addressed_moment_twice_answers_identically_and_starts_nothing() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("addressed-open-idempotence-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "addressed-open-idempotence-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, review_moments, _) = snapshot(&read).expect("the review has a snapshot");
    let reference = ReviewMomentReference::Critical {
        review_moment_id: review_moments[0].review_moment.moment_id.clone(),
    };

    let mut answers = Vec::new();
    for label in ["addressed-open-first", "addressed-open-second"] {
        let events = submit(
            &processor,
            principal.clone(),
            envelope(
                label,
                ReviewSessionCommand::OpenAddressedReviewMoment {
                    game_import_id: game_import_id.clone(),
                    reference: reference.clone(),
                },
            ),
        )
        .await;
        assert!(
            events.iter().all(
                |event| !matches!(&event.event, ReviewSessionEvent::Completed { result }
                if matches!(
                    result.as_ref(),
                    OperationCompletion::ReviewSessionStarted { .. }
                ))
            ),
            "an addressed open must not create a Review Session"
        );
        answers.push(
            addressed_moment(&events)
                .expect("both opens answer")
                .clone(),
        );
    }
    assert_eq!(answers[0], answers[1]);
}

/// A published Review Moment Comment is durable Player-owned data, so the
/// session-free open returns it. The open still starts no Review Session.
#[tokio::test]
async fn an_addressed_open_returns_a_published_comment_without_starting_a_session() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-addressed-comment".to_string()).unwrap(),
    );
    let imported = submit(
        &processor,
        principal.clone(),
        envelope_for(&principal, "addressed-comment-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let unread = open_addressed(
        &processor,
        &principal,
        "addressed-comment-unread",
        &game_import_id,
        ReviewMomentReference::Next {
            after_review_moment_id: None,
            classification: None,
        },
    )
    .await
    .expect("the first Critical Moment opens");
    assert_eq!(unread.comment, None, "nothing published yet is absence");

    submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "addressed-comment-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
    )
    .await;
    let opened = submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "addressed-comment-session-open",
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection: ReviewMomentSelection::PipelineCriticalMoment {
                    critical_moment_id: unread.review_moment_id.clone(),
                },
                idempotency_key: idempotency_key("addressed-comment-session-open"),
            },
        ),
    )
    .await;
    let (text, ledger) = opened
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewMomentOpened {
                    comment: Some(comment),
                    authoring_context: Some(context),
                    ..
                } => Some((
                    comment.text.clone(),
                    context.required_grounding_ledger.clone(),
                )),
                _ => None,
            },
            _ => None,
        })
        .expect("Coach App first-open safe-renders a comment with a grounding ledger");
    assert!(
        text.contains("My best guess"),
        "the unpublished first-open comment must carry the Coach Intent Hypothesis: {text}"
    );

    let publish = ReviewSessionCommand::PublishReviewMomentComment {
        game_import_id: game_import_id.clone(),
        review_moment_id: unread.review_moment_id.clone(),
        text,
        grounding_ledger: ledger,
        idempotency_key: idempotency_key("addressed-comment-publish"),
    };
    let published = submit(
        &processor,
        principal.clone(),
        envelope_for(&principal, "addressed-comment-publish", publish),
    )
    .await;
    assert!(
        published
            .iter()
            .all(|event| !matches!(&event.event, ReviewSessionEvent::Rejected { .. })),
        "the engine's own safe-render publishes on the first attempt: {published:#?}"
    );
    let published_comment = published
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewMomentCommentPublished { comment, .. } => {
                    Some(comment.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("the safe-rendered comment publishes: {published:#?}"));

    let events = submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "addressed-comment-reread",
            ReviewSessionCommand::OpenAddressedReviewMoment {
                game_import_id,
                reference: ReviewMomentReference::Critical {
                    review_moment_id: unread.review_moment_id,
                },
            },
        ),
    )
    .await;
    assert!(
        events.iter().all(
            |event| !matches!(&event.event, ReviewSessionEvent::Completed { result }
            if matches!(
                result.as_ref(),
                OperationCompletion::ReviewSessionStarted { .. }
            ))
        ),
        "an addressed open must not create a Review Session"
    );
    let reread = addressed_moment(&events).expect("the published moment still opens");
    assert_eq!(reread.comment.as_ref(), Some(published_comment.as_ref()));
}

async fn open_addressed(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: &ProcessorPrincipal,
    request: &str,
    game_import_id: &GameImportId,
    reference: ReviewMomentReference,
) -> Option<GroundedReviewMomentDetail> {
    let events = submit(
        processor,
        principal.clone(),
        envelope_for(
            principal,
            request,
            ReviewSessionCommand::OpenAddressedReviewMoment {
                game_import_id: game_import_id.clone(),
                reference,
            },
        ),
    )
    .await;
    addressed_moment(&events).cloned()
}

fn addressed_moment(events: &[ReviewSessionEventEnvelope]) -> Option<&GroundedReviewMomentDetail> {
    events.iter().find_map(|event| match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::AddressedReviewMomentOpened { detail } => Some(detail.as_ref()),
            _ => None,
        },
        _ => None,
    })
}

/// The projection exists to make a proof speakable without shipping the proof,
/// so what is asserted is the ratio, not any one number.
///
/// The per-moment ceiling is stated against the aggregate each proof resolves
/// rather than as a fixed budget. Bounding it absolutely would mean bounding the
/// supporting facts, and a proof whose concept rests on a long line genuinely
/// needs more of them; truncating would deliver a partial proof, which grounding
/// refuses to do.
///
/// A third is measured, not chosen. The ratio a proof reaches depends on how
/// large the aggregate it resolves is, and the synthetic canonical Game's
/// aggregates are small enough that its proofs peak near a third while every
/// projection stays far under the absolute budget the diet was set against. So
/// both are asserted: the ratio, which is what the projection exists for, and
/// the per-proof ceiling of 8 KiB the plan actually named, which is what a
/// reader on the wire feels. See the distribution recorded on #258.
#[tokio::test]
async fn a_grounded_proof_is_a_fraction_of_the_aggregate_it_resolves() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("projection-size-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "projection-size-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, _, review) = snapshot(&read).expect("the review has a snapshot");

    let mut measured = 0;
    let mut ratios: Vec<(usize, usize)> = Vec::new();
    for (index, proven) in review
        .critical_moments
        .iter()
        .filter(|moment| moment.decision_explanation.is_some())
        .enumerate()
    {
        let detailed = submit(
            &processor,
            principal.clone(),
            envelope(
                &format!("projection-size-read-{index}"),
                ReviewSessionCommand::ReadReviewMomentDetail {
                    game_import_id: game_import_id.clone(),
                    review_moment_id: proven.critical_moment_id.clone(),
                    known_content_digest: None,
                },
            ),
        )
        .await;
        let detail = moment_detail(&detailed).expect("the moment address answers");
        // A persisted proof that will not ground is a defect, not a skip: this
        // measurement would otherwise pass by measuring nothing.
        let grounded = detail
            .explanation
            .as_ref()
            .expect("every proof this review persisted must ground whole");
        let raw = serde_json::to_vec(proven.decision_explanation.as_ref().unwrap())
            .unwrap()
            .len();
        let projected = serde_json::to_vec(grounded).unwrap().len();
        assert!(
            projected * 3 <= raw,
            "a grounded proof must stay under a third of the aggregate it \
             resolves: {projected} of {raw}"
        );
        assert!(
            projected <= 8 * 1024,
            "a grounded proof must stay inside the per-proof budget: {projected} bytes"
        );
        ratios.push((projected, raw));
        measured += 1;
    }

    assert!(
        measured > 0,
        "the fixture review must prove at least one Review Moment for this to measure anything"
    );
    let worst = ratios
        .iter()
        .map(|(projected, raw)| projected * 100 / raw)
        .max()
        .expect("at least one proof was measured");
    assert!(
        worst <= 34,
        "worst grounded proof is {worst}% of its aggregate over {measured} proofs"
    );
}

/// The payload diet of #258, measured end to end over a whole review.
///
/// §2.4 attributed 86.5% of a Game Review's bytes to `decisionExplanation`, and
/// recorded that every moment-open re-shipped the whole review on top of its
/// own moment. Neither figure can be recomputed: those payloads are deleted.
/// What is still reproducible is the comparison they were drawn from — the
/// aggregate the Coach Engine still stores against the bytes a surface is
/// handed — so that is what this measures, at the delivery seam, over a real
/// import. It is that comparison remade, not the original measurement rerun.
///
/// The "before" is the shape the wire had, not an estimate of it: session start
/// delivered the whole aggregate once and each `reviewMomentOpened` delivered it
/// again, so walking N moments cost it N+1 times. The "after" is one addressed
/// snapshot read plus one addressed moment read each.
///
/// An eighth is the plan's own target (~48 KB per moment down to ~6-8 KB) and is
/// asserted rather than reported, because a diet that is only recorded is a diet
/// that comes back. The run clears it with room to spare. The exact byte counts
/// are deliberately not repeated here — they move with the fixture and nothing
/// would catch them going stale; they are recorded on #287 with the import they
/// were taken from.
#[tokio::test]
async fn a_whole_review_ships_a_fraction_of_the_aggregate_it_addresses() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("payload-diet-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let read = submit(
        &processor,
        principal.clone(),
        envelope(
            "payload-diet-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
    )
    .await;
    let (_, _, review) = snapshot(&read).expect("the review has a snapshot");
    let moment_ids = review
        .critical_moments
        .iter()
        .map(|moment| moment.critical_moment_id.clone())
        .collect::<Vec<_>>();
    assert!(
        !moment_ids.is_empty(),
        "the fixture review must flag at least one Critical Moment for this to measure anything"
    );

    let aggregate = serde_json::to_vec(review).unwrap().len();
    let proof = review
        .critical_moments
        .iter()
        .filter_map(|moment| moment.decision_explanation.as_ref())
        .map(|explanation| serde_json::to_vec(explanation).unwrap().len())
        .sum::<usize>();
    // The attribution §2.4 made, remade against this import: the proof still
    // dominates the aggregate, which is why removing it from the wire is the
    // whole of the reduction below.
    assert!(
        proof * 2 > aggregate,
        "the proof is {proof} of {aggregate} aggregate bytes, so §2.4's attribution no longer holds \
         and the reduction below is measuring something else"
    );

    let mut delivered = delivered_bytes(read.last().expect("the snapshot read answers"));
    for (index, review_moment_id) in moment_ids.iter().enumerate() {
        let detailed = submit(
            &processor,
            principal.clone(),
            envelope(
                &format!("payload-diet-read-{index}"),
                ReviewSessionCommand::ReadReviewMomentDetail {
                    game_import_id: game_import_id.clone(),
                    review_moment_id: review_moment_id.clone(),
                    known_content_digest: None,
                },
            ),
        )
        .await;
        assert!(
            moment_detail(&detailed).is_some(),
            "every moment the snapshot named must answer, or this measures a shorter walk"
        );
        delivered += delivered_bytes(detailed.last().expect("the moment read answers"));
    }

    let before = aggregate * (moment_ids.len() + 1);
    assert!(
        delivered * 8 <= before,
        "walking {} Review Moments now ships {delivered} bytes against {before} before \
         ({}%), which is above the eighth #258 targeted",
        moment_ids.len(),
        delivered * 100 / before
    );
}

/// How much of it there is, measured after delivery rather than before, because
/// the seam is where the proof is dropped and the drop is the whole reduction.
fn delivered_bytes(event: &ReviewSessionEventEnvelope) -> usize {
    serde_json::to_vec(&delivered_result(event)).unwrap().len()
}

/// What a surface actually receives, after the delivery seam has had its say.
fn delivered_result(event: &ReviewSessionEventEnvelope) -> serde_json::Value {
    let frame = encode_delivery_frame(event.clone());
    let delivered = serde_json::from_slice::<serde_json::Value>(&frame).unwrap();
    delivered
        .pointer("/event/result")
        .cloned()
        .expect("a delivered completion carries its result")
}

fn moment_detail(events: &[ReviewSessionEventEnvelope]) -> Option<&GroundedReviewMomentDetail> {
    events.iter().find_map(|event| match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewMomentDetailRead { detail, .. } => Some(detail.as_ref()),
            _ => None,
        },
        _ => None,
    })
}
