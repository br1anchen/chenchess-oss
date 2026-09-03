//! Getting the in-memory Review Session for one addressed review.
//!
//! There is no session to recover. A Review Session is process-local state
//! derived from two durable things a Player already owns — their Game Import and
//! whatever prepared analysis the shared cache holds for that review — so the
//! session for an address either is resident or is rebuilt from those. No
//! identifier is looked up, no revision is reconciled, nothing expires out from
//! under a Player, and the seven failure modes that recovery used to answer with
//! collapse into "this Player has no such Game Import".
//!
//! The rebuild is also where the cache pays off: a review whose analysis another
//! Player already computed comes back prepared, on the first command as well as
//! on a later one.

use std::{collections::btree_map::Entry, sync::Arc};

use chrono::Utc;

use crate::{
    game_import_store::{
        GameImportLookup, GameImportRecord, ImportedCriticalMoment, ReviewSessionGame,
    },
    lichess::LichessExportClient,
    review_analysis_cache::{RestoredReviewSessionMoment, ReviewAnalysisEntry},
    review_session_contract::*,
    review_session_start::{start_review_session, ReviewSessionStartError},
};

use super::{
    events::EventEmitter,
    session::{ProcessorSession, RestoredReviewSession, SessionRestoreBindings},
    ProcessorPrincipal, ReviewSessionProcessor,
};

/// Why an addressed review has no Review Session.
pub(super) enum SessionResidencyError {
    /// This Player owns no such Game Import — a miss and someone else's review
    /// are deliberately the same answer.
    UnknownGameImport,
    /// Durable state could not be read.
    PersistenceUnavailable,
    /// The stored review cannot be turned into a session at all.
    UnusableReview,
    /// The stored Game itself could not answer for one of its Review Moments.
    Start(ReviewSessionStartError),
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    /// The Review Session for one address, resident or rebuilt.
    pub(super) async fn load_session(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: &GameImportId,
    ) -> Result<Arc<ProcessorSession>, SessionResidencyError> {
        if let Some(session) = self.resident_session(principal, game_import_id).await {
            return Ok(session);
        }
        let imported = match self.game_imports.find(principal, game_import_id).await {
            Ok(GameImportLookup::Found(record)) => *record,
            Ok(GameImportLookup::NotFound | GameImportLookup::OwnerMismatch) => {
                return Err(SessionResidencyError::UnknownGameImport)
            }
            Err(error) => {
                tracing::error!(
                    firestore_operation = "game_import_find",
                    category = error.diagnostic_category(),
                    error = %error,
                    "Game Import persistence failed"
                );
                return Err(SessionResidencyError::PersistenceUnavailable);
            }
        };
        self.build_session(principal, &imported).await
    }

    /// The resident session for an address, if it is still live.
    ///
    /// The Game Import ID carries its owner, so a session found here belongs to
    /// this Player by construction; the owner comparison is a belt-and-braces
    /// assertion rather than the boundary.
    async fn resident_session(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: &GameImportId,
    ) -> Option<Arc<ProcessorSession>> {
        let now = Utc::now();
        let session = self.sessions.lock().await.get(game_import_id).cloned()?;
        if &session.owner != principal {
            return None;
        }
        if session.is_expired(now).await {
            let mut sessions = self.sessions.lock().await;
            if sessions
                .get(game_import_id)
                .is_some_and(|current| Arc::ptr_eq(current, &session))
            {
                sessions.remove(game_import_id);
            }
            return None;
        }
        Some(session)
    }

    /// Builds one Review Session from a stored Game Import and the shared cache.
    pub(super) async fn build_session(
        &self,
        principal: &ProcessorPrincipal,
        imported: &GameImportRecord,
    ) -> Result<Arc<ProcessorSession>, SessionResidencyError> {
        let game_import_id = &imported.game_import_id;
        let game = ReviewSessionGame::from(imported);
        let cached = match self
            .analysis_cache
            .load(game_import_id, &game, Utc::now())
            .await
        {
            Ok(entries) => entries,
            // A cache read that fails costs this review its prepared analysis,
            // not its session: everything the cache holds is recomputable.
            Err(error) => {
                tracing::warn!(
                    firestore_operation = "review_analysis_cache_load",
                    category = error.diagnostic_category(),
                    reason = error.diagnostic_reason(),
                    "prepared Review Moment analysis was not read"
                );
                Vec::new()
            }
        };
        let cache_was_empty = cached.is_empty();
        let moments = restored_moments(imported, &game, cached)?;
        let annotations = self
            .review_annotation_log(principal, game_import_id)
            .await
            .map_err(|error| {
                tracing::error!(
                    category = error.diagnostic_category(),
                    "Review Moment annotation read failed while building a Review Session"
                );
                SessionResidencyError::PersistenceUnavailable
            })?;
        let activity = self.coach_turn_activity(principal, game_import_id).await;
        let session = ProcessorSession::restore(
            RestoredReviewSession {
                owner: principal.clone(),
                game,
                moments,
            },
            SessionRestoreBindings {
                engine: self.engine.clone(),
                human: self.human.clone(),
                author: self.coaching_author.clone(),
                activity,
                annotations,
            },
        )
        .await
        .map_err(|error| {
            tracing::error!(
                category = error.diagnostic_category(),
                "Review Session runtime assembly failed"
            );
            SessionResidencyError::UnusableReview
        })?;
        // Seeding only on a miss is what keeps a reopen cheap: an address whose
        // analysis is already cached costs zero writes, where an unconditional
        // seed would spend one losing commit per Review Moment every time.
        if cache_was_empty {
            self.seed_cache(imported, &session).await;
        }
        let session = Arc::new(session);
        // Losing the insert means another command built the same session first;
        // theirs is as good as this one, so it wins.
        let session = match self.sessions.lock().await.entry(game_import_id.clone()) {
            Entry::Vacant(slot) => slot.insert(session).clone(),
            Entry::Occupied(slot) => slot.get().clone(),
        };
        Ok(session)
    }

    /// The one place a starting review's analysis is written back.
    async fn seed_cache(&self, imported: &GameImportRecord, session: &ProcessorSession) {
        let Some(prepared) = session.prepared_checkpoint_moments().await else {
            return;
        };
        let entries = match crate::review_analysis_cache::ReviewAnalysisEntries::try_new(
            imported,
            prepared,
            Utc::now(),
        ) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::error!(
                    category = "cache-entry-build",
                    reason = %error,
                    "Review Moment analysis could not be assembled for the cache"
                );
                return;
            }
        };
        // A losing seed only means the next Player pays for the same analysis.
        if let Err(error) = self.analysis_cache.seed(entries).await {
            tracing::warn!(
                firestore_operation = "review_analysis_cache_seed",
                category = error.diagnostic_category(),
                reason = error.diagnostic_reason(),
                "prepared Review Moment analysis was not cached"
            );
        }
    }
}

/// Every Review Moment of a frozen review, prepared where the cache has it.
///
/// The set is the review's automatic Critical Moments and only those, so what a
/// session admits is a pure function of the Game Import. A cache entry that does
/// not correspond to one of them is ignored rather than admitted: the cache is
/// an optimization and never a source of moments.
fn restored_moments(
    imported: &GameImportRecord,
    game: &ReviewSessionGame,
    cached: Vec<ReviewAnalysisEntry>,
) -> Result<Vec<RestoredReviewSessionMoment>, SessionResidencyError> {
    let mut cached = cached
        .into_iter()
        .map(|entry| (entry.moment_id.clone(), entry))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut moments = Vec::new();
    for facts in imported.automatic_critical_moments() {
        let moment_id = facts.moment.critical_moment_id.clone();
        if let Some(entry) = cached.remove(&moment_id) {
            if let Ok(restored) = entry.into_restored(game) {
                moments.push(interrupted(restored));
                continue;
            }
        }
        moments.push(pending_moment(imported, facts)?);
    }
    // Player-Selected Moments are not part of the frozen review. A nominated
    // instructive one is restored from the cache; a Neutral walked ply is not —
    // navigating never nominated it, and a Neutral nomination must not rejoin
    // the curated set on resume.
    moments.extend(cached.into_values().filter_map(|entry| {
        let restored = entry.into_restored(game).ok()?;
        if is_neutral_player_selected(restored_facts(&restored)) {
            return None;
        }
        Some(interrupted(restored))
    }));
    Ok(moments)
}

/// Closes any Alternative Move Exploration the rebuilt session inherits.
///
/// The work was process-local and its process is gone, so an operation still
/// recorded as active has nobody running it. Left alone it would refuse the
/// Player's next attempt at the same move on behalf of a computation that will
/// never finish.
fn interrupted(moment: RestoredReviewSessionMoment) -> RestoredReviewSessionMoment {
    match moment {
        RestoredReviewSessionMoment::Prepared {
            facts,
            mut prepared,
        } => {
            prepared.exploration.interrupt_active();
            RestoredReviewSessionMoment::Prepared { facts, prepared }
        }
        pending => pending,
    }
}

fn restored_facts(moment: &RestoredReviewSessionMoment) -> &GameReviewCriticalMoment {
    match moment {
        RestoredReviewSessionMoment::Pending { facts, .. }
        | RestoredReviewSessionMoment::Prepared { facts, .. } => facts,
    }
}

fn is_neutral_player_selected(facts: &GameReviewCriticalMoment) -> bool {
    facts.provenance == GameReviewMomentProvenance::PlayerSelected
        && matches!(
            facts.classification,
            GameReviewMomentClassification::Neutral { .. }
        )
}

fn pending_moment(
    imported: &GameImportRecord,
    facts: ImportedCriticalMoment,
) -> Result<RestoredReviewSessionMoment, SessionResidencyError> {
    let selection = ReviewMomentSelection::PipelineCriticalMoment {
        critical_moment_id: facts.moment.critical_moment_id.clone(),
    };
    let coach_turn_id =
        crate::review_durability::review_moment_coach_turn_id(&imported.game_import_id, &selection)
            .ok_or(SessionResidencyError::UnusableReview)?;
    let request_id =
        crate::review_durability::review_moment_request_id(&imported.game_import_id, &selection)
            .ok_or(SessionResidencyError::UnusableReview)?;
    let core = start_review_session(
        request_id,
        coach_turn_id,
        imported.imported_game.clone(),
        selection,
    )
    .map_err(SessionResidencyError::Start)?;
    Ok(RestoredReviewSessionMoment::Pending {
        facts: facts.moment,
        core: Box::new(core),
    })
}

pub(super) fn emit_session_failure(
    emitter: &EventEmitter,
    operation: OperationKind,
    error: SessionResidencyError,
) {
    match error {
        SessionResidencyError::PersistenceUnavailable => emitter.unavailable(
            operation,
            ProviderUnavailableReason::Persistence,
            RetryDirective::RetryAllowed,
        ),
        SessionResidencyError::UnknownGameImport => emitter.rejected(
            operation,
            CommandRejectionReason::UnknownGameImport,
            RejectionRecovery::CorrectInput,
        ),
        SessionResidencyError::UnusableReview => emitter.rejected(
            operation,
            CommandRejectionReason::MissingEvidence,
            RejectionRecovery::None,
        ),
        SessionResidencyError::Start(error) => {
            super::terminal::emit_start_error(emitter, operation, error)
        }
    }
}
