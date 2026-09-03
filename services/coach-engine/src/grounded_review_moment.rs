//! Resolves one Review Moment's proof into something a reader can act on.
//!
//! A stored Decision Explanation is a content-addressed graph: paths point at
//! candidates, candidates at line steps, line steps at position snapshots, and
//! concepts at knowledge-graph node hashes. All of it is durable so a proof can
//! be reproduced, and none of it is legible outside this process. Delivering
//! the graph raw would ship tens of kilobytes of hashes nothing can dereference,
//! so every reference is followed here once and the resolved names, moves, and
//! positions are what leave.
//!
//! A proof is delivered whole or not at all. Half of one is worse than none:
//! it still carries the capability tag that licenses a claim, while leaving a
//! reader nothing to make the claim out of. So an unresolvable reference
//! withholds the entire aggregate, leaving the reference that addresses the
//! audit copy. A Review Moment with no explanation is ordinary anyway — most
//! moments never had one, which is exactly why withholding is never silent:
//! see [`grounded_or_reported`].

use shakmaty::{fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, Position};

use crate::{
    decision_explanation::resolve_knowledge_concept,
    review_session_contract::{
        AtomicChessFact, AtomicFactRef, Color, DecisionCandidate, DecisionCandidateRef,
        DecisionExplanation, DecisionLineStep, DecisionPositionSnapshot,
        DecisionPositionSnapshotRef, ExplanationPath, GameImportId, GameReviewCriticalMoment,
        GroundedCandidate, GroundedExplanation, GroundedExplanationPath, GroundedMaterialEvent,
        GroundedMaterialTransaction, GroundedReviewMomentDetail, GroundedStep, ImportedGame,
        LineStepRef, MaterialValuePolicyVersion, MoveSequenceOrigin, PositionSnapshot,
    },
};

/// The plies a coach recites before the line stops being a sentence.
const RETAINED_VARIATION_PLIES: usize = 6;

/// Grounds one Review Moment for delivery at its own address.
pub fn ground_review_moment(
    game_import_id: &GameImportId,
    imported_game: &ImportedGame,
    moment: &GameReviewCriticalMoment,
    position: &PositionSnapshot,
    explanation: Option<&DecisionExplanation>,
) -> GroundedReviewMomentDetail {
    GroundedReviewMomentDetail {
        game_import_id: game_import_id.clone(),
        review_moment_id: moment.critical_moment_id.clone(),
        ply: moment.ply,
        continuation: continuation_origin(imported_game, moment, position),
        objective_lines: moment.objective.lines.clone(),
        explanation_ref: moment.decision_explanation_ref.clone(),
        decision_learning_outcome: moment.decision_learning_outcome,
        explanation: explanation.and_then(|stored| grounded_or_reported(moment, stored)),
        comment: None,
    }
}

/// Grounds a stored proof, and says so out loud when it cannot.
///
/// Withholding is the right answer for the reader — an absent proof licenses no
/// claim, which is the safe direction — but it makes a damaged proof look
/// exactly like the ordinary case of a moment that was never proven. Most
/// moments were never proven, so without this the corruption would hide in the
/// noise forever. The Player keeps a working Review Moment; an operator gets
/// the one signal that says something needs reproducing from the audit copy.
fn grounded_or_reported(
    moment: &GameReviewCriticalMoment,
    stored: &DecisionExplanation,
) -> Option<GroundedExplanation> {
    let grounded = ground_explanation(stored);
    match &grounded {
        None => tracing::error!(
            critical_moment_id = moment.critical_moment_id.as_str(),
            decision_explanation_ref = stored.decision_explanation_ref.as_str(),
            "a stored Decision Explanation could not be grounded and was withheld"
        ),
        Some(grounded) => report_heavy_supporting_facts(moment, stored, grounded),
    }
    grounded
}

/// The fact count past which a proof is worth looking at, not cutting down.
///
/// Measured: a Review Moment's proof carries 16 supporting facts at the median,
/// and the heaviest in the corpus carries 105. This sits above the ordinary
/// range so the log names the outliers rather than every proof.
const HEAVY_SUPPORTING_FACT_COUNT: usize = 32;

/// Names a proof whose supporting facts dominate its projection.
///
/// These are real evidence for a concept that rests on a long line, so they are
/// never truncated — a partial proof is what this module exists to refuse. But
/// they are ~79% of a grounded payload and the heaviest run past 30 KB, so the
/// ones worth optimising later have to be findable now. Counting is cheap and
/// happens on every read; the payload is measured only once a proof is already
/// unusual, so the common path never pays for the diagnosis.
fn report_heavy_supporting_facts(
    moment: &GameReviewCriticalMoment,
    stored: &DecisionExplanation,
    grounded: &GroundedExplanation,
) {
    let Some((facts, bytes)) = heavy_supporting_facts(grounded) else {
        return;
    };
    tracing::warn!(
        critical_moment_id = moment.critical_moment_id.as_str(),
        decision_explanation_ref = stored.decision_explanation_ref.as_str(),
        supporting_fact_count = facts,
        supporting_fact_bytes = bytes,
        "a grounded proof's supporting facts dominate its projection"
    );
}

/// Weighs a grounded proof's supporting facts, measuring the payload only once
/// the count already says the proof is unusual.
fn heavy_supporting_facts(grounded: &GroundedExplanation) -> Option<(usize, usize)> {
    let facts = grounded
        .paths
        .iter()
        .map(|path| path.supporting_facts.len())
        .sum::<usize>();
    if facts <= HEAVY_SUPPORTING_FACT_COUNT {
        return None;
    }
    let bytes = grounded
        .paths
        .iter()
        .filter_map(|path| serde_json::to_vec(&path.supporting_facts).ok())
        .map(|encoded| encoded.len())
        .sum::<usize>();
    Some((facts, bytes))
}

fn continuation_origin(
    imported_game: &ImportedGame,
    moment: &GameReviewCriticalMoment,
    position: &PositionSnapshot,
) -> MoveSequenceOrigin {
    let reviewed_move = imported_game
        .game
        .moves
        .iter()
        .find(|game_move| game_move.ply == moment.ply);
    MoveSequenceOrigin {
        fen: position.fen.clone(),
        side_to_move: position.side_to_move,
        review_side: imported_game.review_side,
        reviewed_move_uci: reviewed_move.map(|game_move| game_move.uci.clone()),
    }
}

fn ground_explanation(explanation: &DecisionExplanation) -> Option<GroundedExplanation> {
    let grounded = GroundedExplanation {
        explanation_ref: explanation.decision_explanation_ref.clone(),
        capability: explanation.capability,
        paths: explanation
            .selected_paths
            .iter()
            .map(|path| ground_path(explanation, path))
            .collect::<Option<Vec<_>>>()?,
        candidates: explanation
            .candidates
            .iter()
            .map(|candidate| ground_candidate(explanation, candidate))
            .collect::<Option<Vec<_>>>()?,
    };
    Some(grounded)
}

fn ground_path(
    explanation: &DecisionExplanation,
    path: &ExplanationPath,
) -> Option<GroundedExplanationPath> {
    let proof = &path.concept_validation_proof;
    let candidate = candidate_by_ref(explanation, &path.candidate_ref)?;
    Some(GroundedExplanationPath {
        path_ref: path.path_ref.clone(),
        attribution: path.attribution,
        concept: resolve_knowledge_concept(&path.knowledge_activation.concept_node_ref)?,
        candidate: ground_candidate(explanation, candidate)?,
        position_goal: path
            .candidate_generation_proof
            .as_ref()
            .map(|proof| proof.position_goal.clone()),
        // An unresolvable material event withholds the whole proof, the same as
        // any other reference this module cannot follow: a transaction cut off
        // before its recovery reads as a loss the line never took.
        material_transaction: ground_material_transaction(explanation, candidate).ok()?,
        causal_step: ground_step(explanation, &proof.causal_step_ref)?,
        payoff_step: ground_step(explanation, &proof.payoff_step_ref)?,
        supporting_facts: proof
            .supporting_fact_refs
            .iter()
            .map(|reference| Some(fact_by_ref(explanation, reference)?.data.clone()))
            .collect::<Option<Vec<_>>>()?,
    })
}

/// A persisted line step this module cannot turn into a material event.
///
/// The steps come from a durable document rather than from a checked in-process
/// value, so an unresolvable snapshot, an illegal move, a valueless role, or an
/// impossible ply count are all reachable from a damaged proof.
#[derive(Debug)]
struct UngroundableMaterialEvent;

/// Projects the complete material story of a candidate's line.
///
/// `Ok(None)` says the line moves no material at all, which is ordinary. An
/// `Err` says a material event exists but could not be grounded, which is not.
fn ground_material_transaction(
    explanation: &DecisionExplanation,
    candidate: &DecisionCandidate,
) -> Result<Option<GroundedMaterialTransaction>, UngroundableMaterialEvent> {
    let Some(perspective) = candidate.line_steps.first().map(|step| step.mover) else {
        return Ok(None);
    };
    let events = candidate
        .line_steps
        .iter()
        .enumerate()
        .filter(|(_, step)| step.captured.is_some() || step.promotion.is_some())
        .map(|(index, step)| ground_material_event(explanation, perspective, index, step))
        .collect::<Result<Vec<_>, _>>()?;
    if events.is_empty() {
        return Ok(None);
    }
    let net_conventional_value_delta = events.iter().try_fold(0_i16, |net, event| {
        net.checked_add(event.conventional_value_delta())
            .ok_or(UngroundableMaterialEvent)
    })?;
    Ok(Some(GroundedMaterialTransaction {
        perspective,
        events,
        net_conventional_value_delta,
        value_policy_version: MaterialValuePolicyVersion::V1,
    }))
}

fn ground_material_event(
    explanation: &DecisionExplanation,
    perspective: Color,
    index: usize,
    step: &DecisionLineStep,
) -> Result<GroundedMaterialEvent, UngroundableMaterialEvent> {
    let before =
        snapshot_by_ref(explanation, &step.before_snapshot_ref).ok_or(UngroundableMaterialEvent)?;
    let line_ply = u16::try_from(index + 1).map_err(|_| UngroundableMaterialEvent)?;
    let san = san_at(&before.fen, &step.uci).ok_or(UngroundableMaterialEvent)?;
    let captured_value = step
        .captured
        .as_ref()
        .map(|captured| {
            captured
                .role
                .conventional_material_value()
                .ok_or(UngroundableMaterialEvent)
        })
        .transpose()?
        .unwrap_or(0);
    // A promoting pawn leaves the board, so the gain is the new role less the
    // pawn it replaces.
    let promotion_value = step
        .promotion
        .map(|role| {
            role.conventional_material_value()
                .and_then(|value| value.checked_sub(1))
                .ok_or(UngroundableMaterialEvent)
        })
        .transpose()?
        .unwrap_or(0);
    let unsigned_delta = i16::from(
        captured_value
            .checked_add(promotion_value)
            .ok_or(UngroundableMaterialEvent)?,
    );
    let conventional_value_delta = if step.mover == perspective {
        unsigned_delta
    } else {
        -unsigned_delta
    };
    match (&step.captured, step.promotion) {
        (Some(captured), Some(promotion_role)) => Ok(GroundedMaterialEvent::CaptureAndPromotion {
            line_ply,
            uci: step.uci.clone(),
            san,
            mover: step.mover,
            captured: captured.clone(),
            pawn_from_square: step.from_square.clone(),
            promotion_role,
            conventional_value_delta,
        }),
        (Some(captured), None) => Ok(GroundedMaterialEvent::Capture {
            line_ply,
            uci: step.uci.clone(),
            san,
            mover: step.mover,
            captured: captured.clone(),
            conventional_value_delta,
        }),
        (None, Some(promotion_role)) => Ok(GroundedMaterialEvent::Promotion {
            line_ply,
            uci: step.uci.clone(),
            san,
            mover: step.mover,
            pawn_from_square: step.from_square.clone(),
            promotion_role,
            conventional_value_delta,
        }),
        // Unreachable through the filter that selects these steps, and a step
        // that reached here anyway describes no material event at all.
        (None, None) => Err(UngroundableMaterialEvent),
    }
}

fn ground_candidate(
    explanation: &DecisionExplanation,
    candidate: &DecisionCandidate,
) -> Option<GroundedCandidate> {
    Some(GroundedCandidate {
        root_move_uci: candidate.root_move_uci.clone(),
        san: san_at(&explanation.position_snapshot.fen, &candidate.root_move_uci)?,
        origins: candidate.origins.clone(),
        evaluation: candidate.assessment.score.clone(),
        outcomes: candidate
            .outcomes
            .iter()
            .map(|outcome| outcome.data.clone())
            .collect(),
        retained_variation: san_line(
            &explanation.position_snapshot.fen,
            candidate
                .retained_variation
                .iter()
                .take(RETAINED_VARIATION_PLIES),
        )?,
    })
}

/// A step is named from the position before it and placed by the position after.
fn ground_step(explanation: &DecisionExplanation, step_ref: &LineStepRef) -> Option<GroundedStep> {
    let step = step_by_ref(explanation, step_ref)?;
    let after = snapshot_by_ref(explanation, &step.after_snapshot_ref)?;
    let before = snapshot_by_ref(explanation, &step.before_snapshot_ref)?;
    Some(GroundedStep {
        uci: step.uci.clone(),
        san: san_at(&before.fen, &step.uci)?,
        fen: after.fen.clone(),
    })
}

fn candidate_by_ref<'a>(
    explanation: &'a DecisionExplanation,
    candidate_ref: &DecisionCandidateRef,
) -> Option<&'a DecisionCandidate> {
    explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == *candidate_ref)
}

fn step_by_ref<'a>(
    explanation: &'a DecisionExplanation,
    step_ref: &LineStepRef,
) -> Option<&'a DecisionLineStep> {
    explanation
        .candidates
        .iter()
        .flat_map(|candidate| candidate.line_steps.iter())
        .find(|step| step.step_ref == *step_ref)
}

fn snapshot_by_ref<'a>(
    explanation: &'a DecisionExplanation,
    snapshot_ref: &DecisionPositionSnapshotRef,
) -> Option<&'a DecisionPositionSnapshot> {
    explanation
        .snapshots
        .iter()
        .find(|snapshot| snapshot.snapshot_ref == *snapshot_ref)
}

fn fact_by_ref<'a>(
    explanation: &'a DecisionExplanation,
    fact_ref: &AtomicFactRef,
) -> Option<&'a AtomicChessFact> {
    explanation
        .facts
        .iter()
        .find(|fact| fact.fact_ref == *fact_ref)
}

fn san_at(fen: &str, uci: &str) -> Option<String> {
    let position = position_at(fen)?;
    let played = UciMove::from_ascii(uci.as_bytes())
        .ok()?
        .to_move(&position)
        .ok()?;
    Some(SanPlus::from_move(position, &played).to_string())
}

/// Names a whole line the way a coach would recite it.
///
/// A variation left in UCI is the same problem as a content hash: correct,
/// durable, and not the language anyone speaks the move in.
fn san_line<'a>(fen: &str, ucis: impl Iterator<Item = &'a String>) -> Option<Vec<String>> {
    let mut position = position_at(fen)?;
    ucis.map(|uci| {
        let played = UciMove::from_ascii(uci.as_bytes())
            .ok()?
            .to_move(&position)
            .ok()?;
        let san = SanPlus::from_move(position.clone(), &played).to_string();
        position.play_unchecked(&played);
        Some(san)
    })
    .collect()
}

fn position_at(fen: &str) -> Option<Chess> {
    fen.parse::<Fen>()
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        decision_explanation::{
            explain_decision, DecisionExplanationBuild, DecisionExplanationInput,
        },
        review_session_contract::{
            build_position_snapshot, ArtifactDigest, CandidateEvidence, Color, CriticalMomentId,
            CurriculumLearningConcept, DecisionEngineProvenance, EngineCandidateEvidence,
            EngineEvaluation, GameRef, GameReviewMomentClassification, GameReviewMomentProvenance,
            ImprovementCorrection, ImprovementOutcome, PlayerMoveEvidence,
        },
    };

    /// White to play: Nxc7+ captures immediately, then forks king and rook.
    const FORK_FEN: &str = "r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1";

    /// The proof is a graph of content hashes; a reader gets names and moves.
    #[test]
    fn grounding_resolves_the_ranked_path_into_a_concept_moves_and_facts() {
        let explanation = fork_explanation();
        let grounded = ground_explanation(&explanation).expect("the fork proof grounds whole");

        assert_eq!(grounded.capability, explanation.capability);
        let path = grounded
            .paths
            .first()
            .expect("the fork proof selects one path");
        assert_eq!(path.concept, CurriculumLearningConcept::Advantage);
        assert_eq!(path.candidate.root_move_uci, "b5c7");
        assert_eq!(path.candidate.san, "Nxc7+");
        assert!(matches!(
            path.position_goal,
            Some(crate::review_session_contract::PositionGoal::GainMaterial { ref targets })
                if !targets.is_empty()
        ));
        assert_eq!(path.causal_step.san, "Nxc7+");
        assert!(!path.payoff_step.fen.is_empty());
        // One evaluation source per Review Moment: the projection restates the
        // candidate's own assessment and never a second number derived
        // elsewhere, which is what stopped `objective` and `candidates` from
        // disagreeing (ADR 0041).
        let scored = explanation
            .candidates
            .iter()
            .find(|candidate| candidate.root_move_uci == "b5c7")
            .expect("the fork candidate is in the proof");
        assert_eq!(path.candidate.evaluation, scored.assessment.score);
        assert!(
            !path.supporting_facts.is_empty(),
            "a resolved path carries the facts its proof rests on"
        );
    }

    /// A variation is recited, so it arrives in the language it is spoken in
    /// and only as long as a coach would say it out loud.
    #[test]
    fn grounding_recites_a_candidate_variation_in_san_and_stops_where_a_coach_would() {
        let grounded =
            ground_explanation(&fork_explanation()).expect("the fork proof grounds whole");

        let fork = grounded
            .candidates
            .iter()
            .find(|candidate| candidate.root_move_uci == "b5c7")
            .expect("the fork candidate is compared");
        assert_eq!(fork.retained_variation, ["Nxc7+", "Kd7", "Nxa8"]);
        assert!(grounded
            .candidates
            .iter()
            .all(|candidate| candidate.retained_variation.len() <= RETAINED_VARIATION_PLIES));
    }

    /// Half a proof still carries the capability that licenses a claim, so a
    /// proof that lost a step is withheld rather than delivered with a hole.
    #[test]
    fn a_proof_missing_one_step_is_withheld_rather_than_delivered_partial() {
        let mut explanation = fork_explanation();
        let causal = explanation.selected_paths[0]
            .concept_validation_proof
            .causal_step_ref
            .clone();
        for candidate in &mut explanation.candidates {
            candidate.line_steps.retain(|step| step.step_ref != causal);
        }

        assert!(ground_explanation(&explanation).is_none());
    }

    /// Heavy proofs are named so they can be optimised later, and kept whole in
    /// the meantime — the facts are real evidence, not padding.
    #[test]
    fn a_proof_whose_facts_dominate_it_is_reported_and_still_delivered_whole() {
        let mut grounded =
            ground_explanation(&fork_explanation()).expect("the fork proof grounds whole");
        assert!(
            heavy_supporting_facts(&grounded).is_none(),
            "an ordinary proof must not be reported"
        );

        let path = &mut grounded.paths[0];
        let one = path.supporting_facts[0].clone();
        let delivered = path.supporting_facts.len();
        path.supporting_facts
            .extend(std::iter::repeat_n(one, HEAVY_SUPPORTING_FACT_COUNT));

        let (facts, bytes) =
            heavy_supporting_facts(&grounded).expect("a proof past the threshold is reported");
        assert_eq!(facts, delivered + HEAVY_SUPPORTING_FACT_COUNT);
        assert!(bytes > 0);
        // Reporting is a diagnosis, never a cut: the facts a reader receives are
        // the same ones before and after it runs.
        assert_eq!(grounded.paths[0].supporting_facts.len(), facts);
    }

    fn fork_explanation() -> DecisionExplanation {
        let build = explain_decision(DecisionExplanationInput {
            game_ref: GameRef::try_from(format!("sha256:{}", "1".repeat(64))).unwrap(),
            critical_moment_id: CriticalMomentId::try_from("review-moment:fork:1".to_string())
                .unwrap(),
            position_snapshot: build_position_snapshot(FORK_FEN, &[]).unwrap(),
            classification: GameReviewMomentClassification::ImprovementOpportunity {
                correction: ImprovementCorrection {
                    better_move_uci: "b5c7".to_string(),
                    better_move_san: "Nxc7+".to_string(),
                    outcome: ImprovementOutcome::ImprovedAnalyzed {
                        better_evaluation: evaluation(500),
                    },
                },
            },
            provenance: GameReviewMomentProvenance::Automatic,
            player_move_uci: "e1e2".to_string(),
            candidate_evidence: CandidateEvidence::SinglePv {
                authoritative: EngineCandidateEvidence {
                    rank: 1,
                    root_move_uci: "b5c7".to_string(),
                    evaluation: evaluation(500),
                    variation: ["b5c7", "e8d7", "c7a8"]
                        .iter()
                        .map(|uci| (*uci).to_string())
                        .collect(),
                    provenance: provenance(),
                },
                player_move: PlayerMoveEvidence {
                    root_move_uci: "e1e2".to_string(),
                    evaluation: evaluation(0),
                    retained_variation: vec!["e1e2".to_string()],
                    provenance: provenance(),
                },
            },
        })
        .expect("the fork evidence is consistent");
        match build {
            DecisionExplanationBuild::Durable { explanation, .. } => *explanation,
            DecisionExplanationBuild::Abstained { diagnostics } => {
                panic!("the fork evidence should prove a concept: {diagnostics:?}")
            }
        }
    }

    fn provenance() -> DecisionEngineProvenance {
        DecisionEngineProvenance {
            engine: "Stockfish 18 fixture".to_string(),
            binary_digest: ArtifactDigest::try_from(format!("sha256:{}", "2".repeat(64))).unwrap(),
            depth: 16,
            threads: 1,
            hash_mib: 16,
        }
    }

    fn evaluation(value: i32) -> EngineEvaluation {
        EngineEvaluation::Centipawns {
            value,
            perspective: Color::White,
        }
    }
}
