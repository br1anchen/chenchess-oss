use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    game_import_store::ReviewSessionGame,
    review_session_contract::{
        BranchEvidence, BranchParent, BranchRef, CriticalMomentId, DecisionExplanationRef,
        EngineAnalysisEvidence, EvidenceEntry, EvidenceMetadata, EvidenceProvenance, GameImportId,
        PositionRef, PositionSnapshot, ProvenanceFact, ReviewMomentSelection,
        ReviewSessionEvidencePacket,
    },
    review_session_exploration::AlternativeMoveExplorationCheckpoint,
    review_session_start::{reconstruct_derived_position, start_review_session},
};

use super::comment_publication::ReviewMomentCommentPublicationCheckpoint;
use super::entry::{
    LocalDecisionCheckpoint, ReviewAnalysisEntry, ReviewAnalysisEntryAuthoring,
    ReviewAnalysisEntryCore, ReviewAnalysisEntryError,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DurableReviewMomentPayload {
    /// The moment's own selection is durable rather than re-derived.
    ///
    /// Re-deriving it resolved the stored moment against the analysis reachable
    /// from the Game Import, so an import shared by two Review Sessions could
    /// answer differently than it did at write time and fail every later read.
    /// The selection is small and already authoritative, so persisting it keeps
    /// restoration independent of analysis drift.
    selection: ReviewMomentSelection,
    /// Whether server-owned preparation for this Review Moment already
    /// completed.
    ///
    /// Recorded rather than inferred from the presence of prepared state. A
    /// Review Moment can be fully prepared and still carry no local decision
    /// and no committed Alternative Move, and inferring preparedness from those
    /// made every reopen re-run preparation for nothing.
    preparation_completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    local_decision: Option<DurableLocalDecision>,
    provider_evidence: Vec<DurableEvidenceEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DurableLocalDecision {
    /// The imported facts and engine provenance are already frozen with the
    /// Game Import. Reconstructing from them avoids duplicating the large proof
    /// graph while this content identity makes any reconstruction drift fail
    /// closed.
    decision_explanation_ref: DecisionExplanationRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum DurableBranchParent {
    Root { ply: u16 },
    Move { branch_ref: BranchRef },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum DerivedPosition {
    GamePly {
        ply: u16,
    },
    AlternativeLine {
        root_ply: u16,
        uci_path: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum DurableEvidenceEntry {
    Position {
        metadata: EvidenceMetadata,
        position: DerivedPosition,
    },
    EngineAnalysis {
        metadata: EvidenceMetadata,
        position: DerivedPosition,
        analysis: EngineAnalysisEvidence,
    },
    Branch {
        metadata: EvidenceMetadata,
        branch: DurableBranchEvidence,
    },
    ProviderProvenance {
        metadata: EvidenceMetadata,
        provider: EvidenceProvenance,
    },
    ImportProvenance {
        metadata: EvidenceMetadata,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DurableBranchEvidence {
    branch_ref: BranchRef,
    parent: DurableBranchParent,
    source_position: DerivedPosition,
    move_uci: String,
    resulting_position: DerivedPosition,
}

impl DurableReviewMomentPayload {
    pub(crate) fn from_moment(
        moment: &ReviewAnalysisEntry,
        game: &ReviewSessionGame,
    ) -> Result<Self, ReviewAnalysisEntryError> {
        let positions = PositionIndex::new(moment, game)?;
        let provider_evidence = moment
            .evidence
            .iter()
            .filter_map(|entry| DurableEvidenceEntry::from_entry(entry, &positions).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        // Two things are deliberately absent, and for opposite reasons.
        //
        // Published Review Moment Comments live in the annotation store, which
        // outlives this entry and is keyed by Player.
        //
        // Committed Alternative Moves are not persisted at all. They record
        // which lines *this* Player asked to explore, and this entry is shared
        // by every review of the Game: writing them here would show one Player
        // another's exploration and let two sessions overwrite each other's.
        // Alternative Move Exploration is in-memory state.
        let (preparation_completed, local_decision) = match &moment.authoring {
            ReviewAnalysisEntryAuthoring::Pending => (false, None),
            ReviewAnalysisEntryAuthoring::Prepared { local_decision, .. } => (
                true,
                local_decision
                    .as_deref()
                    .map(DurableLocalDecision::from_checkpoint),
            ),
        };

        Ok(Self {
            selection: moment.core.review_moment.selection.clone(),
            preparation_completed,
            local_decision,
            provider_evidence,
        })
    }

    pub(crate) fn into_moment(
        self,
        game_import_id: &GameImportId,
        moment_id: CriticalMomentId,
        purge_at: chrono::DateTime<chrono::Utc>,
        game: &ReviewSessionGame,
    ) -> Result<ReviewAnalysisEntry, ReviewAnalysisEntryError> {
        let Self {
            selection,
            preparation_completed,
            local_decision,
            provider_evidence,
        } = self;
        let coach_turn_id =
            crate::review_durability::review_moment_coach_turn_id(game_import_id, &selection)
                .ok_or(ReviewAnalysisEntryError::StoredDocumentUndecodable)?;
        let request_id =
            crate::review_durability::review_moment_request_id(game_import_id, &selection)
                .ok_or(ReviewAnalysisEntryError::StoredDocumentUndecodable)?;
        let mut core = start_review_session(
            request_id,
            coach_turn_id,
            game.imported_game.clone(),
            selection,
        )
        .map_err(|_| ReviewAnalysisEntryError::StoredDocumentUndecodable)?;
        if core.review_moment.moment_id != moment_id {
            return Err(ReviewAnalysisEntryError::StoredDocumentUndecodable);
        }

        let mut entries = Vec::with_capacity(provider_evidence.len());
        for entry in provider_evidence {
            entries.push(entry.into_entry(game)?);
        }
        let packet = ReviewSessionEvidencePacket { entries };
        core.coach_turn_context = core.coach_turn_context.objective_context(&packet);
        core.evidence_packet = packet.clone();
        let evidence_refs = packet
            .entries
            .iter()
            .map(|entry| entry.metadata().evidence_id.clone())
            .collect();

        let local_decision = local_decision
            .map(|decision| decision.into_checkpoint(&core, game))
            .transpose()?;
        let authoring = if preparation_completed {
            ReviewAnalysisEntryAuthoring::Prepared {
                local_decision,
                idempotency_keys: BTreeSet::new(),
                exploration: AlternativeMoveExplorationCheckpoint::default(),
                comment_publication: Box::new(ReviewMomentCommentPublicationCheckpoint::default()),
            }
        } else {
            ReviewAnalysisEntryAuthoring::Pending
        };

        Ok(ReviewAnalysisEntry {
            moment_id,
            purge_at,
            evidence: packet.entries,
            core: ReviewAnalysisEntryCore {
                request_id: core.request_id,
                position_snapshot: core.position_snapshot,
                review_moment: core.review_moment,
                coach_turn_context: core.coach_turn_context,
                evidence_refs,
            },
            authoring,
        })
    }
}

impl DurableLocalDecision {
    fn from_checkpoint(checkpoint: &LocalDecisionCheckpoint) -> Self {
        Self {
            decision_explanation_ref: checkpoint.explanation.decision_explanation_ref.clone(),
        }
    }

    fn into_checkpoint(
        self,
        core: &crate::review_session_contract::ReviewSessionCoreContract,
        game: &ReviewSessionGame,
    ) -> Result<Box<LocalDecisionCheckpoint>, ReviewAnalysisEntryError> {
        let ReviewMomentSelection::PlayerSelectedMoment { ply } = &core.review_moment.selection
        else {
            return Err(ReviewAnalysisEntryError::StoredDocumentUndecodable);
        };
        let materialized = crate::player_selected_decision::materialize(
            &core.review_moment.game_ref,
            &core.position_snapshot,
            game.player_selected_moment(*ply)
                .ok_or(ReviewAnalysisEntryError::StoredDocumentUndecodable)?,
        )
        .map_err(|_| ReviewAnalysisEntryError::StoredDocumentUndecodable)?;
        let explanation = materialized
            .decision_explanation
            .filter(|explanation| {
                explanation.decision_explanation_ref == self.decision_explanation_ref
            })
            .ok_or(ReviewAnalysisEntryError::StoredDocumentUndecodable)?;
        Ok(Box::new(LocalDecisionCheckpoint {
            explanation,
            learning_material: materialized.moment.learning_material,
        }))
    }
}

impl DurableEvidenceEntry {
    fn from_entry(
        entry: &EvidenceEntry,
        positions: &PositionIndex,
    ) -> Result<Option<Self>, ReviewAnalysisEntryError> {
        Ok(match entry {
            EvidenceEntry::Position { metadata, position } => Some(Self::Position {
                metadata: metadata.clone(),
                position: positions.locator(&position.position_ref)?,
            }),
            EvidenceEntry::EngineAnalysis {
                metadata,
                position_ref,
                analysis,
            } => Some(Self::EngineAnalysis {
                metadata: metadata.clone(),
                position: positions.locator(position_ref)?,
                analysis: analysis.clone(),
            }),
            EvidenceEntry::Branch { metadata, branch } => Some(Self::Branch {
                metadata: metadata.clone(),
                branch: DurableBranchEvidence {
                    branch_ref: branch.branch_ref.clone(),
                    parent: DurableBranchParent::from_parent(&branch.parent, positions)?,
                    source_position: positions.locator(&branch.source_position_ref)?,
                    move_uci: branch.move_uci.clone(),
                    resulting_position: positions.locator(&branch.resulting_position_ref)?,
                },
            }),
            EvidenceEntry::Provenance {
                metadata,
                fact: ProvenanceFact::Provider { provider },
            } => Some(Self::ProviderProvenance {
                metadata: metadata.clone(),
                provider: provider.clone(),
            }),
            EvidenceEntry::Provenance {
                fact: ProvenanceFact::Import { .. },
                metadata,
            } => Some(Self::ImportProvenance {
                metadata: metadata.clone(),
            }),
            EvidenceEntry::HumanMoveModel { .. } => {
                return Err(ReviewAnalysisEntryError::StoredDocumentUndecodable);
            }
        })
    }

    fn into_entry(
        self,
        game: &ReviewSessionGame,
    ) -> Result<EvidenceEntry, ReviewAnalysisEntryError> {
        let (expected_metadata, entry) = match self {
            Self::Position { metadata, position } => {
                let entry = EvidenceEntry::position(metadata.clone(), position.reconstruct(game)?);
                (metadata, entry)
            }
            Self::EngineAnalysis {
                metadata,
                position,
                analysis,
            } => {
                let position = position.reconstruct(game)?;
                let entry = EvidenceEntry::engine_analysis(
                    metadata.clone(),
                    position.position_ref,
                    analysis,
                );
                (metadata, entry)
            }
            Self::Branch { metadata, branch } => {
                let source = branch.source_position.reconstruct(game)?;
                let resulting = branch.resulting_position.reconstruct(game)?;
                let parent = branch
                    .parent
                    .into_parent(&branch.source_position, &source)?;
                let entry = EvidenceEntry::branch(
                    metadata.clone(),
                    BranchEvidence {
                        branch_ref: branch.branch_ref,
                        parent,
                        source_position_ref: source.position_ref,
                        move_uci: branch.move_uci,
                        resulting_position_ref: resulting.position_ref,
                    },
                );
                (metadata, entry)
            }
            Self::ProviderProvenance { metadata, provider } => {
                let entry = EvidenceEntry::provenance(
                    metadata.clone(),
                    ProvenanceFact::Provider { provider },
                );
                (metadata, entry)
            }
            Self::ImportProvenance { metadata } => {
                let entry = EvidenceEntry::provenance(
                    metadata.clone(),
                    ProvenanceFact::Import {
                        source: game.imported_game.provenance.clone(),
                    },
                );
                (metadata, entry)
            }
        };
        if entry.metadata() == &expected_metadata {
            Ok(entry)
        } else {
            Err(ReviewAnalysisEntryError::StoredDocumentUndecodable)
        }
    }
}

impl DurableBranchParent {
    fn from_parent(
        parent: &BranchParent,
        positions: &PositionIndex,
    ) -> Result<Self, ReviewAnalysisEntryError> {
        match parent {
            BranchParent::Root { position_ref } => match positions.locator(position_ref)? {
                DerivedPosition::GamePly { ply } => Ok(Self::Root { ply }),
                DerivedPosition::AlternativeLine { .. } => {
                    Err(ReviewAnalysisEntryError::StoredDocumentUndecodable)
                }
            },
            BranchParent::Move { branch_ref } => Ok(Self::Move {
                branch_ref: branch_ref.clone(),
            }),
        }
    }

    fn into_parent(
        self,
        source_locator: &DerivedPosition,
        source: &PositionSnapshot,
    ) -> Result<BranchParent, ReviewAnalysisEntryError> {
        match self {
            Self::Root { ply } if source_locator == &DerivedPosition::GamePly { ply } => {
                Ok(BranchParent::Root {
                    position_ref: source.position_ref.clone(),
                })
            }
            Self::Root { .. } => Err(ReviewAnalysisEntryError::StoredDocumentUndecodable),
            Self::Move { branch_ref } => Ok(BranchParent::Move { branch_ref }),
        }
    }
}

impl DerivedPosition {
    fn reconstruct(
        &self,
        game: &ReviewSessionGame,
    ) -> Result<PositionSnapshot, ReviewAnalysisEntryError> {
        let (root_ply, uci_path) = match self {
            Self::GamePly { ply } => (*ply, &[][..]),
            Self::AlternativeLine { root_ply, uci_path } => (*root_ply, uci_path.as_slice()),
        };
        reconstruct_derived_position(&game.imported_game, root_ply, uci_path)
            .map_err(|_| ReviewAnalysisEntryError::StoredDocumentUndecodable)
    }
}

struct PositionIndex {
    by_position: BTreeMap<PositionRef, DerivedPosition>,
}

impl PositionIndex {
    fn new(
        moment: &ReviewAnalysisEntry,
        game: &ReviewSessionGame,
    ) -> Result<Self, ReviewAnalysisEntryError> {
        let root_ply = moment.core.review_moment.ply;
        let root_ref = moment.core.position_snapshot.position_ref.clone();
        let game_moves = &game.imported_game.game.moves;
        let mut by_position = BTreeMap::new();
        for (index, game_move) in game_moves.iter().enumerate() {
            let before = DerivedPosition::GamePly { ply: game_move.ply };
            ensure_consistent(&by_position, &game_move.before_position_ref, &before)?;
            by_position.insert(game_move.before_position_ref.clone(), before);

            let after = match game_moves.get(index + 1) {
                Some(next) if next.before_position_ref == game_move.after_position_ref => {
                    DerivedPosition::GamePly { ply: next.ply }
                }
                Some(_) => return Err(ReviewAnalysisEntryError::StoredDocumentUndecodable),
                None => DerivedPosition::AlternativeLine {
                    root_ply: game_move.ply,
                    uci_path: vec![game_move.uci.clone()],
                },
            };
            ensure_consistent(&by_position, &game_move.after_position_ref, &after)?;
            by_position.insert(game_move.after_position_ref.clone(), after);
        }
        if by_position.get(&root_ref) != Some(&DerivedPosition::GamePly { ply: root_ply }) {
            return Err(ReviewAnalysisEntryError::StoredDocumentUndecodable);
        }
        let mut by_branch = BTreeMap::<BranchRef, (PositionRef, Vec<String>)>::new();
        let mut remaining = moment
            .evidence
            .iter()
            .filter_map(|entry| match entry {
                EvidenceEntry::Branch { branch, .. } => Some(branch),
                _ => None,
            })
            .collect::<Vec<_>>();

        while !remaining.is_empty() {
            let index = remaining
                .iter()
                .position(|branch| match &branch.parent {
                    BranchParent::Root { position_ref } => position_ref == &root_ref,
                    BranchParent::Move { branch_ref } => by_branch.contains_key(branch_ref),
                })
                .ok_or(ReviewAnalysisEntryError::StoredDocumentUndecodable)?;
            let branch = remaining.remove(index);
            let mut path = match &branch.parent {
                BranchParent::Root { .. } => Vec::new(),
                BranchParent::Move { branch_ref } => {
                    let (parent_result, path) = by_branch
                        .get(branch_ref)
                        .ok_or(ReviewAnalysisEntryError::StoredDocumentUndecodable)?;
                    if parent_result != &branch.source_position_ref {
                        return Err(ReviewAnalysisEntryError::StoredDocumentUndecodable);
                    }
                    path.clone()
                }
            };
            let source = if path.is_empty() {
                DerivedPosition::GamePly { ply: root_ply }
            } else {
                DerivedPosition::AlternativeLine {
                    root_ply,
                    uci_path: path.clone(),
                }
            };
            ensure_reconstructs(game, &branch.source_position_ref, &source)?;
            path.push(branch.move_uci.clone());
            let result = DerivedPosition::AlternativeLine {
                root_ply,
                uci_path: path.clone(),
            };
            ensure_reconstructs(game, &branch.resulting_position_ref, &result)?;
            if by_branch.contains_key(&branch.branch_ref) {
                return Err(ReviewAnalysisEntryError::StoredDocumentUndecodable);
            }
            by_position
                .entry(branch.source_position_ref.clone())
                .or_insert(source);
            by_position
                .entry(branch.resulting_position_ref.clone())
                .or_insert(result);
            by_branch.insert(
                branch.branch_ref.clone(),
                (branch.resulting_position_ref.clone(), path),
            );
        }
        Ok(Self { by_position })
    }

    fn locator(
        &self,
        position_ref: &PositionRef,
    ) -> Result<DerivedPosition, ReviewAnalysisEntryError> {
        self.by_position
            .get(position_ref)
            .cloned()
            .ok_or(ReviewAnalysisEntryError::StoredDocumentUndecodable)
    }
}

fn ensure_reconstructs(
    game: &ReviewSessionGame,
    position_ref: &PositionRef,
    position: &DerivedPosition,
) -> Result<(), ReviewAnalysisEntryError> {
    if &position.reconstruct(game)?.position_ref == position_ref {
        Ok(())
    } else {
        Err(ReviewAnalysisEntryError::StoredDocumentUndecodable)
    }
}

fn ensure_consistent(
    positions: &BTreeMap<PositionRef, DerivedPosition>,
    position_ref: &PositionRef,
    position: &DerivedPosition,
) -> Result<(), ReviewAnalysisEntryError> {
    match positions.get(position_ref) {
        Some(existing) if existing != position => {
            Err(ReviewAnalysisEntryError::StoredDocumentUndecodable)
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        review_analysis_cache::entry::{CheckpointReviewSessionMoment, ReviewAnalysisEntries},
        review_session_contract::{
            BranchEvidence, BranchRef, EvidenceMetadata, ReviewSessionCoreContract,
        },
    };
    use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};

    #[test]
    fn alternative_positions_round_trip_only_as_root_relative_paths() {
        let created_at = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = crate::review_analysis_cache::test_fixtures::fixture_import(created_at);
        let core: ReviewSessionCoreContract = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/core-contract.json"
        )))
        .unwrap();
        let mut checkpoint = ReviewAnalysisEntries::try_new(
            &imported,
            vec![CheckpointReviewSessionMoment::Pending {
                core: Box::new(core),
            }],
            created_at,
        )
        .unwrap();
        let game_import_id = imported.game_import_id.clone();
        let moment = &mut checkpoint.entries[0];
        let root_ply = moment.core.review_moment.ply;
        let played_move = imported.imported_game.game.moves[usize::from(root_ply - 1)]
            .uci
            .clone();
        let position: Chess = Fen::from_ascii(moment.core.position_snapshot.fen.as_bytes())
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        let move_uci = position
            .legal_moves()
            .into_iter()
            .map(|candidate| UciMove::from_move(&candidate, CastlingMode::Standard).to_string())
            .find(|candidate| candidate != &played_move)
            .unwrap();
        let resulting = reconstruct_derived_position(
            &imported.imported_game,
            root_ply,
            std::slice::from_ref(&move_uci),
        )
        .unwrap();
        let branch_ref = BranchRef::try_from(format!("branch:{}", "a".repeat(64))).unwrap();
        let branch = EvidenceEntry::branch(
            EvidenceMetadata::derived("durable-moment-test/v1", Vec::new()),
            BranchEvidence {
                branch_ref: branch_ref.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.core.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.core.position_snapshot.position_ref.clone(),
                move_uci,
                resulting_position_ref: resulting.position_ref,
            },
        );
        moment.evidence.push(branch);
        moment.core.evidence_refs = moment
            .evidence
            .iter()
            .map(|entry| entry.metadata().evidence_id.clone())
            .collect();
        moment.authoring = ReviewAnalysisEntryAuthoring::Prepared {
            local_decision: None,
            idempotency_keys: BTreeSet::new(),
            exploration: AlternativeMoveExplorationCheckpoint::default(),
            comment_publication: Box::new(ReviewMomentCommentPublicationCheckpoint::default()),
        };

        let payload = DurableReviewMomentPayload::from_moment(moment, &checkpoint.game).unwrap();
        let encoded = serde_json_canonicalizer::to_string(&payload).unwrap();
        assert!(encoded.contains("\"alternativeLine\""));
        assert!(!encoded.contains("\"fen\""));
        assert!(!encoded.contains("\"positionRef\""));
        assert!(!encoded.contains("\"occupied\""));

        let restored = payload
            .into_moment(
                &game_import_id,
                moment.moment_id.clone(),
                moment.purge_at,
                &checkpoint.game,
            )
            .unwrap();
        assert_eq!(restored.evidence, moment.evidence);
        assert!(matches!(
            restored.authoring,
            ReviewAnalysisEntryAuthoring::Prepared { exploration, .. }
                if exploration == AlternativeMoveExplorationCheckpoint::default()
        ));

        let mut malformed = moment.clone();
        malformed.evidence.push(EvidenceEntry::branch(
            EvidenceMetadata::derived("durable-moment-test/v1", Vec::new()),
            BranchEvidence {
                branch_ref: BranchRef::try_from(format!("branch:{}", "b".repeat(64))).unwrap(),
                parent: BranchParent::Move { branch_ref },
                source_position_ref: moment.core.position_snapshot.position_ref.clone(),
                move_uci: "e7e5".to_string(),
                resulting_position_ref: moment.core.position_snapshot.position_ref.clone(),
            },
        ));
        assert!(
            DurableReviewMomentPayload::from_moment(&malformed, &checkpoint.game).is_err(),
            "a child branch cannot claim a source other than its parent result"
        );
    }

    #[test]
    fn a_published_comment_never_reaches_the_durable_review_moment() {
        let created_at = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = crate::review_analysis_cache::test_fixtures::fixture_import(created_at);
        let core: ReviewSessionCoreContract = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/core-contract.json"
        )))
        .unwrap();
        let facts = imported
            .automatic_critical_moments()
            .into_iter()
            .next()
            .unwrap()
            .moment;
        let comment_facts =
            crate::review_session_contract::ReviewMomentCommentFacts::try_from_presented_moment(
                facts,
            )
            .unwrap();
        let grounding_ledger = crate::critical_moment_comment::grounding_ledger_for(&comment_facts);
        let comment = crate::critical_moment_comment::safely_rendered_comment(&comment_facts, None);
        let idempotency_key = crate::review_session_contract::IdempotencyKey::try_from(
            "idempotency-key:durable-comment".to_string(),
        )
        .unwrap();
        let checkpoint = ReviewAnalysisEntries::try_new(
            &imported,
            vec![CheckpointReviewSessionMoment::Prepared(Box::new(
                crate::review_analysis_cache::PreparedReviewSessionMoment {
                    core,
                    local_decision: None,
                    idempotency_keys: BTreeSet::from([idempotency_key.clone()]),
                    exploration: AlternativeMoveExplorationCheckpoint::default(),
                    comment_publication: {
                        let mut publication = ReviewMomentCommentPublicationCheckpoint::default();
                        publication.record(
                            idempotency_key,
                            crate::review_analysis_cache::ReviewMomentCommentPublicationOutcome::Published(
                                crate::review_analysis_cache::PublishedReviewMomentComment {
                                    comment: comment.clone(),
                                    authoring_provenance:
                                        crate::critical_moment_comment::CriticalMomentCommentAuthoringProvenance::hosted_authored(
                                            grounding_ledger,
                                            1,
                                        ),
                                },
                            ),
                        );
                        publication
                    },
                },
            ))],
            created_at,
        )
        .unwrap();
        let moment = &checkpoint.entries[0];

        let payload = DurableReviewMomentPayload::from_moment(moment, &checkpoint.game).unwrap();
        let encoded = serde_json_canonicalizer::to_string(&payload).unwrap();
        assert!(!encoded.contains("idempotencyKey"));
        // The comment belongs to the annotation store, which outlives this
        // checkpoint and is shared across conversations.
        assert!(!encoded.contains("publishedComment"));
        assert!(!encoded.contains(comment.text.as_str()));

        let restored = payload
            .into_moment(
                &imported.game_import_id,
                moment.moment_id.clone(),
                moment.purge_at,
                &checkpoint.game,
            )
            .unwrap();
        // The Review Moment stays prepared; only its comment is gone from the
        // checkpoint, because the annotation store owns it now.
        assert!(matches!(
            restored.authoring,
            ReviewAnalysisEntryAuthoring::Prepared {
                comment_publication,
                ..
            } if comment_publication.active_comment().is_none()
        ));
    }
}
