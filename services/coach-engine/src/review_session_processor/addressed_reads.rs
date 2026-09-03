//! The session-free reads behind the addressable Game Review resources.
//!
//! Every read here is a pure function of stored Player-owned data: the Game
//! Import and, when one exists, the published Review Moment Comment in the
//! Review Annotation Store. Nothing writes, nothing leases an engine, and
//! nothing touches a Review Session. The owner-scoped lookup is the
//! authorization boundary, so a review another Player owns is unaddressable
//! rather than refused, and the same address answers with the same bytes on
//! first paint, after a reload, and a year later.

use std::sync::Arc;

use crate::{
    game_import_store::{GameImportLookup, GameImportRecord, ImportedCriticalMoment},
    grounded_review_moment::ground_review_moment,
    lichess::LichessExportClient,
    player_selected_decision,
    review_session_contract::*,
    review_session_start::{
        resolve_review_moment_position, review_moment_position, ReviewSessionStartError,
    },
};

use super::{
    events::EventEmitter, terminal::emit_start_error, ProcessorPrincipal, ReviewSessionProcessor,
};

/// Exactly the fields `GameReviewSnapshotRead` answers with, borrowed for one
/// digest. A field added to the completion must be added here too, or a cache
/// would keep an answer the engine no longer gives.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GameReviewSnapshotAnswer<'a> {
    game_import_id: &'a GameImportId,
    review: &'a GameReview,
    imported_game: &'a ImportedGame,
    review_moments: &'a [ReviewSessionMoment],
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    /// Answers one immutable Game Review snapshot for its address.
    ///
    /// The stored Game Import already holds the frozen review and the canonical
    /// Game, and a Review Moment's Position is a pure function of the two, so
    /// rendering a whole review costs one lookup.
    pub(super) async fn read_game_review_snapshot(
        &self,
        principal: ProcessorPrincipal,
        game_import_id: GameImportId,
        known_content_digest: Option<ReviewContentDigest>,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(record) = self
            .addressed_game_import(
                &principal,
                &game_import_id,
                OperationKind::GameReviewOpen,
                &emitter,
            )
            .await
        else {
            return;
        };
        match game_review_snapshot_moments(&record) {
            Ok(review_moments) => {
                let content_digest = ReviewContentDigest::of_answer(&GameReviewSnapshotAnswer {
                    game_import_id: &record.game_import_id,
                    review: &record.frozen_review,
                    imported_game: &record.imported_game,
                    review_moments: &review_moments,
                });
                // The answer had to be built to know this, so a revalidation
                // saves sending it, not producing it.
                if known_content_digest.is_some_and(|known| known == content_digest) {
                    emitter.completed(OperationCompletion::GameReviewSnapshotUnchanged {
                        game_import_id: record.game_import_id,
                        content_digest,
                    });
                    return;
                }
                emitter.completed(OperationCompletion::GameReviewSnapshotRead {
                    game_import_id: record.game_import_id,
                    review: Box::new(record.frozen_review),
                    imported_game: Box::new(record.imported_game),
                    review_moments,
                    content_digest,
                })
            }
            Err(error) => emit_start_error(&emitter, OperationKind::GameReviewOpen, error),
        }
    }

    /// Answers one Review Moment's grounded detail for its address.
    ///
    /// What comes back is the moment and nothing that contains it: the review,
    /// the imported Game, and the sibling moments are a separate read that a
    /// surface has already made. The proof behind the moment arrives resolved
    /// into names and moves; the aggregate itself stays at the audit address.
    pub(super) async fn read_review_moment_detail(
        &self,
        principal: ProcessorPrincipal,
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        known_content_digest: Option<ReviewContentDigest>,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(record) = self
            .addressed_game_import(
                &principal,
                &game_import_id,
                OperationKind::ReviewMomentOpen,
                &emitter,
            )
            .await
        else {
            return;
        };
        let Some(moment) = addressed_moment(&record, &review_moment_id) else {
            emitter.rejected(
                OperationKind::ReviewMomentOpen,
                CommandRejectionReason::UnknownMoment,
                RejectionRecovery::CorrectInput,
            );
            return;
        };
        match review_moment_position(&record.imported_game, moment.moment.ply) {
            Ok(position_snapshot) => {
                let detail = self
                    .grounded_addressed_detail(&principal, &record, &moment, &position_snapshot)
                    .await;
                let content_digest = ReviewContentDigest::of_answer(&detail);
                // A Review Moment Comment is the one part of a review a later
                // build can rewrite, and the wire comment carries no identity,
                // so the answer itself is the only honest thing to compare.
                if known_content_digest.is_some_and(|known| known == content_digest) {
                    emitter.completed(OperationCompletion::ReviewMomentDetailUnchanged {
                        game_import_id: detail.game_import_id,
                        review_moment_id: detail.review_moment_id,
                        content_digest,
                    });
                    return;
                }
                emitter.completed(OperationCompletion::ReviewMomentDetailRead {
                    detail: Box::new(detail),
                    content_digest,
                })
            }
            Err(error) => emit_start_error(&emitter, OperationKind::ReviewMomentOpen, error),
        }
    }

    /// Opens whichever Review Moment a caller's reference names.
    ///
    /// The ways a Player asks — this Critical Moment, that ply, the one after
    /// this, or the next Improvement Opportunity — differ only in how the moment
    /// is found, so they are resolved here and answered identically. Resolution
    /// happens against the stored Game Import, which keeps every forward scan
    /// out of the caller and lets a ply the review never flagged open at all.
    pub(super) async fn open_addressed_review_moment(
        &self,
        principal: ProcessorPrincipal,
        game_import_id: GameImportId,
        reference: ReviewMomentReference,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(record) = self
            .addressed_game_import(
                &principal,
                &game_import_id,
                OperationKind::ReviewMomentOpen,
                &emitter,
            )
            .await
        else {
            return;
        };
        let opened = referenced_ply(&record, &reference).and_then(|ply| {
            let position = review_moment_position(&record.imported_game, ply)?;
            let moment = referenced_moment(&record, ply, &position)?;
            Ok((moment, position))
        });
        let (moment, position) = match opened {
            Ok(opened) => opened,
            Err(error) => return emit_reference_error(&emitter, error),
        };
        let detail = self
            .grounded_addressed_detail(&principal, &record, &moment, &position)
            .await;
        emitter.completed(OperationCompletion::AddressedReviewMomentOpened {
            detail: Box::new(detail),
        })
    }

    /// Grounds one addressed Review Moment and attaches its published comment.
    ///
    /// Store miss and store failure stay absence: the proof still answers, and
    /// this read never builds a Review Session.
    async fn grounded_addressed_detail(
        &self,
        principal: &ProcessorPrincipal,
        record: &GameImportRecord,
        moment: &ImportedCriticalMoment,
        position: &PositionSnapshot,
    ) -> GroundedReviewMomentDetail {
        let mut detail = ground_review_moment(
            &record.game_import_id,
            &record.imported_game,
            &moment.moment,
            position,
            moment.decision_explanation.as_ref(),
        );
        detail.comment = self
            .published_review_moment_comment(
                principal,
                &record.game_import_id,
                &moment.moment.critical_moment_id,
            )
            .await;
        detail
    }

    /// The published Review Moment Comment, if this Player already has one.
    ///
    /// A store miss is absence. A store failure is also absence: the proof
    /// still answers, and a later first-open can retry authoring. This read
    /// never builds a Review Session.
    async fn published_review_moment_comment(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: &GameImportId,
        review_moment_id: &CriticalMomentId,
    ) -> Option<CriticalMomentComment> {
        match self.review_annotation_log(principal, game_import_id).await {
            Ok(log) => log
                .active(review_moment_id)
                .await
                .map(|annotation| annotation.comment),
            Err(error) => {
                tracing::error!(
                    category = error.diagnostic_category(),
                    "published Review Moment Comment could not be read for an addressed open"
                );
                None
            }
        }
    }

    /// Answers the whole proof aggregate behind one Review Moment.
    ///
    /// This address exists so that removing the proof from every rendering
    /// payload costs nothing in reproducibility. A moment whose proof was never
    /// built is a miss rather than an empty aggregate, so an auditor can tell
    /// "not proven" from "proven and empty".
    pub(super) async fn read_review_moment_explanation(
        &self,
        principal: ProcessorPrincipal,
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(record) = self
            .addressed_game_import(
                &principal,
                &game_import_id,
                OperationKind::ReviewMomentOpen,
                &emitter,
            )
            .await
        else {
            return;
        };
        match addressed_moment(&record, &review_moment_id)
            .and_then(|moment| moment.decision_explanation)
        {
            Some(explanation) => {
                emitter.completed(OperationCompletion::ReviewMomentExplanationRead {
                    game_import_id: record.game_import_id,
                    review_moment_id,
                    explanation: Box::new(explanation),
                })
            }
            None => emitter.rejected(
                OperationKind::ReviewMomentOpen,
                CommandRejectionReason::UnknownMoment,
                RejectionRecovery::CorrectInput,
            ),
        }
    }

    /// Looks up the Game Import an address names, for this Player alone.
    ///
    /// A miss and another Player's review are the same answer on purpose: the
    /// owner segment is part of the address, so a review the caller does not own
    /// is one that does not exist for them.
    async fn addressed_game_import(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: &GameImportId,
        operation: OperationKind,
        emitter: &EventEmitter,
    ) -> Option<Box<GameImportRecord>> {
        match self.game_imports.find(principal, game_import_id).await {
            Ok(GameImportLookup::Found(record)) => Some(record),
            Ok(GameImportLookup::NotFound | GameImportLookup::OwnerMismatch) => {
                emitter.rejected(
                    operation,
                    CommandRejectionReason::UnknownGameImport,
                    RejectionRecovery::CorrectInput,
                );
                None
            }
            Err(error) => {
                tracing::error!(
                    firestore_operation = "addressed_game_review_read",
                    category = error.diagnostic_category(),
                    error = %error,
                    "an addressed Game Review read failed"
                );
                emitter.unavailable(
                    operation,
                    ProviderUnavailableReason::Persistence,
                    RetryDirective::RetryAllowed,
                );
                None
            }
        }
    }
}

/// The Review Moments an immutable address is allowed to name.
///
/// The frozen review's automatic Critical Moments and nothing else — exactly
/// the set the snapshot orders. Player-Selected Moments accumulate while a
/// Player works, so admitting them here would let the same address answer
/// differently over time, and answering the same forever is the one property
/// these addresses exist to provide. A Review Moment ID is also a pure function
/// of the Game and the ply, so without this every legal ply in the Game would
/// resolve to an address the review never named.
fn addressed_moment(
    record: &GameImportRecord,
    review_moment_id: &CriticalMomentId,
) -> Option<ImportedCriticalMoment> {
    record
        .automatic_critical_moments()
        .into_iter()
        .find(|imported| imported.moment.critical_moment_id == *review_moment_id)
}

/// Why a Review Moment reference named nothing this Game Import can open.
enum ReferenceError {
    /// The reference names no moment of this review, or none after this one.
    UnknownMoment,
    /// The ply is inside the Game but its analysed moment was never stored.
    MissingEvidence,
    /// The Game itself could not answer for that ply.
    Start(ReviewSessionStartError),
}

impl From<ReviewSessionStartError> for ReferenceError {
    fn from(error: ReviewSessionStartError) -> Self {
        Self::Start(error)
    }
}

fn emit_reference_error(emitter: &EventEmitter, error: ReferenceError) {
    match error {
        ReferenceError::UnknownMoment => emitter.rejected(
            OperationKind::ReviewMomentOpen,
            CommandRejectionReason::UnknownMoment,
            RejectionRecovery::CorrectInput,
        ),
        ReferenceError::MissingEvidence => emitter.rejected(
            OperationKind::ReviewMomentOpen,
            CommandRejectionReason::MissingEvidence,
            RejectionRecovery::None,
        ),
        ReferenceError::Start(error) => {
            emit_start_error(emitter, OperationKind::ReviewMomentOpen, error)
        }
    }
}

/// Resolves a reference to the one ply it names.
///
/// Every reference collapses to a ply, because a ply is what a Position and a
/// stored moment are both keyed by. Doing the collapse first is what keeps the
/// three ways of asking from becoming three ways of answering: once the ply is
/// known, "the Critical Moment the pipeline flagged" and "the move I asked
/// about" are looked up identically.
fn referenced_ply(
    record: &GameImportRecord,
    reference: &ReviewMomentReference,
) -> Result<u16, ReferenceError> {
    match reference {
        ReviewMomentReference::Ply { ply } => Ok(*ply),
        ReviewMomentReference::Critical { review_moment_id } => {
            addressed_moment(record, review_moment_id)
                .map(|imported| imported.moment.ply)
                .ok_or(ReferenceError::UnknownMoment)
        }
        ReviewMomentReference::Next {
            after_review_moment_id,
            classification,
        } => {
            let mut plies =
                ordered_critical_moment_plies(record, classification.as_ref()).into_iter();
            match after_review_moment_id {
                // No current moment is "start at the beginning", not an error:
                // a Player who says "show me the next Critical Moment" before
                // opening one means the first.
                None => plies.next().ok_or(ReferenceError::UnknownMoment),
                Some(current) => {
                    let ply = anchor_ply(record, current)?;
                    plies
                        .find(|candidate| *candidate > ply)
                        .ok_or(ReferenceError::UnknownMoment)
                }
            }
        }
    }
}

/// Where a "next" step starts counting from.
///
/// The moment on screen when a Player says "show me the next Critical Moment"
/// need not be a Critical Moment: they may have just opened a bare ply the
/// review never flagged, and asking what comes after it is the same ordinary
/// request. So the anchor is resolved against the Player-Selected Moments too,
/// and only its ply is kept — the step itself still walks the frozen Critical
/// Moments, because those are what "next Critical Moment" means. Restricting
/// the anchor would make the two requests the parent spec pairs (open a ply,
/// then step onward) fail for no reason the Player could see.
fn anchor_ply(
    record: &GameImportRecord,
    review_moment_id: &CriticalMomentId,
) -> Result<u16, ReferenceError> {
    if let Some(automatic) = addressed_moment(record, review_moment_id) {
        return Ok(automatic.moment.ply);
    }
    record
        .player_selected_moments
        .iter()
        .find(|(_, moment)| moment.critical_moment_id == *review_moment_id)
        .map(|(ply, _)| *ply)
        .ok_or(ReferenceError::UnknownMoment)
}

/// The ply-ordered Critical Moments a chronological or filtered next step walks.
fn ordered_critical_moment_plies(
    record: &GameImportRecord,
    classification: Option<&ReviewMomentReferenceClassification>,
) -> Vec<u16> {
    let mut plies = record
        .automatic_critical_moments()
        .into_iter()
        .filter(|imported| match classification {
            None => true,
            Some(ReviewMomentReferenceClassification::ImprovementOpportunity) => matches!(
                imported.moment.classification,
                GameReviewMomentClassification::ImprovementOpportunity { .. }
            ),
        })
        .map(|imported| imported.moment.ply)
        .collect::<Vec<_>>();
    plies.sort_unstable();
    plies
}

/// The stored moment at one ply, flagged or not.
///
/// The automatic Critical Moment wins whenever there is one, so a Critical
/// Moment a Player happens to name by its bare ply still arrives with the
/// pipeline's Decision Explanation rather than the thinner Player-Selected one.
/// Everything else falls back to the analysed moment the import stored for that
/// ply, materialized into its Player-Selected Decision Explanation the same way
/// a Review Session materializes one.
fn referenced_moment(
    record: &GameImportRecord,
    ply: u16,
    position: &PositionSnapshot,
) -> Result<ImportedCriticalMoment, ReferenceError> {
    if let Some(automatic) = record
        .automatic_critical_moments()
        .into_iter()
        .find(|imported| imported.moment.ply == ply)
    {
        return Ok(automatic);
    }
    let imported = record
        .player_selected_moment(ply)
        .ok_or(ReferenceError::MissingEvidence)?;
    player_selected_decision::materialize(&record.imported_game.game.game_ref, position, imported)
        .map_err(|error| {
            tracing::error!(
                error = %error,
                ply,
                "an addressed Player-Selected Decision Explanation could not be built"
            );
            ReferenceError::MissingEvidence
        })
}

/// Projects a stored Game Import into the ordered Review Moments a snapshot carries.
///
/// The set is the frozen review's automatic Critical Moments and nothing else,
/// which is what keeps a snapshot immutable: Player-Selected Moments accumulate
/// while a Player works, so admitting them here would make the same address
/// answer differently over time. Ordering by ply is what lets a surface step to
/// the next Critical Moment by index instead of searching.
fn game_review_snapshot_moments(
    record: &GameImportRecord,
) -> Result<Vec<ReviewSessionMoment>, ReviewSessionStartError> {
    let mut moments = record
        .automatic_critical_moments()
        .into_iter()
        .map(|facts| {
            let (occurrence, position_snapshot) = resolve_review_moment_position(
                &record.imported_game,
                ReviewMomentSelection::PipelineCriticalMoment {
                    critical_moment_id: facts.moment.critical_moment_id.clone(),
                },
            )?;
            Ok(ReviewSessionMoment {
                review_moment: occurrence,
                position_snapshot,
                learning_material: facts.moment.learning_material,
                authoring: ReviewMomentAuthoringReadiness::Pending,
                classification_kind: Some((&facts.moment.classification).into()),
            })
        })
        .collect::<Result<Vec<_>, ReviewSessionStartError>>()?;
    moments.sort_by_key(|moment| moment.review_moment.ply);
    Ok(moments)
}
