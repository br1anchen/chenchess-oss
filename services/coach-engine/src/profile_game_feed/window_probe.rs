use std::{collections::BTreeSet, fmt};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    is_reviewable_lichess_status, require_content_type, ChessProfileProvider, DailyGameInputSource,
    DailyGameReviewRequest, LichessPlayers, ProfileGameClient, ProfileGameFeed,
    ProfileGameFeedError, ProfileGameRequest, PublicChessProfile, RecentProfileGameCount,
    INITIAL_BACKFILL_WINDOW, LICHESS_NDJSON_MEDIA_TYPE,
};
use crate::{
    chess_com::{chess_com_game_id_is_canonical, parse_chess_com_game_identity, ChessComGameKind},
    game_eligibility::daily_coaching_outcome,
    lichess::LichessGameUrl,
    pgn::parse_pgn_with_metadata,
    review_session_contract::{
        RequestedEloProfile, RequestedReviewSide, ReviewSessionLimits, ReviewSide,
    },
};

const INITIAL_BACKFILL_PAGE_SIZE: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProfileGameSourceIdentity {
    pub(crate) provider: ChessProfileProvider,
    pub(crate) game_id: String,
}

impl ProfileGameSourceIdentity {
    pub(crate) fn lichess(game_id: String) -> Self {
        Self {
            provider: ChessProfileProvider::Lichess,
            game_id,
        }
    }

    pub(crate) fn chess_com(game_id: String) -> Self {
        Self {
            provider: ChessProfileProvider::ChessCom,
            game_id,
        }
    }

    pub(crate) fn chess_com_url(url: &str) -> Option<Self> {
        parse_chess_com_game_identity(url)
            .ok()
            .map(|(kind, game_id)| Self::chess_com(format!("{}:{game_id}", kind.as_path())))
    }

    pub(crate) fn canonical_key(&self) -> String {
        self.to_string()
    }

    pub(crate) fn is_valid_for_profile(&self, source_profile: &str) -> bool {
        let Ok(profile) = PublicChessProfile::parse(source_profile) else {
            return false;
        };
        if profile.provider() != self.provider {
            return false;
        }
        match self.provider {
            ChessProfileProvider::Lichess => {
                LichessGameUrl::parse(&format!("https://lichess.org/{}", self.game_id))
                    .is_ok_and(|source| source.canonical_game_id() == self.game_id)
            }
            ChessProfileProvider::ChessCom => {
                let Some((kind, game_id)) = self.game_id.split_once(':') else {
                    return false;
                };
                ChessComGameKind::from_path(kind).is_some()
                    && chess_com_game_id_is_canonical(game_id)
            }
        }
    }
}

impl fmt::Display for ProfileGameSourceIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let provider = match self.provider {
            ChessProfileProvider::Lichess => "lichess",
            ChessProfileProvider::ChessCom => "chessCom",
        };
        write!(formatter, "{provider}:{}", self.game_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProfileGameTimeControlClass {
    Classical,
    Correspondence,
    Rapid,
    Blitz,
    Bullet,
    UltraBullet,
}

impl ProfileGameTimeControlClass {
    pub(crate) fn facts_are_valid(
        self,
        time_control_raw: &str,
        expected_clock_seconds: Option<u64>,
    ) -> bool {
        if self == Self::Correspondence {
            return time_control_raw == "correspondence" && expected_clock_seconds.is_none();
        }
        let (initial, increment) =
            if let Some((initial_raw, increment_raw)) = time_control_raw.split_once('+') {
                let (Ok(initial), Ok(increment)) =
                    (initial_raw.parse::<u64>(), increment_raw.parse::<u64>())
                else {
                    return false;
                };
                if format!("{initial}+{increment}") != time_control_raw {
                    return false;
                }
                (initial, increment)
            } else {
                let Ok(initial) = time_control_raw.parse::<u64>() else {
                    return false;
                };
                if initial.to_string() != time_control_raw {
                    return false;
                }
                (initial, 0)
            };
        increment
            .checked_mul(40)
            .and_then(|increment| initial.checked_add(increment))
            .filter(|seconds| *seconds > 0)
            == expected_clock_seconds
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProfileGameWindowEntry {
    pub(crate) source_identity: ProfileGameSourceIdentity,
    pub(crate) source_profile: String,
    pub(crate) review_request: DailyGameReviewRequest,
    pub(crate) ended_at_unix_milliseconds: u64,
    pub(crate) time_control_raw: String,
    pub(crate) time_control_class: ProfileGameTimeControlClass,
    pub(crate) expected_clock_seconds: Option<u64>,
    pub(crate) played_plies: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RecentProfileGameCursor {
    until: u64,
    seen_at_until: BTreeSet<ProfileGameSourceIdentity>,
    #[serde(default, skip_serializing_if = "is_false")]
    exhausting_cutoff: bool,
}

impl RecentProfileGameCursor {
    pub(crate) fn is_valid(&self) -> bool {
        self.until > 0
            && !self.seen_at_until.is_empty()
            && self.seen_at_until.len() <= INITIAL_BACKFILL_PAGE_SIZE
            && self.seen_at_until.iter().all(|identity| {
                identity.provider == ChessProfileProvider::Lichess
                    && LichessGameUrl::parse(&format!("https://lichess.org/{}", identity.game_id))
                        .is_ok_and(|game| game.canonical_game_id() == identity.game_id)
            })
    }

    pub(crate) fn is_exhausting_cutoff(&self) -> bool {
        self.exhausting_cutoff
    }

    #[cfg(test)]
    pub(crate) fn test(
        until: u64,
        seen_at_until: impl IntoIterator<Item = ProfileGameSourceIdentity>,
    ) -> Self {
        Self {
            until,
            seen_at_until: seen_at_until.into_iter().collect(),
            exhausting_cutoff: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_exhausting_cutoff(
        until: u64,
        seen_at_until: impl IntoIterator<Item = ProfileGameSourceIdentity>,
    ) -> Self {
        Self {
            until,
            seen_at_until: seen_at_until.into_iter().collect(),
            exhausting_cutoff: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RecentProfileGameScanPage {
    Complete(Vec<ProfileGameWindowEntry>),
    Continue {
        games: Vec<ProfileGameWindowEntry>,
        cursor: RecentProfileGameCursor,
    },
    Stalled(Vec<ProfileGameWindowEntry>),
}

impl ProfileGameWindowEntry {
    pub(crate) fn is_valid(&self) -> bool {
        let source_matches = match (&self.source_identity.provider, &self.review_request.source) {
            (ChessProfileProvider::Lichess, DailyGameInputSource::LichessUrl { url }) => {
                LichessGameUrl::parse(url)
                    .is_ok_and(|source| source.canonical_game_id() == self.source_identity.game_id)
            }
            (
                ChessProfileProvider::ChessCom,
                DailyGameInputSource::ChessComArchive { url, pgn, .. },
            ) => {
                let Some((kind, game_id)) = self.source_identity.game_id.split_once(':') else {
                    return false;
                };
                parse_chess_com_game_identity(url).is_ok_and(|(parsed_kind, parsed_game_id)| {
                    parsed_kind.as_path() == kind && parsed_game_id == game_id
                }) && pgn.len() <= usize::try_from(ReviewSessionLimits::V1.max_pgn_bytes).unwrap()
                    && parse_pgn_with_metadata(pgn)
                        .is_ok_and(|parsed| daily_coaching_outcome(&parsed).is_some())
            }
            _ => false,
        };
        self.source_identity
            .is_valid_for_profile(&self.source_profile)
            && source_matches
            && matches!(
                self.review_request.review_side,
                RequestedReviewSide::Selected {
                    review_side: ReviewSide::White | ReviewSide::Black
                }
            )
            && self.review_request.elo_profile == RequestedEloProfile::FromImportedMetadata
            && self.review_request.ended_at_unix_milliseconds
                == Some(self.ended_at_unix_milliseconds)
            && self.ended_at_unix_milliseconds > 0
            && self.played_plies > 0
            && self
                .time_control_class
                .facts_are_valid(&self.time_control_raw, self.expected_clock_seconds)
    }
}

impl ProfileGameRequest {
    fn lichess_initial_backfill_page(
        profile: &PublicChessProfile,
        cursor: Option<&RecentProfileGameCursor>,
        as_of: DateTime<Utc>,
    ) -> Result<Self, ProfileGameFeedError> {
        let since = as_of
            .checked_sub_signed(INITIAL_BACKFILL_WINDOW)
            .map(|floor| floor.timestamp_millis())
            .filter(|since| *since >= 0)
            .ok_or(ProfileGameFeedError::InvalidWindow)?;
        let until = cursor.map_or_else(String::new, |cursor| format!("&until={}", cursor.until));
        Ok(Self {
            provider: ChessProfileProvider::Lichess,
            url: format!(
                "https://lichess.org/api/games/user/{}?max={INITIAL_BACKFILL_PAGE_SIZE}&since={since}{until}&perfType=ultraBullet%2Cbullet%2Cblitz%2Crapid%2Cclassical%2Ccorrespondence&moves=true&tags=false&clocks=false&evals=false&accuracy=false&opening=false&division=false&ongoing=false&finished=true&literate=false&sort=dateDesc",
                profile.username(),
            ),
            accept: LICHESS_NDJSON_MEDIA_TYPE,
        })
    }

    fn lichess_window(
        profile: &PublicChessProfile,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Self, ProfileGameFeedError> {
        let since = starts_at.timestamp_millis();
        let until = ends_at
            .timestamp_millis()
            .checked_sub(1)
            .ok_or(ProfileGameFeedError::InvalidWindow)?;
        if since < 0 || since > until {
            return Err(ProfileGameFeedError::InvalidWindow);
        }
        Ok(Self {
            provider: ChessProfileProvider::Lichess,
            url: format!(
                "https://lichess.org/api/games/user/{}?since={since}&until={until}&perfType=ultraBullet%2Cbullet%2Cblitz%2Crapid%2Cclassical%2Ccorrespondence&moves=true&tags=false&clocks=false&evals=false&accuracy=false&opening=false&division=false&ongoing=false&finished=true&literate=false&sort=dateDesc",
                profile.username(),
            ),
            accept: LICHESS_NDJSON_MEDIA_TYPE,
        })
    }
}

impl<C> ProfileGameFeed<C>
where
    C: ProfileGameClient,
{
    #[cfg(test)]
    pub(crate) async fn scan_latest_eligible_games(
        &self,
        profile_url: &str,
        count: RecentProfileGameCount,
        cursor: Option<&RecentProfileGameCursor>,
    ) -> Result<RecentProfileGameScanPage, ProfileGameFeedError> {
        self.scan_latest_eligible_games_at(profile_url, count, cursor, Utc::now())
            .await
    }

    pub(crate) async fn scan_latest_eligible_games_at(
        &self,
        profile_url: &str,
        count: RecentProfileGameCount,
        cursor: Option<&RecentProfileGameCursor>,
        as_of: DateTime<Utc>,
    ) -> Result<RecentProfileGameScanPage, ProfileGameFeedError> {
        let profile = PublicChessProfile::parse(profile_url)?;
        if cursor.is_some_and(|cursor| !cursor.is_valid()) {
            return Err(ProfileGameFeedError::MalformedProviderResponse);
        }
        let _request_guard = self.request_gate.lock().await;
        match profile.provider() {
            ChessProfileProvider::Lichess => {
                self.scan_latest_eligible_lichess_games(&profile, count, cursor, as_of)
                    .await
            }
            ChessProfileProvider::ChessCom if cursor.is_some() => {
                Err(ProfileGameFeedError::MalformedProviderResponse)
            }
            ChessProfileProvider::ChessCom => {
                self.scan_latest_eligible_chess_com_games(&profile, count, as_of)
                    .await
            }
        }
    }

    pub(crate) async fn eligible_games_in_window(
        &self,
        profile_url: &str,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Vec<ProfileGameWindowEntry>, ProfileGameFeedError> {
        let profile = PublicChessProfile::parse(profile_url)?;
        let _request_guard = self.request_gate.lock().await;
        match profile.provider() {
            ChessProfileProvider::Lichess => {
                self.eligible_lichess_games(&profile, starts_at, ends_at)
                    .await
            }
            ChessProfileProvider::ChessCom => {
                self.eligible_chess_com_games(&profile, starts_at, ends_at)
                    .await
            }
        }
    }

    async fn eligible_lichess_games(
        &self,
        profile: &PublicChessProfile,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Vec<ProfileGameWindowEntry>, ProfileGameFeedError> {
        let request = ProfileGameRequest::lichess_window(profile, starts_at, ends_at)?;
        let response = self.client.fetch(&request).await?;
        require_content_type(&response, LICHESS_NDJSON_MEDIA_TYPE)?;
        let body = std::str::from_utf8(&response.body)
            .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?;
        body.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<LichessWindowGame>(line)
                    .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?
                    .into_window_entry(profile, starts_at, ends_at)
            })
            .filter_map(Result::transpose)
            .collect()
    }

    async fn scan_latest_eligible_lichess_games(
        &self,
        profile: &PublicChessProfile,
        count: RecentProfileGameCount,
        cursor: Option<&RecentProfileGameCursor>,
        as_of: DateTime<Utc>,
    ) -> Result<RecentProfileGameScanPage, ProfileGameFeedError> {
        let mut eligible = Vec::with_capacity(usize::from(count.value()));
        let response = self
            .client
            .fetch(&ProfileGameRequest::lichess_initial_backfill_page(
                profile, cursor, as_of,
            )?)
            .await?;
        require_content_type(&response, LICHESS_NDJSON_MEDIA_TYPE)?;
        let body = std::str::from_utf8(&response.body)
            .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?;
        let games = body
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(INITIAL_BACKFILL_PAGE_SIZE + 1)
            .map(|line| {
                serde_json::from_str::<LichessWindowGame>(line)
                    .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let game_count = games.len();
        if game_count > INITIAL_BACKFILL_PAGE_SIZE {
            return Err(ProfileGameFeedError::MalformedProviderResponse);
        }
        let mut previous_ended_at = None;
        let mut oldest_ended_at = None;
        let mut cutoff_ended_at = cursor
            .filter(|cursor| cursor.exhausting_cutoff)
            .map(|cursor| cursor.until);
        let mut cutoff_exhausted = false;
        let mut seen_at_oldest = BTreeSet::new();
        let mut seen_in_page = BTreeSet::new();
        for game in games {
            let ended_at = game
                .last_move_at
                .ok_or(ProfileGameFeedError::MalformedProviderResponse)?;
            if cursor.is_some_and(|cursor| ended_at > cursor.until)
                || previous_ended_at.is_some_and(|previous| ended_at > previous)
            {
                return Err(ProfileGameFeedError::MalformedProviderResponse);
            }
            previous_ended_at = Some(ended_at);
            if cutoff_ended_at.is_some_and(|cutoff| ended_at < cutoff) {
                cutoff_exhausted = true;
            }
            let source_identity = game.source_identity()?;
            if oldest_ended_at != Some(ended_at) {
                oldest_ended_at = Some(ended_at);
                seen_at_oldest.clear();
            }
            seen_at_oldest.insert(source_identity.clone());
            let already_seen = !seen_in_page.insert(source_identity.clone())
                || cursor.is_some_and(|cursor| {
                    ended_at == cursor.until && cursor.seen_at_until.contains(&source_identity)
                });
            let entry = game.into_entry(profile)?;
            if !already_seen && cutoff_ended_at.is_none_or(|cutoff| ended_at >= cutoff) {
                if let Some(game) = entry {
                    eligible.push(game);
                    if eligible.len() == usize::from(count.value()) {
                        cutoff_ended_at = Some(ended_at);
                    }
                }
            }
        }
        if cutoff_exhausted || game_count < INITIAL_BACKFILL_PAGE_SIZE {
            return Ok(RecentProfileGameScanPage::Complete(eligible));
        }
        let until = oldest_ended_at.expect("a full Lichess page has an oldest end time");
        let mut seen_at_until = seen_at_oldest;
        if let Some(cursor) = cursor.filter(|cursor| cursor.until == until) {
            seen_at_until.extend(cursor.seen_at_until.iter().cloned());
        }
        let next_cursor = RecentProfileGameCursor {
            until,
            seen_at_until,
            exhausting_cutoff: cutoff_ended_at == Some(until),
        };
        if cursor == Some(&next_cursor) || !next_cursor.is_valid() {
            Ok(RecentProfileGameScanPage::Stalled(eligible))
        } else {
            Ok(RecentProfileGameScanPage::Continue {
                games: eligible,
                cursor: next_cursor,
            })
        }
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

/// Builds the `moves` field a Lichess Game carries for a given ply count. The feed counts
/// whitespace-separated tokens and never parses them, so the tokens stay deliberately opaque.
#[cfg(test)]
pub(crate) fn lichess_moves(played_plies: u32) -> String {
    vec!["e4"; played_plies as usize].join(" ")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LichessWindowGame {
    id: String,
    variant: String,
    status: String,
    players: LichessPlayers,
    #[serde(default)]
    last_move_at: Option<u64>,
    #[serde(default)]
    speed: Option<String>,
    #[serde(default)]
    clock: Option<LichessWindowClock>,
    #[serde(default)]
    moves: Option<String>,
}

impl LichessWindowGame {
    /// Lichess has no ply count field; the requested `moves` list is the only source.
    fn played_plies(&self) -> Result<u32, ProfileGameFeedError> {
        let moves = self
            .moves
            .as_deref()
            .ok_or(ProfileGameFeedError::MalformedProviderResponse)?;
        u32::try_from(moves.split_whitespace().count())
            .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)
    }

    fn source_identity(&self) -> Result<ProfileGameSourceIdentity, ProfileGameFeedError> {
        let game_url = LichessGameUrl::parse(&format!("https://lichess.org/{}", self.id))
            .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?;
        Ok(ProfileGameSourceIdentity::lichess(
            game_url.canonical_game_id().to_string(),
        ))
    }

    fn into_window_entry(
        self,
        profile: &PublicChessProfile,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Option<ProfileGameWindowEntry>, ProfileGameFeedError> {
        let entry = self.into_entry(profile)?;
        let Some(entry) = entry else {
            return Ok(None);
        };
        let ended_at = i64::try_from(entry.ended_at_unix_milliseconds)
            .ok()
            .and_then(DateTime::from_timestamp_millis)
            .ok_or(ProfileGameFeedError::MalformedProviderResponse)?;
        if ended_at < starts_at || ended_at >= ends_at {
            return Err(ProfileGameFeedError::MalformedProviderResponse);
        }
        Ok(Some(entry))
    }

    fn into_entry(
        self,
        profile: &PublicChessProfile,
    ) -> Result<Option<ProfileGameWindowEntry>, ProfileGameFeedError> {
        let ended_at_unix_milliseconds = self
            .last_move_at
            .ok_or(ProfileGameFeedError::MalformedProviderResponse)?;
        let played_plies = self.played_plies()?;
        if self.variant != "standard" || !is_reviewable_lichess_status(&self.status) {
            return Ok(None);
        }
        let Some(time_control) = LichessTimeControl::parse(self.speed.as_deref(), self.clock)
        else {
            return Ok(None);
        };
        if played_plies == 0 {
            return Ok(None);
        }
        let review_side = self.players.review_side(profile.username())?;
        let source = LichessGameUrl::parse(&format!("https://lichess.org/{}", self.id))
            .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?;
        let source_identity =
            ProfileGameSourceIdentity::lichess(source.canonical_game_id().to_string());
        let review_request = DailyGameReviewRequest::new(
            DailyGameInputSource::LichessUrl {
                url: source.canonical_url(),
            },
            review_side,
            ended_at_unix_milliseconds,
        );
        Ok(Some(ProfileGameWindowEntry {
            source_identity,
            source_profile: format!("https://lichess.org/@/{}", profile.username()),
            review_request,
            ended_at_unix_milliseconds,
            time_control_raw: time_control.raw,
            time_control_class: time_control.class,
            expected_clock_seconds: time_control.expected_clock_seconds,
            played_plies,
        }))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct LichessWindowClock {
    initial: u64,
    increment: u64,
}

struct LichessTimeControl {
    raw: String,
    class: ProfileGameTimeControlClass,
    expected_clock_seconds: Option<u64>,
}

impl LichessTimeControl {
    fn parse(speed: Option<&str>, clock: Option<LichessWindowClock>) -> Option<Self> {
        let class = match speed? {
            "classical" => ProfileGameTimeControlClass::Classical,
            "correspondence" => {
                return Some(Self {
                    raw: "correspondence".to_string(),
                    class: ProfileGameTimeControlClass::Correspondence,
                    expected_clock_seconds: None,
                });
            }
            "rapid" => ProfileGameTimeControlClass::Rapid,
            "blitz" => ProfileGameTimeControlClass::Blitz,
            "bullet" => ProfileGameTimeControlClass::Bullet,
            "ultraBullet" => ProfileGameTimeControlClass::UltraBullet,
            _ => return None,
        };
        let clock = clock?;
        let expected_clock_seconds = clock
            .increment
            .checked_mul(40)?
            .checked_add(clock.initial)?;
        Some(Self {
            raw: format!("{}+{}", clock.initial, clock.increment),
            class,
            expected_clock_seconds: Some(expected_clock_seconds),
        })
    }
}
