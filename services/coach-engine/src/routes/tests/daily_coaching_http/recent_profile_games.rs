use super::*;

#[tokio::test]
async fn no_playing_profile_is_a_typed_outcome() {
    let application = application(Arc::new(FakeProfileValidator::default()));

    let read = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/recent-profile-games",
        Value::Null,
    )
    .await;
    assert_eq!(read.0, StatusCode::OK);
    assert_eq!(read.1, json!({ "outcome": "noPlayingProfile" }));
}

#[tokio::test]
async fn connected_profile_returns_recent_games_without_importing() {
    let client = Arc::new(RecordingLatestClient::default());
    let application = application_with_runtime(
        DailyCoachingRuntime::in_memory_with_pipeline(
            Arc::new(FakeProfileValidator::default()),
            "UTC",
            client.clone(),
            Arc::new(NoopExecutor),
        ),
        Arc::new(NoopExecutor),
    );

    let connected = request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections",
        json!({
            "profileUrl": "https://lichess.org/@/PlayerOne",
            "timezone": "UTC"
        }),
    )
    .await;
    assert_eq!(connected.0, StatusCode::OK);

    let read = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/recent-profile-games",
        Value::Null,
    )
    .await;
    assert_eq!(read.0, StatusCode::OK);
    assert_eq!(
        read.1,
        json!({
            "outcome": "found",
            "games": [
                {
                    "source": "https://lichess.org/abcdefgh",
                    "reviewSide": "white",
                    "provider": "lichess",
                    "endedAtUnixMilliseconds": 2000
                },
                {
                    "source": "https://lichess.org/hgfedcba",
                    "reviewSide": "black",
                    "provider": "lichess",
                    "endedAtUnixMilliseconds": 1000
                }
            ]
        })
    );

    let imported = request(
        &application,
        Method::GET,
        "/api/v1/imported-games",
        Value::Null,
    )
    .await;
    assert_eq!(imported.0, StatusCode::OK);
    assert_eq!(imported.1["games"], json!([]));

    let urls = client.request_urls();
    assert!(
        urls.iter().any(|url| {
            url.starts_with("https://lichess.org/api/games/user/PlayerOne?")
                && url.contains("sort=dateDesc")
        }),
        "the read must reuse the recent-games feed, got {urls:?}"
    );
    assert!(
        urls.iter()
            .all(|url| !url.contains("/api/v1/reviewed-games")),
        "reviewed-Game search is not a substitute, got {urls:?}"
    );
}

#[tokio::test]
async fn repeat_reads_serve_the_cached_outcome_without_a_second_provider_fetch() {
    let client = Arc::new(RecordingLatestClient::default());
    let application = application_with_runtime(
        DailyCoachingRuntime::in_memory_with_pipeline(
            Arc::new(FakeProfileValidator::default()),
            "UTC",
            client.clone(),
            Arc::new(NoopExecutor),
        ),
        Arc::new(NoopExecutor),
    );
    request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections",
        json!({
            "profileUrl": "https://lichess.org/@/PlayerOne",
            "timezone": "UTC"
        }),
    )
    .await;

    let first = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/recent-profile-games",
        Value::Null,
    )
    .await;
    let fetches_after_first = client.request_urls().len();
    let second = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/recent-profile-games",
        Value::Null,
    )
    .await;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(first.1, second.1);
    assert_eq!(
        client.request_urls().len(),
        fetches_after_first,
        "the second read within the TTL must not hit the provider again"
    );
}

#[tokio::test]
async fn connected_profile_with_no_recent_games_is_found_empty() {
    let application = application(Arc::new(FakeProfileValidator::default()));
    request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections",
        json!({
            "profileUrl": "https://lichess.org/@/PlayerOne",
            "timezone": "UTC"
        }),
    )
    .await;

    let read = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/recent-profile-games",
        Value::Null,
    )
    .await;
    assert_eq!(read.0, StatusCode::OK);
    assert_eq!(read.1, json!({ "outcome": "found", "games": [] }));
}

#[tokio::test]
async fn provider_failure_is_unavailable_not_an_empty_error() {
    let client = Arc::new(ControllableProfileGameClient(AtomicUsize::new(1)));
    let application = application_with_runtime(
        DailyCoachingRuntime::in_memory_with_pipeline(
            Arc::new(FakeProfileValidator::default()),
            "UTC",
            client,
            Arc::new(NoopExecutor),
        ),
        Arc::new(NoopExecutor),
    );
    request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections",
        json!({
            "profileUrl": "https://lichess.org/@/PlayerOne",
            "timezone": "UTC"
        }),
    )
    .await;

    let read = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/recent-profile-games",
        Value::Null,
    )
    .await;
    assert_eq!(read.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(read.1["outcome"], "unavailable");
    assert_eq!(read.1["reason"], "providerUnreachable");
    assert_eq!(
        read.1["retry"],
        json!({ "kind": "retryAfter", "seconds": 120 })
    );
}

#[derive(Default)]
struct RecordingLatestClient {
    requests: std::sync::Mutex<Vec<String>>,
}

impl RecordingLatestClient {
    fn request_urls(&self) -> Vec<String> {
        self.requests.lock().expect("request log").clone()
    }
}

impl ProfileGameClient for RecordingLatestClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        self.requests
            .lock()
            .expect("request log")
            .push(request.url().to_string());
        Box::pin(async move {
            Ok(ProfileGameResponse {
                body: br#"{"id":"abcdefgh","variant":"standard","status":"mate","lastMoveAt":2000,"players":{"white":{"userId":"playerone"},"black":{"user":{"name":"Opponent"}}}}
{"id":"hgfedcba","variant":"standard","status":"resign","lastMoveAt":1000,"players":{"white":{"user":{"name":"Opponent"}},"black":{"userId":"playerone"}}}"#
                    .to_vec(),
                content_type: request.accept().to_string(),
            })
        })
    }
}
