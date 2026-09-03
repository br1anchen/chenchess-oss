use std::collections::BTreeSet;

use serde::{de, Deserialize, Deserializer};

use super::{
    CoachTurnContext, CoachTurnTarget, CriticalMomentId, EvidenceEntry, EvidenceId, ImportedGame,
    PositionSnapshot, RequestId, ReviewMomentOccurrence, ReviewMomentSelection,
    ReviewSessionCoreContract, ReviewSessionEvidencePacket,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewSessionCoreContractWire {
    request_id: RequestId,
    imported_game: ImportedGame,
    position_snapshot: PositionSnapshot,
    review_moment: ReviewMomentOccurrence,
    coach_turn_context: CoachTurnContext,
    evidence_packet: ReviewSessionEvidencePacket,
}

impl<'de> Deserialize<'de> for ReviewSessionCoreContract {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ReviewSessionCoreContractWire::deserialize(deserializer)?;
        let contract = Self {
            request_id: wire.request_id,
            imported_game: wire.imported_game,
            position_snapshot: wire.position_snapshot,
            review_moment: wire.review_moment,
            coach_turn_context: wire.coach_turn_context,
            evidence_packet: wire.evidence_packet,
        };
        contract
            .validate_integrity()
            .map_err(de::Error::custom)
            .map(|()| contract)
    }
}

impl ReviewSessionCoreContract {
    pub fn validate_integrity(&self) -> Result<(), &'static str> {
        if self.coach_turn_context.selected_position_ref != self.position_snapshot.position_ref {
            return Err("Coach Turn selected Position does not match the core Position Snapshot");
        }
        if !self.has_grounded_review_moment() {
            return Err("Review Moment occurrence is absent from the imported game");
        }
        if !self
            .coach_turn_context
            .has_grounded_reviewed_move(&self.imported_game)
        {
            return Err("Coach Turn reviewed-move anchor is absent from the imported game");
        }
        if !self
            .coach_turn_context
            .has_grounded_target(&self.imported_game, &self.evidence_packet.entries)
        {
            return Err("Coach Turn target is absent from the imported game or Evidence Packet");
        }
        if !self.evidence_packet.entries.iter().any(|entry| {
            matches!(
                entry,
                EvidenceEntry::Position { position, .. } if position == &self.position_snapshot
            )
        }) {
            return Err("core Position Snapshot is absent from the Evidence Packet");
        }

        let evidence_ids = self
            .evidence_packet
            .entries
            .iter()
            .map(|entry| &entry.metadata().evidence_id)
            .collect::<BTreeSet<_>>();
        if !self
            .coach_turn_context
            .has_grounded_evidence_refs(&evidence_ids)
        {
            return Err("Coach Turn references evidence absent from the core Evidence Packet");
        }
        Ok(())
    }

    fn has_grounded_review_moment(&self) -> bool {
        let context = &self.coach_turn_context;
        let occurrence = &self.review_moment;
        let game = &self.imported_game.game;
        if occurrence.moment_id
            != CriticalMomentId::for_imported_game(&game.game_ref, occurrence.ply)
            || occurrence.moment_id != context.reviewed_move.critical_moment_id
            || occurrence.ply != context.reviewed_move.ply
            || occurrence.preceding_move
                != occurrence
                    .ply
                    .checked_sub(2)
                    .and_then(|index| game.moves.get(usize::from(index)))
                    .cloned()
            || occurrence.game_ref != game.game_ref
        {
            return false;
        }
        match &occurrence.selection {
            ReviewMomentSelection::PipelineCriticalMoment { critical_moment_id } => {
                critical_moment_id == &occurrence.moment_id
            }
            ReviewMomentSelection::PlayerSelectedMoment { ply } => ply == &occurrence.ply,
        }
    }
}

impl CoachTurnContext {
    pub fn objective_context(&self, packet: &ReviewSessionEvidencePacket) -> CoachTurnContext {
        CoachTurnContext {
            coach_turn_id: self.coach_turn_id.clone(),
            reviewed_move: self.reviewed_move.clone(),
            selected_position_ref: self.selected_position_ref.clone(),
            target: self.target.clone(),
            required_evidence_refs: self
                .required_evidence_refs
                .iter()
                .filter(|evidence_id| packet.contains(evidence_id))
                .cloned()
                .collect(),
        }
    }

    fn has_grounded_reviewed_move(&self, imported_game: &ImportedGame) -> bool {
        imported_game.game.moves.iter().any(|game_move| {
            game_move.ply == self.reviewed_move.ply
                && game_move.side == self.reviewed_move.side
                && game_move.before_position_ref == self.reviewed_move.position_ref
                && game_move.uci == self.reviewed_move.played_move_uci
        })
    }

    fn has_grounded_target(
        &self,
        imported_game: &ImportedGame,
        evidence_entries: &[EvidenceEntry],
    ) -> bool {
        match &self.target {
            CoachTurnTarget::ImportedGameMove {
                critical_moment_id,
                ply,
                uci,
            } => {
                critical_moment_id == &self.reviewed_move.critical_moment_id
                    && ply == &self.reviewed_move.ply
                    && uci == &self.reviewed_move.played_move_uci
                    && imported_game.game.moves.iter().any(|game_move| {
                        game_move.ply == *ply
                            && game_move.uci == *uci
                            && game_move.before_position_ref == self.selected_position_ref
                    })
            }
            CoachTurnTarget::AlternativeMove { branch_ref, uci } => {
                evidence_entries.iter().any(|entry| match entry {
                    EvidenceEntry::Branch { branch, .. }
                        if branch.branch_ref == *branch_ref
                            && branch.move_uci == *uci
                            && branch.source_position_ref == self.selected_position_ref =>
                    {
                        evidence_entries.iter().any(|candidate| {
                            matches!(
                                candidate,
                                EvidenceEntry::Position { position, .. }
                                    if position.position_ref == branch.resulting_position_ref
                            )
                        })
                    }
                    _ => false,
                })
            }
        }
    }

    fn has_grounded_evidence_refs(&self, available: &BTreeSet<&EvidenceId>) -> bool {
        self.required_evidence_refs
            .iter()
            .all(|evidence_id| available.contains(evidence_id))
    }
}
