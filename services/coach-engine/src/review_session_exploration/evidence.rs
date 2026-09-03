use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};

use crate::{
    evaluation_recording::{
        PINNED_STOCKFISH_BINARY_DIGESTS, PINNED_STOCKFISH_DEPTH, PINNED_STOCKFISH_HASH_MIB,
        PINNED_STOCKFISH_THREADS, PINNED_STOCKFISH_VERSION,
    },
    operating_limits::MAX_REVIEW_MOMENT_CACHED_EVIDENCE_ENTRIES,
    review_session_contract::*,
};

use super::{AlternativeMoveExplorationStartError, ExploreAlternativeMoveError, PreparedMove};

const EVIDENCE_CACHE_BYTE_LIMIT: usize = 2 * 1024 * 1024;
const EXPLORATION_PRODUCER: &str = "alternative-move-exploration/v1";

pub(super) fn initialize_packet(
    mut packet: ReviewSessionEvidencePacket,
    root_position: &PositionSnapshot,
    root_engine_evidence: &EvidenceEntry,
) -> Result<(ReviewSessionEvidencePacket, EvidenceProvenance), AlternativeMoveExplorationStartError>
{
    let (recorded_position_ref, root_analysis, engine_provenance) = match root_engine_evidence {
        EvidenceEntry::EngineAnalysis {
            metadata,
            position_ref,
            analysis,
        } => (position_ref, analysis.clone(), metadata.provenance.clone()),
        _ => {
            return Err(AlternativeMoveExplorationStartError::InvalidRootEvidence(
                "entry is not Engine Analysis",
            ));
        }
    };
    if recorded_position_ref != &root_position.position_ref {
        return Err(AlternativeMoveExplorationStartError::InvalidRootEvidence(
            "position-reference",
        ));
    }
    if !is_pinned_stockfish_provenance(&engine_provenance) {
        return Err(AlternativeMoveExplorationStartError::InvalidRootEvidence(
            "provider-provenance",
        ));
    }
    if validate_engine_evidence(root_position, &root_analysis).is_err() {
        return Err(AlternativeMoveExplorationStartError::InvalidRootEvidence(
            "analysis-validation",
        ));
    }

    let position_evidence_id = packet
        .position_evidence_id(&root_position.position_ref)
        .ok_or(AlternativeMoveExplorationStartError::InvalidCore(
            "root Position evidence is missing",
        ))?;
    let root_entry = EvidenceEntry::engine_analysis(
        EvidenceMetadata::pending(vec![position_evidence_id], engine_provenance.clone()),
        root_position.position_ref.clone(),
        root_analysis,
    );
    if !packet.contains(&root_entry.metadata().evidence_id) {
        packet.entries.push(root_entry);
    }
    if !within_cache_limits(&packet) {
        return Err(AlternativeMoveExplorationStartError::EvidenceCacheLimit);
    }
    Ok((packet, engine_provenance))
}

pub(super) fn build_evidence_entries(
    prepared: &PreparedMove,
    result: &AlternativeMoveResult,
    child_analysis: EngineAnalysisEvidence,
    engine_provenance: &EvidenceProvenance,
) -> Result<Vec<EvidenceEntry>, ExploreAlternativeMoveError> {
    let mut entries = Vec::new();
    let child_position_evidence = prepared
        .base_packet
        .position_evidence_id(&prepared.resulting_position.position_ref)
        .unwrap_or_else(|| {
            let entry = EvidenceEntry::position(
                EvidenceMetadata::derived(EXPLORATION_PRODUCER, Vec::new()),
                prepared.resulting_position.clone(),
            );
            let evidence_id = entry.metadata().evidence_id.clone();
            entries.push(entry);
            evidence_id
        });
    if exact_engine_analysis(
        &prepared.base_packet,
        &prepared.resulting_position,
        engine_provenance,
    )
    .is_none()
    {
        entries.push(EvidenceEntry::engine_analysis(
            EvidenceMetadata::pending(
                vec![child_position_evidence.clone()],
                engine_provenance.clone(),
            ),
            prepared.resulting_position.position_ref.clone(),
            child_analysis,
        ));
    }
    let source_position_evidence = prepared
        .base_packet
        .position_evidence_id(&prepared.source_position.position_ref)
        .ok_or(ExploreAlternativeMoveError::Rejected {
            reason: CommandRejectionReason::MissingEvidence,
            recovery: RejectionRecovery::None,
        })?;
    entries.push(EvidenceEntry::branch(
        EvidenceMetadata::derived(
            EXPLORATION_PRODUCER,
            vec![source_position_evidence, child_position_evidence],
        ),
        BranchEvidence {
            branch_ref: result.branch_ref.clone(),
            parent: result.parent.clone(),
            source_position_ref: result.source_position_ref.clone(),
            move_uci: result.move_uci.clone(),
            resulting_position_ref: result.resulting_position.position_ref.clone(),
        },
    ));

    Ok(entries)
}

pub(super) fn validate_engine_evidence(
    position: &PositionSnapshot,
    analysis: &EngineAnalysisEvidence,
) -> Result<(), ()> {
    if evaluation_perspective(&analysis.evaluation) != position.side_to_move {
        return Err(());
    }
    if !analysis
        .evaluation
        .has_valid_mate_zero_context(&position.status)
    {
        return Err(());
    }
    if !matches!(position.status, PositionStatus::Ongoing { .. }) {
        let valid = analysis.best_move_uci == "0000"
            && analysis.principal_variation.is_empty()
            && matches!(
                (&position.status, &analysis.evaluation),
                (
                    PositionStatus::Checkmate { .. },
                    EngineEvaluation::Mate {
                        outcome: MateOutcome::Loss,
                        distance_plies: 0,
                        ..
                    },
                ) | (
                    PositionStatus::Draw { .. },
                    EngineEvaluation::Centipawns { value: 0, .. }
                )
            );
        return valid.then_some(()).ok_or(());
    }
    if analysis.principal_variation.first() != Some(&analysis.best_move_uci) {
        return Err(());
    }
    let mut chess = parse_position(position).map_err(|_| ())?;
    for uci in &analysis.principal_variation {
        let chess_move = UciMove::from_ascii(uci.as_bytes())
            .map_err(|_| ())?
            .to_move(&chess)
            .map_err(|_| ())?;
        chess.play_unchecked(&chess_move);
    }
    Ok(())
}

pub(super) fn exact_engine_analysis(
    packet: &ReviewSessionEvidencePacket,
    position: &PositionSnapshot,
    provenance: &EvidenceProvenance,
) -> Option<EngineAnalysisEvidence> {
    packet
        .engine_analysis(&position.position_ref)
        .filter(|(metadata, analysis)| {
            &metadata.provenance == provenance
                && validate_engine_evidence(position, analysis).is_ok()
        })
        .map(|(_, analysis)| analysis.clone())
}

pub(super) fn within_cache_limits(packet: &ReviewSessionEvidencePacket) -> bool {
    let cached = packet
        .entries
        .iter()
        .filter(|entry| entry.kind().is_cached())
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&cached).expect("normalized evidence is serializable");
    cached.len() <= MAX_REVIEW_MOMENT_CACHED_EVIDENCE_ENTRIES
        && bytes.len() <= EVIDENCE_CACHE_BYTE_LIMIT
}

fn is_pinned_stockfish_provenance(provenance: &EvidenceProvenance) -> bool {
    matches!(
        provenance,
        EvidenceProvenance::Stockfish {
            version,
            binary_digest,
            depth: PINNED_STOCKFISH_DEPTH,
            threads: PINNED_STOCKFISH_THREADS,
            hash_mib: PINNED_STOCKFISH_HASH_MIB,
        } if version == PINNED_STOCKFISH_VERSION
            && PINNED_STOCKFISH_BINARY_DIGESTS.contains(&binary_digest.as_str())
    )
}

fn parse_position(position: &PositionSnapshot) -> anyhow::Result<Chess> {
    Ok(Fen::from_ascii(position.fen.as_bytes())?.into_position(CastlingMode::Standard)?)
}

fn evaluation_perspective(evaluation: &EngineEvaluation) -> Color {
    match evaluation {
        EngineEvaluation::Centipawns { perspective, .. }
        | EngineEvaluation::Mate { perspective, .. } => *perspective,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINUX_X86_64_STOCKFISH_18_DIGEST: &str =
        "sha256:7a44d64fd877ee888a5160349827563444e1935ca6c1095d0f8e0859d57101c7";
    const LINUX_X86_64_AVX2_STOCKFISH_18_DIGEST: &str =
        "sha256:6b087694916228c905a5e14db74cca8c7e5643602226af1fa5d42353c455b9f9";

    #[test]
    fn accepts_the_verified_linux_stockfish_18_binary() {
        assert!(is_pinned_stockfish_provenance(&stockfish_provenance(
            LINUX_X86_64_STOCKFISH_18_DIGEST,
        )));
    }

    #[test]
    fn accepts_the_verified_linux_avx2_stockfish_18_binary() {
        assert!(is_pinned_stockfish_provenance(&stockfish_provenance(
            LINUX_X86_64_AVX2_STOCKFISH_18_DIGEST,
        )));
    }

    #[test]
    fn rejects_an_unknown_stockfish_binary() {
        assert!(!is_pinned_stockfish_provenance(&stockfish_provenance(
            &format!("sha256:{}", "0".repeat(64)),
        )));
    }

    fn stockfish_provenance(binary_digest: &str) -> EvidenceProvenance {
        EvidenceProvenance::Stockfish {
            version: PINNED_STOCKFISH_VERSION.to_string(),
            binary_digest: ArtifactDigest::try_from(binary_digest.to_string()).unwrap(),
            depth: PINNED_STOCKFISH_DEPTH,
            threads: PINNED_STOCKFISH_THREADS,
            hash_mib: PINNED_STOCKFISH_HASH_MIB,
        }
    }
}
