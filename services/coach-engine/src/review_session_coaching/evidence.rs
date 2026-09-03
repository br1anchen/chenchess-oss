use std::collections::BTreeSet;

use crate::{
    evaluation_recording::PINNED_MAIA_CANDIDATE_LIMIT, human_move_model::HumanMovePrediction,
    review_session_contract::*,
};

use super::{
    prose::{CoachTurnDimension, CoachTurnProseContext, CoachTurnRejection},
    CoachTurnEvidenceRefs, PreparedCoachTurnTarget,
};

pub fn prepare_evidence(
    target: &PreparedCoachTurnTarget,
    source_prediction: HumanMovePrediction,
    resulting_prediction: HumanMovePrediction,
) -> Result<
    (
        Vec<EvidenceEntry>,
        ReviewSessionEvidencePacket,
        CoachTurnEvidenceRefs,
    ),
    ProviderUnavailableReason,
> {
    let elo = target.elo;
    let source_position_id = target
        .evidence_packet
        .position_evidence_id(&target.source_position.position_ref)
        .ok_or(ProviderUnavailableReason::LanguageLayer)?;
    let result_position_id = target
        .evidence_packet
        .position_evidence_id(&target.target.resulting_position.position_ref)
        .ok_or(ProviderUnavailableReason::LanguageLayer)?;
    let source_human = human_entry(
        &target.source_position.position_ref,
        source_position_id,
        source_prediction,
        elo,
    )?;
    let resulting_human = human_entry(
        &target.target.resulting_position.position_ref,
        result_position_id,
        resulting_prediction,
        elo,
    )?;
    let source_human_id = source_human.metadata().evidence_id.clone();
    let resulting_human_id = resulting_human.metadata().evidence_id.clone();
    let entries = vec![source_human, resulting_human];
    let mut packet = target.evidence_packet.clone();
    packet.entries.extend(entries.iter().cloned());
    let refs = CoachTurnEvidenceRefs {
        target_branch: branch_evidence_id(&packet, &target.target.branch_ref)
            .ok_or(ProviderUnavailableReason::LanguageLayer)?,
        source_engine: packet
            .engine_analysis(&target.source_position.position_ref)
            .map(|(metadata, _)| metadata.evidence_id.clone())
            .ok_or(ProviderUnavailableReason::LanguageLayer)?,
        resulting_engine: packet
            .engine_analysis(&target.target.resulting_position.position_ref)
            .map(|(metadata, _)| metadata.evidence_id.clone())
            .ok_or(ProviderUnavailableReason::LanguageLayer)?,
        source_human: source_human_id,
        resulting_human: resulting_human_id,
    };
    Ok((entries, packet, refs))
}

fn human_entry(
    position_ref: &PositionRef,
    position_evidence_id: EvidenceId,
    prediction: HumanMovePrediction,
    elo: EloRating,
) -> Result<EvidenceEntry, ProviderUnavailableReason> {
    let candidates = prediction
        .candidates
        .into_iter()
        .map(|candidate| {
            Ok(HumanMoveCandidateEvidence {
                uci: candidate.uci,
                probability: Probability::try_from(candidate.probability)
                    .map_err(|_| ProviderUnavailableReason::MaiaTransport)?,
                rank: u8::try_from(candidate.rank)
                    .map_err(|_| ProviderUnavailableReason::MaiaTransport)?,
            })
        })
        .collect::<Result<Vec<_>, ProviderUnavailableReason>>()?;
    if candidates.is_empty() || candidates.len() > usize::from(PINNED_MAIA_CANDIDATE_LIMIT) {
        return Err(ProviderUnavailableReason::MaiaTransport);
    }
    let win_probability = match prediction.win_probability {
        Some(value) => ProbabilityState::Available {
            probability: Probability::try_from(value)
                .map_err(|_| ProviderUnavailableReason::MaiaTransport)?,
        },
        None => ProbabilityState::Unavailable,
    };
    Ok(EvidenceEntry::human_move_model(
        EvidenceMetadata::pending(
            vec![position_evidence_id],
            crate::provider_provenance::pinned_maia(elo),
        ),
        position_ref.clone(),
        HumanMoveModelEvidence {
            player_elo: elo,
            opponent_elo: elo,
            candidates,
            win_probability,
        },
    ))
}

/// Grounds one authored assessment and returns the assessment the Player sees.
///
/// The structural half is unchanged: `coachTurnId`, `alternativeMoveId` and
/// each dimension's evidence refs are set-equal to computed values, so the
/// runtime fills them and the model can only match or fail. What is new is the
/// prose half — markers, chess literals, vocabulary and the no-URL rule — and
/// because markers are substituted, grounding *produces* the assessment rather
/// than merely approving it.
pub(super) fn ground_assessment(
    coach_turn_id: &CoachTurnId,
    target: &PreparedCoachTurnTarget,
    assessment: &AlternativeMoveAssessment,
    packet: &ReviewSessionEvidencePacket,
    evidence: &CoachTurnEvidenceRefs,
) -> Result<AlternativeMoveAssessment, ProviderUnavailableReason> {
    diagnose_assessment(coach_turn_id, target, assessment, packet, evidence)
        .map_err(CoachTurnRejection::into_wire)
}

/// The same gate, resolving its failures at full width. See
/// [`ground_assessment`] for the narrowed form every Player-facing seam uses.
pub fn diagnose_assessment(
    coach_turn_id: &CoachTurnId,
    target: &PreparedCoachTurnTarget,
    assessment: &AlternativeMoveAssessment,
    packet: &ReviewSessionEvidencePacket,
    evidence: &CoachTurnEvidenceRefs,
) -> Result<AlternativeMoveAssessment, CoachTurnRejection> {
    if &assessment.coach_turn_id != coach_turn_id
        || assessment.alternative_move_id != target.target.alternative_move_id
    {
        return Err(CoachTurnRejection::TargetMismatch);
    }
    let prose = CoachTurnProseContext::prepare(target, packet)?;
    Ok(AlternativeMoveAssessment {
        coach_turn_id: assessment.coach_turn_id.clone(),
        alternative_move_id: assessment.alternative_move_id.clone(),
        objective_quality: ground_dimension(
            &assessment.objective_quality,
            CoachTurnDimension::ObjectiveQuality,
            &prose,
            packet,
            [
                &evidence.target_branch,
                &evidence.source_engine,
                &evidence.resulting_engine,
            ],
        )?,
        findability: ground_dimension(
            &assessment.findability,
            CoachTurnDimension::Findability,
            &prose,
            packet,
            [&evidence.target_branch, &evidence.source_human],
        )?,
        resilience: ground_dimension(
            &assessment.resilience,
            CoachTurnDimension::Resilience,
            &prose,
            packet,
            [
                &evidence.target_branch,
                &evidence.resulting_engine,
                &evidence.resulting_human,
            ],
        )?,
    })
}

pub(super) fn ground_publication(
    coach_turn_id: &CoachTurnId,
    target: &PreparedCoachTurnTarget,
    assessment: &AlternativeMoveAssessment,
    packet: &ReviewSessionEvidencePacket,
) -> Result<AlternativeMoveAssessment, ProviderUnavailableReason> {
    diagnose_publication(coach_turn_id, target, assessment, packet)
        .map_err(CoachTurnRejection::into_wire)
}

/// The evidence refs a turn on this target is required to cite, computed rather
/// than authored. The runtime fills them; the model only writes prose.
pub fn required_evidence_refs(
    target: &PreparedCoachTurnTarget,
    packet: &ReviewSessionEvidencePacket,
) -> Result<CoachTurnEvidenceRefs, CoachTurnRejection> {
    Ok(CoachTurnEvidenceRefs {
        target_branch: branch_evidence_id(packet, &target.target.branch_ref)
            .ok_or(CoachTurnRejection::EvidenceUnreadable)?,
        source_engine: packet
            .engine_analysis(&target.source_position.position_ref)
            .map(|(metadata, _)| metadata.evidence_id.clone())
            .ok_or(CoachTurnRejection::EvidenceUnreadable)?,
        resulting_engine: packet
            .engine_analysis(&target.target.resulting_position.position_ref)
            .map(|(metadata, _)| metadata.evidence_id.clone())
            .ok_or(CoachTurnRejection::EvidenceUnreadable)?,
        source_human: packet
            .human_move_model(&target.source_position.position_ref)
            .map(|(metadata, _)| metadata.evidence_id.clone())
            .ok_or(CoachTurnRejection::EvidenceUnreadable)?,
        resulting_human: packet
            .human_move_model(&target.target.resulting_position.position_ref)
            .map(|(metadata, _)| metadata.evidence_id.clone())
            .ok_or(CoachTurnRejection::EvidenceUnreadable)?,
    })
}

pub fn diagnose_publication(
    coach_turn_id: &CoachTurnId,
    target: &PreparedCoachTurnTarget,
    assessment: &AlternativeMoveAssessment,
    packet: &ReviewSessionEvidencePacket,
) -> Result<AlternativeMoveAssessment, CoachTurnRejection> {
    let evidence = required_evidence_refs(target, packet)?;
    diagnose_assessment(coach_turn_id, target, assessment, packet, &evidence)
}

fn ground_dimension<'a>(
    dimension: &AssessmentDimension,
    kind: CoachTurnDimension,
    prose: &CoachTurnProseContext,
    packet: &ReviewSessionEvidencePacket,
    required: impl IntoIterator<Item = &'a EvidenceId>,
) -> Result<AssessmentDimension, CoachTurnRejection> {
    let refs = &dimension.evidence_refs;
    let cited = refs.iter().collect::<BTreeSet<_>>();
    let required = required.into_iter().collect::<BTreeSet<_>>();
    if cited.len() != refs.len()
        || refs.iter().any(|evidence_id| !packet.contains(evidence_id))
        || cited != required
    {
        return Err(CoachTurnRejection::EvidenceRefsMismatch { dimension: kind });
    }
    Ok(AssessmentDimension {
        explanation: prose.ground(kind, &dimension.explanation)?.explanation,
        evidence_refs: refs.clone(),
    })
}

pub(super) fn branch_evidence_id(
    packet: &ReviewSessionEvidencePacket,
    branch_ref: &BranchRef,
) -> Option<EvidenceId> {
    packet
        .branch(branch_ref)
        .map(|(metadata, _)| metadata.evidence_id.clone())
}
