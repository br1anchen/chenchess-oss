use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::Duration,
};

use chrono::{DateTime, TimeDelta, Utc};
use reqwest::{
    header::{ACCEPT, CONTENT_TYPE, RETRY_AFTER},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    lichess::LichessGameUrl,
    provider_user_agent::{provider_user_agent, DAILY_COACHING_PATH},
    review_session_contract::{
        ArtifactDigest, GameInputSource, RequestedEloProfile, RequestedReviewSide, ReviewSide,
    },
};

mod chess_com_archive;
mod profile_validation;
mod window_probe;

#[cfg(test)]
pub(crate) use window_probe::lichess_moves;
pub(crate) use window_probe::{
    ProfileGameSourceIdentity, ProfileGameTimeControlClass, ProfileGameWindowEntry,
    RecentProfileGameCursor, RecentProfileGameScanPage,
};

pub use profile_validation::{
    ChessProfileProvider, ProfileUrlError, ProfileValidationError, ProfileValidationFuture,
    PublicChessProfile, PublicProfileValidator, ValidatedPublicChessProfile,
};

pub const MAX_RECENT_PROFILE_GAMES: u8 = 10;
/// Both providers' initial backfills reach back exactly two weeks; older Games are never coached.
const INITIAL_BACKFILL_WINDOW: TimeDelta = TimeDelta::weeks(2);
const PROFILE_FEED_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const PROFILE_FEED_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const PROFILE_FEED_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const LICHESS_NDJSON_MEDIA_TYPE: &str = "application/x-ndjson";
const JSON_MEDIA_TYPE: &str = "application/json";

static HTTP_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecentProfileGameCount(u8);

impl RecentProfileGameCount {
    pub fn value(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for RecentProfileGameCount {
    type Error = ProfileGameCountError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if (1..=MAX_RECENT_PROFILE_GAMES).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ProfileGameCountError)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("recent profile Game count must be between 1 and {MAX_RECENT_PROFILE_GAMES}")]
pub struct ProfileGameCountError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileGameReviewRequest {
    pub source: GameInputSource,
    pub review_side: RequestedReviewSide,
    pub elo_profile: RequestedEloProfile,
    pub ended_at_unix_milliseconds: Option<u64>,
}

impl ProfileGameReviewRequest {
    fn new(
        source: GameInputSource,
        review_side: ReviewSide,
        ended_at_unix_milliseconds: Option<u64>,
    ) -> Self {
        Self {
            source,
            review_side: RequestedReviewSide::Selected { review_side },
            elo_profile: RequestedEloProfile::FromImportedMetadata,
            ended_at_unix_milliseconds,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum DailyGameInputSource {
    LichessUrl {
        url: String,
    },
    ChessComArchive {
        url: String,
        pgn: String,
        captured_at: DateTime<Utc>,
        response_digest: ArtifactDigest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DailyGameReviewRequest {
    pub(crate) source: DailyGameInputSource,
    pub(crate) review_side: RequestedReviewSide,
    pub(crate) elo_profile: RequestedEloProfile,
    pub(crate) ended_at_unix_milliseconds: Option<u64>,
}

impl DailyGameReviewRequest {
    fn new(
        source: DailyGameInputSource,
        review_side: ReviewSide,
        ended_at_unix_milliseconds: u64,
    ) -> Self {
        Self {
            source,
            review_side: RequestedReviewSide::Selected { review_side },
            elo_profile: RequestedEloProfile::FromImportedMetadata,
            ended_at_unix_milliseconds: Some(ended_at_unix_milliseconds),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileGameRequest {
    provider: ChessProfileProvider,
    url: String,
    accept: &'static str,
}

impl ProfileGameRequest {
    pub fn provider(&self) -> ChessProfileProvider {
        self.provider
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn accept(&self) -> &'static str {
        self.accept
    }

    fn lichess(profile: &PublicChessProfile, count: RecentProfileGameCount) -> Self {
        let scan_count = count.value().saturating_mul(3);
        Self {
            provider: ChessProfileProvider::Lichess,
            url: format!(
                "https://lichess.org/api/games/user/{}?max={}&perfType=ultraBullet%2Cbullet%2Cblitz%2Crapid%2Cclassical%2Ccorrespondence&moves=false&tags=false&clocks=false&evals=false&accuracy=false&opening=false&division=false&ongoing=false&finished=true&literate=false&sort=dateDesc",
                profile.username(),
                scan_count,
            ),
            accept: LICHESS_NDJSON_MEDIA_TYPE,
        }
    }

    fn lichess_profile(profile: &PublicChessProfile) -> Self {
        Self {
            provider: ChessProfileProvider::Lichess,
            url: format!("https://lichess.org/api/user/{}", profile.username()),
            accept: JSON_MEDIA_TYPE,
        }
    }

    fn chess_com_profile(profile: &PublicChessProfile) -> Self {
        Self {
            provider: ChessProfileProvider::ChessCom,
            url: format!(
                "https://api.chess.com/pub/player/{}",
                profile.identity_username()
            ),
            accept: JSON_MEDIA_TYPE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileGameResponse {
    pub body: Vec<u8>,
    pub content_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileGameFetchError {
    #[error("could not construct the public profile Game client: {0}")]
    Client(String),
    #[error("could not connect to {provider:?}")]
    Connection { provider: ChessProfileProvider },
    #[error("{provider:?} public profile Game request timed out")]
    Timeout { provider: ChessProfileProvider },
    #[error("{provider:?} public profile Game request failed: {message}")]
    Transport {
        provider: ChessProfileProvider,
        message: String,
    },
    #[error("{provider:?} public profile Game request returned HTTP {code}")]
    Status {
        provider: ChessProfileProvider,
        code: u16,
        retry_after_seconds: Option<u32>,
    },
    #[error(
        "{provider:?} public profile Game response exceeded the {limit_bytes}-byte response limit"
    )]
    ResponseTooLarge {
        provider: ChessProfileProvider,
        limit_bytes: usize,
    },
}

pub trait ProfileGameClient: Send + Sync {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>;
}

impl<C> ProfileGameClient for Arc<C>
where
    C: ProfileGameClient + ?Sized,
{
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        (**self).fetch(request)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReqwestProfileGameClient;

impl ProfileGameClient for ReqwestProfileGameClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let provider = request.provider();
            let client = HTTP_CLIENT
                .get_or_init(|| {
                    reqwest::Client::builder()
                        .redirect(Policy::none())
                        .connect_timeout(PROFILE_FEED_CONNECT_TIMEOUT)
                        .timeout(PROFILE_FEED_RESPONSE_TIMEOUT)
                        .user_agent(provider_user_agent(DAILY_COACHING_PATH))
                        .build()
                        .map_err(|error| error.to_string())
                })
                .as_ref()
                .map_err(|error| ProfileGameFetchError::Client(error.clone()))?;
            let mut response = client
                .get(request.url())
                .header(ACCEPT, request.accept())
                .send()
                .await
                .map_err(|error| classify_reqwest_error(provider, error))?;
            if !response.status().is_success() {
                return Err(ProfileGameFetchError::Status {
                    provider,
                    code: response.status().as_u16(),
                    retry_after_seconds: response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.trim().parse().ok()),
                });
            }
            if response
                .content_length()
                .is_some_and(|length| length > PROFILE_FEED_MAX_RESPONSE_BYTES as u64)
            {
                return Err(ProfileGameFetchError::ResponseTooLarge {
                    provider,
                    limit_bytes: PROFILE_FEED_MAX_RESPONSE_BYTES,
                });
            }
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string();
            let mut body = Vec::with_capacity(
                response
                    .content_length()
                    .unwrap_or_default()
                    .min(PROFILE_FEED_MAX_RESPONSE_BYTES as u64) as usize,
            );
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| classify_reqwest_error(provider, error))?
            {
                if chunk.len() > PROFILE_FEED_MAX_RESPONSE_BYTES - body.len() {
                    return Err(ProfileGameFetchError::ResponseTooLarge {
                        provider,
                        limit_bytes: PROFILE_FEED_MAX_RESPONSE_BYTES,
                    });
                }
                body.extend_from_slice(&chunk);
            }
            Ok(ProfileGameResponse { body, content_type })
        })
    }
}

fn classify_reqwest_error(
    provider: ChessProfileProvider,
    error: reqwest::Error,
) -> ProfileGameFetchError {
    if error.is_timeout() {
        ProfileGameFetchError::Timeout { provider }
    } else if error.is_connect() {
        ProfileGameFetchError::Connection { provider }
    } else {
        ProfileGameFetchError::Transport {
            provider,
            message: error.to_string(),
        }
    }
}

pub struct ProfileGameFeed<C = ReqwestProfileGameClient> {
    client: C,
    request_gate: Arc<Mutex<()>>,
}

impl Default for ProfileGameFeed<ReqwestProfileGameClient> {
    fn default() -> Self {
        Self::new(ReqwestProfileGameClient)
    }
}

impl<C> ProfileGameFeed<C> {
    pub fn new(client: C) -> Self {
        Self {
            client,
            request_gate: Arc::new(Mutex::new(())),
        }
    }
}

impl<C> ProfileGameFeed<C>
where
    C: ProfileGameClient,
{
    pub async fn latest(
        &self,
        profile_url: &str,
        count: RecentProfileGameCount,
    ) -> Result<Vec<ProfileGameReviewRequest>, ProfileGameFeedError> {
        let profile = PublicChessProfile::parse(profile_url)?;
        let _request_guard = self.request_gate.lock().await;
        match profile.provider() {
            ChessProfileProvider::Lichess => self.latest_lichess(&profile, count).await,
            ChessProfileProvider::ChessCom => self.latest_chess_com(&profile, count).await,
        }
    }

    async fn latest_lichess(
        &self,
        profile: &PublicChessProfile,
        count: RecentProfileGameCount,
    ) -> Result<Vec<ProfileGameReviewRequest>, ProfileGameFeedError> {
        let response = self
            .client
            .fetch(&ProfileGameRequest::lichess(profile, count))
            .await?;
        require_content_type(&response, LICHESS_NDJSON_MEDIA_TYPE)?;
        let body = std::str::from_utf8(&response.body)
            .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?;
        let mut requests = Vec::with_capacity(usize::from(count.value()));
        for line in body.lines().filter(|line| !line.trim().is_empty()) {
            let game: LichessProfileGame = serde_json::from_str(line)
                .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?;
            if game.variant != "standard" {
                return Err(ProfileGameFeedError::MalformedProviderResponse);
            }
            if !is_reviewable_lichess_status(&game.status) {
                continue;
            }
            let review_side = game.players.review_side(profile.username())?;
            let source_url = format!("https://lichess.org/{}", game.id);
            let source = LichessGameUrl::parse(&source_url)
                .map_err(|_| ProfileGameFeedError::MalformedProviderResponse)?;
            requests.push(ProfileGameReviewRequest::new(
                GameInputSource::LichessUrl {
                    url: source.canonical_url(),
                },
                review_side,
                game.last_move_at,
            ));
            if requests.len() == usize::from(count.value()) {
                break;
            }
        }
        Ok(requests)
    }
}

fn require_content_type(
    response: &ProfileGameResponse,
    expected: &'static str,
) -> Result<(), ProfileGameFeedError> {
    if response
        .content_type
        .split(';')
        .next()
        .is_some_and(|actual| actual.trim().eq_ignore_ascii_case(expected))
    {
        Ok(())
    } else {
        Err(ProfileGameFeedError::UnexpectedContentType {
            expected,
            actual: response.content_type.clone(),
        })
    }
}

fn is_reviewable_lichess_status(status: &str) -> bool {
    matches!(
        status,
        "mate"
            | "resign"
            | "stalemate"
            | "timeout"
            | "draw"
            | "outoftime"
            | "cheat"
            | "unknownFinish"
            | "insufficientMaterialClaim"
    )
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileGameFeedError {
    #[error(transparent)]
    InvalidProfileUrl(#[from] ProfileUrlError),
    #[error(transparent)]
    Fetch(#[from] ProfileGameFetchError),
    #[error("public profile Game provider returned an unexpected content type; expected {expected}, got {actual}")]
    UnexpectedContentType {
        expected: &'static str,
        actual: String,
    },
    #[error("public profile Game provider returned malformed or contradictory data")]
    MalformedProviderResponse,
    #[error("profile Game window must be a non-empty supported UTC range")]
    InvalidWindow,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LichessProfileGame {
    id: String,
    variant: String,
    status: String,
    players: LichessPlayers,
    #[serde(default)]
    last_move_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct LichessPlayers {
    white: LichessPlayer,
    black: LichessPlayer,
}

impl LichessPlayers {
    fn review_side(&self, username: &str) -> Result<ReviewSide, ProfileGameFeedError> {
        unique_review_side(
            self.white.username().as_deref(),
            self.black.username().as_deref(),
            username,
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LichessPlayer {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    user: Option<LichessUser>,
}

impl LichessPlayer {
    fn username(&self) -> Option<String> {
        self.user_id
            .clone()
            .or_else(|| self.user.as_ref().map(|user| user.name.clone()))
    }
}

#[derive(Debug, Deserialize)]
struct LichessUser {
    name: String,
}

fn unique_review_side(
    white: Option<&str>,
    black: Option<&str>,
    username: &str,
) -> Result<ReviewSide, ProfileGameFeedError> {
    match (
        white.is_some_and(|player| player.eq_ignore_ascii_case(username)),
        black.is_some_and(|player| player.eq_ignore_ascii_case(username)),
    ) {
        (true, false) => Ok(ReviewSide::White),
        (false, true) => Ok(ReviewSide::Black),
        _ => Err(ProfileGameFeedError::MalformedProviderResponse),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex as StdMutex},
    };

    use chrono::DateTime;

    use super::*;

    #[test]
    fn parses_only_exact_public_profile_urls() {
        let lichess = PublicChessProfile::parse("https://lichess.org/@/SynthPlayer").unwrap();
        assert_eq!(lichess.provider(), ChessProfileProvider::Lichess);
        assert_eq!(lichess.username(), "SynthPlayer");
        assert_eq!(
            PublicChessProfile::parse("https://lichess.org/@/synthetic-white/all/")
                .unwrap()
                .username(),
            "synthetic-white"
        );

        let chess_com =
            PublicChessProfile::parse("https://www.chess.com/member/synthetic-white").unwrap();
        assert_eq!(chess_com.provider(), ChessProfileProvider::ChessCom);
        assert_eq!(chess_com.username(), "synthetic-white");

        for invalid in [
            "http://lichess.org/@/player",
            "https://lichess.org/@/player/playing",
            "https://lichess.org/@/player?games=1",
            "https://www.chess.com/member/player#games",
            "https://chess.com/member/player",
            "https://www.chess.com/member/player.name",
        ] {
            assert_eq!(
                PublicChessProfile::parse(invalid),
                Err(ProfileUrlError::UnparseableProfileUrl),
                "{invalid}"
            );
        }
        assert_eq!(
            PublicChessProfile::parse("https://example.test/@/player"),
            Err(ProfileUrlError::UnsupportedProvider)
        );
        assert_eq!(lichess.identity_username(), "synthplayer");
    }

    #[tokio::test]
    async fn validates_lichess_identity_and_returns_provider_casing() {
        let client = ScriptedClient::new([json_response(r#"{"username":"Player_One"}"#)]);

        let validated = ProfileGameFeed::new(client.clone())
            .validate_profile(
                &PublicChessProfile::parse("https://lichess.org/@/player_one/all/").unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            client.request_urls(),
            vec!["https://lichess.org/api/user/player_one"]
        );
        assert_eq!(validated.identity_username(), "player_one");
        assert_eq!(validated.username(), "Player_One");
        assert_eq!(
            validated.canonical_url(),
            "https://lichess.org/@/Player_One"
        );
    }

    #[tokio::test]
    async fn validates_chess_com_identity_with_provider_casing_and_ignores_player_id() {
        let client = ScriptedClient::new([json_response(
            r#"{"player_id":987654,"username":"MixedCase","status":"premium"}"#,
        )]);

        let validated = ProfileGameFeed::new(client.clone())
            .validate_profile(
                &PublicChessProfile::parse("https://www.chess.com/member/mIxEdCaSe").unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            client.request_urls(),
            vec!["https://api.chess.com/pub/player/mixedcase"]
        );
        assert_eq!(validated.identity_username(), "mixedcase");
        assert_eq!(validated.username(), "MixedCase");
        assert_eq!(
            validated.canonical_url(),
            "https://www.chess.com/member/MixedCase"
        );
    }

    #[tokio::test]
    async fn treats_both_chess_com_closed_statuses_as_missing_profiles() {
        for status in ["closed", "closed:fair_play_violations"] {
            let body = serde_json::json!({
                "player_id": 987654,
                "username": "MixedCase",
                "status": status,
            });
            let feed =
                ProfileGameFeed::new(ScriptedClient::new([json_response(&body.to_string())]));

            assert_eq!(
                feed.validate_profile(
                    &PublicChessProfile::parse("https://www.chess.com/member/mixedcase").unwrap(),
                )
                .await,
                Err(ProfileValidationError::ProfileNotFound),
                "{status}"
            );
        }
    }

    #[tokio::test]
    async fn profile_validation_maps_a_provider_404_to_not_found() {
        let feed = ProfileGameFeed::new(FailingClient(ProfileGameFetchError::Status {
            provider: ChessProfileProvider::Lichess,
            code: 404,
            retry_after_seconds: None,
        }));

        let error = feed
            .validate_profile(
                &PublicChessProfile::parse("https://lichess.org/@/missing_player").unwrap(),
            )
            .await
            .unwrap_err();

        assert_eq!(error, ProfileValidationError::ProfileNotFound);
    }

    #[test]
    fn recent_game_count_is_explicitly_bounded() {
        assert_eq!(RecentProfileGameCount::try_from(1).unwrap().value(), 1);
        assert_eq!(
            RecentProfileGameCount::try_from(MAX_RECENT_PROFILE_GAMES)
                .unwrap()
                .value(),
            MAX_RECENT_PROFILE_GAMES
        );
        assert_eq!(
            RecentProfileGameCount::try_from(0),
            Err(ProfileGameCountError)
        );
        assert_eq!(
            RecentProfileGameCount::try_from(MAX_RECENT_PROFILE_GAMES + 1),
            Err(ProfileGameCountError)
        );
    }

    #[path = "window_probe_tests.rs"]
    mod window_probe_tests;

    #[tokio::test]
    async fn resolves_newest_lichess_games_with_profile_side() {
        let client = ScriptedClient::new([ProfileGameResponse {
            body: br#"{"id":"abcdefgh","variant":"standard","status":"mate","lastMoveAt":2000,"players":{"white":{"userId":"Player_1"},"black":{"user":{"name":"Opponent"}}}}
{"id":"hgfedcba","variant":"standard","status":"resign","lastMoveAt":1000,"players":{"white":{"user":{"name":"Opponent"}},"black":{"userId":"player_1"}}}"#
                .to_vec(),
            content_type: "application/x-ndjson".to_string(),
        }]);
        let requests = ProfileGameFeed::new(client.clone())
            .latest(
                "https://lichess.org/@/Player_1",
                RecentProfileGameCount::try_from(2).unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            client.request_urls(),
            vec!["https://lichess.org/api/games/user/Player_1?max=6&perfType=ultraBullet%2Cbullet%2Cblitz%2Crapid%2Cclassical%2Ccorrespondence&moves=false&tags=false&clocks=false&evals=false&accuracy=false&opening=false&division=false&ongoing=false&finished=true&literate=false&sort=dateDesc"]
        );
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].source,
            GameInputSource::LichessUrl {
                url: "https://lichess.org/abcdefgh".to_string()
            }
        );
        assert_eq!(
            requests[0].review_side,
            RequestedReviewSide::Selected {
                review_side: ReviewSide::White
            }
        );
        assert_eq!(requests[0].ended_at_unix_milliseconds, Some(2000));
        assert_eq!(
            requests[1].review_side,
            RequestedReviewSide::Selected {
                review_side: ReviewSide::Black
            }
        );
        assert_eq!(
            requests[1].elo_profile,
            RequestedEloProfile::FromImportedMetadata
        );
    }

    fn json_response(body: &str) -> ProfileGameResponse {
        ProfileGameResponse {
            body: body.as_bytes().to_vec(),
            content_type: "application/json; charset=utf-8".to_string(),
        }
    }

    #[derive(Clone)]
    struct ScriptedClient {
        responses: Arc<StdMutex<VecDeque<ProfileGameResponse>>>,
        requests: Arc<StdMutex<Vec<String>>>,
    }

    impl ScriptedClient {
        fn new(responses: impl IntoIterator<Item = ProfileGameResponse>) -> Self {
            Self {
                responses: Arc::new(StdMutex::new(responses.into_iter().collect())),
                requests: Arc::new(StdMutex::new(Vec::new())),
            }
        }

        fn request_urls(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl ProfileGameClient for ScriptedClient {
        fn fetch<'a>(
            &'a self,
            request: &'a ProfileGameRequest,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a,
            >,
        > {
            self.requests
                .lock()
                .unwrap()
                .push(request.url().to_string());
            let response = self.responses.lock().unwrap().pop_front().unwrap();
            Box::pin(async move { Ok(response) })
        }
    }

    struct FailingClient(ProfileGameFetchError);

    impl ProfileGameClient for FailingClient {
        fn fetch<'a>(
            &'a self,
            _request: &'a ProfileGameRequest,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a,
            >,
        > {
            let error = self.0.clone();
            Box::pin(async move { Err(error) })
        }
    }
}
