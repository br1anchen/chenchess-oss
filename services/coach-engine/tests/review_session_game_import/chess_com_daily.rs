use super::*;

#[tokio::test]
async fn shared_chess_com_daily_game_builds_a_canonical_imported_game() {
    let chess_com = FakeChessComClient::daily_pvp_game();
    let requests = Arc::clone(&chess_com.requests);
    let importer =
        GameImporter::with_chess_com(FakeLichessClient::with_canonical_capture(), chess_com);

    let snapshot = importer
        .import(
            &GameInputSource::ChessComUrl {
                url: "https://www.chess.com/game/daily/100000000002".to_string(),
            },
            RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
            &RequestedEloProfile::FromImportedMetadata,
        )
        .await
        .expect("public completed Chess.com Daily PvP Game should import");

    assert_eq!(snapshot.review_side, ReviewSide::White);
    assert_eq!(snapshot.elo_profile.rating.value(), 1458);
    let ImportProvenance::ChessCom {
        canonical_game_id,
        canonical_url,
        fetch_contract_version,
        ..
    } = snapshot.provenance
    else {
        panic!("Chess.com Daily URL should retain Chess.com provenance")
    };
    assert_eq!(canonical_game_id.as_str(), "100000000002");
    assert_eq!(
        canonical_url,
        "https://www.chess.com/game/daily/100000000002"
    );
    assert_eq!(fetch_contract_version, "chess-com-daily-game-callback/v1");
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url(),
        "https://www.chess.com/callback/daily/game/100000000002"
    );
    assert_eq!(requests[0].accept(), CHESS_COM_JSON_MEDIA_TYPE);
}

#[tokio::test]
async fn daily_import_requires_days_per_turn_instead_of_the_live_discriminator() {
    let importer = GameImporter::with_chess_com(
        FakeLichessClient::with_canonical_capture(),
        FakeChessComClient::live_pvp_game(),
    );

    assert!(matches!(
        importer
            .import(
                &GameInputSource::ChessComUrl {
                    url: "https://www.chess.com/game/daily/100000000001".to_string(),
                },
                RequestedReviewSide::Selected {
                    review_side: ReviewSide::White,
                },
                &RequestedEloProfile::FromImportedMetadata,
            )
            .await,
        Err(GameImportError::ChessCom(
            ChessComImportError::MalformedResponse
        ))
    ));
}

impl FakeChessComClient {
    fn daily_pvp_game() -> Self {
        let body = serde_json::to_vec(&serde_json::json!({
            "game": {
                "id": 100000000002_u64,
                "initialSetup": "",
                "isFinished": true,
                "daysPerTurn": 3,
                "gameEndReason": "resigned",
                "moveList": "gv1Tow0KlB5Qmu9zbszsjs!0fo8!egZRcj6EdeKCvlXHoCTLCQ0QBJQ0sA",
                "pgnHeaders": {
                    "Event": "Daily Chess",
                    "Site": "Chess.com",
                    "Date": "2025.04.12",
                    "White": "synthetic-white",
                    "Black": "nbank22",
                    "Result": "1-0",
                    "ECO": "A04",
                    "WhiteElo": 1458,
                    "BlackElo": 1400,
                    "TimeControl": "1/259200",
                    "EndTime": "14:06:30 GMT+0000",
                    "Termination": "synthetic-white won by resignation",
                    "SetUp": "1",
                    "FEN": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
                },
                "plyCount": 29,
                "type": "chess"
            },
            "players": {
                "top": { "isComputer": false },
                "bottom": { "isComputer": false }
            }
        }))
        .unwrap();
        Self {
            response: Ok(ChessComGameResponse {
                body,
                content_type: CHESS_COM_JSON_MEDIA_TYPE.to_string(),
                captured_at: "2026-08-09T17:24:58Z".parse().unwrap(),
            }),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
