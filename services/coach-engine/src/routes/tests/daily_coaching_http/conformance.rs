use super::*;

#[tokio::test]
async fn one_through_ten_eligible_games_are_all_included_in_the_published_digest() {
    for game_count in 1..=10 {
        let executor = Arc::new(PublishingExecutor);
        let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
            Arc::new(FakeProfileValidator::default()),
            "UTC",
            Arc::new(EligibleGameCountClient { game_count }),
            executor.clone(),
        );
        let now = Utc::now();
        let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
        assert!(matches!(
            runtime
                .connect_at(
                    &player_id,
                    ConnectPlayingProfileRequest {
                        profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                        timezone: Some(midday_fixed_timezone(&now)),
                    },
                    now - TimeDelta::days(1),
                )
                .await,
            ConnectPlayingProfileOutcome::Completed { .. }
        ));
        assert_eq!(runtime.tick(now).await.unwrap().published, 1);
        let application = application_with_runtime(runtime, executor);
        let dashboard = request(
            &application,
            Method::GET,
            "/api/v1/daily-coaching/dashboard",
            Value::Null,
        )
        .await;
        let digest_id = dashboard.1["lead"]["digestId"].as_str().unwrap();

        let digest = request(
            &application,
            Method::GET,
            &format!("/api/v1/daily-coaching/digests/{digest_id}"),
            Value::Null,
        )
        .await;

        assert_eq!(digest.0, StatusCode::OK);
        assert_eq!(digest.1["gameCount"], u64::try_from(game_count).unwrap());
        assert_eq!(digest.1["games"].as_array().unwrap().len(), game_count);
    }
}

struct EligibleGameCountClient {
    game_count: usize,
}

impl ProfileGameClient for EligibleGameCountClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = reqwest::Url::parse(request.url()).expect("the profile request URL is valid");
            let until = url
                .query_pairs()
                .find_map(|(name, value)| (name == "until").then_some(value))
                .map(|value| {
                    value
                        .parse::<u64>()
                        .expect("the window upper bound is milliseconds")
                })
                .unwrap_or_else(|| u64::try_from(Utc::now().timestamp_millis()).unwrap());
            let body = (0..self.game_count)
                .map(|index| {
                    json!({
                        "id": format!("{index:08}ABCD"),
                        "variant": "standard",
                        "status": "mate",
                        "speed": "rapid",
                        "clock": { "initial": 600, "increment": 0 },
                        "moves": lichess_moves(90),
                        "lastMoveAt": until
                            - u64::try_from(index + 1).unwrap() * 60 * 60 * 1_000,
                        "players": {
                            "white": { "userId": "Opponent" },
                            "black": { "userId": "PlayerOne" }
                        }
                    })
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes();
            Ok(ProfileGameResponse {
                body,
                content_type: request.accept().to_string(),
            })
        })
    }
}
