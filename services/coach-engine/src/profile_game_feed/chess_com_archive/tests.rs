use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use chrono::TimeDelta;
use serde_json::{json, Value};

use super::*;
use crate::profile_game_feed::{ProfileGameFetchError, ProfileGameResponse};

const PGN: &str = r#"[Event "Live Chess"]
[Site "Chess.com"]
[Date "2026.07.31"]
[Round "-"]
[White "MixedCase"]
[Black "Opponent"]
[Result "0-1"]
[WhiteElo "1200"]
[BlackElo "1300"]

1. f3 e5 2. g4 Qh4# 0-1"#;

#[tokio::test]
async fn reads_the_two_boundary_months_and_carries_archive_provenance() {
    let starts_at = instant("2026-07-31T00:00:00Z");
    let ends_at = instant("2026-08-02T00:00:00Z");
    let july_body = archive_body([
        game(101, starts_at.timestamp() as u64, PGN),
        game(
            102,
            (starts_at + TimeDelta::hours(1)).timestamp() as u64,
            PGN,
        ),
        game(
            103,
            (starts_at - TimeDelta::seconds(1)).timestamp() as u64,
            PGN,
        ),
    ]);
    let august_body = archive_body([
        game(
            104,
            (ends_at - TimeDelta::seconds(1)).timestamp() as u64,
            PGN,
        ),
        game(105, ends_at.timestamp() as u64, PGN),
    ]);
    let client = ScriptedClient::new([
        Ok(response(july_body.clone())),
        Ok(response(august_body.clone())),
    ]);

    let games = ProfileGameFeed::new(client.clone())
        .eligible_games_in_window("https://www.chess.com/member/MixedCase", starts_at, ends_at)
        .await
        .unwrap();

    assert_eq!(
        client.request_urls(),
        vec![
            "https://api.chess.com/pub/player/mixedcase/games/2026/07",
            "https://api.chess.com/pub/player/mixedcase/games/2026/08",
        ]
    );
    assert_eq!(games.len(), 3);
    assert!(games.iter().all(ProfileGameWindowEntry::is_valid));
    let july_digest = archive_response_digest(&july_body).unwrap();
    let july_sources = games
        .iter()
        .filter_map(|game| match &game.review_request.source {
            DailyGameInputSource::ChessComArchive {
                pgn,
                captured_at,
                response_digest,
                ..
            } if game.ended_at_unix_milliseconds
                < u64::try_from(instant("2026-08-01T00:00:00Z").timestamp_millis()).unwrap() =>
            {
                Some((pgn, captured_at, response_digest))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(july_sources.len(), 2);
    assert!(july_sources
        .iter()
        .all(|(pgn, _, digest)| { pgn.as_str() == PGN && *digest == &july_digest }));
    assert_eq!(july_sources[0].1, july_sources[1].1);
}

#[tokio::test]
async fn daily_games_are_correspondence_candidates_without_a_clock_rank() {
    let window_start = instant("2026-07-01T00:00:00Z");
    let end_time = (window_start + TimeDelta::hours(1)).timestamp() as u64;
    let client = ScriptedClient::new([Ok(response(archive_body([
        daily_game(201, end_time, "1/86400"),
        daily_game(202, end_time, "1/1209600"),
    ])))]);

    let games = ProfileGameFeed::new(client)
        .eligible_games_in_window(
            "https://www.chess.com/member/MixedCase",
            window_start,
            instant("2026-08-01T00:00:00Z"),
        )
        .await
        .unwrap();

    assert_eq!(games.len(), 2);
    assert!(games.iter().all(ProfileGameWindowEntry::is_valid));
    assert!(games.iter().all(|game| {
        game.time_control_class == ProfileGameTimeControlClass::Correspondence
            && game.time_control_raw == "correspondence"
            && game.expected_clock_seconds.is_none()
    }));
    assert_eq!(
        games
            .iter()
            .map(|game| game.source_identity.canonical_key())
            .collect::<Vec<_>>(),
        vec!["chessCom:daily:201", "chessCom:daily:202"]
    );
}

#[tokio::test]
async fn skips_ineligible_archive_records_without_losing_valid_games() {
    let window_start = instant("2026-07-01T00:00:00Z");
    let end_time = (window_start + TimeDelta::hours(1)).timestamp() as u64;
    let mut wrong_rules = game(201, end_time, PGN);
    wrong_rules["rules"] = json!("chess960");
    let invalid_daily = daily_game(202, end_time, "not-a-daily-control");
    let mut player_abandoned = game(203, end_time, PGN);
    player_abandoned["black"]["result"] = json!("abandoned");
    let mut missing_pgn = game(204, end_time, PGN);
    missing_pgn.as_object_mut().unwrap().remove("pgn");
    let mut malformed_pgn = game(205, end_time, "not pgn");
    malformed_pgn["pgn"] = json!("not pgn");
    let abandoned_pgn = PGN.replace(
        "[BlackElo \"1300\"]",
        "[BlackElo \"1300\"]\n[Termination \"Opponent won - game abandoned\"]",
    );

    let client = ScriptedClient::new([Ok(response(archive_body([
        wrong_rules,
        invalid_daily,
        player_abandoned,
        missing_pgn,
        malformed_pgn,
        game(206, end_time, &abandoned_pgn),
        game(207, end_time, PGN),
    ])))]);

    let games = ProfileGameFeed::new(client)
        .eligible_games_in_window(
            "https://www.chess.com/member/MixedCase",
            window_start,
            instant("2026-08-01T00:00:00Z"),
        )
        .await
        .unwrap();

    assert_eq!(games.len(), 1);
    assert_eq!(
        games[0].source_identity.canonical_key(),
        "chessCom:live:207"
    );
}

#[tokio::test]
async fn initial_backfill_reads_direct_current_month_without_the_archive_index() {
    let as_of = instant("2026-08-12T12:00:00Z");
    let games = (301..=305).map(|id| game(id, as_of.timestamp() as u64, PGN));
    let client = ScriptedClient::new([Ok(response(archive_body(games)))]);

    let page = ProfileGameFeed::new(client.clone())
        .scan_latest_eligible_games_at(
            "https://www.chess.com/member/MixedCase",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
            as_of,
        )
        .await
        .unwrap();

    assert_eq!(
        client.request_urls(),
        vec!["https://api.chess.com/pub/player/mixedcase/games/2026/08"]
    );
    let RecentProfileGameScanPage::Complete(games) = page else {
        panic!("Chess.com initial backfill should complete in one direct scan");
    };
    assert_eq!(games.len(), 5);
}

#[tokio::test]
async fn initial_backfill_drops_archive_games_older_than_two_weeks() {
    let as_of = instant("2026-08-12T12:00:00Z");
    let inside = as_of - TimeDelta::days(13);
    let outside = as_of - TimeDelta::days(15);
    let august = archive_body([game(301, as_of.timestamp() as u64, PGN)]);
    let july = archive_body([
        game(302, inside.timestamp() as u64, PGN),
        game(303, outside.timestamp() as u64, PGN),
    ]);
    let client = ScriptedClient::new([Ok(response(august)), Ok(response(july))]);

    let page = ProfileGameFeed::new(client.clone())
        .scan_latest_eligible_games_at(
            "https://www.chess.com/member/MixedCase",
            RecentProfileGameCount::try_from(5).unwrap(),
            None,
            as_of,
        )
        .await
        .unwrap();

    assert_eq!(
        client.request_urls(),
        vec![
            "https://api.chess.com/pub/player/mixedcase/games/2026/08",
            "https://api.chess.com/pub/player/mixedcase/games/2026/07",
        ],
        "the traversal stops at the month holding the two-week floor"
    );
    let RecentProfileGameScanPage::Complete(games) = page else {
        panic!("Chess.com initial backfill should complete once the floor month is read");
    };
    assert_eq!(
        games
            .iter()
            .map(|game| game.source_identity.game_id.as_str())
            .collect::<Vec<_>>(),
        vec!["live:301", "live:302"]
    );
}

fn game(id: u64, end_time: u64, pgn: &str) -> Value {
    json!({
        "url": format!("https://www.chess.com/game/live/{id}"),
        "pgn": pgn,
        "rules": "chess",
        "time_class": "rapid",
        "time_control": "600+5",
        "end_time": end_time,
        "white": { "username": "MixedCase", "result": "checkmated" },
        "black": { "username": "Opponent", "result": "win" }
    })
}

fn daily_game(id: u64, end_time: u64, time_control: &str) -> Value {
    let mut value = game(id, end_time, PGN);
    value["url"] = json!(format!("https://www.chess.com/game/daily/{id}"));
    value["time_class"] = json!("daily");
    value["time_control"] = json!(time_control);
    value
}

fn archive_body(games: impl IntoIterator<Item = Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({ "games": games.into_iter().collect::<Vec<_>>() })).unwrap()
}

fn response(body: Vec<u8>) -> ProfileGameResponse {
    ProfileGameResponse {
        body,
        content_type: "application/json".to_string(),
    }
}

fn instant(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

#[derive(Clone)]
struct ScriptedClient {
    responses: Arc<Mutex<VecDeque<Result<ProfileGameResponse, ProfileGameFetchError>>>>,
    requests: Arc<Mutex<Vec<String>>>,
}

impl ScriptedClient {
    fn new(
        responses: impl IntoIterator<Item = Result<ProfileGameResponse, ProfileGameFetchError>>,
    ) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            requests: Arc::new(Mutex::new(Vec::new())),
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
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        self.requests
            .lock()
            .unwrap()
            .push(request.url().to_string());
        let response = self.responses.lock().unwrap().pop_front().unwrap();
        Box::pin(async move { response })
    }
}
