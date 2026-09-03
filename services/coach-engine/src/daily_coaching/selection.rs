use std::{
    cmp::Ordering,
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
};

use serde::{Deserialize, Serialize};

use crate::profile_game_feed::{ProfileGameSourceIdentity, ProfileGameWindowEntry};

use super::digest::CoachingWindowKind;

pub(crate) const MAX_DAILY_COACHING_GAMES: usize = 10;
pub(crate) const MAX_INITIAL_BACKFILL_GAMES: usize = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SelectedDailyCoachingGame {
    pub(crate) selected: ProfileGameWindowEntry,
    #[serde(default)]
    pub(crate) window_kind: CoachingWindowKind,
}

impl SelectedDailyCoachingGame {
    #[cfg(test)]
    pub(crate) fn daily(selected: ProfileGameWindowEntry) -> Self {
        Self {
            selected,
            window_kind: CoachingWindowKind::Daily,
        }
    }
}

#[cfg(test)]
pub(crate) fn select_daily_games(
    candidates: Vec<ProfileGameWindowEntry>,
    digested_games: &BTreeSet<ProfileGameSourceIdentity>,
) -> Result<Vec<ProfileGameWindowEntry>, DailyGameSelectionError> {
    let mut candidates = unique_candidates(candidates, digested_games)?;
    candidates.sort_by(canonical_daily_game_order);
    candidates.truncate(MAX_DAILY_COACHING_GAMES);
    Ok(candidates)
}

pub(crate) fn resolve_initial_backfill(
    candidates: Vec<ProfileGameWindowEntry>,
) -> Result<Vec<ProfileGameWindowEntry>, DailyGameSelectionError> {
    let mut candidates = unique_candidates(candidates, &BTreeSet::new())?;
    candidates.sort_by(canonical_initial_backfill_order);
    candidates.truncate(MAX_INITIAL_BACKFILL_GAMES);
    Ok(candidates)
}

pub(crate) fn select_daily_and_backfill_games(
    daily_candidates: Vec<ProfileGameWindowEntry>,
    backfill_candidates: Vec<ProfileGameWindowEntry>,
    digested_games: &BTreeSet<ProfileGameSourceIdentity>,
) -> Result<Vec<SelectedDailyCoachingGame>, DailyGameSelectionError> {
    let mut excluded = digested_games.clone();
    let mut backfill = unique_candidates(backfill_candidates, &excluded)?;
    backfill.sort_by(canonical_initial_backfill_order);
    backfill.truncate(MAX_DAILY_COACHING_GAMES);
    excluded.extend(backfill.iter().map(|game| game.source_identity.clone()));
    let mut daily = unique_candidates(daily_candidates, &excluded)?;
    daily.sort_by(canonical_daily_game_order);
    daily.truncate(MAX_DAILY_COACHING_GAMES.saturating_sub(backfill.len()));

    Ok(daily
        .into_iter()
        .map(|selected| SelectedDailyCoachingGame {
            selected,
            window_kind: CoachingWindowKind::Daily,
        })
        .chain(
            backfill
                .into_iter()
                .map(|selected| SelectedDailyCoachingGame {
                    selected,
                    window_kind: CoachingWindowKind::InitialBackfill,
                }),
        )
        .collect())
}

fn unique_candidates(
    candidates: Vec<ProfileGameWindowEntry>,
    excluded: &BTreeSet<ProfileGameSourceIdentity>,
) -> Result<Vec<ProfileGameWindowEntry>, DailyGameSelectionError> {
    let mut unique = BTreeMap::new();
    for candidate in candidates {
        if excluded.contains(&candidate.source_identity) {
            continue;
        }
        match unique.entry(candidate.source_identity.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(candidate);
            }
            Entry::Occupied(entry) if entry.get() == &candidate => {}
            Entry::Occupied(_) => return Err(DailyGameSelectionError::ConflictingSourceIdentity),
        }
    }
    Ok(unique.into_values().collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum DailyGameSelectionError {
    #[error("one provider Game identity carried conflicting selection facts")]
    ConflictingSourceIdentity,
}

fn canonical_daily_game_order(
    left: &ProfileGameWindowEntry,
    right: &ProfileGameWindowEntry,
) -> Ordering {
    left.time_control_class
        .cmp(&right.time_control_class)
        .then_with(|| match left.time_control_class {
            crate::profile_game_feed::ProfileGameTimeControlClass::Correspondence => {
                Ordering::Equal
            }
            _ => right
                .expected_clock_seconds
                .cmp(&left.expected_clock_seconds),
        })
        .then_with(|| right.played_plies.cmp(&left.played_plies))
        .then_with(|| {
            right
                .ended_at_unix_milliseconds
                .cmp(&left.ended_at_unix_milliseconds)
        })
        .then_with(|| left.source_identity.cmp(&right.source_identity))
}

fn canonical_initial_backfill_order(
    left: &ProfileGameWindowEntry,
    right: &ProfileGameWindowEntry,
) -> Ordering {
    right
        .ended_at_unix_milliseconds
        .cmp(&left.ended_at_unix_milliseconds)
        .then_with(|| left.source_identity.cmp(&right.source_identity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        profile_game_feed::{
            ChessProfileProvider, DailyGameInputSource, DailyGameReviewRequest,
            ProfileGameTimeControlClass,
        },
        review_session_contract::{RequestedEloProfile, RequestedReviewSide, ReviewSide},
    };

    #[test]
    fn applies_the_complete_daily_game_order_before_the_cap() {
        let candidates = vec![
            candidate(
                "blitz",
                ProfileGameTimeControlClass::Blitz,
                Some(300),
                60,
                100,
            ),
            candidate(
                "rapid-short",
                ProfileGameTimeControlClass::Rapid,
                Some(300),
                80,
                100,
            ),
            candidate(
                "ultra",
                ProfileGameTimeControlClass::UltraBullet,
                Some(30),
                60,
                100,
            ),
            candidate(
                "bullet",
                ProfileGameTimeControlClass::Bullet,
                Some(60),
                60,
                100,
            ),
            candidate(
                "classical",
                ProfileGameTimeControlClass::Classical,
                Some(3_600),
                20,
                100,
            ),
            candidate(
                "rapid-fewer",
                ProfileGameTimeControlClass::Rapid,
                Some(600),
                60,
                100,
            ),
            candidate(
                "rapid-newer",
                ProfileGameTimeControlClass::Rapid,
                Some(600),
                80,
                200,
            ),
            candidate(
                "rapid-older-b",
                ProfileGameTimeControlClass::Rapid,
                Some(600),
                80,
                100,
            ),
            candidate(
                "rapid-older-a",
                ProfileGameTimeControlClass::Rapid,
                Some(600),
                80,
                100,
            ),
            candidate(
                "correspondence",
                ProfileGameTimeControlClass::Correspondence,
                None,
                10,
                100,
            ),
            candidate(
                "rapid-last",
                ProfileGameTimeControlClass::Rapid,
                Some(120),
                10,
                100,
            ),
            candidate(
                "rapid-capped",
                ProfileGameTimeControlClass::Rapid,
                Some(60),
                10,
                100,
            ),
        ];

        let selected = select_daily_games(candidates, &BTreeSet::new()).unwrap();

        assert_eq!(
            selected_ids(&selected),
            vec![
                "classical",
                "correspondence",
                "rapid-newer",
                "rapid-older-a",
                "rapid-older-b",
                "rapid-fewer",
                "rapid-short",
                "rapid-last",
                "rapid-capped",
                "blitz",
            ]
        );
    }

    #[test]
    fn correspondence_ties_use_plies_then_recency_then_identity() {
        let candidates = vec![
            candidate(
                "long-b",
                ProfileGameTimeControlClass::Correspondence,
                None,
                80,
                100,
            ),
            candidate(
                "recent",
                ProfileGameTimeControlClass::Correspondence,
                None,
                60,
                200,
            ),
            candidate(
                "long-a",
                ProfileGameTimeControlClass::Correspondence,
                None,
                80,
                100,
            ),
            candidate(
                "older",
                ProfileGameTimeControlClass::Correspondence,
                None,
                60,
                100,
            ),
        ];

        let selected = select_daily_games(candidates, &BTreeSet::new()).unwrap();

        assert_eq!(
            selected_ids(&selected),
            vec!["long-a", "long-b", "recent", "older"]
        );
    }

    #[test]
    fn correspondence_games_can_fill_the_player_cap_without_a_class_quota() {
        let mut candidates = (0..12)
            .map(|index| {
                candidate(
                    &format!("correspondence-{index:02}"),
                    ProfileGameTimeControlClass::Correspondence,
                    None,
                    100 - index,
                    100,
                )
            })
            .collect::<Vec<_>>();
        candidates.extend((0..9).map(|index| {
            candidate(
                &format!("blitz-{index:02}"),
                ProfileGameTimeControlClass::Blitz,
                Some(300),
                200,
                100,
            )
        }));

        let selected = select_daily_games(candidates, &BTreeSet::new()).unwrap();

        assert_eq!(selected.len(), MAX_DAILY_COACHING_GAMES);
        assert!(selected.iter().all(|game| {
            game.time_control_class == ProfileGameTimeControlClass::Correspondence
        }));
    }

    #[test]
    fn filters_digested_games_and_duplicates_before_applying_the_cap() {
        let mut candidates = (0..=10)
            .map(|index| {
                candidate(
                    &format!("game-{index:02}"),
                    ProfileGameTimeControlClass::Rapid,
                    Some(600),
                    100 - index,
                    100,
                )
            })
            .collect::<Vec<_>>();
        candidates.push(candidate(
            "game-00",
            ProfileGameTimeControlClass::Rapid,
            Some(600),
            100,
            100,
        ));
        let digested_games =
            BTreeSet::from([ProfileGameSourceIdentity::lichess("game-01".to_string())]);

        let selected = select_daily_games(candidates, &digested_games).unwrap();

        assert_eq!(
            selected_ids(&selected),
            vec![
                "game-00", "game-02", "game-03", "game-04", "game-05", "game-06", "game-07",
                "game-08", "game-09", "game-10",
            ]
        );
    }

    #[test]
    fn provider_is_only_the_final_identity_tie_breaker() {
        let lichess = candidate_for_provider(
            ChessProfileProvider::Lichess,
            "same-rank",
            ProfileGameTimeControlClass::Rapid,
            Some(600),
            80,
            100,
        );
        let chess_com = candidate_for_provider(
            ChessProfileProvider::ChessCom,
            "same-rank",
            ProfileGameTimeControlClass::Rapid,
            Some(600),
            80,
            100,
        );

        let selected = select_daily_games(vec![chess_com, lichess], &BTreeSet::new()).unwrap();

        assert_eq!(
            selected
                .iter()
                .map(|entry| entry.source_identity.provider)
                .collect::<Vec<_>>(),
            vec![
                ChessProfileProvider::Lichess,
                ChessProfileProvider::ChessCom
            ]
        );
    }

    #[test]
    fn rejects_conflicting_facts_for_one_source_identity() {
        let first = candidate(
            "same-game",
            ProfileGameTimeControlClass::Rapid,
            Some(600),
            80,
            100,
        );
        let conflicting = candidate(
            "same-game",
            ProfileGameTimeControlClass::Blitz,
            Some(300),
            60,
            100,
        );

        assert_eq!(
            select_daily_games(vec![first, conflicting], &BTreeSet::new()),
            Err(DailyGameSelectionError::ConflictingSourceIdentity)
        );
    }

    #[test]
    fn resolves_the_newest_five_eligible_games_as_a_fixed_backfill_plan() {
        let candidates = (0..7)
            .map(|index| {
                candidate(
                    &format!("backfill-{index}"),
                    ProfileGameTimeControlClass::Rapid,
                    Some(600),
                    40,
                    100 + index,
                )
            })
            .collect();

        let resolved = resolve_initial_backfill(candidates).unwrap();

        assert_eq!(
            selected_ids(&resolved),
            vec![
                "backfill-6",
                "backfill-5",
                "backfill-4",
                "backfill-3",
                "backfill-2",
            ]
        );
    }

    #[test]
    fn initial_backfill_keeps_every_eligible_game_when_fewer_than_five_exist() {
        let candidates = (0..3)
            .map(|index| {
                candidate(
                    &format!("backfill-{index}"),
                    ProfileGameTimeControlClass::Rapid,
                    Some(600),
                    40,
                    100 + index,
                )
            })
            .collect();

        let resolved = resolve_initial_backfill(candidates).unwrap();

        assert_eq!(
            selected_ids(&resolved),
            vec!["backfill-2", "backfill-1", "backfill-0"]
        );
    }

    #[test]
    fn owed_backfill_reserves_capacity_even_when_the_daily_window_is_saturated() {
        let daily = (0..8)
            .map(|index| {
                candidate(
                    &format!("daily-{index}"),
                    ProfileGameTimeControlClass::Rapid,
                    Some(600),
                    40,
                    200 + index,
                )
            })
            .collect();
        let backfill = (0..5)
            .map(|index| {
                candidate(
                    &format!("backfill-{index}"),
                    ProfileGameTimeControlClass::Rapid,
                    Some(600),
                    40,
                    100 + index,
                )
            })
            .collect();

        let selected = select_daily_and_backfill_games(daily, backfill, &BTreeSet::new()).unwrap();

        assert_eq!(selected.len(), MAX_DAILY_COACHING_GAMES);
        assert!(selected[..5]
            .iter()
            .all(|game| game.window_kind == CoachingWindowKind::Daily));
        assert_eq!(
            selected[5..]
                .iter()
                .map(|game| game.selected.source_identity.game_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "backfill-4",
                "backfill-3",
                "backfill-2",
                "backfill-1",
                "backfill-0",
            ]
        );
        assert!(selected[5..]
            .iter()
            .all(|game| game.window_kind == CoachingWindowKind::InitialBackfill));
    }

    #[test]
    fn a_previously_digested_backfill_game_does_not_consume_capacity() {
        let backfill = (0..3)
            .map(|index| {
                candidate(
                    &format!("backfill-{index}"),
                    ProfileGameTimeControlClass::Rapid,
                    Some(600),
                    40,
                    100 + index,
                )
            })
            .collect();
        let digested =
            BTreeSet::from([ProfileGameSourceIdentity::lichess("backfill-2".to_string())]);

        let selected = select_daily_and_backfill_games(Vec::new(), backfill, &digested).unwrap();

        assert_eq!(
            selected
                .iter()
                .map(|game| game.selected.source_identity.game_id.as_str())
                .collect::<Vec<_>>(),
            vec!["backfill-1", "backfill-0"]
        );
    }

    fn candidate(
        game_id: &str,
        time_control_class: ProfileGameTimeControlClass,
        expected_clock_seconds: Option<u64>,
        played_plies: u32,
        ended_at_unix_milliseconds: u64,
    ) -> ProfileGameWindowEntry {
        candidate_for_provider(
            ChessProfileProvider::Lichess,
            game_id,
            time_control_class,
            expected_clock_seconds,
            played_plies,
            ended_at_unix_milliseconds,
        )
    }

    fn candidate_for_provider(
        provider: ChessProfileProvider,
        game_id: &str,
        time_control_class: ProfileGameTimeControlClass,
        expected_clock_seconds: Option<u64>,
        played_plies: u32,
        ended_at_unix_milliseconds: u64,
    ) -> ProfileGameWindowEntry {
        let source_identity = match provider {
            ChessProfileProvider::Lichess => {
                ProfileGameSourceIdentity::lichess(game_id.to_string())
            }
            ChessProfileProvider::ChessCom => {
                ProfileGameSourceIdentity::chess_com(game_id.to_string())
            }
        };
        ProfileGameWindowEntry {
            source_identity,
            source_profile: "https://lichess.org/@/player".to_string(),
            review_request: DailyGameReviewRequest {
                source: DailyGameInputSource::LichessUrl {
                    url: format!("https://lichess.org/{game_id}"),
                },
                review_side: RequestedReviewSide::Selected {
                    review_side: ReviewSide::White,
                },
                elo_profile: RequestedEloProfile::FromImportedMetadata,
                ended_at_unix_milliseconds: Some(ended_at_unix_milliseconds),
            },
            ended_at_unix_milliseconds,
            time_control_raw: expected_clock_seconds.map_or_else(
                || "correspondence".to_string(),
                |seconds| seconds.to_string(),
            ),
            time_control_class,
            expected_clock_seconds,
            played_plies,
        }
    }

    fn selected_ids(selected: &[ProfileGameWindowEntry]) -> Vec<&str> {
        selected
            .iter()
            .map(|entry| entry.source_identity.game_id.as_str())
            .collect()
    }
}
