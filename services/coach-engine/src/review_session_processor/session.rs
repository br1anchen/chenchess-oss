use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};

use tokio::sync::{Mutex, Notify, OwnedMutexGuard};

use crate::{
    critical_moment_comment::{
        grounding_ledger_for, safely_rendered_comment, CriticalMomentCommentAuthoringProvenance,
        Reauthor,
    },
    engine_analysis::EngineAnalyzer,
    evaluation_recording::ReviewSessionProviderRecording,
    game_import_store::{ImportedCriticalMoment, ReviewSessionGame},
    human_move_model::HumanMoveModel,
    language_layer_ledger::ReviewSessionSpend,
    projected_plan::ProjectedPlanBuilder,
    review_analysis_cache::ReviewMomentCommentPublicationCheckpoint,
    review_annotation_store::ReviewAnnotationLog,
    review_session_board::coordinate_text_board,
    review_session_cancellation::ReviewSessionCancellation,
    review_session_coaching::{
        AlternativeMoveCoaching, CoachTurnTargetError, PreparedCoachTurnTarget,
    },
    review_session_contract::*,
    review_session_exploration::{
        analyze_root_engine_evidence, retained_root_engine_evidence, AlternativeMoveExploration,
        AlternativeMoveExplorationStartError,
    },
};

use super::ProcessorPrincipal;

mod checkpoint;
mod comment_publication;
mod intent_authoring;
mod lifetime;
mod restoration;

use intent_authoring::LazyIntentAuthoring;
pub(crate) use lifetime::ReviewSessionLifetime;
pub(super) use restoration::{RestoredReviewSession, SessionRestoreBindings};

pub(super) use checkpoint::{
    EvidenceMutationStageError, ExplorationAdmissionStage, StagedEvidenceMutation,
    StagedExplorationMutation,
};
use comment_publication::seed_from_annotations;
pub(super) use comment_publication::{
    CommentPublicationStage, CommentPublicationStageError, StagedCommentPublication,
};

/// State shared by every Review Moment in one imported Game.
///
/// Session start prepares the complete automatic set as equal peers. Later
/// operations address a keyed moment; the delivery surface owns which moment
/// is currently displayed.
/// The ephemeral pieces `from_review_moments` installs on a rebuilt session.
pub(super) struct AssembledSession {
    pub owner: ProcessorPrincipal,
    pub game_import: Arc<ReviewSessionGame>,
    pub lifetime: ReviewSessionLifetime,
    pub checkpoint_revision: u64,
    pub review_moments: Vec<Arc<ProcessorReviewMomentEntry>>,
    pub coaching: Arc<AlternativeMoveCoaching>,
    pub annotations: Arc<ReviewAnnotationLog>,
    pub spend: Arc<ReviewSessionSpend>,
    pub captures: Arc<crate::quality_capture::HostedCaptureBuffer>,
}

pub(super) struct ProcessorSession {
    pub(super) owner: ProcessorPrincipal,
    pub(super) coaching: Arc<AlternativeMoveCoaching>,
    annotations: Arc<ReviewAnnotationLog>,
    game_import: Arc<ReviewSessionGame>,
    lifetime: Mutex<ReviewSessionLifetime>,
    checkpoint_revision: Mutex<u64>,
    mutation: Arc<Mutex<()>>,
    review_moments: Mutex<BTreeMap<CriticalMomentId, Arc<ProcessorReviewMomentEntry>>>,
    pub(super) spend: Arc<ReviewSessionSpend>,
    pub(super) captures: Arc<crate::quality_capture::HostedCaptureBuffer>,
    host_turns: Mutex<BTreeMap<IdempotencyKey, HostTurnTerminal>>,
    open_review_moment: Mutex<OpenReviewMomentSlot>,
}

/// Last-intent slot for the Review Moment HostTurn grounds on.
///
/// `complete_open_review_moment` records after a possible hosted-authoring
/// wait, so two opens can complete out of start order. A newer `begin`
/// invalidates an older generation so a stale completion cannot replace the
/// Player-visible open ply.
#[derive(Debug, Default)]
struct OpenReviewMomentSlot {
    generation: u64,
    ply: Option<u16>,
}

impl OpenReviewMomentSlot {
    fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    fn record(&mut self, ply: u16, generation: u64) {
        if self.generation == generation {
            self.ply = Some(ply);
        }
    }
}

#[cfg(test)]
mod open_review_moment_slot_tests {
    use super::OpenReviewMomentSlot;

    #[test]
    fn last_intent_wins_when_an_older_open_completes_last() {
        let mut slot = OpenReviewMomentSlot::default();
        let first = slot.begin();
        let second = slot.begin();
        slot.record(1, first);
        assert_eq!(slot.ply, None, "a stale first-open must not record");
        slot.record(3, second);
        assert_eq!(slot.ply, Some(3));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HostTurnUnavailableCause {
    GroundingRejected,
    Cancelled,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HostTurnTerminal {
    Completed {
        answer: String,
        focus_moment: Option<u16>,
        show_line: Option<HostTurnShowLine>,
    },
    Refused {
        reason: HostTurnRefusalReason,
    },
    Unavailable {
        cause: HostTurnUnavailableCause,
    },
}

/// The durable admission seam for one Review Moment.
///
/// The deterministic core is sufficient to render the moment. Rich intent and
/// exploration state is installed only after server-owned preparation
/// succeeds, so callers never infer readiness from absent fields.
pub(super) struct ProcessorReviewMomentEntry {
    core: ReviewSessionCoreContract,
    factual_moment: Option<ImportedCriticalMoment>,
    prepared: Mutex<Option<Arc<ProcessorReviewMoment>>>,
    prefetched: Mutex<PrefetchedReviewMoment>,
    prefetch_changed: Notify,
}

enum PrefetchedReviewMoment {
    Empty,
    Running {
        base_revision: u64,
        cancellation: ReviewSessionCancellation,
    },
    Ready {
        base_revision: u64,
        prepared: Arc<ProcessorReviewMoment>,
    },
}

pub(super) struct SessionCheckpointSuccessor {
    pub(super) expected_revision: u64,
    pub(super) revision: u64,
    pub(super) lifetime: ReviewSessionLifetime,
}

/// All mutable state whose authority belongs to one reviewed Game occurrence.
pub(super) struct ProcessorReviewMoment {
    core: ReviewSessionCoreContract,
    critical_moment: Option<GameReviewCriticalMoment>,
    automatic_decision_explanation: Option<crate::review_session_contract::DecisionExplanation>,
    local_decision: Option<Box<crate::review_analysis_cache::LocalDecisionCheckpoint>>,
    pub(super) exploration: Arc<AlternativeMoveExploration>,
    intent_authoring: LazyIntentAuthoring,
    idempotency_keys: Mutex<BTreeSet<IdempotencyKey>>,
    comment_facts: Option<ReviewMomentCommentFacts>,
    annotations: Arc<ReviewAnnotationLog>,
    pub(super) comment_publication: Mutex<ReviewMomentCommentPublicationCheckpoint>,
}

pub(super) enum ReviewMomentCommentPublication {
    Published {
        comment: CriticalMomentComment,
        authoring_provenance: Box<CriticalMomentCommentAuthoringProvenance>,
    },
    RetryRejected,
}

/// The stored comment a web open finds, classified by whether the prompt that
/// wrote it is still the prompt this build compiles.
pub(super) enum WebOpeningComment {
    /// Nothing published yet, so this open authors.
    Absent,
    /// Prose from the compiled prompt. Serve it.
    Current(CriticalMomentComment),
    /// Text this open is entitled to replace: prose from a prompt that has
    /// since been edited, or a safe rendering with a retry unspent. Re-author,
    /// and fall back to this text if authoring cannot land — superseded
    /// coaching prose still beats dropping the Player to a template rendering.
    Stale(SupersededComment),
}

/// Text an open is entitled to replace, and why.
///
/// The reason travels with the text because both halves of the decision are
/// read later and apart: the guard that refuses to clobber real prose, and the
/// idempotency key that has to differ between a first write and its retry.
pub(super) struct SupersededComment {
    pub(super) comment: CriticalMomentComment,
    pub(super) reason: Reauthor,
}

#[cfg(test)]
pub(super) struct ProcessorSessionBuildInput<'a> {
    pub(super) recording: Option<&'a ReviewSessionProviderRecording>,
    pub(super) engine: Arc<dyn EngineAnalyzer>,
    pub(super) human: Arc<dyn HumanMoveModel>,
    pub(super) author: Arc<dyn crate::review_session_coaching::AlternativeMoveAssessmentAuthor>,
    pub(super) activity: Arc<crate::review_session_coaching::CoachTurnActivity>,
    pub(super) factual_moment: Option<&'a ImportedCriticalMoment>,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum SessionBuildError {
    #[error(transparent)]
    Exploration(#[from] AlternativeMoveExplorationStartError),
    #[error("Intent Projection failed without reusable root Engine evidence")]
    MissingRootEngine,
    #[error("Review Moment facts are invalid for comment publication")]
    InvalidCommentFacts,
    #[error("Review Session already contains Review Moment {0:?}")]
    DuplicateReviewMoment(CriticalMomentId),
}

impl SessionBuildError {
    pub(super) fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::Exploration(AlternativeMoveExplorationStartError::InvalidCore(_)) => {
                "exploration.invalid-core"
            }
            Self::Exploration(AlternativeMoveExplorationStartError::InvalidRootEvidence(
                "position-reference",
            )) => "exploration.invalid-root-position-reference",
            Self::Exploration(AlternativeMoveExplorationStartError::InvalidRootEvidence(
                "provider-provenance",
            )) => "exploration.invalid-root-provider-provenance",
            Self::Exploration(AlternativeMoveExplorationStartError::InvalidRootEvidence(
                "analysis-validation",
            )) => "exploration.invalid-root-analysis",
            Self::Exploration(AlternativeMoveExplorationStartError::InvalidRootEvidence(_)) => {
                "exploration.invalid-root-evidence"
            }
            Self::Exploration(AlternativeMoveExplorationStartError::EvidenceCacheLimit) => {
                "exploration.evidence-cache-limit"
            }
            Self::MissingRootEngine => "missing-root-engine",
            Self::InvalidCommentFacts => "invalid-comment-facts",
            Self::DuplicateReviewMoment(_) => "duplicate-review-moment",
        }
    }

    pub(super) fn unavailable_reason(&self) -> Option<ProviderUnavailableReason> {
        match self {
            Self::MissingRootEngine => Some(ProviderUnavailableReason::StockfishProcess),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum SessionInspectionError {
    #[error("the selected Alternative Move is unknown")]
    UnknownTarget,
}

impl ProcessorSession {
    #[cfg(test)]
    pub(super) async fn new(
        owner: ProcessorPrincipal,
        game_import: Arc<ReviewSessionGame>,
        lifetime: ReviewSessionLifetime,
        core: ReviewSessionCoreContract,
        annotations: Arc<ReviewAnnotationLog>,
        input: ProcessorSessionBuildInput<'_>,
    ) -> Result<Self, SessionBuildError> {
        let ProcessorSessionBuildInput {
            recording,
            engine,
            human,
            author,
            activity,
            factual_moment,
        } = input;
        let review_moment = Arc::new(
            ProcessorReviewMoment::new(
                core.clone(),
                recording,
                engine.clone(),
                human.clone(),
                factual_moment,
                annotations.clone(),
            )
            .await?,
        );
        let review_moment = Arc::new(ProcessorReviewMomentEntry::from_prepared(
            core,
            factual_moment.cloned(),
            review_moment,
        ));
        let session = Self {
            owner,
            coaching: Arc::new(AlternativeMoveCoaching::new(human, author, activity)),
            annotations,
            game_import,
            lifetime: Mutex::new(lifetime),
            checkpoint_revision: Mutex::new(1),
            mutation: Arc::new(Mutex::new(())),
            review_moments: Mutex::new(BTreeMap::new()),
            spend: Arc::new(ReviewSessionSpend::new()),
            captures: Arc::new(crate::quality_capture::HostedCaptureBuffer::new()),
            host_turns: Mutex::new(BTreeMap::new()),
            open_review_moment: Mutex::new(OpenReviewMomentSlot::default()),
        };
        session.insert_review_moment(review_moment).await?;
        Ok(session)
    }

    pub(super) async fn from_review_moments(
        assembled: AssembledSession,
    ) -> Result<Self, SessionBuildError> {
        let AssembledSession {
            owner,
            game_import,
            lifetime,
            checkpoint_revision,
            review_moments,
            coaching,
            annotations,
            spend,
            captures,
        } = assembled;
        let session = Self {
            owner,
            coaching,
            annotations,
            game_import,
            lifetime: Mutex::new(lifetime),
            checkpoint_revision: Mutex::new(checkpoint_revision),
            mutation: Arc::new(Mutex::new(())),
            review_moments: Mutex::new(BTreeMap::new()),
            spend,
            captures,
            host_turns: Mutex::new(BTreeMap::new()),
            open_review_moment: Mutex::new(OpenReviewMomentSlot::default()),
        };
        for review_moment in review_moments {
            session.insert_review_moment(review_moment).await?;
        }
        Ok(session)
    }

    pub(super) async fn review_moment_entry(
        &self,
        moment_id: &CriticalMomentId,
    ) -> Option<Arc<ProcessorReviewMomentEntry>> {
        self.review_moments.lock().await.get(moment_id).cloned()
    }

    pub(super) async fn review_moment_entries(&self) -> Vec<Arc<ProcessorReviewMomentEntry>> {
        self.review_moments.lock().await.values().cloned().collect()
    }

    pub(super) async fn review_moment(
        &self,
        moment_id: &CriticalMomentId,
    ) -> Option<Arc<ProcessorReviewMoment>> {
        self.review_moment_entry(moment_id)
            .await?
            .prepared_moment()
            .await
    }

    pub(super) fn annotations(&self) -> &Arc<ReviewAnnotationLog> {
        &self.annotations
    }

    /// The review this session is, which is also its address.
    ///
    /// A Review Session has no identifier of its own — it is process-local state
    /// derived from one Game Import — so the address is read off the review
    /// rather than threaded alongside the session everywhere it travels.
    pub(super) fn address(&self) -> &GameImportId {
        &self.game_import.source_game_import_id
    }

    pub(super) fn game_import(&self) -> &ReviewSessionGame {
        self.game_import.as_ref()
    }

    pub(super) async fn is_expired(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        self.lifetime.lock().await.is_expired(now)
    }

    pub(super) async fn begin_mutation(&self) -> OwnedMutexGuard<()> {
        self.mutation.clone().lock_owned().await
    }

    pub(super) async fn checkpoint_successor(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<SessionCheckpointSuccessor> {
        let expected_revision = *self.checkpoint_revision.lock().await;
        let revision = expected_revision.checked_add(1)?;
        let lifetime = self.lifetime.lock().await.refreshed_at(now)?;
        Some(SessionCheckpointSuccessor {
            expected_revision,
            revision,
            lifetime,
        })
    }

    pub(super) async fn commit_checkpoint_successor(&self, successor: &SessionCheckpointSuccessor) {
        *self.checkpoint_revision.lock().await = successor.revision;
        *self.lifetime.lock().await = successor.lifetime;
    }

    pub(super) async fn checkpoint_revision(&self) -> u64 {
        *self.checkpoint_revision.lock().await
    }

    pub(super) async fn host_turn_replay(
        &self,
        idempotency_key: &IdempotencyKey,
    ) -> Option<HostTurnTerminal> {
        self.host_turns.lock().await.get(idempotency_key).cloned()
    }

    pub(super) async fn record_host_turn(
        &self,
        idempotency_key: IdempotencyKey,
        terminal: HostTurnTerminal,
    ) {
        self.host_turns
            .lock()
            .await
            .insert(idempotency_key, terminal);
    }

    pub(super) async fn begin_open_review_moment(&self) -> u64 {
        self.open_review_moment.lock().await.begin()
    }

    pub(super) async fn record_open_review_moment(&self, ply: u16, generation: u64) {
        self.open_review_moment.lock().await.record(ply, generation);
    }

    pub(super) async fn open_review_moment_ply(&self) -> Option<u16> {
        self.open_review_moment.lock().await.ply
    }

    pub(super) async fn insert_review_moment(
        &self,
        review_moment: Arc<ProcessorReviewMomentEntry>,
    ) -> Result<(), SessionBuildError> {
        let moment_id = review_moment.moment_id().clone();
        match self.review_moments.lock().await.entry(moment_id.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(review_moment);
                Ok(())
            }
            Entry::Occupied(_) => Err(SessionBuildError::DuplicateReviewMoment(moment_id)),
        }
    }

    pub(super) async fn has_active_alternative_move_evaluation(&self) -> bool {
        let moments = self.review_moments.lock().await;
        for moment in moments.values() {
            if let Some(prepared) = moment.prepared_moment().await {
                if prepared
                    .exploration
                    .current_state()
                    .await
                    .active_evaluation
                    .is_some()
                {
                    return true;
                }
            }
        }
        false
    }

    #[cfg(test)]
    pub(super) async fn prepared_review_moments(&self) -> Vec<ReviewSessionCoreContract> {
        let moments = self.review_moments.lock().await;
        let mut cores = Vec::with_capacity(moments.len());
        for moment in moments.values() {
            if let Some(moment) = moment.prepared_moment().await {
                cores.push(moment.core_snapshot().await);
            }
        }
        cores.sort_by_key(|core| core.review_moment.ply);
        cores
    }

    pub(super) async fn review_session_moments(&self) -> Vec<ReviewSessionMoment> {
        let moments = self.review_moments.lock().await;
        let mut states = Vec::with_capacity(moments.len());
        for moment in moments.values() {
            states.push(moment.contract().await);
        }
        states.sort_by_key(|moment| moment.review_moment.ply);
        states
    }

    pub(super) async fn cancel_prefetches_except(&self, retained: Option<&CriticalMomentId>) {
        let entries = self.review_moment_entries().await;
        for entry in entries {
            if retained.is_some_and(|moment_id| entry.moment_id() == moment_id) {
                continue;
            }
            entry.cancel_prefetch().await;
        }
    }
}

impl ProcessorReviewMomentEntry {
    pub(super) fn pending(
        core: ReviewSessionCoreContract,
        factual_moment: Option<ImportedCriticalMoment>,
    ) -> Self {
        Self {
            core,
            factual_moment,
            prepared: Mutex::new(None),
            prefetched: Mutex::new(PrefetchedReviewMoment::Empty),
            prefetch_changed: Notify::new(),
        }
    }

    pub(super) fn from_prepared(
        core: ReviewSessionCoreContract,
        factual_moment: Option<ImportedCriticalMoment>,
        prepared: Arc<ProcessorReviewMoment>,
    ) -> Self {
        Self {
            core,
            factual_moment,
            prepared: Mutex::new(Some(prepared)),
            prefetched: Mutex::new(PrefetchedReviewMoment::Empty),
            prefetch_changed: Notify::new(),
        }
    }

    pub(super) fn moment_id(&self) -> &CriticalMomentId {
        &self.core.review_moment.moment_id
    }

    pub(super) fn ply(&self) -> u16 {
        self.core.review_moment.ply
    }

    pub(super) async fn prepared_moment(&self) -> Option<Arc<ProcessorReviewMoment>> {
        self.prepared.lock().await.clone()
    }

    pub(super) async fn contract(&self) -> ReviewSessionMoment {
        let factual_moment = self
            .factual_moment
            .as_ref()
            .expect("every deliverable Review Moment retains imported facts");
        let learning_material = factual_moment.moment.learning_material.clone();
        let classification_kind = Some((&factual_moment.moment.classification).into());
        match self.prepared_moment().await {
            Some(prepared) => ReviewSessionMoment::prepared(
                prepared.core_snapshot().await,
                learning_material,
                classification_kind,
            ),
            None => {
                ReviewSessionMoment::pending(&self.core, learning_material, classification_kind)
            }
        }
    }

    pub(super) async fn prepare_candidate(
        &self,
        recording: Option<&ReviewSessionProviderRecording>,
        engine: Arc<dyn EngineAnalyzer>,
        human: Arc<dyn HumanMoveModel>,
        annotations: Arc<ReviewAnnotationLog>,
        _projection_deadline: Instant,
    ) -> Result<Arc<ProcessorReviewMoment>, SessionBuildError> {
        let prepared = Arc::new(
            ProcessorReviewMoment::new_with_projection_deadline(
                self.core.clone(),
                recording,
                engine,
                human,
                self.factual_moment.as_ref(),
                annotations,
                _projection_deadline,
            )
            .await?,
        );
        Ok(prepared)
    }

    pub(super) async fn install_prepared(&self, prepared: Arc<ProcessorReviewMoment>) {
        *self.prepared.lock().await = Some(prepared);
        let mut prefetched = self.prefetched.lock().await;
        if let PrefetchedReviewMoment::Running { cancellation, .. } = &*prefetched {
            cancellation.cancel();
        }
        *prefetched = PrefetchedReviewMoment::Empty;
        drop(prefetched);
        self.prefetch_changed.notify_waiters();
    }

    pub(super) async fn begin_prefetch(
        &self,
        base_revision: u64,
        cancellation: ReviewSessionCancellation,
    ) -> bool {
        let mut prefetched = self.prefetched.lock().await;
        if !matches!(*prefetched, PrefetchedReviewMoment::Empty)
            || self.prepared_moment().await.is_some()
        {
            return false;
        }
        *prefetched = PrefetchedReviewMoment::Running {
            base_revision,
            cancellation,
        };
        true
    }

    pub(super) async fn finish_prefetch(
        &self,
        base_revision: u64,
        cancellation: &ReviewSessionCancellation,
        prepared: Option<Arc<ProcessorReviewMoment>>,
    ) {
        let mut prefetched = self.prefetched.lock().await;
        let is_current = matches!(
            &*prefetched,
            PrefetchedReviewMoment::Running {
                base_revision: current,
                ..
            } if *current == base_revision
        );
        if !is_current {
            return;
        }
        *prefetched = match prepared {
            Some(prepared) if !cancellation.is_cancelled() => PrefetchedReviewMoment::Ready {
                base_revision,
                prepared,
            },
            _ => PrefetchedReviewMoment::Empty,
        };
        drop(prefetched);
        self.prefetch_changed.notify_waiters();
    }

    pub(super) async fn prefetched_candidate(
        &self,
        base_revision: u64,
    ) -> Option<Arc<ProcessorReviewMoment>> {
        match &*self.prefetched.lock().await {
            PrefetchedReviewMoment::Ready {
                base_revision: current,
                prepared,
            } if *current == base_revision => Some(prepared.clone()),
            _ => None,
        }
    }

    pub(super) async fn has_prefetched_candidate(&self, base_revision: u64) -> bool {
        matches!(
            &*self.prefetched.lock().await,
            PrefetchedReviewMoment::Running {
                base_revision: current,
                ..
            } | PrefetchedReviewMoment::Ready {
                base_revision: current,
                ..
            } if *current == base_revision
        )
    }

    pub(super) async fn await_prefetched_candidate(
        &self,
        base_revision: u64,
        operation_cancellation: &ReviewSessionCancellation,
    ) -> Result<Option<Arc<ProcessorReviewMoment>>, ()> {
        loop {
            let changed = self.prefetch_changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let background_cancellation = {
                let prefetched = self.prefetched.lock().await;
                match &*prefetched {
                    PrefetchedReviewMoment::Ready {
                        base_revision: current,
                        prepared,
                    } if *current == base_revision => return Ok(Some(prepared.clone())),
                    PrefetchedReviewMoment::Running {
                        base_revision: current,
                        cancellation,
                    } if *current == base_revision => cancellation.clone(),
                    _ => return Ok(None),
                }
            };
            tokio::select! {
                biased;
                _ = operation_cancellation.cancelled() => {
                    background_cancellation.cancel();
                    return Err(());
                }
                _ = changed.as_mut() => {}
            }
        }
    }

    pub(super) async fn cancel_prefetch(&self) {
        let mut prefetched = self.prefetched.lock().await;
        match &*prefetched {
            PrefetchedReviewMoment::Running { cancellation, .. } => cancellation.cancel(),
            PrefetchedReviewMoment::Ready { .. } => {
                *prefetched = PrefetchedReviewMoment::Empty;
                drop(prefetched);
                self.prefetch_changed.notify_waiters();
            }
            PrefetchedReviewMoment::Empty => {}
        }
    }
}

impl ProcessorReviewMoment {
    /// The moment's Game Review entry, delivered in place of the whole review.
    pub(super) fn critical_moment(&self) -> &GameReviewCriticalMoment {
        self.critical_moment
            .as_ref()
            .expect("every deliverable Review Moment retains imported facts")
    }

    pub(super) fn moment_id(&self) -> &CriticalMomentId {
        &self.core.review_moment.moment_id
    }

    pub(super) fn decision_explanation(
        &self,
    ) -> Option<crate::review_session_contract::DecisionExplanation> {
        self.local_decision
            .as_ref()
            .map(|decision| decision.explanation.clone())
            .or_else(|| self.automatic_decision_explanation.clone())
    }

    #[cfg(test)]
    pub(super) async fn new(
        core: ReviewSessionCoreContract,
        recording: Option<&ReviewSessionProviderRecording>,
        engine: Arc<dyn EngineAnalyzer>,
        human: Arc<dyn HumanMoveModel>,
        factual_moment: Option<&ImportedCriticalMoment>,
        annotations: Arc<ReviewAnnotationLog>,
    ) -> Result<Self, SessionBuildError> {
        Self::new_with_projection_deadline(
            core,
            recording,
            engine,
            human,
            factual_moment,
            annotations,
            Instant::now(),
        )
        .await
    }

    pub(super) async fn new_with_projection_deadline(
        mut core: ReviewSessionCoreContract,
        recording: Option<&ReviewSessionProviderRecording>,
        engine: Arc<dyn EngineAnalyzer>,
        human: Arc<dyn HumanMoveModel>,
        factual_moment: Option<&ImportedCriticalMoment>,
        annotations: Arc<ReviewAnnotationLog>,
        _projection_deadline: Instant,
    ) -> Result<Self, SessionBuildError> {
        let comment_facts = factual_moment
            .map(|factual_moment| {
                ReviewMomentCommentFacts::try_from_presented_moment(factual_moment.moment.clone())
            })
            .transpose()
            .map_err(|_| SessionBuildError::InvalidCommentFacts)?;
        let automatic_decision_explanation = factual_moment
            .filter(|factual_moment| {
                factual_moment.moment.provenance
                    == crate::review_session_contract::GameReviewMomentProvenance::Automatic
            })
            .and_then(|factual_moment| factual_moment.decision_explanation.clone());
        let local_decision = factual_moment
            .filter(|factual_moment| {
                factual_moment.moment.provenance
                    == crate::review_session_contract::GameReviewMomentProvenance::PlayerSelected
            })
            .and_then(|factual_moment| {
                factual_moment
                    .decision_explanation
                    .clone()
                    .map(|explanation| {
                        Box::new(crate::review_analysis_cache::LocalDecisionCheckpoint {
                            explanation,
                            learning_material: factual_moment.moment.learning_material.clone(),
                        })
                    })
            });
        let recorded_root = recording.and_then(|recording| {
            recording.content.entries.iter().find(|entry| {
                matches!(
                    entry,
                    EvidenceEntry::EngineAnalysis { position_ref, .. }
                        if position_ref == &core.position_snapshot.position_ref
                )
            })
        });
        let retained_root = recorded_root.cloned().or_else(|| {
            let retained = factual_moment?;
            retained_root_engine_evidence(
                &core,
                &retained.moment,
                retained.engine_provenance.as_ref()?,
            )
        });
        let root_engine = match retained_root {
            Some(root_engine) => root_engine,
            None => analyze_root_engine_evidence(&core, engine.clone())
                .await
                .ok_or(SessionBuildError::MissingRootEngine)?,
        };
        let exploration = Arc::new(AlternativeMoveExploration::new(
            core.clone(),
            &root_engine,
            engine.clone(),
        )?);
        core.evidence_packet = exploration.current_state().await.evidence_packet;
        let mut comment_publication = ReviewMomentCommentPublicationCheckpoint::default();
        seed_from_annotations(
            &mut comment_publication,
            &annotations,
            &core.review_moment.moment_id,
        )
        .await;
        Ok(Self {
            core,
            critical_moment: factual_moment.map(|factual| factual.moment.clone()),
            automatic_decision_explanation,
            local_decision,
            exploration,
            intent_authoring: LazyIntentAuthoring::new(ProjectedPlanBuilder::new(engine, human)),
            idempotency_keys: Mutex::new(BTreeSet::new()),
            comment_facts,
            annotations,
            comment_publication: Mutex::new(comment_publication),
        })
    }

    #[allow(dead_code)]
    pub(super) fn comment_facts(&self) -> Option<&ReviewMomentCommentFacts> {
        self.comment_facts.as_ref()
    }

    #[allow(dead_code)]
    pub(super) fn host_learning_material(&self) -> Option<ReviewMomentLearningMaterial> {
        self.critical_moment
            .as_ref()
            .map(|moment| moment.learning_material.clone())
    }

    #[cfg(test)]
    pub(super) async fn claim_idempotency_key(&self, key: IdempotencyKey) -> bool {
        self.idempotency_keys.lock().await.insert(key)
    }

    pub(super) async fn comment_authoring_context(
        &self,
    ) -> Option<ReviewMomentCommentAuthoringContext> {
        let facts = self.comment_facts.clone()?;
        let core = self.core_snapshot().await;
        let intent = self.intent_authoring.prepare(&facts, &core).await;
        let required_grounding_ledger = grounding_ledger_for(&facts);
        Some(ReviewMomentCommentAuthoringContext {
            facts,
            intent,
            required_grounding_ledger,
        })
    }

    /// The authoring context to disclose when opening this Review Moment.
    ///
    /// A Review Moment whose comment is current discloses none: authoring a
    /// safe rendering nobody will read would run Intent Enrichment for
    /// nothing. A comment left behind by an edited prompt does disclose one,
    /// because re-authoring needs the same authority a first open needs.
    /// Publication itself authors unconditionally, because a new idempotency
    /// key is a new logical write however many comments precede it.
    pub(super) async fn opening_authoring_context(
        &self,
    ) -> Option<ReviewMomentCommentAuthoringContext> {
        if matches!(
            self.web_opening_comment().await,
            WebOpeningComment::Current(_)
        ) {
            return None;
        }
        self.comment_authoring_context().await
    }

    /// What the web surface should do with this Review Moment's stored comment.
    pub(super) async fn web_opening_comment(&self) -> WebOpeningComment {
        let publication = self.comment_publication.lock().await;
        match publication.active_comment() {
            None => WebOpeningComment::Absent,
            Some(published) => match published.authoring_provenance.reauthor() {
                Some(reason) => WebOpeningComment::Stale(SupersededComment {
                    comment: published.comment.clone(),
                    reason,
                }),
                None => WebOpeningComment::Current(published.comment.clone()),
            },
        }
    }

    /// The active comment only when the Coach App's own host model published
    /// it. Engine-hosted web artifacts are invisible on that surface for now.
    pub(super) async fn host_submitted_opening_comment(&self) -> Option<CriticalMomentComment> {
        self.comment_publication
            .lock()
            .await
            .active_comment()
            .filter(|published| published.authoring_provenance.is_host_submitted())
            .map(|published| published.comment.clone())
    }

    pub(super) async fn opening_comment(
        &self,
        authoring_context: Option<&ReviewMomentCommentAuthoringContext>,
    ) -> Option<(CriticalMomentComment, bool)> {
        if let Some(published) = self.comment_publication.lock().await.active_comment() {
            return Some((published.comment.clone(), true));
        }
        let authoring_context = authoring_context?;
        Some((
            safely_rendered_comment(&authoring_context.facts, authoring_context.intent.clone()),
            false,
        ))
    }

    pub(super) async fn core_snapshot(&self) -> ReviewSessionCoreContract {
        let packet = self.exploration.current_state().await.evidence_packet;
        self.core_with_packet(packet).await
    }

    pub(super) async fn target_for_context(
        &self,
        context: &CoachTurnContext,
    ) -> Result<PreparedCoachTurnTarget, CoachTurnTargetError> {
        let snapshot = self.exploration.current_state().await;
        let core = self
            .core_with_packet(snapshot.evidence_packet.clone())
            .await;
        let (branch_ref, uci) = match &context.target {
            CoachTurnTarget::AlternativeMove { branch_ref, uci } => (branch_ref, uci),
            CoachTurnTarget::ImportedGameMove { .. } => {
                return Err(CoachTurnTargetError::UnknownTarget)
            }
        };
        let alternative_move_id = snapshot
            .committed_moves
            .iter()
            .find(|candidate| &candidate.branch_ref == branch_ref && &candidate.move_uci == uci)
            .map(|candidate| &candidate.alternative_move_id)
            .ok_or(CoachTurnTargetError::UnknownTarget)?;

        let target = PreparedCoachTurnTarget::capture(&core, &snapshot, alternative_move_id)?;
        let mut expected = target.context().clone();
        expected.coach_turn_id = context.coach_turn_id.clone();
        if expected != *context {
            return Err(CoachTurnTargetError::MissingEvidence);
        }
        Ok(target)
    }

    pub(super) async fn inspect(
        &self,
        target: PositionInspectionTarget,
    ) -> Result<PositionInspection, SessionInspectionError> {
        let exploration = self.exploration.current_state().await;
        let core = self
            .authoring_core_with_packet(exploration.evidence_packet.clone())
            .await;
        let (context, position, evidence_packet) = match target {
            PositionInspectionTarget::ReviewedMove => {
                let (context, evidence_packet) = PreparedCoachTurnTarget::reviewed_move(&core);
                (context, core.position_snapshot.clone(), evidence_packet)
            }
            PositionInspectionTarget::AlternativeMove {
                alternative_move_id,
            } => {
                let target =
                    PreparedCoachTurnTarget::capture(&core, &exploration, &alternative_move_id)
                        .map_err(|_| SessionInspectionError::UnknownTarget)?;
                (
                    target.context().clone(),
                    target.target().resulting_position.clone(),
                    target.evidence_packet().clone(),
                )
            }
        };
        let evaluation = self
            .exploration
            .grounded_evaluation(&core.evidence_packet, &position)
            .expect("every session Position carries verified Stockfish evidence");
        Ok(PositionInspection {
            text_board: coordinate_text_board(&position),
            side_to_move: position.side_to_move,
            evaluation,
            position_snapshot: position,
            context,
            evidence_packet,
        })
    }

    pub(super) async fn player_plan_evaluation_context(
        &self,
    ) -> Option<PlayerPlanEvaluationContext> {
        let moment = self.comment_facts.as_ref()?.moment();
        let objective_counterplay_san = moment
            .objective
            .lines
            .as_ref()?
            .refutation
            .iter()
            .map(|line_move| line_move.san.clone())
            .collect::<Vec<_>>();
        if objective_counterplay_san.is_empty() {
            return None;
        }
        let core = self.core_snapshot().await;
        Some(PlayerPlanEvaluationContext::new(
            PlayerPlanEvaluationFacts {
                text_board: coordinate_text_board(&core.position_snapshot),
                position_snapshot: core.position_snapshot,
                reviewed_move_san: moment.played_san.clone(),
                objective_counterplay_san,
                best_move_evaluation: moment.objective.best_evaluation.clone(),
                played_move_evaluation: moment.objective.played_evaluation.clone(),
            },
        ))
    }

    pub(super) async fn target_for_assessment(
        &self,
        assessment: &AlternativeMoveAssessment,
    ) -> Result<PreparedCoachTurnTarget, CoachTurnTargetError> {
        let snapshot = self.exploration.current_state().await;
        let core = self
            .core_with_packet(snapshot.evidence_packet.clone())
            .await;
        PreparedCoachTurnTarget::capture(&core, &snapshot, &assessment.alternative_move_id)
    }

    async fn core_with_packet(
        &self,
        packet: ReviewSessionEvidencePacket,
    ) -> ReviewSessionCoreContract {
        let mut core = self.core.clone();
        core.evidence_packet = packet;
        core
    }

    async fn authoring_core_with_packet(
        &self,
        packet: ReviewSessionEvidencePacket,
    ) -> ReviewSessionCoreContract {
        self.core_with_packet(packet).await
    }
}

#[cfg(test)]
mod tests;
