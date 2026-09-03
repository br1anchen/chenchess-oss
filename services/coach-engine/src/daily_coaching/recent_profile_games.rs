//! The recent-games read: what a connected Playing Profile has finished
//! lately, answered from the provider feed behind a short-lived per-Player
//! cache. Importing is a separate decision, so nothing here writes.

use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    profile_game_feed::{
        ProfileGameFeedError, ProfileGameFetchError, ProfileGameReviewRequest,
        RecentProfileGameCount, MAX_RECENT_PROFILE_GAMES,
    },
    review_session_contract::{
        GameInputSource, PlayerId, RequestedReviewSide, RetryDirective, ReviewSide,
    },
};

use super::{
    DailyCoachingOwnerKey, DailyCoachingProvider, DailyCoachingReviewSide, DailyCoachingRuntime,
    DailyCoachingUnavailableReason,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecentPlayingProfileGame {
    pub source: String,
    pub review_side: DailyCoachingReviewSide,
    pub provider: DailyCoachingProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ended_at_unix_milliseconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "outcome",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RecentPlayingProfileGamesOutcome {
    Found {
        games: Vec<RecentPlayingProfileGame>,
    },
    NoPlayingProfile,
    Unavailable {
        reason: DailyCoachingUnavailableReason,
        retry: RetryDirective,
    },
}

pub(super) const RECENT_PROFILE_GAMES_CACHE_TTL: Duration = Duration::from_secs(30);

pub(super) struct CachedRecentProfileGames {
    pub(super) fetched_at: Instant,
    pub(super) outcome: RecentPlayingProfileGamesOutcome,
}

impl DailyCoachingRuntime {
    pub async fn recent_playing_profile_games(
        &self,
        player_id: &PlayerId,
    ) -> RecentPlayingProfileGamesOutcome {
        if let Some(cached) = self.cached_recent_profile_games(player_id) {
            return cached;
        }
        let outcome = self.fetch_recent_playing_profile_games(player_id).await;
        if !matches!(
            outcome,
            RecentPlayingProfileGamesOutcome::Unavailable { .. }
        ) {
            let mut cache = self
                .recent_games_cache
                .lock()
                .expect("recent-games cache lock should not be poisoned");
            // A read only expires an entry logically, so without this sweep a
            // Player who asks once and never returns is held forever.
            cache.retain(|_, held| held.fetched_at.elapsed() < RECENT_PROFILE_GAMES_CACHE_TTL);
            cache.insert(
                player_id.clone(),
                CachedRecentProfileGames {
                    fetched_at: Instant::now(),
                    outcome: outcome.clone(),
                },
            );
        }
        outcome
    }

    fn cached_recent_profile_games(
        &self,
        player_id: &PlayerId,
    ) -> Option<RecentPlayingProfileGamesOutcome> {
        let cache = self
            .recent_games_cache
            .lock()
            .expect("recent-games cache lock should not be poisoned");
        let entry = cache.get(player_id)?;
        (entry.fetched_at.elapsed() < RECENT_PROFILE_GAMES_CACHE_TTL).then(|| entry.outcome.clone())
    }

    async fn fetch_recent_playing_profile_games(
        &self,
        player_id: &PlayerId,
    ) -> RecentPlayingProfileGamesOutcome {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        let state = match self.store.bind_player(&owner_key, player_id).await {
            Ok(state) => state,
            Err(_) => {
                return persistence_unavailable();
            }
        };
        if state.connections().is_empty() {
            return RecentPlayingProfileGamesOutcome::NoPlayingProfile;
        }
        let count = RecentProfileGameCount::try_from(MAX_RECENT_PROFILE_GAMES)
            .expect("the feed admits its own maximum");
        let mut games = Vec::new();
        for connection in state.connections() {
            let requests = match self
                .lifecycle
                .profile_feed()
                .latest(connection.canonical_url(), count)
                .await
            {
                Ok(requests) => requests,
                Err(error) => return feed_unavailable(error),
            };
            for request in requests {
                match recent_playing_profile_game(request, connection.provider()) {
                    Some(game) => games.push(game),
                    None => {
                        return feed_unavailable(ProfileGameFeedError::MalformedProviderResponse)
                    }
                }
            }
        }
        games.sort_by(|left, right| {
            right
                .ended_at_unix_milliseconds
                .cmp(&left.ended_at_unix_milliseconds)
                .then_with(|| left.source.cmp(&right.source))
        });
        games.truncate(usize::from(MAX_RECENT_PROFILE_GAMES));
        RecentPlayingProfileGamesOutcome::Found { games }
    }
}

fn recent_playing_profile_game(
    request: ProfileGameReviewRequest,
    provider: DailyCoachingProvider,
) -> Option<RecentPlayingProfileGame> {
    let source = match request.source {
        GameInputSource::LichessUrl { url } | GameInputSource::ChessComUrl { url } => url,
        GameInputSource::PastedPgn { .. } | GameInputSource::LocalPgnFile { .. } => return None,
    };
    let review_side = match request.review_side {
        RequestedReviewSide::Selected {
            review_side: ReviewSide::White,
        } => DailyCoachingReviewSide::White,
        RequestedReviewSide::Selected {
            review_side: ReviewSide::Black,
        } => DailyCoachingReviewSide::Black,
        RequestedReviewSide::Selected {
            review_side: ReviewSide::Both,
        }
        | RequestedReviewSide::FromQualifiedUrl
        | RequestedReviewSide::Required => return None,
    };
    Some(RecentPlayingProfileGame {
        source,
        review_side,
        provider,
        ended_at_unix_milliseconds: request.ended_at_unix_milliseconds,
    })
}

fn persistence_unavailable() -> RecentPlayingProfileGamesOutcome {
    RecentPlayingProfileGamesOutcome::Unavailable {
        reason: DailyCoachingUnavailableReason::Persistence,
        retry: RetryDirective::RetryAllowed,
    }
}

fn feed_unavailable(error: ProfileGameFeedError) -> RecentPlayingProfileGamesOutcome {
    let retry = match &error {
        ProfileGameFeedError::Fetch(ProfileGameFetchError::Status {
            retry_after_seconds: Some(seconds),
            ..
        }) => RetryDirective::RetryAfter { seconds: *seconds },
        ProfileGameFeedError::Fetch(_)
        | ProfileGameFeedError::InvalidProfileUrl(_)
        | ProfileGameFeedError::UnexpectedContentType { .. }
        | ProfileGameFeedError::MalformedProviderResponse
        | ProfileGameFeedError::InvalidWindow => RetryDirective::RetryAllowed,
    };
    RecentPlayingProfileGamesOutcome::Unavailable {
        reason: DailyCoachingUnavailableReason::ProviderUnreachable,
        retry,
    }
}
