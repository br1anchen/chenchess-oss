use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::{
    domain::{MoveSide, ReviewSide},
    review_session_contract::{PositionPhaseKind, PositiveHighlightGrade},
};

pub const HARD_MAXIMUM: usize = 10;
const DIVERSITY_PENALTY: i64 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CandidateKind {
    ForcedMateDeterioration,
    GreatPositiveHighlight,
    AdvantageLost,
    NowWorse,
    GoodPositiveHighlight,
    AdvantageReduced,
    StandingKept,
}

impl CandidateKind {
    fn priority_band(self) -> i64 {
        match self {
            Self::ForcedMateDeterioration => 1_050,
            Self::GreatPositiveHighlight => 900,
            Self::AdvantageLost => 830,
            Self::NowWorse => 730,
            Self::GoodPositiveHighlight => 650,
            Self::AdvantageReduced => 630,
            Self::StandingKept => 480,
        }
    }

    pub fn is_positive(self) -> bool {
        matches!(
            self,
            Self::GreatPositiveHighlight | Self::GoodPositiveHighlight
        )
    }

    pub fn from_positive_grade(grade: PositiveHighlightGrade) -> Self {
        match grade {
            PositiveHighlightGrade::Great => Self::GreatPositiveHighlight,
            PositiveHighlightGrade::Good => Self::GoodPositiveHighlight,
        }
    }
}

fn phase_index(kind: PositionPhaseKind) -> usize {
    match kind {
        PositionPhaseKind::Opening => 0,
        PositionPhaseKind::Middlegame => 1,
        PositionPhaseKind::Endgame => 2,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachingEpisode {
    pub id: String,
    pub role: EpisodeRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum EpisodeRole {
    Decision,
    Continuation,
    Payoff,
}

#[derive(Debug, Clone)]
pub struct Candidate<T> {
    pub ply: usize,
    pub side: MoveSide,
    pub kind: CandidateKind,
    pub tactical: bool,
    pub phase: PositionPhaseKind,
    pub evidence_strength: u8,
    pub episode: Option<CoachingEpisode>,
    pub payload: T,
}

impl<T> Candidate<T> {
    fn priority(&self) -> i64 {
        self.kind.priority_band() + i64::from(self.evidence_strength)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorTrace {
    pub game_plies: usize,
    pub adaptive_target: usize,
    pub hard_maximum: usize,
    pub positive_reservation_required: bool,
    pub diversity_penalty: i64,
    pub candidates: Vec<CandidateTrace>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateTrace {
    pub ply: usize,
    pub kind: CandidateKind,
    pub evidence_strength: u8,
    pub priority: i64,
    pub episode: Option<CoachingEpisode>,
    pub episode_outcome: EpisodeOutcome,
    pub selected: bool,
    pub priority_rank: Option<usize>,
    pub game_order_position: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EpisodeOutcome {
    Retained,
    Collapsed { retained_ply: usize },
    Suppressed,
}

#[derive(Debug)]
pub struct Selection<T> {
    pub selected: Vec<T>,
    pub trace: SelectorTrace,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SelectorError {
    #[error("selector target {target} exceeds hard maximum {hard_maximum}")]
    HardMaximum { target: usize, hard_maximum: usize },
}

pub fn adaptive_target(game_plies: usize) -> usize {
    (2 + game_plies.div_ceil(18)).clamp(3, 8)
}

pub fn select<T: Clone>(
    game_plies: usize,
    review_side: ReviewSide,
    candidates: Vec<Candidate<T>>,
) -> Result<Selection<T>, SelectorError> {
    let target = adaptive_target(game_plies);
    if target > HARD_MAXIMUM {
        return Err(SelectorError::HardMaximum {
            target,
            hard_maximum: HARD_MAXIMUM,
        });
    }
    let candidate_pool = candidates.clone();
    let (collapsed, outcomes) = collapse_episodes(candidates);
    let positive_required = collapsed
        .iter()
        .any(|candidate| candidate.kind.is_positive());
    let plan = optimize(&collapsed, target, positive_required, review_side);
    let selected_indices = plan
        .as_ref()
        .map_or(&[][..], |plan| plan.indices.as_slice());
    let selected_plies = selected_indices
        .iter()
        .map(|index| collapsed[*index].ply)
        .collect::<std::collections::BTreeSet<_>>();
    let mut priority_order = selected_indices.to_vec();
    priority_order.sort_by(|left, right| {
        collapsed[*right]
            .priority()
            .cmp(&collapsed[*left].priority())
            .then_with(|| collapsed[*left].ply.cmp(&collapsed[*right].ply))
    });
    let priority_ranks = priority_order
        .iter()
        .enumerate()
        .map(|(index, selected)| (collapsed[*selected].ply, index + 1))
        .collect::<HashMap<_, _>>();
    let game_order = selected_indices
        .iter()
        .enumerate()
        .map(|(index, selected)| (collapsed[*selected].ply, index + 1))
        .collect::<HashMap<_, _>>();
    let diversity_penalty = plan.as_ref().map_or(0, |plan| plan.diversity_penalty);
    let traces = candidate_pool
        .iter()
        .map(|candidate| CandidateTrace {
            ply: candidate.ply,
            kind: candidate.kind,
            evidence_strength: candidate.evidence_strength,
            priority: candidate.priority(),
            episode: candidate.episode.clone(),
            episode_outcome: outcomes
                .get(&candidate.ply)
                .cloned()
                .unwrap_or(EpisodeOutcome::Retained),
            selected: selected_plies.contains(&candidate.ply),
            priority_rank: priority_ranks.get(&candidate.ply).copied(),
            game_order_position: game_order.get(&candidate.ply).copied(),
        })
        .collect();
    let mut selected = selected_indices
        .iter()
        .map(|index| collapsed[*index].payload.clone())
        .collect::<Vec<_>>();
    // `collapsed` is already in Game order; preserve that order for product output.
    selected.shrink_to_fit();
    Ok(Selection {
        selected,
        trace: SelectorTrace {
            game_plies,
            adaptive_target: target,
            hard_maximum: HARD_MAXIMUM,
            positive_reservation_required: positive_required,
            diversity_penalty,
            candidates: traces,
        },
    })
}

fn collapse_episodes<T: Clone>(
    mut candidates: Vec<Candidate<T>>,
) -> (Vec<Candidate<T>>, HashMap<usize, EpisodeOutcome>) {
    candidates.sort_by_key(|candidate| candidate.ply);
    let mut retained = Vec::new();
    let mut outcomes = HashMap::new();
    let mut groups = BTreeMap::<String, Vec<Candidate<T>>>::new();
    for candidate in candidates {
        let Some(episode) = candidate.episode.as_ref() else {
            retained.push(candidate);
            continue;
        };
        groups
            .entry(episode.id.clone())
            .or_default()
            .push(candidate);
    }
    for group in groups.into_values() {
        let representative = group
            .iter()
            .filter(|candidate| {
                candidate
                    .episode
                    .as_ref()
                    .is_some_and(|episode| episode.role == EpisodeRole::Decision)
            })
            .min_by_key(|candidate| candidate.ply)
            .or_else(|| {
                group
                    .iter()
                    .filter(|candidate| {
                        candidate
                            .episode
                            .as_ref()
                            .is_some_and(|episode| episode.role == EpisodeRole::Payoff)
                    })
                    .max_by_key(|candidate| candidate.ply)
            });
        let Some(representative) = representative else {
            for candidate in group {
                outcomes.insert(candidate.ply, EpisodeOutcome::Suppressed);
            }
            continue;
        };
        let representative_ply = representative.ply;
        retained.push(representative.clone());
        for candidate in group {
            if candidate.ply != representative_ply {
                outcomes.insert(
                    candidate.ply,
                    EpisodeOutcome::Collapsed {
                        retained_ply: representative_ply,
                    },
                );
            }
        }
    }
    retained.sort_by_key(|candidate| candidate.ply);
    (retained, outcomes)
}

#[derive(Clone)]
struct Plan {
    utility: i64,
    diversity_penalty: i64,
    indices: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    count: u8,
    has_positive: bool,
    tactical: [u8; 2],
    phase: [u8; 3],
    side: [u8; 2],
}

fn optimize<T>(
    candidates: &[Candidate<T>],
    target: usize,
    positive_required: bool,
    review_side: ReviewSide,
) -> Option<Plan> {
    let category_allowance = target.saturating_mul(2).div_ceil(3).max(2) as u8;
    let phase_allowance = category_allowance;
    let side_allowance = target.saturating_mul(3).div_ceil(4).max(2) as u8;
    let mut states = BTreeMap::new();
    states.insert(
        State {
            count: 0,
            has_positive: false,
            tactical: [0, 0],
            phase: [0, 0, 0],
            side: [0, 0],
        },
        Plan {
            utility: 0,
            diversity_penalty: 0,
            indices: Vec::new(),
        },
    );
    for (index, candidate) in candidates.iter().enumerate() {
        let mut additions = Vec::new();
        for (state, plan) in &states {
            if usize::from(state.count) == target {
                continue;
            }
            let mut next = *state;
            next.count += 1;
            next.has_positive |= candidate.kind.is_positive();
            let category_index = usize::from(!candidate.tactical);
            let side_index = usize::from(matches!(candidate.side, MoveSide::Black));
            let mut crossings = 0;
            next.tactical[category_index] += 1;
            crossings += usize::from(next.tactical[category_index] > category_allowance);
            next.phase[phase_index(candidate.phase)] += 1;
            crossings += usize::from(next.phase[phase_index(candidate.phase)] > phase_allowance);
            if review_side == ReviewSide::Both {
                next.side[side_index] += 1;
                crossings += usize::from(next.side[side_index] > side_allowance);
            }
            let penalty =
                i64::try_from(crossings).expect("small fixed diversity count") * DIVERSITY_PENALTY;
            let mut next_plan = plan.clone();
            next_plan.utility += candidate.priority() - penalty;
            next_plan.diversity_penalty += penalty;
            next_plan.indices.push(index);
            additions.push((next, next_plan));
        }
        for (state, plan) in additions {
            match states.get(&state) {
                Some(existing) if !better(&plan, existing, candidates) => {}
                _ => {
                    states.insert(state, plan);
                }
            }
        }
    }
    states
        .into_iter()
        .filter(|(state, _)| !positive_required || state.has_positive)
        .max_by(|(left_state, left), (right_state, right)| {
            left_state
                .count
                .cmp(&right_state.count)
                .then_with(|| left.utility.cmp(&right.utility))
                .then_with(|| {
                    ordered_plies(right, candidates).cmp(&ordered_plies(left, candidates))
                })
        })
        .map(|(_, plan)| plan)
}

fn better<T>(left: &Plan, right: &Plan, candidates: &[Candidate<T>]) -> bool {
    left.utility > right.utility
        || (left.utility == right.utility
            && ordered_plies(left, candidates) < ordered_plies(right, candidates))
}

fn ordered_plies<T>(plan: &Plan, candidates: &[Candidate<T>]) -> Vec<usize> {
    plan.indices
        .iter()
        .map(|index| candidates[*index].ply)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        adaptive_target, select, Candidate, CandidateKind, CoachingEpisode, EpisodeOutcome,
        EpisodeRole,
    };
    use crate::{
        domain::{MoveSide, ReviewSide},
        review_session_contract::PositionPhaseKind,
    };

    fn candidate(ply: usize, kind: CandidateKind) -> Candidate<usize> {
        Candidate {
            ply,
            side: MoveSide::White,
            kind,
            tactical: false,
            phase: PositionPhaseKind::Middlegame,
            evidence_strength: 0,
            episode: None,
            payload: ply,
        }
    }

    #[test]
    fn target_uses_complete_game_length_boundaries() {
        assert_eq!(adaptive_target(0), 3);
        assert_eq!(adaptive_target(18), 3);
        assert_eq!(adaptive_target(19), 4);
        assert_eq!(adaptive_target(90), 7);
        assert_eq!(adaptive_target(91), 8);
        assert_eq!(adaptive_target(400), 8);
    }

    #[test]
    fn reserves_an_in_target_positive_slot_and_restores_game_order() {
        let selection = select(
            19,
            ReviewSide::Both,
            vec![
                candidate(1, CandidateKind::StandingKept),
                candidate(2, CandidateKind::StandingKept),
                candidate(3, CandidateKind::StandingKept),
                candidate(4, CandidateKind::StandingKept),
                candidate(5, CandidateKind::GreatPositiveHighlight),
            ],
        )
        .unwrap();

        assert_eq!(selection.selected, vec![1, 2, 3, 5]);
        assert!(selection.trace.positive_reservation_required);
        assert_eq!(selection.trace.candidates[4].game_order_position, Some(4));
    }

    #[test]
    fn retains_earliest_decision_and_suppresses_forced_only_episode() {
        let mut decision = candidate(3, CandidateKind::AdvantageLost);
        decision.episode = Some(CoachingEpisode {
            id: "a".into(),
            role: EpisodeRole::Decision,
        });
        let mut continuation = candidate(4, CandidateKind::NowWorse);
        continuation.episode = Some(CoachingEpisode {
            id: "a".into(),
            role: EpisodeRole::Continuation,
        });
        let mut forced = candidate(7, CandidateKind::StandingKept);
        forced.episode = Some(CoachingEpisode {
            id: "b".into(),
            role: EpisodeRole::Continuation,
        });
        let selection =
            select(20, ReviewSide::White, vec![decision, continuation, forced]).unwrap();

        assert_eq!(selection.selected, vec![3]);
        assert_eq!(
            selection.trace.candidates[1].episode_outcome,
            EpisodeOutcome::Collapsed { retained_ply: 3 }
        );
    }

    #[test]
    fn exact_utility_ties_choose_the_earlier_ply_list() {
        let selection = select(
            19,
            ReviewSide::White,
            vec![
                candidate(1, CandidateKind::StandingKept),
                candidate(2, CandidateKind::StandingKept),
                candidate(3, CandidateKind::StandingKept),
                candidate(4, CandidateKind::StandingKept),
                candidate(5, CandidateKind::StandingKept),
            ],
        )
        .unwrap();

        assert_eq!(selection.selected, vec![1, 2, 3, 4]);
    }
}
