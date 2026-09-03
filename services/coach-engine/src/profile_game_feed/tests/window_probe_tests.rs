use super::*;

#[tokio::test]
async fn returns_a_reviewable_game_with_frozen_selection_facts() {
    let client = ScriptedClient::new([ProfileGameResponse {
        body: format!(
            r#"{{"id":"abcdefgh","variant":"standard","status":"mate","lastMoveAt":2000,"speed":"rapid","clock":{{"initial":600,"increment":5}},"moves":"{}","players":{{"white":{{"userId":"Player_1"}},"black":{{"user":{{"name":"Opponent"}}}}}}}}"#,
            lichess_moves(73),
        )
        .into_bytes(),
        content_type: "application/x-ndjson".to_string(),
    }]);

    let entries = ProfileGameFeed::new(client)
        .eligible_games_in_window(
            "https://lichess.org/@/Player_1/all/",
            DateTime::from_timestamp_millis(1_000).unwrap(),
            DateTime::from_timestamp_millis(3_000).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.source_identity.canonical_key(), "lichess:abcdefgh");
    assert_eq!(entry.source_profile, "https://lichess.org/@/Player_1");
    assert_eq!(entry.ended_at_unix_milliseconds, 2_000);
    assert_eq!(entry.time_control_raw, "600+5");
    assert_eq!(entry.time_control_class, ProfileGameTimeControlClass::Rapid);
    assert_eq!(entry.expected_clock_seconds, Some(800));
    assert_eq!(entry.played_plies, 73);
    assert_eq!(
        entry.review_request,
        DailyGameReviewRequest {
            source: DailyGameInputSource::LichessUrl {
                url: "https://lichess.org/abcdefgh".to_string(),
            },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
            elo_profile: RequestedEloProfile::FromImportedMetadata,
            ended_at_unix_milliseconds: Some(2_000),
        }
    );
}

#[tokio::test]
async fn scans_older_lichess_pages_until_the_initial_backfill_is_resolved() {
    let first_page = (0..300)
        .map(|index| {
            serde_json::json!({
                "id": format!("X{index:07}"),
                "variant": "chess960",
                "status": "mate",
                "lastMoveAt": 1_000_u64 - u64::try_from(index).unwrap(),
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(73),
                "players": {
                    "white": { "userId": "Player_1" },
                    "black": { "userId": "Opponent" }
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let client = ScriptedClient::new([
        ProfileGameResponse {
            body: first_page,
            content_type: "application/x-ndjson".to_string(),
        },
        ProfileGameResponse {
            body: format!(
                r#"{{"id":"abcdefgh","variant":"standard","status":"mate","lastMoveAt":600,"speed":"rapid","clock":{{"initial":600,"increment":5}},"moves":"{}","players":{{"white":{{"userId":"Player_1"}},"black":{{"userId":"Opponent"}}}}}}"#,
                lichess_moves(73),
            )
            .into_bytes(),
            content_type: "application/x-ndjson".to_string(),
        },
    ]);

    let feed = ProfileGameFeed::new(client.clone());
    let first = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Continue { games, cursor } = first else {
        panic!("a full ineligible page must checkpoint the scan")
    };
    assert!(games.is_empty());
    let second = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            Some(&cursor),
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Complete(entries) = second else {
        panic!("the short second page exhausts the archive")
    };

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source_identity.game_id, "abcdefgh");
    assert_eq!(client.request_urls().len(), 2);
    assert!(client.request_urls()[0].contains("max=300"));
    assert!(client.request_urls()[1].contains("until=701"));
}

#[tokio::test]
async fn a_full_partially_eligible_page_continues_until_the_backfill_is_resolved() {
    let first_page = (0..300)
        .map(|index| {
            serde_json::json!({
                "id": format!("X{index:07}"),
                "variant": if index < 4 { "standard" } else { "chess960" },
                "status": "mate",
                "lastMoveAt": 1_000_u64 - u64::try_from(index).unwrap(),
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(73),
                "players": {
                    "white": { "userId": "Player_1" },
                    "black": { "userId": "Opponent" }
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let client = ScriptedClient::new([
        ProfileGameResponse {
            body: first_page,
            content_type: "application/x-ndjson".to_string(),
        },
        ProfileGameResponse {
            body: format!(
                r#"{{"id":"abcdefgh","variant":"standard","status":"mate","lastMoveAt":600,"speed":"rapid","clock":{{"initial":600,"increment":5}},"moves":"{}","players":{{"white":{{"userId":"Player_1"}},"black":{{"userId":"Opponent"}}}}}}"#,
                lichess_moves(73),
            )
            .into_bytes(),
            content_type: "application/x-ndjson".to_string(),
        },
    ]);
    let feed = ProfileGameFeed::new(client.clone());

    let first = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Continue { games, cursor } = first else {
        panic!("a full page below the requested count must continue")
    };
    assert_eq!(games.len(), 4);
    let second = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(1).unwrap(),
            Some(&cursor),
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Complete(games) = second else {
        panic!("the fifth game on a short page must complete the scan")
    };

    assert_eq!(games.len(), 1);
    assert_eq!(games[0].source_identity.game_id, "abcdefgh");
    assert!(client.request_urls()[1].contains("until=701"));
}

#[tokio::test]
async fn rejects_a_lichess_backfill_page_that_ignores_its_upper_bound() {
    let page = (0..300)
        .map(|index| {
            serde_json::json!({
                "id": format!("X{index:07}"),
                "variant": "chess960",
                "status": "mate",
                "lastMoveAt": 1_000_u64 - u64::try_from(index).unwrap(),
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(73),
                "players": {
                    "white": { "userId": "Player_1" },
                    "black": { "userId": "Opponent" }
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let client = ScriptedClient::new([
        ProfileGameResponse {
            body: page.clone(),
            content_type: "application/x-ndjson".to_string(),
        },
        ProfileGameResponse {
            body: page,
            content_type: "application/x-ndjson".to_string(),
        },
    ]);

    let feed = ProfileGameFeed::new(client);
    let first = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Continue { cursor, .. } = first else {
        panic!("a full first page must checkpoint")
    };
    let error = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            Some(&cursor),
        )
        .await
        .unwrap_err();

    assert_eq!(error, ProfileGameFeedError::MalformedProviderResponse);
}

#[tokio::test]
async fn rejects_a_lichess_backfill_page_above_the_record_budget() {
    let page = (0..301)
        .map(|index| {
            serde_json::json!({
                "id": format!("X{index:07}"),
                "variant": "chess960",
                "status": "mate",
                "lastMoveAt": 1_000_u64 - u64::try_from(index).unwrap(),
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(73),
                "players": {
                    "white": { "userId": "Player_1" },
                    "black": { "userId": "Opponent" }
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let client = ScriptedClient::new([ProfileGameResponse {
        body: page,
        content_type: "application/x-ndjson".to_string(),
    }]);

    let error = ProfileGameFeed::new(client)
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
        )
        .await
        .unwrap_err();

    assert_eq!(error, ProfileGameFeedError::MalformedProviderResponse);
}

#[tokio::test]
async fn a_repeated_full_boundary_page_stalls_instead_of_looping() {
    let identities = (0..300)
        .map(|index| ProfileGameSourceIdentity::lichess(format!("X{index:07}")))
        .collect::<Vec<_>>();
    let page = identities
        .iter()
        .map(|identity| {
            serde_json::json!({
                "id": identity.game_id,
                "variant": "chess960",
                "status": "mate",
                "lastMoveAt": 700,
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(73),
                "players": {
                    "white": { "userId": "Player_1" },
                    "black": { "userId": "Opponent" }
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let client = ScriptedClient::new([ProfileGameResponse {
        body: page,
        content_type: "application/x-ndjson".to_string(),
    }]);
    let cursor = RecentProfileGameCursor::test(700, identities);

    let page = ProfileGameFeed::new(client)
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            Some(&cursor),
        )
        .await
        .unwrap();

    assert!(matches!(page, RecentProfileGameScanPage::Stalled(games) if games.is_empty()));
}

#[tokio::test]
async fn preserves_unseen_games_tied_at_the_lichess_page_boundary() {
    let first_page = (0..299)
        .map(|index| {
            serde_json::json!({
                "id": format!("X{index:07}"),
                "variant": "chess960",
                "status": "mate",
                "lastMoveAt": 1_000_u64 - u64::try_from(index).unwrap(),
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(73),
                "players": {
                    "white": { "userId": "Player_1" },
                    "black": { "userId": "Opponent" }
                }
            })
            .to_string()
        })
        .chain(std::iter::once(
            serde_json::json!({
                "id": "BOUND001",
                "variant": "chess960",
                "status": "mate",
                "lastMoveAt": 700,
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(73),
                "players": {
                    "white": { "userId": "Player_1" },
                    "black": { "userId": "Opponent" }
                }
            })
            .to_string(),
        ))
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let second_page = br#"{"id":"BOUND001","variant":"chess960","status":"mate","lastMoveAt":700,"speed":"rapid","clock":{"initial":600,"increment":0},"moves":"e4 e5 Nf3","players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}
{"id":"ELIG0001","variant":"standard","status":"mate","lastMoveAt":700,"speed":"rapid","clock":{"initial":600,"increment":0},"moves":"e4 e5 Nf3","players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}"#
        .to_vec();
    let client = ScriptedClient::new([
        ProfileGameResponse {
            body: first_page,
            content_type: "application/x-ndjson".to_string(),
        },
        ProfileGameResponse {
            body: second_page,
            content_type: "application/x-ndjson".to_string(),
        },
    ]);
    let feed = ProfileGameFeed::new(client.clone());

    let first = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Continue { cursor, .. } = first else {
        panic!("the full first page must checkpoint")
    };
    let second = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            Some(&cursor),
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Complete(entries) = second else {
        panic!("the short tied page must exhaust")
    };

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source_identity.game_id, "ELIG0001");
    assert!(client.request_urls()[1].contains("until=700"));
}

#[tokio::test]
async fn collects_the_complete_cutoff_tie_before_canonical_backfill_truncation() {
    let page = [
        "ZZZZ0001", "ZZZZ0002", "ZZZZ0003", "ZZZZ0004", "ZZZZ0005", "AAAA0001",
    ]
    .into_iter()
    .map(|id| {
        serde_json::json!({
            "id": id,
            "variant": "standard",
            "status": "mate",
            "lastMoveAt": 700,
            "speed": "rapid",
            "clock": { "initial": 600, "increment": 0 },
            "moves": lichess_moves(73),
            "players": {
                "white": { "userId": "Player_1" },
                "black": { "userId": "Opponent" }
            }
        })
        .to_string()
    })
    .chain((0_u64..294).map(|index| {
        serde_json::json!({
            "id": format!("X{index:07}"),
            "variant": "chess960",
            "status": "mate",
            "lastMoveAt": 699 - index,
            "speed": "rapid",
            "clock": { "initial": 600, "increment": 0 },
            "moves": lichess_moves(73),
            "players": {
                "white": { "userId": "Player_1" },
                "black": { "userId": "Opponent" }
            }
        })
        .to_string()
    }))
    .collect::<Vec<_>>()
    .join("\n")
    .into_bytes();
    let client = ScriptedClient::new([ProfileGameResponse {
        body: page,
        content_type: "application/x-ndjson".to_string(),
    }]);

    let page = ProfileGameFeed::new(client)
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Complete(entries) = page else {
        panic!("reaching the cap must complete the bounded scan")
    };
    assert_eq!(entries.len(), 6);

    let resolved = crate::daily_coaching::selection::resolve_initial_backfill(entries).unwrap();
    assert_eq!(
        resolved
            .iter()
            .map(|entry| entry.source_identity.game_id.as_str())
            .collect::<Vec<_>>(),
        vec!["AAAA0001", "ZZZZ0001", "ZZZZ0002", "ZZZZ0003", "ZZZZ0004"]
    );
}

#[tokio::test]
async fn exhausts_a_full_page_cutoff_tie_before_canonical_backfill_truncation() {
    let first_page = (0_u64..295)
        .map(|index| {
            serde_json::json!({
                "id": format!("X{index:07}"),
                "variant": "chess960",
                "status": "mate",
                "lastMoveAt": 1_000 - index,
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(73),
                "players": {
                    "white": { "userId": "Player_1" },
                    "black": { "userId": "Opponent" }
                }
            })
            .to_string()
        })
        .chain((1..=5).map(|index| {
            serde_json::json!({
                "id": format!("ZZZZ000{index}"),
                "variant": "standard",
                "status": "mate",
                "lastMoveAt": 700,
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(73),
                "players": {
                    "white": { "userId": "Player_1" },
                    "black": { "userId": "Opponent" }
                }
            })
            .to_string()
        }))
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let second_page = br#"{"id":"ZZZZ0005","variant":"standard","status":"mate","lastMoveAt":700,"speed":"rapid","clock":{"initial":600,"increment":0},"moves":"e4 e5 Nf3","players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}
{"id":"AAAA0001","variant":"standard","status":"mate","lastMoveAt":700,"speed":"rapid","clock":{"initial":600,"increment":0},"moves":"e4 e5 Nf3","players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}
{"id":"OLDER001","variant":"chess960","status":"mate","lastMoveAt":699,"speed":"rapid","clock":{"initial":600,"increment":0},"moves":"e4 e5 Nf3","players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}"#
        .to_vec();
    let client = ScriptedClient::new([
        ProfileGameResponse {
            body: first_page,
            content_type: "application/x-ndjson".to_string(),
        },
        ProfileGameResponse {
            body: second_page,
            content_type: "application/x-ndjson".to_string(),
        },
    ]);
    let feed = ProfileGameFeed::new(client.clone());

    let first = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Continue {
        games: mut candidates,
        cursor,
    } = first
    else {
        panic!("a full page ending at the cutoff tie must continue")
    };
    assert_eq!(candidates.len(), 5);
    let second = feed
        .scan_latest_eligible_games(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(1).unwrap(),
            Some(&cursor),
        )
        .await
        .unwrap();
    let RecentProfileGameScanPage::Complete(found) = second else {
        panic!("observing an older timestamp must exhaust the cutoff tie")
    };
    candidates.extend(found);

    let resolved = crate::daily_coaching::selection::resolve_initial_backfill(candidates).unwrap();
    assert_eq!(
        resolved
            .iter()
            .map(|entry| entry.source_identity.game_id.as_str())
            .collect::<Vec<_>>(),
        vec!["AAAA0001", "ZZZZ0001", "ZZZZ0002", "ZZZZ0003", "ZZZZ0004"]
    );
    assert!(client.request_urls()[1].contains("until=700"));
}

#[tokio::test]
async fn excludes_every_ineligible_lichess_window_game_before_selection() {
    let client = ScriptedClient::new([ProfileGameResponse {
        body: br#"{"id":"variant1","variant":"chess960","status":"mate","lastMoveAt":2000,"speed":"rapid","clock":{"initial":600,"increment":0},"moves":"e4 e5 Nf3","players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}
{"id":"aborted1","variant":"standard","status":"aborted","lastMoveAt":2100,"speed":"rapid","clock":{"initial":600,"increment":0},"moves":"e4 e5 Nf3","players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}
{"id":"noclock1","variant":"standard","status":"resign","lastMoveAt":2200,"speed":"rapid","moves":"e4 e5 Nf3","players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}
{"id":"daily001","variant":"standard","status":"draw","lastMoveAt":2300,"speed":"correspondence","moves":"e4 e5 Nf3","players":{"white":{"userId":"Opponent"},"black":{"userId":"Player_1"}}}"#
            .to_vec(),
        content_type: "application/x-ndjson".to_string(),
    }]);

    let entries = ProfileGameFeed::new(client)
        .eligible_games_in_window(
            "https://lichess.org/@/Player_1",
            DateTime::from_timestamp_millis(1_000).unwrap(),
            DateTime::from_timestamp_millis(3_000).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source_identity.game_id, "daily001");
    assert_eq!(
        entries[0].time_control_class,
        ProfileGameTimeControlClass::Correspondence
    );
    assert_eq!(entries[0].time_control_raw, "correspondence");
    assert_eq!(entries[0].expected_clock_seconds, None);
}

#[tokio::test]
async fn rejects_a_provider_game_outside_the_exact_lichess_window() {
    let client = ScriptedClient::new([ProfileGameResponse {
        body: br#"{"id":"abcdefgh","variant":"standard","status":"mate","lastMoveAt":3000,"speed":"rapid","clock":{"initial":600,"increment":0},"moves":"e4 e5 Nf3","players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}"#
            .to_vec(),
        content_type: "application/x-ndjson".to_string(),
    }]);

    let error = ProfileGameFeed::new(client)
        .eligible_games_in_window(
            "https://lichess.org/@/Player_1",
            DateTime::from_timestamp_millis(1_000).unwrap(),
            DateTime::from_timestamp_millis(3_000).unwrap(),
        )
        .await
        .unwrap_err();

    assert_eq!(error, ProfileGameFeedError::MalformedProviderResponse);
}

#[tokio::test]
async fn derives_played_plies_from_the_requested_lichess_moves() {
    let client = ScriptedClient::new([ProfileGameResponse {
        body: format!(
            r#"{{"id":"abcdefgh","variant":"standard","status":"mate","lastMoveAt":2000,"speed":"rapid","clock":{{"initial":600,"increment":0}},"moves":"{}","players":{{"white":{{"userId":"Player_1"}},"black":{{"userId":"Opponent"}}}}}}"#,
            lichess_moves(84),
        )
        .into_bytes(),
        content_type: "application/x-ndjson".to_string(),
    }]);

    let entries = ProfileGameFeed::new(client)
        .eligible_games_in_window(
            "https://lichess.org/@/Player_1",
            DateTime::from_timestamp_millis(1_000).unwrap(),
            DateTime::from_timestamp_millis(3_000).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(entries[0].played_plies, 84);
}

#[tokio::test]
async fn rejects_a_lichess_game_that_omits_the_requested_moves() {
    let client = ScriptedClient::new([ProfileGameResponse {
        body: br#"{"id":"abcdefgh","variant":"standard","status":"mate","lastMoveAt":2000,"speed":"rapid","clock":{"initial":600,"increment":0},"players":{"white":{"userId":"Player_1"},"black":{"userId":"Opponent"}}}"#
            .to_vec(),
        content_type: "application/x-ndjson".to_string(),
    }]);

    let error = ProfileGameFeed::new(client)
        .eligible_games_in_window(
            "https://lichess.org/@/Player_1",
            DateTime::from_timestamp_millis(1_000).unwrap(),
            DateTime::from_timestamp_millis(3_000).unwrap(),
        )
        .await
        .unwrap_err();

    assert_eq!(error, ProfileGameFeedError::MalformedProviderResponse);
}

#[tokio::test]
async fn bounds_the_lichess_initial_backfill_scan_to_the_last_two_weeks() {
    let client = ScriptedClient::new([ProfileGameResponse {
        body: Vec::new(),
        content_type: "application/x-ndjson".to_string(),
    }]);
    let as_of = DateTime::from_timestamp_millis(1_800_000_000_000).unwrap();

    ProfileGameFeed::new(client.clone())
        .scan_latest_eligible_games_at(
            "https://lichess.org/@/Player_1",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
            as_of,
        )
        .await
        .unwrap();

    let requested = client.request_urls();
    assert_eq!(requested.len(), 1);
    assert!(
        requested[0].contains("&since=1798790400000"),
        "the backfill scan reaches back exactly two weeks: {}",
        requested[0]
    );
    assert!(requested[0].contains("&moves=true"), "{}", requested[0]);
}

#[test]
fn source_identity_owns_the_provider_neutral_canonical_key() {
    assert_eq!(
        ProfileGameSourceIdentity::lichess("abcdefgh".to_string()).canonical_key(),
        "lichess:abcdefgh"
    );
    assert_eq!(
        ProfileGameSourceIdentity::chess_com("daily:42".to_string()).canonical_key(),
        "chessCom:daily:42"
    );
}
