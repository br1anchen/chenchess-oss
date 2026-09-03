use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::profile_game_feed::{
    ProfileGameSourceIdentity, ProfileGameWindowEntry, PublicChessProfile, RecentProfileGameCursor,
};

use super::{DailyCoachingProvider, DailyCoachingStoreError, StoredPlayingProfileConnection};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum InitialBackfill {
    Pending {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        games: Vec<ProfileGameWindowEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<RecentProfileGameCursor>,
    },
    Owed {
        games: Vec<ProfileGameWindowEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unavailable_reason: Option<InitialBackfillUnavailableReason>,
    },
    Completed {
        had_eligible_games: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unavailable_reason: Option<InitialBackfillUnavailableReason>,
    },
}

impl Default for InitialBackfill {
    fn default() -> Self {
        Self::Pending {
            games: Vec::new(),
            cursor: None,
        }
    }
}

impl InitialBackfill {
    pub(super) fn resolved(
        games: Vec<ProfileGameWindowEntry>,
    ) -> Result<Self, DailyCoachingStoreError> {
        if games.is_empty() {
            Ok(Self::Completed {
                had_eligible_games: false,
                unavailable_reason: None,
            })
        } else if games.len() <= crate::daily_coaching::selection::MAX_INITIAL_BACKFILL_GAMES {
            Ok(Self::Owed {
                games,
                unavailable_reason: None,
            })
        } else {
            Err(DailyCoachingStoreError::InvalidRecord)
        }
    }

    pub(super) fn resolved_stalled(
        games: Vec<ProfileGameWindowEntry>,
    ) -> Result<Self, DailyCoachingStoreError> {
        if games.is_empty() {
            Ok(Self::Completed {
                had_eligible_games: false,
                unavailable_reason: Some(InitialBackfillUnavailableReason::ScanStalled),
            })
        } else if games.len() <= crate::daily_coaching::selection::MAX_INITIAL_BACKFILL_GAMES {
            Ok(Self::Owed {
                games,
                unavailable_reason: Some(InitialBackfillUnavailableReason::ScanStalled),
            })
        } else {
            Err(DailyCoachingStoreError::InvalidRecord)
        }
    }

    pub(super) fn checkpointed(
        games: Vec<ProfileGameWindowEntry>,
        cursor: RecentProfileGameCursor,
    ) -> Result<Self, DailyCoachingStoreError> {
        if games.len() > crate::daily_coaching::selection::MAX_INITIAL_BACKFILL_GAMES
            || !cursor.is_valid()
            || (games.len() == crate::daily_coaching::selection::MAX_INITIAL_BACKFILL_GAMES)
                != cursor.is_exhausting_cutoff()
        {
            Err(DailyCoachingStoreError::InvalidRecord)
        } else {
            Ok(Self::Pending {
                games,
                cursor: Some(cursor),
            })
        }
    }

    pub(super) fn reconcile(
        &mut self,
        digested_games: &BTreeSet<ProfileGameSourceIdentity>,
    ) -> bool {
        let Self::Owed {
            games,
            unavailable_reason,
        } = self
        else {
            return false;
        };
        let previous_len = games.len();
        games.retain(|game| !digested_games.contains(&game.source_identity));
        if games.is_empty() {
            let unavailable_reason = *unavailable_reason;
            *self = Self::Completed {
                had_eligible_games: true,
                unavailable_reason,
            };
            true
        } else {
            games.len() != previous_len
        }
    }

    pub(super) fn is_valid_for(&self, connection: &StoredPlayingProfileConnection) -> bool {
        let (games, valid_count) = match self {
            Self::Pending { games, cursor } => {
                if (!games.is_empty() && cursor.is_none())
                    || cursor.as_ref().is_some_and(|cursor| {
                        connection.provider != DailyCoachingProvider::Lichess
                            || !cursor.is_valid()
                            || (games.len()
                                == crate::daily_coaching::selection::MAX_INITIAL_BACKFILL_GAMES)
                                != cursor.is_exhausting_cutoff()
                    })
                {
                    return false;
                }
                (
                    games,
                    games.len() <= crate::daily_coaching::selection::MAX_INITIAL_BACKFILL_GAMES,
                )
            }
            Self::Owed {
                games,
                unavailable_reason,
            } => {
                let valid_unavailable_reason = unavailable_reason.is_none()
                    || (*unavailable_reason == Some(InitialBackfillUnavailableReason::ScanStalled)
                        && connection.provider == DailyCoachingProvider::Lichess);
                (
                    games,
                    valid_unavailable_reason
                        && !games.is_empty()
                        && games.len()
                            <= crate::daily_coaching::selection::MAX_INITIAL_BACKFILL_GAMES,
                )
            }
            Self::Completed {
                had_eligible_games,
                unavailable_reason,
            } => {
                return match unavailable_reason {
                    None => true,
                    Some(InitialBackfillUnavailableReason::ProviderUnsupported) => {
                        !had_eligible_games
                            && connection.provider == DailyCoachingProvider::ChessCom
                    }
                    Some(InitialBackfillUnavailableReason::ScanStalled) => {
                        connection.provider == DailyCoachingProvider::Lichess
                    }
                };
            }
        };
        let identities = games
            .iter()
            .map(|game| game.source_identity.clone())
            .collect::<BTreeSet<_>>();
        valid_count
            && identities.len() == games.len()
            && games.iter().all(|game| {
                game.is_valid()
                    && DailyCoachingProvider::from(game.source_identity.provider)
                        == connection.provider
                    && PublicChessProfile::parse(&game.source_profile).is_ok_and(|profile| {
                        DailyCoachingProvider::from(profile.provider()) == connection.provider
                            && profile.identity_username() == connection.identity_username
                    })
            })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InitialBackfillSnapshot {
    Pending {
        games: Vec<ProfileGameWindowEntry>,
        cursor: Option<RecentProfileGameCursor>,
    },
    Owed(Vec<ProfileGameWindowEntry>),
    Completed,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InitialBackfillMutation {
    Resolve(Vec<ProfileGameWindowEntry>),
    ResolveStalled(Vec<ProfileGameWindowEntry>),
    Checkpoint {
        games: Vec<ProfileGameWindowEntry>,
        cursor: RecentProfileGameCursor,
    },
    Unavailable(InitialBackfillUnavailableReason),
    Reconcile(BTreeSet<ProfileGameSourceIdentity>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum InitialBackfillUnavailableReason {
    ProviderUnsupported,
    ScanStalled,
}
