use std::collections::{BTreeMap, BTreeSet};

use shakmaty::{
    fen::Fen, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position, Role,
    Square as ChessSquare,
};

use crate::review_session_contract::{
    build_position_snapshot, AtomicChessFact, AtomicFactRef, CandidateEvidence, Color,
    DecisionCandidate, DecisionCandidateOrigin, DecisionCandidateRef, DecisionEngineProvenance,
    DecisionLineStep, DecisionPositionSnapshot, DecisionPositionSnapshotRef, EngineAssessment,
    EngineAssessmentRef, EngineAssessmentScore, PieceAtSquare, PieceRole, PositionSnapshot, Square,
};

use super::{facts::record_step_facts, DecisionExplanationContractError};

#[derive(Debug, Clone)]
pub(super) struct NormalizedCandidate {
    pub(super) root_move_uci: String,
    pub(super) origins: Vec<DecisionCandidateOrigin>,
    pub(super) rank: Option<u8>,
    pub(super) score: EngineAssessmentScore,
    pub(super) variation: Vec<String>,
    pub(super) provenance: DecisionEngineProvenance,
}

#[derive(Debug, Clone)]
pub(super) struct ReplayedCandidate {
    pub(super) contract: DecisionCandidate,
    pub(super) positions: Vec<Chess>,
}

#[derive(Debug, Clone)]
pub(super) struct CandidateConstruction {
    pub(super) snapshots: Vec<DecisionPositionSnapshot>,
    pub(super) facts: Vec<AtomicChessFact>,
    pub(super) candidates: Vec<ReplayedCandidate>,
}

pub(super) fn normalize_evidence(
    evidence: &CandidateEvidence,
    player_move_uci: &str,
    fen: &str,
) -> Result<Vec<NormalizedCandidate>, DecisionExplanationContractError> {
    let position: Chess = Fen::from_ascii(fen.as_bytes())
        .map_err(|_| DecisionExplanationContractError::InvalidPosition)?
        .into_position(CastlingMode::Standard)
        .map_err(|_| DecisionExplanationContractError::InvalidPosition)?;

    let (authoritative, alternatives, player) = match evidence {
        CandidateEvidence::SinglePv {
            authoritative,
            player_move,
        } => (authoritative, [].as_slice(), player_move),
        CandidateEvidence::MultiPv {
            authoritative_single_pv,
            requested_count,
            ranked_alternatives,
            player_move,
        } => {
            if *requested_count != 3 {
                return Err(DecisionExplanationContractError::InvalidCandidateEvidence(
                    "MultiPV requested count must be exactly three",
                ));
            }
            // Rank one is the authoritative record, so the alternatives are one
            // short of the searched roots.
            let expected = position.legal_moves().len().min(3).saturating_sub(1);
            if ranked_alternatives.len() != expected {
                return Err(DecisionExplanationContractError::InvalidCandidateEvidence(
                    "MultiPV alternatives must match the available legal roots below rank one",
                ));
            }
            (
                authoritative_single_pv,
                ranked_alternatives.as_slice(),
                player_move,
            )
        }
    };

    if authoritative.rank != 1
        || authoritative.variation.first() != Some(&authoritative.root_move_uci)
    {
        return Err(DecisionExplanationContractError::InvalidCandidateEvidence(
            "authoritative SinglePV must be rank one and begin with its root move",
        ));
    }
    if player.root_move_uci != player_move_uci
        || player.retained_variation.first() != Some(&player.root_move_uci)
    {
        return Err(DecisionExplanationContractError::InvalidCandidateEvidence(
            "Player move evidence must identify and begin with the supplied Player move",
        ));
    }
    if player.provenance != authoritative.provenance {
        return Err(DecisionExplanationContractError::InvalidCandidateEvidence(
            "all candidate assessments must share exact engine provenance",
        ));
    }
    // Both absolutes are SinglePV readings of the same Moment and must be stated
    // for the same side, or the comparison between them means nothing.
    if player.evaluation.perspective() != authoritative.evaluation.perspective() {
        return Err(DecisionExplanationContractError::InvalidCandidateEvidence(
            "all candidate assessments must share one stated perspective",
        ));
    }

    let mut roots = BTreeSet::new();
    roots.insert(authoritative.root_move_uci.clone());
    let mut normalized = Vec::with_capacity(alternatives.len() + 2);
    let mut authoritative_origins = vec![
        DecisionCandidateOrigin::EngineRanked,
        DecisionCandidateOrigin::AuthoritativeSinglePv,
    ];
    if authoritative.root_move_uci == player.root_move_uci {
        authoritative_origins.push(DecisionCandidateOrigin::PlayerPlayed);
    }
    authoritative_origins.sort();
    normalized.push(NormalizedCandidate {
        root_move_uci: authoritative.root_move_uci.clone(),
        origins: authoritative_origins,
        rank: Some(authoritative.rank),
        score: EngineAssessmentScore::Absolute {
            evaluation: authoritative.evaluation.clone(),
        },
        variation: authoritative.variation.clone(),
        provenance: authoritative.provenance.clone(),
    });

    for (index, alternative) in alternatives.iter().enumerate() {
        // Alternatives start at rank two; rank one left the list with the
        // authoritative record.
        let expected_rank = u8::try_from(index + 2).expect("MultiPV is bounded to three");
        if alternative.rank != expected_rank
            || alternative.variation.first() != Some(&alternative.root_move_uci)
            || alternative.provenance != authoritative.provenance
        {
            return Err(DecisionExplanationContractError::InvalidCandidateEvidence(
                "ranked alternatives must be contiguous below rank one, root-owned, and provenance-compatible",
            ));
        }
        if !roots.insert(alternative.root_move_uci.clone()) {
            return Err(DecisionExplanationContractError::InvalidCandidateEvidence(
                "candidate root moves must be unique",
            ));
        }
        let mut origins = vec![DecisionCandidateOrigin::EngineRanked];
        let played = alternative.root_move_uci == player.root_move_uci;
        if played {
            origins.push(DecisionCandidateOrigin::PlayerPlayed);
        }
        origins.sort();
        // A ranked root that is also the Player's move keeps its MultiPV rank but is
        // scored by SinglePV, exactly as the authoritative root above it: the Player move
        // has its own measured absolute, and publishing only a gap for it would drop the
        // one number a Moment states about what the Player actually played (ADR 0041).
        // The absolute travels with the line that produced it, never a MultiPV line.
        let (score, variation) = if played {
            (
                EngineAssessmentScore::Absolute {
                    evaluation: player.evaluation.clone(),
                },
                player.retained_variation.clone(),
            )
        } else {
            (
                EngineAssessmentScore::BehindBest {
                    gap: alternative.gap,
                },
                alternative.variation.clone(),
            )
        };
        normalized.push(NormalizedCandidate {
            root_move_uci: alternative.root_move_uci.clone(),
            origins,
            rank: Some(alternative.rank),
            score,
            variation,
            provenance: alternative.provenance.clone(),
        });
    }
    if !roots.contains(&player.root_move_uci) {
        normalized.push(NormalizedCandidate {
            root_move_uci: player.root_move_uci.clone(),
            origins: vec![DecisionCandidateOrigin::PlayerPlayed],
            rank: None,
            score: EngineAssessmentScore::Absolute {
                evaluation: player.evaluation.clone(),
            },
            variation: player.retained_variation.clone(),
            provenance: player.provenance.clone(),
        });
    }
    Ok(normalized)
}

pub(super) fn replay_candidates(
    initial: &PositionSnapshot,
    normalized: Vec<NormalizedCandidate>,
) -> Result<CandidateConstruction, DecisionExplanationContractError> {
    let mut snapshots = BTreeMap::new();
    let mut facts = BTreeMap::new();
    let mut candidates = Vec::with_capacity(normalized.len());

    let initial_record = snapshot_record(initial);
    snapshots.insert(initial_record.snapshot_ref.clone(), initial_record);

    for normalized_candidate in normalized {
        let replayed = replay_candidate(initial, normalized_candidate, &mut snapshots, &mut facts)?;
        candidates.push(replayed);
    }

    Ok(CandidateConstruction {
        snapshots: snapshots.into_values().collect(),
        facts: facts.into_values().collect(),
        candidates,
    })
}

fn replay_candidate(
    initial: &PositionSnapshot,
    normalized: NormalizedCandidate,
    snapshots: &mut BTreeMap<DecisionPositionSnapshotRef, DecisionPositionSnapshot>,
    facts: &mut BTreeMap<AtomicFactRef, AtomicChessFact>,
) -> Result<ReplayedCandidate, DecisionExplanationContractError> {
    let mut position: Chess = Fen::from_ascii(initial.fen.as_bytes())
        .map_err(|_| DecisionExplanationContractError::InvalidPosition)?
        .into_position(CastlingMode::Standard)
        .map_err(|_| DecisionExplanationContractError::InvalidPosition)?;
    let mut positions = vec![position.clone()];
    let mut prior_fens = vec![initial.fen.clone()];
    let mut line_steps = Vec::new();
    let mut candidate_fact_refs = Vec::new();
    let mut outcomes = Vec::new();

    for (index, uci) in normalized.variation.iter().enumerate() {
        let before_record = position_record(&position, &prior_fens[..prior_fens.len() - 1])?;
        snapshots
            .entry(before_record.snapshot_ref.clone())
            .or_insert(before_record.clone());
        let chess_move = UciMove::from_ascii(uci.as_bytes())
            .ok()
            .and_then(|parsed| parsed.to_move(&position).ok())
            .ok_or_else(|| DecisionExplanationContractError::InvalidCandidateLine {
                candidate: normalized.root_move_uci.clone(),
                index,
            })?;
        let Some(from) = chess_move.from() else {
            return Err(DecisionExplanationContractError::InvalidCandidateLine {
                candidate: normalized.root_move_uci.clone(),
                index,
            });
        };
        let mover = position.turn();
        let to = match &chess_move {
            shakmaty::Move::Castle { king, rook } => shakmaty::Square::from_coords(
                if rook.file() > king.file() {
                    shakmaty::File::G
                } else {
                    shakmaty::File::C
                },
                king.rank(),
            ),
            _ => chess_move.to(),
        };
        let role = chess_move.role();
        let captured = captured_piece(&position, &chess_move);
        position.play_unchecked(&chess_move);
        let after_fen = Fen::from_position(position.clone(), EnPassantMode::Legal).to_string();
        prior_fens.push(after_fen);
        let after_record = position_record(&position, &prior_fens[..prior_fens.len() - 1])?;
        snapshots
            .entry(after_record.snapshot_ref.clone())
            .or_insert(after_record.clone());

        let step_content = (
            &before_record.snapshot_ref,
            &after_record.snapshot_ref,
            uci,
            contract_color(mover),
            contract_role(role),
            contract_square(from),
            contract_square(to),
            &captured,
            chess_move.promotion().map(contract_role),
        );
        let step_ref = crate::review_session_contract::LineStepRef::from_content(&step_content);
        let step = DecisionLineStep {
            step_ref: step_ref.clone(),
            before_snapshot_ref: before_record.snapshot_ref,
            after_snapshot_ref: after_record.snapshot_ref.clone(),
            uci: uci.clone(),
            mover: contract_color(mover),
            role: contract_role(role),
            from_square: contract_square(from),
            to_square: contract_square(to),
            captured: captured.clone(),
            promotion: chess_move.promotion().map(contract_role),
        };
        line_steps.push(step.clone());

        record_step_facts(
            &positions[index],
            &position,
            &step,
            chess_move.promotion(),
            facts,
            &mut candidate_fact_refs,
            &mut outcomes,
        )?;
        positions.push(position.clone());
    }

    candidate_fact_refs.sort();
    candidate_fact_refs.dedup();
    let assessment_content = (normalized.rank, &normalized.score, &normalized.provenance);
    let assessment = EngineAssessment {
        assessment_ref: EngineAssessmentRef::from_content(&assessment_content),
        rank: normalized.rank,
        score: normalized.score,
        provenance: normalized.provenance,
    };
    let candidate_content = (
        &normalized.root_move_uci,
        &normalized.origins,
        &normalized.variation,
        line_steps
            .iter()
            .map(|step| &step.step_ref)
            .collect::<Vec<_>>(),
        &candidate_fact_refs,
        outcomes
            .iter()
            .map(|outcome| &outcome.outcome_ref)
            .collect::<Vec<_>>(),
        &assessment.assessment_ref,
    );
    let candidate_ref = DecisionCandidateRef::from_content(&candidate_content);
    Ok(ReplayedCandidate {
        contract: DecisionCandidate {
            candidate_ref,
            root_move_uci: normalized.root_move_uci,
            origins: normalized.origins,
            retained_variation: normalized.variation,
            line_steps,
            fact_refs: candidate_fact_refs,
            outcomes,
            assessment,
        },
        positions,
    })
}

fn snapshot_record(snapshot: &PositionSnapshot) -> DecisionPositionSnapshot {
    let content = (&snapshot.position_ref, &snapshot.fen);
    DecisionPositionSnapshot {
        snapshot_ref: DecisionPositionSnapshotRef::from_content(&content),
        canonical_position_ref: snapshot.position_ref.clone(),
        fen: snapshot.fen.clone(),
    }
}

fn position_record(
    position: &Chess,
    preceding_fens: &[String],
) -> Result<DecisionPositionSnapshot, DecisionExplanationContractError> {
    let fen = Fen::from_position(position.clone(), EnPassantMode::Legal).to_string();
    let preceding = preceding_fens
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let snapshot = build_position_snapshot(&fen, &preceding)
        .map_err(|_| DecisionExplanationContractError::InvalidPosition)?;
    Ok(snapshot_record(&snapshot))
}

fn captured_piece(position: &Chess, chess_move: &shakmaty::Move) -> Option<PieceAtSquare> {
    if !chess_move.is_capture() {
        return None;
    }
    let square = if chess_move.is_en_passant() {
        ChessSquare::from_coords(chess_move.to().file(), chess_move.from()?.rank())
    } else {
        chess_move.to()
    };
    position
        .board()
        .piece_at(square)
        .map(|piece| PieceAtSquare {
            color: contract_color(piece.color),
            role: contract_role(piece.role),
            square: contract_square(square),
        })
}

pub(super) fn contract_square(square: ChessSquare) -> Square {
    Square::try_from(square.to_string())
        .expect("a shakmaty square always fits the validated contract square grammar")
}

pub(super) fn chess_square(square: &Square) -> ChessSquare {
    ChessSquare::from_ascii(square.as_str().as_bytes())
        .expect("contract squares are validated before candidate replay")
}

pub(super) fn contract_color(color: shakmaty::Color) -> Color {
    color.fold_wb(Color::White, Color::Black)
}

pub(super) fn contract_role(role: Role) -> PieceRole {
    match role {
        Role::Pawn => PieceRole::Pawn,
        Role::Knight => PieceRole::Knight,
        Role::Bishop => PieceRole::Bishop,
        Role::Rook => PieceRole::Rook,
        Role::Queen => PieceRole::Queen,
        Role::King => PieceRole::King,
    }
}
