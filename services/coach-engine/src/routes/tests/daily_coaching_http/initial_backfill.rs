use super::*;
use std::sync::Mutex;

#[tokio::test]
async fn saturated_daily_selection_still_resolves_and_durably_drains_initial_backfill() {
    let executor = Arc::new(PublishingExecutor);
    let client = Arc::new(InitialBackfillClient::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        client.clone(),
        executor.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    runtime
        .connect_at(
            &player_id,
            ConnectPlayingProfileRequest {
                profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                timezone: Some(midday_fixed_timezone(&now)),
            },
            now - TimeDelta::days(1),
        )
        .await;

    let first = runtime.tick(now).await.unwrap();
    let second = runtime.tick(now + TimeDelta::days(1)).await.unwrap();
    let third = runtime.tick(now + TimeDelta::days(2)).await.unwrap();
    let application = application_with_runtime(runtime, executor);
    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    let archive = dashboard.1["archive"].as_array().unwrap();

    assert_eq!(first.published, 1);
    assert_eq!(second.published, 1);
    assert_eq!(third.no_digest, 1);
    assert_eq!(client.latest_calls.load(Ordering::SeqCst), 1);
    assert_eq!(archive.len(), 2);
    assert_eq!(archive[0]["gameCount"], 4);
    assert_eq!(archive[1]["gameCount"], 10);

    let first_digest_id = archive[1]["digestId"].as_str().unwrap();
    let first_digest = request(
        &application,
        Method::GET,
        &format!("/api/v1/daily-coaching/digests/{first_digest_id}"),
        Value::Null,
    )
    .await;
    assert_eq!(
        first_digest.1["games"]
            .as_array()
            .unwrap()
            .iter()
            .map(|game| game["gameImportId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "game-import:daily-http:DA000002",
            "game-import:daily-http:DA000003",
            "game-import:daily-http:DA000004",
            "game-import:daily-http:DA000005",
            "game-import:daily-http:DA000006",
            "game-import:daily-http:DA000001",
            "game-import:daily-http:BF000001",
            "game-import:daily-http:BF000002",
            "game-import:daily-http:BF000003",
            "game-import:daily-http:BF000004",
        ]
    );

    let second_digest_id = archive[0]["digestId"].as_str().unwrap();
    let second_digest = request(
        &application,
        Method::GET,
        &format!("/api/v1/daily-coaching/digests/{second_digest_id}"),
        Value::Null,
    )
    .await;
    assert_eq!(
        second_digest.1["games"]
            .as_array()
            .unwrap()
            .iter()
            .map(|game| game["gameImportId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "game-import:daily-http:DA000007",
            "game-import:daily-http:DA000008",
            "game-import:daily-http:DA000009",
            "game-import:daily-http:DA000010",
        ]
    );
}

#[tokio::test]
async fn every_saturated_daily_window_reserves_capacity_for_the_owed_backfill() {
    let executor = Arc::new(PublishingExecutor);
    let client = Arc::new(AlwaysSaturatedBackfillClient::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        client.clone(),
        executor.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    runtime
        .connect_at(
            &player_id,
            ConnectPlayingProfileRequest {
                profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                timezone: Some(midday_fixed_timezone(&now)),
            },
            now - TimeDelta::days(1),
        )
        .await;

    let first = runtime.tick(now).await.unwrap();
    let second = runtime.tick(now + TimeDelta::days(1)).await.unwrap();
    let application = application_with_runtime(runtime, executor);
    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    let first_digest_id = dashboard.1["archive"][1]["digestId"].as_str().unwrap();
    let first_digest = request(
        &application,
        Method::GET,
        &format!("/api/v1/daily-coaching/digests/{first_digest_id}"),
        Value::Null,
    )
    .await;

    assert_eq!(first.published, 1);
    assert_eq!(second.published, 1);
    assert_eq!(client.backfill_calls.load(Ordering::SeqCst), 1);
    assert_eq!(dashboard.1["archive"][0]["gameCount"], 10);
    assert_eq!(dashboard.1["archive"][1]["gameCount"], 10);
    assert_eq!(
        first_digest.1["games"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|game| {
                game["gameImportId"]
                    .as_str()
                    .is_some_and(|id| id.contains(":BF"))
            })
            .count(),
        5
    );
}

#[tokio::test]
async fn zero_eligible_games_projects_no_eligible_games_yet_without_an_empty_digest() {
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(EmptyEligibleClient),
        Arc::new(NoopExecutor),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    runtime
        .connect_at(
            &player_id,
            ConnectPlayingProfileRequest {
                profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                timezone: Some(midday_fixed_timezone(&now)),
            },
            now - TimeDelta::days(1),
        )
        .await;

    let report = runtime.tick(now).await.unwrap();
    let application = application_with_runtime(runtime, Arc::new(NoopExecutor));
    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;

    assert_eq!(report.no_digest, 1);
    assert_eq!(dashboard.1["lead"], json!({ "kind": "noEligibleGamesYet" }));
    assert_eq!(dashboard.1["archive"], json!([]));
}

#[tokio::test]
async fn terminally_rejected_backfill_is_retired_instead_of_reselected() {
    let executor = Arc::new(CountingTerminalExecutor::default());
    let client = Arc::new(TerminalBackfillClient::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        client.clone(),
        executor.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    runtime
        .connect_at(
            &player_id,
            ConnectPlayingProfileRequest {
                profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                timezone: Some(midday_fixed_timezone(&now)),
            },
            now - TimeDelta::days(1),
        )
        .await;

    let first = runtime.tick(now).await.unwrap();
    let second = runtime.tick(now + TimeDelta::days(1)).await.unwrap();
    let application = application_with_runtime(runtime, executor.clone());
    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;

    assert_eq!(first.no_digest, 1);
    assert_eq!(first.permanent_game_failures, 1);
    assert_eq!(second.no_digest, 1);
    assert_eq!(client.backfill_calls.load(Ordering::SeqCst), 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(dashboard.1["lead"]["kind"], "noEligibleGames");
}

#[tokio::test]
async fn chess_com_backfill_resolves_an_empty_direct_archive_scan() {
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(EmptyEligibleClient),
        Arc::new(NoopExecutor),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    let connected = runtime
        .connect_at(
            &player_id,
            ConnectPlayingProfileRequest {
                profile_url: "https://www.chess.com/member/PlayerOne".to_string(),
                timezone: Some(midday_fixed_timezone(&now)),
            },
            now - TimeDelta::days(1),
        )
        .await;

    let report = runtime.tick(now).await.unwrap();
    let application = application_with_runtime(runtime, Arc::new(NoopExecutor));
    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;

    assert!(matches!(
        connected,
        ConnectPlayingProfileOutcome::Completed {
            provider: crate::daily_coaching::DailyCoachingProvider::ChessCom,
            ..
        }
    ));
    assert_eq!(report.no_digest, 1);
    assert_eq!(dashboard.1["lead"], json!({ "kind": "noEligibleGamesYet" }));
    assert_eq!(dashboard.1["archive"], json!([]));
}

/// Connects both providers at `now`, ticks once, and returns the profile-feed
/// call count with the dashboard body.
///
/// `now` is a parameter because the call count depends on the calendar. Reading
/// it from the wall clock made this assertion pass or fail by the date the
/// suite happened to run — it failed every first of the month, when the
/// previous local day straddles a UTC month boundary. The profile client was
/// already a pure in-memory fake; the clock was the only live input.
async fn empty_backfill_calls(now: DateTime<Utc>) -> (usize, Value) {
    let client = Arc::new(CountingEmptyEligibleClient::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        client.clone(),
        Arc::new(NoopExecutor),
    );
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    for profile_url in [
        "https://lichess.org/@/PlayerOne",
        "https://www.chess.com/member/PlayerTwo",
    ] {
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: profile_url.to_string(),
                    timezone: Some(midday_fixed_timezone(&now)),
                },
                now - TimeDelta::days(1),
            )
            .await;
    }

    let report = runtime.tick(now).await.unwrap();
    let application = application_with_runtime(runtime, Arc::new(NoopExecutor));
    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;

    assert_eq!(report.no_digest, 1);
    assert_eq!(report.pending_selection, 0);
    assert_eq!(dashboard.1["lead"], json!({ "kind": "noEligibleGamesYet" }));
    assert_eq!(dashboard.1["connections"].as_array().unwrap().len(), 2);
    (client.calls.load(Ordering::SeqCst), dashboard.1)
}

#[tokio::test]
async fn both_supported_providers_resolve_an_empty_daily_window_and_backfill() {
    // Mid-month: the previous local day and the backfill floor both sit inside
    // one archive month. One Lichess daily window, one Lichess backfill page,
    // one Chess.com daily-window month, one Chess.com backfill month.
    let now = "2026-06-15T18:00:00Z".parse::<DateTime<Utc>>().unwrap();

    let (calls, _dashboard) = empty_backfill_calls(now).await;

    assert_eq!(
        calls, 4,
        "each provider reads one daily window, and Chess.com adds one backfill month"
    );
}

#[tokio::test]
async fn a_daily_window_straddling_a_month_reads_both_archive_months() {
    // `midday_fixed_timezone` puts local noon at `now`, so an evening `now`
    // gives a timezone behind UTC. The previous local day is then 31 August
    // local, which runs from 31 August into 1 September in UTC — two archive
    // months for the daily window, and two more for a backfill floor that has
    // fallen back into August.
    let now = "2026-09-01T18:00:00Z".parse::<DateTime<Utc>>().unwrap();

    let (calls, _dashboard) = empty_backfill_calls(now).await;

    assert_eq!(
        calls, 6,
        "a straddling window and a straddling floor each cost one extra Chess.com month"
    );
}

#[tokio::test]
async fn multi_profile_backfill_and_later_supported_daily_window_both_publish() {
    let executor = Arc::new(PublishingExecutor);
    let client = Arc::new(MultiProfileBackfillClient::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        client.clone(),
        executor.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    for profile_url in [
        "https://lichess.org/@/PlayerOne",
        "https://www.chess.com/member/PlayerTwo",
    ] {
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: profile_url.to_string(),
                    timezone: Some(midday_fixed_timezone(&now)),
                },
                now - TimeDelta::days(1),
            )
            .await;
    }

    let first = runtime.tick(now).await.unwrap();
    let second = runtime.tick(now + TimeDelta::days(1)).await.unwrap();
    let application = application_with_runtime(runtime, executor);
    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;

    assert_eq!(first.published, 1);
    assert_eq!(second.published, 1);
    assert_eq!(client.backfill_calls.load(Ordering::SeqCst), 1);
    assert_eq!(client.daily_calls.load(Ordering::SeqCst), 2);
    assert_eq!(dashboard.1["lead"]["kind"], "digest");
    assert_eq!(dashboard.1["archive"][0]["gameCount"], 1);
    assert_eq!(dashboard.1["archive"][1]["gameCount"], 2);
}

#[tokio::test]
async fn a_bounded_backfill_page_checkpoints_and_resumes_on_the_next_run() {
    let executor = Arc::new(PublishingExecutor);
    let client = Arc::new(CheckpointingBackfillClient::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        client.clone(),
        executor.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    runtime
        .connect_at(
            &player_id,
            ConnectPlayingProfileRequest {
                profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                timezone: Some(midday_fixed_timezone(&now)),
            },
            now - TimeDelta::days(1),
        )
        .await;

    let first = runtime.tick(now).await.unwrap();
    assert_eq!(first.no_digest, 1);
    assert_eq!(client.backfill_calls.load(Ordering::SeqCst), 1);

    let second = runtime.tick(now + TimeDelta::days(1)).await.unwrap();
    let application = application_with_runtime(runtime, executor);
    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;

    assert_eq!(second.published, 1);
    assert_eq!(client.backfill_calls.load(Ordering::SeqCst), 2);
    let backfill_urls = client.backfill_urls.lock().unwrap();
    assert!(!backfill_urls[0].contains("until="));
    assert!(backfill_urls[1].contains("until=701"));
    assert_eq!(dashboard.1["lead"]["kind"], "digest");
    assert_eq!(dashboard.1["archive"][0]["gameCount"], 1);
}

#[derive(Default)]
struct InitialBackfillClient {
    latest_calls: AtomicUsize,
}

#[derive(Default)]
struct AlwaysSaturatedBackfillClient {
    backfill_calls: AtomicUsize,
    daily_calls: AtomicUsize,
}

impl ProfileGameClient for AlwaysSaturatedBackfillClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = reqwest::Url::parse(request.url()).expect("the profile request URL is valid");
            let is_daily = url.query_pairs().all(|(name, _)| name != "max");
            let games = if is_daily {
                let call = self.daily_calls.fetch_add(1, Ordering::SeqCst);
                let until = url
                    .query_pairs()
                    .find_map(|(name, value)| (name == "until").then_some(value))
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                (0_u64..10)
                    .map(|index| (format!("D{call}{index:06}"), until - index))
                    .collect::<Vec<_>>()
            } else {
                self.backfill_calls.fetch_add(1, Ordering::SeqCst);
                (0_u64..5)
                    .map(|index| (format!("BF{index:06}"), 700 - index))
                    .collect::<Vec<_>>()
            };
            Ok(ProfileGameResponse {
                body: lichess_games_body(&games),
                content_type: request.accept().to_string(),
            })
        })
    }
}

#[derive(Default)]
struct TerminalBackfillClient {
    backfill_calls: AtomicUsize,
}

impl ProfileGameClient for TerminalBackfillClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = reqwest::Url::parse(request.url()).expect("the profile request URL is valid");
            let body = if url.query_pairs().all(|(name, _)| name != "max") {
                Vec::new()
            } else {
                self.backfill_calls.fetch_add(1, Ordering::SeqCst);
                lichess_games_body(&[("TERM0001".to_string(), 700)])
            };
            Ok(ProfileGameResponse {
                body,
                content_type: request.accept().to_string(),
            })
        })
    }
}

#[derive(Default)]
struct CountingTerminalExecutor {
    calls: AtomicUsize,
}

impl ReviewSessionCommandExecutor for CountingTerminalExecutor {
    fn submit(
        self: Arc<Self>,
        _principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let envelope = admission
            .envelope()
            .expect("Daily Coaching submits a valid command");
        let (sender, receiver) = mpsc::unbounded_channel();
        sender
            .send(ReviewSessionEventEnvelope {
                request_id: envelope.request_id.clone(),
                operation_id: envelope.operation_id.clone(),
                sequence: 0,
                event: ReviewSessionEvent::Rejected {
                    operation: OperationKind::GameImport,
                    reason: CommandRejectionReason::InvalidCommand,
                    recovery: RejectionRecovery::None,
                },
            })
            .unwrap();
        receiver
    }
}

impl ProfileGameClient for InitialBackfillClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = reqwest::Url::parse(request.url()).expect("the profile request URL is valid");
            let until = url.query_pairs().find_map(|(name, value)| {
                (name == "until").then(|| {
                    value
                        .parse::<u64>()
                        .expect("the window upper bound is milliseconds")
                })
            });
            let games = if let Some(until) = until {
                (1_u64..=10)
                    .map(|index| (format!("DA{index:06}aaaa"), until - index * 60 * 60 * 1_000))
                    .collect::<Vec<_>>()
            } else {
                self.latest_calls.fetch_add(1, Ordering::SeqCst);
                let newest = u64::try_from(Utc::now().timestamp_millis()).unwrap()
                    - 20 * 24 * 60 * 60 * 1_000;
                std::iter::once(("DA000001aaaa".to_string(), newest))
                    .chain((1_u64..=5).map(|index| {
                        (
                            format!("BF{index:06}aaaa"),
                            newest - index * 60 * 60 * 1_000,
                        )
                    }))
                    .collect::<Vec<_>>()
            };
            Ok(ProfileGameResponse {
                body: lichess_games_body(&games),
                content_type: request.accept().to_string(),
            })
        })
    }
}

struct EmptyEligibleClient;

impl ProfileGameClient for EmptyEligibleClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            Ok(ProfileGameResponse {
                body: empty_profile_body(request),
                content_type: request.accept().to_string(),
            })
        })
    }
}

#[derive(Default)]
struct CountingEmptyEligibleClient {
    calls: AtomicUsize,
}

#[derive(Default)]
struct MultiProfileBackfillClient {
    backfill_calls: AtomicUsize,
    daily_calls: AtomicUsize,
}

impl ProfileGameClient for MultiProfileBackfillClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = reqwest::Url::parse(request.url()).expect("the profile request URL is valid");
            if url.host_str() == Some("api.chess.com") {
                return Ok(ProfileGameResponse {
                    body: br#"{"games":[]}"#.to_vec(),
                    content_type: request.accept().to_string(),
                });
            }
            let games = if url.query_pairs().all(|(name, _)| name != "max") {
                let call = self.daily_calls.fetch_add(1, Ordering::SeqCst) + 1;
                let until = url
                    .query_pairs()
                    .find_map(|(name, value)| (name == "until").then_some(value))
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                vec![(format!("MULTIDA{call}"), until)]
            } else {
                self.backfill_calls.fetch_add(1, Ordering::SeqCst);
                vec![("MULTIBF1".to_string(), 700)]
            };
            Ok(ProfileGameResponse {
                body: lichess_games_body(&games),
                content_type: request.accept().to_string(),
            })
        })
    }
}

impl ProfileGameClient for CountingEmptyEligibleClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(ProfileGameResponse {
                body: empty_profile_body(request),
                content_type: request.accept().to_string(),
            })
        })
    }
}

#[derive(Default)]
struct CheckpointingBackfillClient {
    backfill_calls: AtomicUsize,
    backfill_urls: Mutex<Vec<String>>,
}

impl ProfileGameClient for CheckpointingBackfillClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = reqwest::Url::parse(request.url()).expect("the profile request URL is valid");
            let is_backfill = url
                .query_pairs()
                .any(|(name, value)| name == "max" && value == "300");
            let body = if is_backfill {
                let call = self.backfill_calls.fetch_add(1, Ordering::SeqCst);
                self.backfill_urls
                    .lock()
                    .unwrap()
                    .push(request.url().to_string());
                if call == 0 {
                    let games = (0_u64..300)
                        .map(|index| (format!("X{index:07}"), 1_000 - index))
                        .collect::<Vec<_>>();
                    lichess_games_body_with_variant(&games, "chess960")
                } else {
                    lichess_games_body(&[("ELIG0001".to_string(), 701)])
                }
            } else {
                Vec::new()
            };
            Ok(ProfileGameResponse {
                body,
                content_type: request.accept().to_string(),
            })
        })
    }
}

fn lichess_games_body(games: &[(String, u64)]) -> Vec<u8> {
    lichess_games_body_with_variant(games, "standard")
}

fn empty_profile_body(request: &ProfileGameRequest) -> Vec<u8> {
    if request.url().starts_with("https://api.chess.com/") {
        br#"{"games":[]}"#.to_vec()
    } else {
        Vec::new()
    }
}

fn lichess_games_body_with_variant(games: &[(String, u64)], variant: &str) -> Vec<u8> {
    games
        .iter()
        .map(|(id, ended_at)| {
            json!({
                "id": id,
                "variant": variant,
                "status": "mate",
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(90),
                "lastMoveAt": ended_at,
                "players": {
                    "white": { "userId": "Opponent" },
                    "black": { "userId": "PlayerOne" }
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}
