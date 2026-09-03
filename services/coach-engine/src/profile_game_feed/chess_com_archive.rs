use std::collections::BTreeSet;

use chrono::{DateTime, Datelike, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{
    require_content_type, unique_review_side, ChessProfileProvider, DailyGameInputSource,
    DailyGameReviewRequest, ProfileGameClient, ProfileGameFeed, ProfileGameFeedError,
    ProfileGameRequest, ProfileGameReviewRequest, ProfileGameSourceIdentity,
    ProfileGameTimeControlClass, ProfileGameWindowEntry, PublicChessProfile,
    RecentProfileGameCount, RecentProfileGameScanPage, ReviewSide, INITIAL_BACKFILL_WINDOW,
    JSON_MEDIA_TYPE,
};
use crate::{
    chess_com::{parse_chess_com_game_identity, ChessComGameUrl},
    game_eligibility::daily_coaching_outcome,
    pgn::parse_pgn_with_metadata,
    review_session_contract::{ArtifactDigest, GameInputSource, ReviewSessionLimits},
};

const MAX_RECENT_ARCHIVE_MONTHS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ChessComArchiveMonth {
    year: u16,
    month: u8,
}

impl ChessComArchiveMonth {
    fn containing(instant: DateTime<Utc>) -> Result<Self, ProfileGameFeedError> {
        let year = u16::try_from(instant.year())
            .ok()
            .filter(|year| *year > 0)
            .ok_or(ProfileGameFeedError::InvalidWindow)?;
        let month =
            u8::try_from(instant.month()).map_err(|_| ProfileGameFeedError::InvalidWindow)?;
        Ok(Self { year, month })
    }

    fn previous(self) -> Option<Self> {
        if self.month == 1 {
            Some(Self {
                year: self.year.checked_sub(1)?,
                month: 12,
            })
        } else {
            Some(Self {
                year: self.year,
                month: self.month - 1,
            })
        }
    }

    fn next(self) -> Option<Self> {
        if self.month == 12 {
            Some(Self {
                year: self.year.checked_add(1)?,
                month: 1,
            })
        } else {
            Some(Self {
                year: self.year,
                month: self.month + 1,
            })
        }
    }
}

impl ProfileGameRequest {
    fn chess_com_month(profile: &PublicChessProfile, month: ChessComArchiveMonth) -> Self {
        Self {
            provider: ChessProfileProvider::ChessCom,
            url: format!(
                "https://api.chess.com/pub/player/{}/games/{}/{:02}",
                profile.identity_username(),
                month.year,
                month.month
            ),
            accept: JSON_MEDIA_TYPE,
        }
    }
}

impl<C> ProfileGameFeed<C>
where
    C: ProfileGameClient,
{
    pub(super) async fn latest_chess_com(
        &self,
        profile: &PublicChessProfile,
        count: RecentProfileGameCount,
    ) -> Result<Vec<ProfileGameReviewRequest>, ProfileGameFeedError> {
        self.latest_chess_com_at(profile, count, Utc::now()).await
    }

    async fn latest_chess_com_at(
        &self,
        profile: &PublicChessProfile,
        count: RecentProfileGameCount,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<ProfileGameReviewRequest>, ProfileGameFeedError> {
        let mut candidates = self
            .recent_chess_com_candidates(profile, count, as_of, None)
            .await?;
        candidates.sort_by(|left, right| {
            right
                .end_time_seconds
                .cmp(&left.end_time_seconds)
                .then_with(|| right.canonical_url.cmp(&left.canonical_url))
        });
        let mut seen = BTreeSet::new();
        Ok(candidates
            .into_iter()
            .filter(|candidate| seen.insert(candidate.canonical_url.clone()))
            .take(usize::from(count.value()))
            .map(ChessComArchiveCandidate::into_public_review_request)
            .collect())
    }

    pub(super) async fn eligible_chess_com_games(
        &self,
        profile: &PublicChessProfile,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Vec<ProfileGameWindowEntry>, ProfileGameFeedError> {
        let months = window_months(starts_at, ends_at)?;
        let mut games = Vec::new();
        let mut seen = BTreeSet::new();
        for month in months {
            games.extend(
                self.fetch_chess_com_month(profile, month)
                    .await?
                    .into_iter()
                    .filter(|candidate| seen.insert(candidate.canonical_url.clone()))
                    .filter(|candidate| {
                        candidate.ended_at >= starts_at && candidate.ended_at < ends_at
                    })
                    .map(|candidate| candidate.into_window_entry(profile)),
            );
        }
        games.into_iter().collect()
    }

    pub(super) async fn scan_latest_eligible_chess_com_games(
        &self,
        profile: &PublicChessProfile,
        count: RecentProfileGameCount,
        as_of: DateTime<Utc>,
    ) -> Result<RecentProfileGameScanPage, ProfileGameFeedError> {
        let backfill_floor = as_of
            .checked_sub_signed(INITIAL_BACKFILL_WINDOW)
            .ok_or(ProfileGameFeedError::InvalidWindow)?;
        let mut candidates = self
            .recent_chess_com_candidates(profile, count, as_of, Some(backfill_floor))
            .await?;
        candidates.sort_by(|left, right| {
            right
                .end_time_seconds
                .cmp(&left.end_time_seconds)
                .then_with(|| right.canonical_url.cmp(&left.canonical_url))
        });
        let games = candidates
            .into_iter()
            .take(usize::from(count.value()))
            .map(|candidate| candidate.into_window_entry(profile))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RecentProfileGameScanPage::Complete(games))
    }

    /// Walks archive months newest-first. `floor` bounds the initial backfill to its window; the
    /// public recent-Games read passes `None` and keeps its twelve-month discovery traversal.
    async fn recent_chess_com_candidates(
        &self,
        profile: &PublicChessProfile,
        count: RecentProfileGameCount,
        as_of: DateTime<Utc>,
        floor: Option<DateTime<Utc>>,
    ) -> Result<Vec<ChessComArchiveCandidate>, ProfileGameFeedError> {
        let oldest_month = floor.map(ChessComArchiveMonth::containing).transpose()?;
        let mut month = ChessComArchiveMonth::containing(as_of)?;
        let mut candidates = Vec::new();
        let mut seen = BTreeSet::new();
        for _ in 0..MAX_RECENT_ARCHIVE_MONTHS {
            candidates.extend(
                self.fetch_chess_com_month(profile, month)
                    .await?
                    .into_iter()
                    .filter(|candidate| floor.is_none_or(|floor| candidate.ended_at >= floor))
                    .filter(|candidate| seen.insert(candidate.canonical_url.clone())),
            );
            if candidates.len() >= usize::from(count.value())
                || oldest_month.is_some_and(|oldest| month <= oldest)
            {
                break;
            }
            let Some(previous) = month.previous() else {
                break;
            };
            month = previous;
        }
        Ok(candidates)
    }

    async fn fetch_chess_com_month(
        &self,
        profile: &PublicChessProfile,
        month: ChessComArchiveMonth,
    ) -> Result<Vec<ChessComArchiveCandidate>, ProfileGameFeedError> {
        let response = self
            .client
            .fetch(&ProfileGameRequest::chess_com_month(profile, month))
            .await?;
        require_content_type(&response, JSON_MEDIA_TYPE)?;
        let response_digest = archive_response_digest(&response.body)?;
        let captured_at = Utc::now();
        let archive: ChessComMonthlyGames = serde_json::from_slice(&response.body)
            .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?;
        archive
            .games
            .into_iter()
            .filter_map(|game| game.into_candidate(profile, captured_at, response_digest.clone()))
            .collect()
    }
}

fn window_months(
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
) -> Result<Vec<ChessComArchiveMonth>, ProfileGameFeedError> {
    let last_millisecond = ends_at
        .timestamp_millis()
        .checked_sub(1)
        .and_then(DateTime::from_timestamp_millis)
        .ok_or(ProfileGameFeedError::InvalidWindow)?;
    if starts_at >= ends_at {
        return Err(ProfileGameFeedError::InvalidWindow);
    }
    let first = ChessComArchiveMonth::containing(starts_at)?;
    let last = ChessComArchiveMonth::containing(last_millisecond)?;
    if first == last {
        Ok(vec![first])
    } else if first.next() == Some(last) {
        Ok(vec![first, last])
    } else {
        Err(ProfileGameFeedError::InvalidWindow)
    }
}

fn archive_response_digest(body: &[u8]) -> Result<ArtifactDigest, ProfileGameFeedError> {
    ArtifactDigest::try_from(format!("sha256:{:x}", Sha256::digest(body)))
        .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)
}

#[derive(Debug, Deserialize)]
struct ChessComMonthlyGames {
    games: Vec<ChessComArchiveGame>,
}

#[derive(Debug, Deserialize)]
struct ChessComArchiveGame {
    url: String,
    #[serde(default)]
    pgn: Option<String>,
    rules: String,
    time_class: String,
    time_control: String,
    end_time: u64,
    white: ChessComPlayer,
    black: ChessComPlayer,
}

impl ChessComArchiveGame {
    fn into_candidate(
        self,
        profile: &PublicChessProfile,
        captured_at: DateTime<Utc>,
        response_digest: ArtifactDigest,
    ) -> Option<Result<ChessComArchiveCandidate, ProfileGameFeedError>> {
        if self.rules != "chess" || self.white.is_abandoned() || self.black.is_abandoned() {
            return None;
        }
        let pgn = self.pgn.filter(|pgn| !pgn.trim().is_empty())?;
        if pgn.len() > usize::try_from(ReviewSessionLimits::V1.max_pgn_bytes).ok()? {
            return None;
        }
        let parsed = parse_pgn_with_metadata(&pgn).ok()?;
        daily_coaching_outcome(&parsed)?;
        let played_plies = u32::try_from(parsed.game.moves.len()).ok()?;
        let source = ChessComGameUrl::parse(&self.url).ok()?;
        let review_side = match unique_review_side(
            Some(&self.white.username),
            Some(&self.black.username),
            profile.username(),
        ) {
            Ok(side) => side,
            Err(error) => return Some(Err(error)),
        };
        let (time_control_raw, time_control_class, expected_clock_seconds) =
            parse_time_control(&self.time_class, &self.time_control)?;
        let ended_at_unix_milliseconds = self.end_time.checked_mul(1_000)?;
        let ended_at = i64::try_from(ended_at_unix_milliseconds)
            .ok()
            .and_then(DateTime::from_timestamp_millis)?;
        Some(Ok(ChessComArchiveCandidate {
            canonical_url: source.canonical_url(),
            pgn,
            captured_at,
            response_digest,
            review_side,
            ended_at,
            end_time_seconds: self.end_time,
            ended_at_unix_milliseconds,
            time_control_raw,
            time_control_class,
            expected_clock_seconds,
            played_plies,
        }))
    }
}

#[derive(Debug, Deserialize)]
struct ChessComPlayer {
    username: String,
    result: String,
}

impl ChessComPlayer {
    fn is_abandoned(&self) -> bool {
        self.result.eq_ignore_ascii_case("abandoned")
    }
}

fn parse_time_control(
    time_class: &str,
    raw: &str,
) -> Option<(String, ProfileGameTimeControlClass, Option<u64>)> {
    let class = match time_class {
        "daily" => {
            let (moves, seconds) = raw.split_once('/')?;
            let seconds = seconds.parse::<u64>().ok()?;
            if moves != "1" || seconds == 0 {
                return None;
            }
            return Some((
                "correspondence".to_string(),
                ProfileGameTimeControlClass::Correspondence,
                None,
            ));
        }
        "rapid" => ProfileGameTimeControlClass::Rapid,
        "blitz" => ProfileGameTimeControlClass::Blitz,
        "bullet" => ProfileGameTimeControlClass::Bullet,
        _ => return None,
    };
    let expected_clock_seconds = if let Some((initial, increment)) = raw.split_once('+') {
        let initial = initial.parse::<u64>().ok()?;
        let increment = increment.parse::<u64>().ok()?;
        increment.checked_mul(40)?.checked_add(initial)?
    } else {
        raw.parse::<u64>().ok()?
    };
    if expected_clock_seconds == 0 {
        return None;
    }
    Some((raw.to_string(), class, Some(expected_clock_seconds)))
}

struct ChessComArchiveCandidate {
    canonical_url: String,
    pgn: String,
    captured_at: DateTime<Utc>,
    response_digest: ArtifactDigest,
    review_side: ReviewSide,
    ended_at: DateTime<Utc>,
    end_time_seconds: u64,
    ended_at_unix_milliseconds: u64,
    time_control_raw: String,
    time_control_class: ProfileGameTimeControlClass,
    expected_clock_seconds: Option<u64>,
    played_plies: u32,
}

impl ChessComArchiveCandidate {
    fn into_public_review_request(self) -> ProfileGameReviewRequest {
        ProfileGameReviewRequest::new(
            GameInputSource::ChessComUrl {
                url: self.canonical_url,
            },
            self.review_side,
            Some(self.ended_at_unix_milliseconds),
        )
    }

    fn into_window_entry(
        self,
        profile: &PublicChessProfile,
    ) -> Result<ProfileGameWindowEntry, ProfileGameFeedError> {
        let (kind, game_id) = parse_chess_com_game_identity(&self.canonical_url)
            .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?;
        let source_identity =
            ProfileGameSourceIdentity::chess_com(format!("{}:{game_id}", kind.as_path()));
        let review_request = DailyGameReviewRequest::new(
            DailyGameInputSource::ChessComArchive {
                url: self.canonical_url,
                pgn: self.pgn,
                captured_at: self.captured_at,
                response_digest: self.response_digest,
            },
            self.review_side,
            self.ended_at_unix_milliseconds,
        );
        Ok(ProfileGameWindowEntry {
            source_identity,
            source_profile: format!("https://www.chess.com/member/{}", profile.username()),
            review_request,
            ended_at_unix_milliseconds: self.ended_at_unix_milliseconds,
            time_control_raw: self.time_control_raw,
            time_control_class: self.time_control_class,
            expected_clock_seconds: self.expected_clock_seconds,
            played_plies: self.played_plies,
        })
    }
}

#[cfg(test)]
#[path = "chess_com_archive/tests.rs"]
mod tests;
