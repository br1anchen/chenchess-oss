//! One cached Review Moment: what it holds, and what makes it valid.
//!
//! An entry is a pure function of the Game, the reviewed side, and the Elo, so
//! it belongs to the review rather than to whoever happened to compute it.
//! Nothing here names a Review Session, a revision, or a lifetime of its own:
//! entries are seeded first-writer-wins, upgraded when a Player prepares more
//! analysis than the stored entry carries, and evicted by the cache's own
//! retention.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    game_import_store::{GameImportRecord, ReviewSessionGame, GAME_IMPORT_SCHEMA_VERSION},
    quality_capture::QualityCaptureDraft,
    review_session_contract::{
        CoachTurnContext, CriticalMomentId, DecisionExplanation, EvidenceEntry, EvidenceId,
        EvidenceKind, GameImportId, GameReviewCriticalMoment, IdempotencyKey, ImportedGame,
        PositionSnapshot, ProofCapability, RequestId, ReviewMomentLearningMaterial,
        ReviewMomentOccurrence, ReviewMomentSelection, ReviewSessionCoreContract,
    },
    review_session_exploration::AlternativeMoveExplorationCheckpoint,
    review_session_processor::ProcessorPrincipal,
};

use super::{cache_purge_at, comment_publication::ReviewMomentCommentPublicationCheckpoint};

/// Firestore allows larger documents, but Review Moment persistence deliberately keeps this
/// lower guard so serialization and indexing overhead retain useful headroom.
///
/// The bytes counted are the document Firestore actually receives, whose
/// payload field is compressed (`firestore::codec::DurablePayload`). Measured
/// Review analysis compresses about six times, so this bounds a much larger
/// moment than the number alone suggests.
pub(super) const MAX_REVIEW_MOMENT_DOCUMENT_BYTES: usize = 700 * 1024;

pub type ReviewAnalysisCacheFuture<'a, T = ()> =
    Pin<Box<dyn Future<Output = Result<T, ReviewAnalysisCacheError>> + Send + 'a>>;

/// The shared cache of prepared Review Moment analysis.
///
/// Reads are keyed by the review, not by a Player and not by a session, so the
/// second Player to review a Game and the same Player returning next week both
/// read analysis they did not compute. Authorization is upstream: a caller
/// reaches a review key only by resolving a Game Import they own.
pub trait ReviewAnalysisCacheStore: Send + Sync {
    /// Writes this review's analysis, first writer wins.
    ///
    /// A conflict is a cache hit rather than a failure: whoever got there first
    /// prepared at least as much as this caller has.
    fn seed<'a>(&'a self, entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a>;

    /// The prepared analysis stored for one review, within retention.
    fn load<'a>(
        &'a self,
        game_import_id: &'a GameImportId,
        game: &'a ReviewSessionGame,
        now: DateTime<Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>>;

    /// Upgrades one entry with analysis the stored one does not carry.
    fn replace_moment<'a>(
        &'a self,
        mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a>;
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedReviewSessionMoment {
    pub(crate) core: ReviewSessionCoreContract,
    pub(crate) local_decision: Option<Box<LocalDecisionCheckpoint>>,
    pub(crate) idempotency_keys: BTreeSet<IdempotencyKey>,
    pub(crate) exploration: AlternativeMoveExplorationCheckpoint,
    pub(crate) comment_publication: ReviewMomentCommentPublicationCheckpoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalDecisionCheckpoint {
    pub(crate) explanation: DecisionExplanation,
    pub(crate) learning_material: ReviewMomentLearningMaterial,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CheckpointReviewSessionMoment {
    Pending {
        core: Box<ReviewSessionCoreContract>,
    },
    Prepared(Box<PreparedReviewSessionMoment>),
}

impl CheckpointReviewSessionMoment {
    fn core(&self) -> &ReviewSessionCoreContract {
        match self {
            Self::Pending { core } => core,
            Self::Prepared(prepared) => &prepared.core,
        }
    }
}

/// A whole review's entries, ready to be seeded.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewAnalysisEntries {
    pub(crate) game_import_id: GameImportId,
    pub(crate) owner: ProcessorPrincipal,
    pub(crate) game: ReviewSessionGame,
    pub(crate) entries: Vec<ReviewAnalysisEntry>,
}

impl ReviewAnalysisEntries {
    /// Builds every entry of a starting review, or none of them.
    ///
    /// The admitted set must be exactly the frozen review's automatic Critical
    /// Moments: an entry is addressed by the review, so admitting anything a
    /// second reader would not also see would make the shared address answer
    /// differently depending on who wrote it.
    pub(crate) fn try_new(
        imported: &GameImportRecord,
        review_moments: Vec<CheckpointReviewSessionMoment>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ReviewAnalysisEntryError> {
        if imported.schema_version != GAME_IMPORT_SCHEMA_VERSION {
            return Err(ReviewAnalysisEntryError::GameImportSchemaVersion {
                found: imported.schema_version,
            });
        }
        let game = ReviewSessionGame::from(imported);
        let expected_ids = game
            .automatic_critical_moments()
            .into_iter()
            .map(|moment| moment.moment.critical_moment_id)
            .collect::<Vec<_>>();
        let mut actual_moments = review_moments
            .iter()
            .map(|moment| {
                let core = moment.core();
                (core.review_moment.ply, core.review_moment.moment_id.clone())
            })
            .collect::<Vec<_>>();
        actual_moments.sort_by_key(|(ply, _)| *ply);
        if actual_moments
            .into_iter()
            .map(|(_, moment_id)| moment_id)
            .collect::<Vec<_>>()
            != expected_ids
            || review_moments.iter().any(|moment| {
                let core = moment.core();
                !matches!(
                    core.review_moment.selection,
                    ReviewMomentSelection::PipelineCriticalMoment { .. }
                )
            })
        {
            return Err(ReviewAnalysisEntryError::AdmittedMomentsNotAutomaticSet);
        }
        let mut seen = BTreeSet::new();
        let mut entries = Vec::with_capacity(review_moments.len());
        for admitted in review_moments {
            if !seen.insert(admitted.core().review_moment.moment_id.clone()) {
                return Err(ReviewAnalysisEntryError::DuplicateAdmittedMoment);
            }
            entries.push(ReviewAnalysisEntry::try_new(&game, admitted, created_at)?);
        }
        entries.sort_by_key(|entry| entry.core.review_moment.ply);
        Ok(Self {
            game_import_id: imported.game_import_id.clone(),
            owner: imported.owner.clone(),
            game,
            entries,
        })
    }
}

/// The stored shape of one cached Review Moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewAnalysisEntry {
    pub(crate) moment_id: CriticalMomentId,
    pub(crate) purge_at: DateTime<Utc>,
    pub(crate) evidence: Vec<EvidenceEntry>,
    pub(crate) core: ReviewAnalysisEntryCore,
    pub(crate) authoring: ReviewAnalysisEntryAuthoring,
}

impl ReviewAnalysisEntry {
    pub(crate) fn try_new(
        game: &ReviewSessionGame,
        admitted: CheckpointReviewSessionMoment,
        written_at: DateTime<Utc>,
    ) -> Result<Self, ReviewAnalysisEntryError> {
        validated_facts(game, &admitted)?;
        let (core, authoring) = match admitted {
            CheckpointReviewSessionMoment::Pending { core } => {
                (*core, ReviewAnalysisEntryAuthoring::Pending)
            }
            CheckpointReviewSessionMoment::Prepared(prepared) => {
                let authoring = ReviewAnalysisEntryAuthoring::Prepared {
                    local_decision: prepared.local_decision,
                    idempotency_keys: prepared.idempotency_keys,
                    exploration: prepared.exploration,
                    comment_publication: Box::new(prepared.comment_publication),
                };
                (prepared.core, authoring)
            }
        };
        let ReviewSessionCoreContract {
            request_id,
            imported_game: _,
            position_snapshot,
            review_moment,
            coach_turn_context,
            evidence_packet,
        } = core;
        let durable_evidence = evidence_packet.durable_review_session_evidence();
        let coach_turn_context = coach_turn_context.objective_context(&durable_evidence);
        let evidence_entries = durable_evidence.entries;
        let mut indexed = BTreeMap::new();
        for entry in &evidence_entries {
            let evidence_id = entry.metadata().evidence_id.clone();
            if indexed.insert(evidence_id.clone(), entry.clone()).is_some() {
                return Err(ReviewAnalysisEntryError::DuplicateEvidenceId {
                    evidence_id,
                    kind: entry.kind(),
                });
            }
        }
        let evidence_refs = evidence_entries
            .iter()
            .map(|entry| entry.metadata().evidence_id.clone())
            .collect::<Vec<_>>();
        let entry = Self {
            moment_id: review_moment.moment_id.clone(),
            purge_at: cache_purge_at(written_at)
                .ok_or(ReviewAnalysisEntryError::PurgeInstantUnrepresentable)?,
            evidence: evidence_entries,
            core: ReviewAnalysisEntryCore {
                request_id,
                position_snapshot,
                review_moment,
                coach_turn_context,
                evidence_refs,
            },
            authoring,
        };
        let encoded_bytes = super::firestore::encoded_moment_bytes(&entry, game)
            .map_err(|_| ReviewAnalysisEntryError::DocumentEncodingFailed)?;
        if encoded_bytes > MAX_REVIEW_MOMENT_DOCUMENT_BYTES {
            return Err(ReviewAnalysisEntryError::DocumentTooLarge {
                bytes: encoded_bytes,
            });
        }
        entry.validate(game)?;
        Ok(entry)
    }

    /// Reconstructs the live Review Moment one stored entry describes.
    pub(crate) fn into_restored(
        self,
        game: &ReviewSessionGame,
    ) -> Result<RestoredReviewSessionMoment, ReviewAnalysisEntryError> {
        let (admitted, facts) = self.restored_parts(game)?;
        Ok(match admitted {
            CheckpointReviewSessionMoment::Pending { core } => {
                RestoredReviewSessionMoment::Pending { facts, core }
            }
            CheckpointReviewSessionMoment::Prepared(prepared) => {
                let mut facts = facts;
                if let Some(local_decision) = &prepared.local_decision {
                    facts.learning_material = local_decision.learning_material.clone();
                }
                RestoredReviewSessionMoment::Prepared { facts, prepared }
            }
        })
    }

    pub(crate) fn validate(
        &self,
        game: &ReviewSessionGame,
    ) -> Result<(), ReviewAnalysisEntryError> {
        self.restored_parts(game).map(|_| ())
    }

    fn restored_parts(
        &self,
        game: &ReviewSessionGame,
    ) -> Result<(CheckpointReviewSessionMoment, GameReviewCriticalMoment), ReviewAnalysisEntryError>
    {
        if self.moment_id != self.core.review_moment.moment_id {
            return Err(ReviewAnalysisEntryError::StoredMomentIdMismatch);
        }
        if self
            .evidence
            .iter()
            .map(|entry| &entry.metadata().evidence_id)
            .ne(self.core.evidence_refs.iter())
        {
            return Err(ReviewAnalysisEntryError::StoredEvidenceRefsMismatch);
        }
        if let Some(duplicate) = first_duplicate_evidence(&self.evidence) {
            return Err(ReviewAnalysisEntryError::DuplicateEvidenceId {
                evidence_id: duplicate.metadata().evidence_id.clone(),
                kind: duplicate.kind(),
            });
        }
        let core = ReviewSessionCoreContract {
            request_id: self.core.request_id.clone(),
            imported_game: game.imported_game.clone(),
            position_snapshot: self.core.position_snapshot.clone(),
            review_moment: self.core.review_moment.clone(),
            coach_turn_context: self.core.coach_turn_context.clone(),
            evidence_packet: crate::review_session_contract::ReviewSessionEvidencePacket {
                entries: self.evidence.clone(),
            },
        };
        let admitted = match &self.authoring {
            ReviewAnalysisEntryAuthoring::Pending => CheckpointReviewSessionMoment::Pending {
                core: Box::new(core),
            },
            ReviewAnalysisEntryAuthoring::Prepared {
                local_decision,
                idempotency_keys,
                exploration,
                comment_publication,
            } => CheckpointReviewSessionMoment::Prepared(Box::new(PreparedReviewSessionMoment {
                core,
                local_decision: local_decision.clone(),
                idempotency_keys: idempotency_keys.clone(),
                exploration: exploration.clone(),
                comment_publication: comment_publication.as_ref().clone(),
            })),
        };
        let facts = validated_facts(game, &admitted)?;
        Ok((admitted, facts))
    }
}

pub(crate) enum RestoredReviewSessionMoment {
    Pending {
        facts: GameReviewCriticalMoment,
        core: Box<ReviewSessionCoreContract>,
    },
    Prepared {
        facts: GameReviewCriticalMoment,
        prepared: Box<PreparedReviewSessionMoment>,
    },
}

/// Why one Review Moment could not be assembled into a cache entry.
///
/// Assembly is the last gate before a durable write, and its rejection is what
/// a Player sees as a `persistence` failure. Every variant therefore names one
/// condition: a reason that reaches the operator as a bucket covering several
/// unrelated causes cannot be acted on, and this type exists to be acted on.
///
/// Payloads carry only what varies. A guard's own limit is a constant the
/// message interpolates directly, so it informs the log without widening the
/// type.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ReviewAnalysisEntryError {
    #[error(
        "Game Import schema version {found} is not the supported version {}",
        GAME_IMPORT_SCHEMA_VERSION
    )]
    GameImportSchemaVersion { found: u8 },
    #[error("admitted Review Moments are not the review's automatic Critical Moments")]
    AdmittedMomentsNotAutomaticSet,
    #[error("the same Review Moment was admitted twice")]
    DuplicateAdmittedMoment,
    #[error("the Review Moment is not a moment of this Game Import")]
    MomentNotInGameImport,
    #[error("the entry's Game Import does not match the review's")]
    EntryGameMismatch,
    #[error("the entry's ply does not match the Critical Moment's")]
    EntryPlyMismatch,
    #[error("the entry's Review Moment id does not match the Critical Moment's")]
    EntryMomentIdMismatch,
    #[error("the entry's moment selection does not address the Critical Moment")]
    EntrySelectionMismatch,
    #[error("the entry's core contract failed its integrity check: {0}")]
    EntryIntegrity(&'static str),
    #[error("the entry's local decision is not supported by its explanation")]
    EntryLocalDecision,
    #[error("the retained Alternative Move Exploration is malformed: {0}")]
    EntryExplorationShape(&'static str),
    /// Unlike the exploration and integrity checks, the publication predicate
    /// resolves through nested per-attempt and per-outcome rules with no reason
    /// vocabulary of its own. Naming its branches means authoring that
    /// vocabulary, which is worth doing when this fires and guesswork when it
    /// has not.
    #[error("the entry's comment publication does not match the Critical Moment")]
    EntryCommentPublication,
    /// Carries the collision, not just its existence. An evidence id is the
    /// entry's own content digest, so a repeat means the same evidence was
    /// appended twice and the id and kind identify which append path did it.
    #[error("two {kind:?} evidence entries share the evidence id {evidence_id:?}")]
    DuplicateEvidenceId {
        evidence_id: EvidenceId,
        kind: EvidenceKind,
    },
    #[error("the cache purge instant is not representable")]
    PurgeInstantUnrepresentable,
    #[error("the Review Moment document could not be encoded")]
    DocumentEncodingFailed,
    #[error(
        "the encoded Review Moment document is {bytes} bytes, over the {} byte guard",
        MAX_REVIEW_MOMENT_DOCUMENT_BYTES
    )]
    DocumentTooLarge { bytes: usize },
    #[error("the stored moment id does not match the entry's Review Moment id")]
    StoredMomentIdMismatch,
    #[error("the stored evidence does not match the entry's evidence refs")]
    StoredEvidenceRefsMismatch,
    /// Reconstructing an entry from its stored document is a different entry
    /// point from assembly, and shares one reason across its sites. Every one
    /// of them means the same thing to a reader — the document on disk does not
    /// describe a moment of this Game Import — and none has been observed
    /// firing, so a reason vocabulary for them would be authored blind.
    #[error("the stored Review Moment document could not be decoded")]
    StoredDocumentUndecodable,
}

/// The first entry whose evidence id another entry already used, if any.
fn first_duplicate_evidence(entries: &[EvidenceEntry]) -> Option<&EvidenceEntry> {
    let mut seen = BTreeSet::new();
    entries
        .iter()
        .find(|entry| !seen.insert(&entry.metadata().evidence_id))
}

/// Resolves the Critical Moment an admitted moment addresses, having checked
/// that the moment is a valid entry against it.
///
/// The two are one step because neither is meaningful alone: the facts are what
/// the checks are against, and every caller needs both.
fn validated_facts(
    game: &ReviewSessionGame,
    admitted: &CheckpointReviewSessionMoment,
) -> Result<GameReviewCriticalMoment, ReviewAnalysisEntryError> {
    let facts = imported_moment_facts(game, admitted.core())
        .ok_or(ReviewAnalysisEntryError::MomentNotInGameImport)?;
    validate_entry(admitted, &facts, &game.imported_game)?;
    Ok(facts)
}

/// Names the first way one admitted moment fails the review it claims to
/// belong to, or nothing.
///
/// These are eight unrelated ways to be invalid. Which one fired is what the
/// operator acts on, so the check reports the condition rather than a bare
/// verdict.
fn validate_entry(
    admitted: &CheckpointReviewSessionMoment,
    facts: &GameReviewCriticalMoment,
    imported_game: &ImportedGame,
) -> Result<(), ReviewAnalysisEntryError> {
    let core = admitted.core();
    if core.imported_game != *imported_game {
        return Err(ReviewAnalysisEntryError::EntryGameMismatch);
    }
    if core.review_moment.ply != facts.ply {
        return Err(ReviewAnalysisEntryError::EntryPlyMismatch);
    }
    if core.review_moment.moment_id != facts.critical_moment_id {
        return Err(ReviewAnalysisEntryError::EntryMomentIdMismatch);
    }
    let selection_addresses_moment = match &core.review_moment.selection {
        ReviewMomentSelection::PipelineCriticalMoment { critical_moment_id } => {
            critical_moment_id == &facts.critical_moment_id
        }
        ReviewMomentSelection::PlayerSelectedMoment { ply } => *ply == facts.ply,
    };
    if !selection_addresses_moment {
        return Err(ReviewAnalysisEntryError::EntrySelectionMismatch);
    }
    if let Err(reason) = core.validate_integrity() {
        return Err(ReviewAnalysisEntryError::EntryIntegrity(reason));
    }
    let CheckpointReviewSessionMoment::Prepared(prepared) = admitted else {
        return Ok(());
    };
    if !prepared
        .local_decision
        .as_ref()
        .is_none_or(|decision| valid_local_decision(decision, core))
    {
        return Err(ReviewAnalysisEntryError::EntryLocalDecision);
    }
    if let Err(reason) = prepared
        .exploration
        .validate_shape(&prepared.idempotency_keys)
    {
        return Err(ReviewAnalysisEntryError::EntryExplorationShape(reason));
    }
    if !prepared
        .comment_publication
        .validate(facts, &prepared.idempotency_keys)
    {
        return Err(ReviewAnalysisEntryError::EntryCommentPublication);
    }
    Ok(())
}

fn valid_local_decision(
    decision: &LocalDecisionCheckpoint,
    core: &ReviewSessionCoreContract,
) -> bool {
    let explanation = &decision.explanation;
    if !matches!(
        core.review_moment.selection,
        ReviewMomentSelection::PlayerSelectedMoment { .. }
    ) || explanation.game_ref != core.review_moment.game_ref
        || explanation.critical_moment_id != core.review_moment.moment_id
        || explanation.position_snapshot != core.position_snapshot
        || explanation.capability != ProofCapability::ValidationOnly
        || crate::decision_explanation::validate_decision_explanation(explanation).is_err()
        || decision.learning_material.tracks.len() > 2
    {
        return false;
    }
    let selected_paths = explanation
        .selected_paths
        .iter()
        .map(|path| &path.path_ref)
        .collect::<BTreeSet<_>>();
    decision.learning_material.tracks.iter().all(|track| {
        if matches!(
            track.key,
            crate::review_session_contract::LearningTrackKey::Opening { .. }
        ) {
            return true;
        }
        let [support] = track.support.as_slice() else {
            return false;
        };
        let basis = match support {
            crate::review_session_contract::LearningTrackSupport::Improvement { basis, .. }
            | crate::review_session_contract::LearningTrackSupport::Reinforcement {
                basis, ..
            } => basis,
        };
        matches!(
            basis,
            crate::review_session_contract::LearningTrackSupportBasis::DecisionExplanation {
                explanation_path_ref,
            } if selected_paths.contains(explanation_path_ref)
        )
    })
}

fn imported_moment_facts(
    imported_game: &ReviewSessionGame,
    core: &ReviewSessionCoreContract,
) -> Option<GameReviewCriticalMoment> {
    match &core.review_moment.selection {
        // Looked up by identity, not by membership in the automatic set. The
        // stored selection already records how this moment was chosen, and
        // re-deriving that from whatever analysis the read resolves is the
        // drift that durable selections exist to remove.
        ReviewMomentSelection::PipelineCriticalMoment { critical_moment_id } => {
            imported_game.critical_moment(critical_moment_id)
        }
        ReviewMomentSelection::PlayerSelectedMoment { ply } => imported_game
            .player_selected_moment(*ply)
            .map(|moment| moment.moment),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReviewAnalysisEntryAuthoring {
    Pending,
    Prepared {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        local_decision: Option<Box<LocalDecisionCheckpoint>>,
        idempotency_keys: BTreeSet<IdempotencyKey>,
        exploration: AlternativeMoveExplorationCheckpoint,
        comment_publication: Box<ReviewMomentCommentPublicationCheckpoint>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewAnalysisEntryCore {
    pub(crate) request_id: RequestId,
    pub(crate) position_snapshot: PositionSnapshot,
    pub(crate) review_moment: ReviewMomentOccurrence,
    pub(crate) coach_turn_context: CoachTurnContext,
    pub(crate) evidence_refs: Vec<EvidenceId>,
}

/// One entry upgraded in place, with whatever quality capture the write earned.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewAnalysisMutation {
    pub(crate) game_import_id: GameImportId,
    pub(crate) owner: ProcessorPrincipal,
    pub(crate) entry: ReviewAnalysisEntry,
    pub(crate) game: ReviewSessionGame,
    pub(crate) quality_captures: Vec<QualityCaptureDraft>,
}

impl ReviewAnalysisMutation {
    pub(crate) fn try_new(
        game_import_id: GameImportId,
        owner: ProcessorPrincipal,
        game: ReviewSessionGame,
        replacement: PreparedReviewSessionMoment,
        written_at: DateTime<Utc>,
        quality_captures: Vec<QualityCaptureDraft>,
    ) -> Result<Self, ReviewAnalysisEntryError> {
        let entry = ReviewAnalysisEntry::try_new(
            &game,
            CheckpointReviewSessionMoment::Prepared(Box::new(replacement)),
            written_at,
        )?;
        Ok(Self {
            game_import_id,
            owner,
            entry,
            game,
            quality_captures,
        })
    }

    pub fn moment_id(&self) -> &CriticalMomentId {
        &self.entry.moment_id
    }
}

/// The in-memory cache used by local and test runtimes.
#[derive(Default)]
pub struct InMemoryReviewAnalysisCache {
    entries: Mutex<BTreeMap<GameImportId, BTreeMap<CriticalMomentId, ReviewAnalysisEntry>>>,
}

impl ReviewAnalysisCacheStore for InMemoryReviewAnalysisCache {
    fn seed<'a>(&'a self, entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            let mut stored = self.entries.lock().await;
            let review = stored.entry(entries.game_import_id.clone()).or_default();
            for entry in entries.entries {
                review.entry(entry.moment_id.clone()).or_insert(entry);
            }
            Ok(())
        })
    }

    fn load<'a>(
        &'a self,
        game_import_id: &'a GameImportId,
        game: &'a ReviewSessionGame,
        now: DateTime<Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>> {
        Box::pin(async move {
            let stored = self.entries.lock().await;
            let mut entries = stored
                .get(game_import_id)
                .map(|review| {
                    review
                        .values()
                        .filter(|entry| now < entry.purge_at)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            entries.retain(|entry| entry.validate(game).is_ok());
            entries.sort_by_key(|entry| entry.core.review_moment.ply);
            Ok(entries)
        })
    }

    fn replace_moment<'a>(
        &'a self,
        mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            let mut stored = self.entries.lock().await;
            stored
                .entry(mutation.game_import_id.clone())
                .or_default()
                .insert(mutation.entry.moment_id.clone(), mutation.entry);
            Ok(())
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewAnalysisCacheError {
    #[error("Review analysis persistence is misconfigured: {0}")]
    Configuration(String),
    #[error("Review analysis persistence transport failed")]
    Transport,
    #[error("Review analysis persistence is unavailable")]
    Unavailable,
    #[error("Review analysis identity already exists")]
    Conflict,
    #[error("Review analysis entry is invalid: {0}")]
    InvalidEntry(InvalidEntryReason),
}

/// Which of the many independent invalidity exits actually fired.
///
/// A cache entry is rejected from several places for unrelated reasons, and
/// every one of them used to log the same sentence. Localising the
/// selection-drift failure that motivated durable selections cost a staging
/// round trip purely because the log could not say which exit it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidEntryReason {
    /// A stored Review Moment document could not be decoded into a moment.
    MomentDecode,
    /// A live Review Moment could not be encoded for persistence.
    MomentEncode,
    /// Firestore returned a document that does not match its schema.
    DocumentDecode,
    /// The decoded entry is not valid against its Game Import.
    EntryValidation,
    /// The Game Import backing the review is unusable.
    GameImport,
    /// A cache value failed to serialize while measuring it.
    Serialization,
}

impl InvalidEntryReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MomentDecode => "moment-decode",
            Self::MomentEncode => "moment-encode",
            Self::DocumentDecode => "document-decode",
            Self::EntryValidation => "entry-validation",
            Self::GameImport => "game-import",
            Self::Serialization => "serialization",
        }
    }
}

impl std::fmt::Display for InvalidEntryReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl ReviewAnalysisCacheError {
    pub(crate) fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::Configuration(_) => "configuration",
            Self::Transport => "transport",
            Self::Unavailable => "unavailable",
            Self::Conflict => "conflict",
            Self::InvalidEntry(_) => "invalid-entry",
        }
    }

    /// Names the invalidity exit, so `invalid-entry` stays one category while
    /// remaining diagnosable.
    pub(crate) fn diagnostic_reason(&self) -> Option<&'static str> {
        match self {
            Self::InvalidEntry(reason) => Some(reason.as_str()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_analysis_cache::test_fixtures::{fixture_empty_import, fixture_import};

    /// A review the pipeline flagged nothing in has nothing to cache, and that
    /// is a complete answer rather than a failure: admitting one entry for it
    /// would be admitting a moment the review never named.
    #[test]
    fn a_review_with_no_automatic_moments_produces_no_entries() {
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let imported = fixture_empty_import(created_at);
        assert!(imported.automatic_critical_moments().is_empty());

        let entries = ReviewAnalysisEntries::try_new(&imported, Vec::new(), created_at).unwrap();

        assert!(entries.entries.is_empty());
    }

    /// The admitted set is exactly the frozen review's automatic Critical
    /// Moments. An entry is addressed by the review and read by every Player of
    /// it, so a seed that admitted fewer — or more — would make one shared
    /// address answer differently depending on who wrote it.
    #[test]
    fn a_seed_that_does_not_cover_the_frozen_review_is_refused() {
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let imported = fixture_import(created_at);
        assert!(!imported.automatic_critical_moments().is_empty());

        assert_eq!(
            ReviewAnalysisEntries::try_new(&imported, Vec::new(), created_at).unwrap_err(),
            ReviewAnalysisEntryError::AdmittedMomentsNotAutomaticSet,
        );
    }

    /// A rejected assembly names the condition that rejected it. The reason
    /// reaches the operator as the `reason` field of `cache-entry-build` and is
    /// the only thing separating a stale schema from a size guard or a
    /// malformed exploration, each of which reaches the Player identically as a
    /// `persistence` failure.
    #[test]
    fn an_unsupported_schema_version_names_the_version_it_found() {
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let mut imported = fixture_import(created_at);
        imported.schema_version = GAME_IMPORT_SCHEMA_VERSION + 1;

        let error = ReviewAnalysisEntries::try_new(&imported, Vec::new(), created_at).unwrap_err();

        assert_eq!(
            error,
            ReviewAnalysisEntryError::GameImportSchemaVersion {
                found: GAME_IMPORT_SCHEMA_VERSION + 1,
            },
        );
        assert!(
            error.to_string().contains("is not the supported version 1"),
            "the operator's line has to carry the version that was expected: {error}",
        );
    }
}
