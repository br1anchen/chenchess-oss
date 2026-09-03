use super::*;
use crate::{
    daily_coaching::{DailyGameReviewFuture, DailyGameReviewResult, DailyGameReviewer},
    profile_game_feed::{ChessProfileProvider, DailyGameInputSource},
};

const CHESS_COM_PGN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/shared-assets/fixtures/Synthet1/lichess-export.pgn"
));

#[tokio::test]
async fn two_connections_publish_one_capped_provider_visible_digest_and_disconnect_independently() {
    let runtime = merged_runtime(ChessComOutcome::Healthy);
    let now = Utc::now();
    connect_both(&runtime, now).await;

    let report = runtime.tick(now).await.unwrap();
    let application = application_with_runtime(runtime, Arc::new(NoopExecutor));
    let dashboard = dashboard(&application).await;

    assert_eq!(report.published, 1);
    assert_eq!(dashboard["archive"].as_array().unwrap().len(), 1);
    assert_eq!(dashboard["archive"][0]["gameCount"], 10);
    let digest_id = dashboard["archive"][0]["digestId"].as_str().unwrap();
    let digest = request(
        &application,
        Method::GET,
        &format!("/api/v1/daily-coaching/digests/{digest_id}"),
        Value::Null,
    )
    .await;
    let games = digest.1["games"].as_array().unwrap();
    assert_eq!(games.len(), 10);
    assert_eq!(
        games
            .iter()
            .filter(|game| game["provider"] == "lichess")
            .count(),
        5
    );
    assert_eq!(
        games
            .iter()
            .filter(|game| game["provider"] == "chessCom")
            .count(),
        5
    );

    let removed_lichess = request(
        &application,
        Method::DELETE,
        "/api/v1/daily-coaching/connections/lichess",
        json!({ "expectedUsername": "synthetic-white" }),
    )
    .await;
    assert_eq!(removed_lichess.0, StatusCode::OK);
    assert_eq!(removed_lichess.1["state"]["kind"], "connected");
    assert_eq!(removed_lichess.1["state"]["enabled"], true);
    assert_eq!(
        removed_lichess.1["state"]["connections"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        removed_lichess.1["state"]["connections"][0]["provider"],
        "chessCom"
    );

    let removed_chess_com = request(
        &application,
        Method::DELETE,
        "/api/v1/daily-coaching/connections/chessCom",
        json!({ "expectedUsername": "synthetic-white" }),
    )
    .await;
    assert_eq!(removed_chess_com.0, StatusCode::OK);
    assert_eq!(removed_chess_com.1["state"]["kind"], "notConnected");
}

#[tokio::test]
async fn one_permanent_feed_failure_marks_only_that_connection_and_publishes_the_healthy_feed() {
    let runtime = merged_runtime(ChessComOutcome::PermanentFailure);
    let now = Utc::now();
    connect_both(&runtime, now).await;

    let report = runtime.tick(now).await.unwrap();
    let application = application_with_runtime(runtime, Arc::new(NoopExecutor));
    let dashboard = dashboard(&application).await;

    assert_eq!(report.published, 1);
    assert_eq!(dashboard["lead"]["kind"], "digest");
    assert_eq!(dashboard["archive"][0]["gameCount"], 5);
    assert_eq!(dashboard["connections"][0]["provider"], "lichess");
    assert_eq!(dashboard["connections"][0]["status"], "connected");
    assert_eq!(dashboard["connections"][1]["provider"], "chessCom");
    assert_eq!(dashboard["connections"][1]["status"], "profileUnavailable");
}

#[tokio::test]
async fn one_transient_feed_failure_publishes_partial_without_changing_connection_health() {
    let runtime = merged_runtime(ChessComOutcome::TransientFailure);
    let now = Utc::now();
    connect_both(&runtime, now).await;

    let report = runtime.tick(now).await.unwrap();
    let application = application_with_runtime(runtime, Arc::new(NoopExecutor));
    let dashboard = dashboard(&application).await;

    assert_eq!(report.published, 1);
    assert_eq!(dashboard["lead"]["kind"], "digest");
    assert_eq!(dashboard["archive"][0]["gameCount"], 5);
    assert!(dashboard["connections"]
        .as_array()
        .unwrap()
        .iter()
        .all(|connection| connection["status"] == "connected"));
}

#[tokio::test]
async fn a_transient_failure_without_any_publishable_feed_keeps_the_run_retryable() {
    let runtime = merged_runtime(ChessComOutcome::TransientFailure);
    let now = Utc::now();
    connect_profiles(
        &runtime,
        now,
        &["https://www.chess.com/member/synthetic-white"],
    )
    .await;

    let report = runtime.tick(now).await.unwrap();
    let application = application_with_runtime(runtime, Arc::new(NoopExecutor));
    let dashboard = dashboard(&application).await;

    assert_eq!(report.failed, 1);
    assert_eq!(report.no_digest, 0);
    assert_eq!(report.published, 0);
    assert_eq!(dashboard["lead"]["kind"], "preparingFirstDigest");
    assert!(dashboard["archive"].as_array().unwrap().is_empty());
    assert_eq!(dashboard["connections"][0]["status"], "connected");
}

fn merged_runtime(chess_com_outcome: ChessComOutcome) -> DailyCoachingRuntime {
    DailyCoachingRuntime::in_memory_with_reviewer(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(MergedProfileGameClient { chess_com_outcome }),
        Arc::new(PublishingDailyReviewer),
    )
}

async fn connect_both(runtime: &DailyCoachingRuntime, now: DateTime<Utc>) {
    connect_profiles(
        runtime,
        now,
        &[
            "https://lichess.org/@/synthetic-white",
            "https://www.chess.com/member/synthetic-white",
        ],
    )
    .await;
}

async fn connect_profiles(
    runtime: &DailyCoachingRuntime,
    now: DateTime<Utc>,
    profile_urls: &[&str],
) {
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    for profile_url in profile_urls {
        assert!(matches!(
            runtime
                .connect_at(
                    &player_id,
                    ConnectPlayingProfileRequest {
                        profile_url: (*profile_url).to_string(),
                        timezone: Some(midday_fixed_timezone(&now)),
                    },
                    now - TimeDelta::days(1),
                )
                .await,
            ConnectPlayingProfileOutcome::Completed { .. }
        ));
    }
}

async fn dashboard(application: &Router) -> Value {
    let response = request(
        application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    assert_eq!(response.0, StatusCode::OK);
    response.1
}

#[derive(Clone, Copy)]
enum ChessComOutcome {
    Healthy,
    PermanentFailure,
    TransientFailure,
}

struct MergedProfileGameClient {
    chess_com_outcome: ChessComOutcome,
}

impl ProfileGameClient for MergedProfileGameClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            match request.provider() {
                ChessProfileProvider::Lichess => Ok(ProfileGameResponse {
                    body: lichess_games(request),
                    content_type: request.accept().to_string(),
                }),
                ChessProfileProvider::ChessCom => match self.chess_com_outcome {
                    ChessComOutcome::Healthy => Ok(ProfileGameResponse {
                        body: chess_com_games(),
                        content_type: request.accept().to_string(),
                    }),
                    ChessComOutcome::PermanentFailure => Err(ProfileGameFetchError::Status {
                        provider: ChessProfileProvider::ChessCom,
                        code: 404,
                        retry_after_seconds: None,
                    }),
                    ChessComOutcome::TransientFailure => Err(ProfileGameFetchError::Status {
                        provider: ChessProfileProvider::ChessCom,
                        code: 503,
                        retry_after_seconds: Some(120),
                    }),
                },
            }
        })
    }
}

fn lichess_games(request: &ProfileGameRequest) -> Vec<u8> {
    let until = reqwest::Url::parse(request.url())
        .unwrap()
        .query_pairs()
        .find_map(|(name, value)| (name == "until").then_some(value))
        .map(|value| value.parse::<u64>().unwrap())
        .unwrap_or_else(|| u64::try_from(Utc::now().timestamp_millis()).unwrap());
    (1..=5)
        .map(|index| {
            json!({
                "id": format!("L{index:07}"),
                "variant": "standard",
                "status": "mate",
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(90),
                "lastMoveAt": until - index * 60 * 60 * 1_000,
                "players": {
                    "white": { "userId": format!("Opponent{index}") },
                    "black": { "userId": "synthetic-white" }
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

fn chess_com_games() -> Vec<u8> {
    let end_time = u64::try_from(Utc::now().timestamp()).unwrap();
    json!({
        "games": (1..=5)
            .map(|index| json!({
                "url": format!("https://www.chess.com/game/live/90000000{index}"),
                "pgn": CHESS_COM_PGN,
                "rules": "chess",
                "time_class": "rapid",
                "time_control": "600",
                "end_time": end_time - index * 60 * 60,
                "white": { "username": format!("Opponent{index}"), "result": "checkmated" },
                "black": { "username": "synthetic-white", "result": "win" }
            }))
            .collect::<Vec<_>>()
    })
    .to_string()
    .into_bytes()
}

struct PublishingDailyReviewer;

impl DailyGameReviewer for PublishingDailyReviewer {
    fn review<'a>(
        &'a self,
        _player_id: &'a PlayerId,
        request: &'a crate::profile_game_feed::DailyGameReviewRequest,
    ) -> DailyGameReviewFuture<'a> {
        let result = match &request.source {
            DailyGameInputSource::LichessUrl { url } => {
                let game_id = url.rsplit('/').next().unwrap();
                reviewed_game(
                    game_id,
                    published_imported_game(game_id),
                    published_review(game_id),
                )
            }
            DailyGameInputSource::ChessComArchive {
                url,
                pgn,
                captured_at,
                response_digest,
            } => {
                let game_id = url.rsplit('/').next().unwrap();
                let mut imported = published_imported_game("Synthet1");
                imported.provenance = ImportProvenance::ChessCom {
                    canonical_game_id: CanonicalGameId::try_from(game_id.to_string()).unwrap(),
                    canonical_url: url.clone(),
                    fetch_contract_version: "chess-com-pubapi-archive/v1".to_string(),
                    captured_at: captured_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    response_digest: response_digest.clone(),
                    pgn_digest: crate::game_import::artifact_digest(pgn.as_bytes()).unwrap(),
                };
                reviewed_game(game_id, imported, published_review(game_id))
            }
        };
        Box::pin(async move { result })
    }
}

fn reviewed_game(
    game_id: &str,
    imported_game: ImportedGame,
    review: GameReview,
) -> DailyGameReviewResult {
    DailyGameReviewResult::Reviewed {
        game_import_id: GameImportId::try_from(format!("game-import:merged:{game_id}")).unwrap(),
        imported_game: Box::new(imported_game),
        review: Box::new(review),
    }
}
