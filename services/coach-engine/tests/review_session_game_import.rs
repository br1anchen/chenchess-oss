use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
};

use chen_chess_coach_engine::{
    chess_com::{
        chess_com_game_url_pattern, ChessComGameClient, ChessComGameFetchError, ChessComGameKind,
        ChessComGameRequest, ChessComGameResponse, ChessComGameUrl, CHESS_COM_JSON_MEDIA_TYPE,
    },
    chess_com_import::ChessComImportError,
    game_import::{GameImportError, GameImporter},
    lichess::{
        LichessExportClient, LichessExportError, LichessExportRequest, LichessExportResponse,
        LichessGameUrl, LichessSide, LICHESS_PGN_MEDIA_TYPE,
    },
    lichess_import::LichessImportError,
    review_session_contract::{
        CommandRejectionReason, EloRating, EloSource, GameInputSource, ImportProgressStage,
        ImportProvenance, ImportedGame, MetadataText, OpeningCatalogVersion,
        OpeningIdentificationProvenance, OpeningMetadata, OpeningServiceAttribution,
        OpeningServiceProvider, OperationKind, ProviderUnavailableReason, RatingMetadata,
        RejectionRecovery, RequestedEloProfile, RequestedReviewSide, RetryDirective,
        ReviewSessionCommand, ReviewSessionEvent, ReviewSide,
    },
};
use regex::Regex;

#[path = "review_session_game_import/chess_com_daily.rs"]
mod chess_com_daily;

#[tokio::test]
async fn shared_chess_com_computer_game_builds_a_canonical_imported_game() {
    let chess_com = FakeChessComClient::lorenzo_game();
    let requests = Arc::clone(&chess_com.requests);
    let importer =
        GameImporter::with_chess_com(FakeLichessClient::with_canonical_capture(), chess_com);

    let snapshot = importer
        .import(
            &GameInputSource::ChessComUrl {
                url: "https://www.chess.com/game/computer/1403674481".to_string(),
            },
            RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
            &RequestedEloProfile::FromImportedMetadata,
        )
        .await
        .expect("public completed Chess.com computer Game should import");

    assert_eq!(snapshot.review_side, ReviewSide::White);
    assert_eq!(snapshot.elo_profile.rating.value(), 636);
    assert_eq!(snapshot.game.moves.len(), 80);
    assert_eq!(
        snapshot
            .game
            .moves
            .iter()
            .take(6)
            .map(|game_move| game_move.uci.as_str())
            .collect::<Vec<_>>(),
        vec!["g1f3", "d7d5", "g2g3", "d8d6", "f1g2", "b8c6"]
    );
    assert_eq!(snapshot.game.moves.last().unwrap().uci.as_str(), "c2c1q");
    assert_eq!(snapshot.game.moves.last().unwrap().san.as_str(), "c1=Q");
    assert_eq!(
        snapshot.game.white.name,
        MetadataText::Present {
            value: "synthetic-white".to_string(),
        }
    );
    assert_eq!(
        snapshot.game.black.name,
        MetadataText::Present {
            value: "Lorenzo-BOT".to_string(),
        }
    );
    assert_eq!(
        snapshot.game.opening,
        OpeningMetadata::Present {
            eco: "A07".to_string(),
            name: "King's Indian Attack".to_string(),
            provenance: OpeningIdentificationProvenance::Service {
                provider: OpeningServiceProvider::ChessCom,
                attribution: OpeningServiceAttribution::DirectImport,
            },
        }
    );
    let ImportProvenance::ChessCom {
        canonical_game_id,
        canonical_url,
        fetch_contract_version,
        ..
    } = snapshot.provenance
    else {
        panic!("Chess.com URL should retain Chess.com provenance")
    };
    assert_eq!(canonical_game_id.as_str(), "1403674481");
    assert_eq!(
        canonical_url,
        "https://www.chess.com/game/computer/1403674481"
    );
    assert_eq!(
        fetch_contract_version,
        "chess-com-computer-game-callback/v1"
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url(),
        "https://www.chess.com/computer/callback/game/1403674481"
    );
    assert_eq!(requests[0].accept(), CHESS_COM_JSON_MEDIA_TYPE);
}

#[tokio::test]
async fn shared_chess_com_live_game_builds_a_canonical_imported_game() {
    let chess_com = FakeChessComClient::live_pvp_game();
    let requests = Arc::clone(&chess_com.requests);
    let importer =
        GameImporter::with_chess_com(FakeLichessClient::with_canonical_capture(), chess_com);

    let snapshot = importer
        .import(
            &GameInputSource::ChessComUrl {
                url: "https://www.chess.com/game/live/100000000001".to_string(),
            },
            RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
            &RequestedEloProfile::FromImportedMetadata,
        )
        .await
        .expect("public completed Chess.com live PvP Game should import");

    assert_eq!(snapshot.review_side, ReviewSide::White);
    assert_eq!(snapshot.elo_profile.rating.value(), 681);
    assert_eq!(snapshot.game.moves.len(), 29);
    assert_eq!(snapshot.game.moves.last().unwrap().uci.as_str(), "c3c4");
    assert_eq!(snapshot.game.moves.last().unwrap().san.as_str(), "c4");
    assert_eq!(
        snapshot.game.white.name,
        MetadataText::Present {
            value: "synthetic-white".to_string(),
        }
    );
    assert_eq!(
        snapshot.game.black.name,
        MetadataText::Present {
            value: "nbank22".to_string(),
        }
    );
    let ImportProvenance::ChessCom {
        canonical_game_id,
        canonical_url,
        fetch_contract_version,
        ..
    } = snapshot.provenance
    else {
        panic!("Chess.com URL should retain Chess.com provenance")
    };
    assert_eq!(canonical_game_id.as_str(), "100000000001");
    assert_eq!(
        canonical_url,
        "https://www.chess.com/game/live/100000000001"
    );
    assert_eq!(fetch_contract_version, "chess-com-live-game-callback/v1");
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url(),
        "https://www.chess.com/callback/live/game/100000000001"
    );
    assert_eq!(requests[0].accept(), CHESS_COM_JSON_MEDIA_TYPE);
}

#[test]
fn chess_com_shared_urls_accept_only_the_supported_fixed_origin_forms() {
    let published = Regex::new(&chess_com_game_url_pattern()).unwrap();

    for (url, id, kind) in [
        (
            "https://www.chess.com/game/computer/1403674481",
            "1403674481",
            ChessComGameKind::Computer,
        ),
        (
            "https://www.chess.com/game/daily/100000000002",
            "100000000002",
            ChessComGameKind::Daily,
        ),
        (
            "https://www.chess.com/game/live/100000000001",
            "100000000001",
            ChessComGameKind::Live,
        ),
    ] {
        let source = ChessComGameUrl::parse(url).unwrap();
        assert_eq!(source.canonical_game_id(), id);
        assert_eq!(source.canonical_url(), url);
        assert_eq!(source.kind(), kind);
        assert!(
            published.is_match(url),
            "the Engine imports {url} but the published pattern refuses it"
        );
    }

    for invalid in [
        "http://www.chess.com/game/computer/1403674481",
        "https://chess.com/game/computer/1403674481",
        "https://www.chess.com/game/computer/",
        "https://www.chess.com/game/computer/0",
        "https://www.chess.com/game/computer/01403674481",
        "https://www.chess.com/game/computer/1403674481/",
        "https://www.chess.com/game/computer/1403674481?move=1",
        "https://www.chess.com/game/computer/1403674481#analysis",
        "https://www.chess.com/game/computer/%31%34%30%33%36%37%34%34%38%31",
        "https://www.chess.com/game/daily/",
        "https://www.chess.com/game/daily/0",
        "https://www.chess.com/game/daily/0100000000002",
        "https://www.chess.com/game/daily/100000000002/",
        "https://www.chess.com/game/daily/100000000002?move=1",
        "https://www.chess.com/game/live/",
        "https://www.chess.com/game/live/0",
        "https://www.chess.com/game/live/0100000000001",
        "https://www.chess.com/game/live/100000000001/",
        "https://www.chess.com/game/live/100000000001?move=1",
    ] {
        assert!(
            ChessComGameUrl::parse(invalid).is_err(),
            "accepted {invalid}"
        );
        assert!(
            !published.is_match(invalid),
            "the published pattern admits {invalid}, which the Engine rejects"
        );
    }

    // The published pattern is a client-side pre-filter, not a second parser.
    // It admits an id too large for the Engine to canonicalise, so a Player
    // naming one reaches the Engine and gets the typed rejection and its
    // recovery instead of a host-side schema mismatch.
    let beyond_engine_range = "https://www.chess.com/game/live/99999999999999999999999";
    assert!(ChessComGameUrl::parse(beyond_engine_range).is_err());
    assert!(published.is_match(beyond_engine_range));
}

#[tokio::test]
async fn chess_com_import_requires_a_selected_single_review_side() {
    let chess_com = FakeChessComClient::lorenzo_game();
    let requests = Arc::clone(&chess_com.requests);
    let importer =
        GameImporter::with_chess_com(FakeLichessClient::with_canonical_capture(), chess_com);
    let source = GameInputSource::ChessComUrl {
        url: "https://www.chess.com/game/computer/1403674481".to_string(),
    };

    for review_side in [
        RequestedReviewSide::Required,
        RequestedReviewSide::FromQualifiedUrl,
        RequestedReviewSide::Selected {
            review_side: ReviewSide::Both,
        },
    ] {
        assert!(matches!(
            importer
                .import(
                    &source,
                    review_side,
                    &RequestedEloProfile::FromImportedMetadata,
                )
                .await,
            Err(GameImportError::ReviewSideRequired)
        ));
    }
    assert!(requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn chess_com_import_rejects_a_payload_from_the_wrong_game_kind() {
    let importer = GameImporter::with_chess_com(
        FakeLichessClient::with_canonical_capture(),
        FakeChessComClient::live_pvp_game(),
    );

    assert!(matches!(
        importer
            .import(
                &GameInputSource::ChessComUrl {
                    url: "https://www.chess.com/game/computer/100000000001".to_string(),
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

#[tokio::test]
async fn captured_lichess_export_builds_the_canonical_imported_game() {
    let client = FakeLichessClient::with_canonical_capture();
    let requests = client.requests.clone();
    let importer = GameImporter::new(client);
    let command = ReviewSessionCommand::ImportGame {
        source: GameInputSource::LichessUrl {
            url: "https://lichess.org/Synthet1Demo/black".to_string(),
        },
        review_side: RequestedReviewSide::FromQualifiedUrl,
        elo_profile: RequestedEloProfile::FromImportedMetadata,
    };
    let ReviewSessionCommand::ImportGame {
        source,
        review_side,
        elo_profile,
    } = command
    else {
        unreachable!()
    };

    let snapshot = importer
        .import(&source, review_side, &elo_profile)
        .await
        .expect("captured Lichess Game should import");
    let expected: ImportedGame = fixture("imported-game.json");
    assert_eq!(snapshot, expected);

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url(),
        "https://lichess.org/game/export/Synthet1?clocks=false&evals=false&accuracy=false&literate=false&opening=true"
    );
    assert_eq!(requests[0].accept(), LICHESS_PGN_MEDIA_TYPE);
}

#[test]
fn lichess_game_urls_accept_only_the_supported_fixed_origin_forms() {
    let qualified = LichessGameUrl::parse("https://lichess.org/Synthet1Demo/black").unwrap();
    assert_eq!(qualified.canonical_game_id(), "Synthet1");
    assert_eq!(qualified.side(), Some(LichessSide::Black));
    assert_eq!(qualified.canonical_url(), "https://lichess.org/Synthet1");

    for bare in [
        "https://lichess.org/Synthet1",
        "https://lichess.org/Synthet1Demo",
    ] {
        let source = LichessGameUrl::parse(bare).unwrap();
        assert_eq!(source.canonical_game_id(), "Synthet1");
        assert_eq!(source.side(), None);
    }

    for invalid in [
        "http://lichess.org/Synthet1",
        "https://www.lichess.org/Synthet1",
        "https://lichess.org/85SQH9d",
        "https://lichess.org/Synthet1/analysis",
        "https://lichess.org/Synthet1/black/extra",
        "https://lichess.org/Synthet1/",
        "https://lichess.org/Synthet1?color=black",
        "https://lichess.org:443/Synthet1",
        "https://lichess.org/%38%35SQH9do",
        "https://lichess.org/Synthet1/%62lack",
        "https://lichess.org/game/export/Synthet1",
    ] {
        assert!(
            LichessGameUrl::parse(invalid).is_err(),
            "accepted {invalid}"
        );
    }
}

#[tokio::test]
async fn bare_urls_require_white_or_black_while_qualified_urls_preselect_the_side() {
    let client = FakeLichessClient::with_canonical_capture();
    let requests = client.requests.clone();
    let importer = GameImporter::new(client);
    let bare = GameInputSource::LichessUrl {
        url: "https://lichess.org/Synthet1".to_string(),
    };
    let missing_side = importer
        .import(
            &bare,
            RequestedReviewSide::Required,
            &RequestedEloProfile::FromImportedMetadata,
        )
        .await;
    assert!(matches!(
        missing_side,
        Err(GameImportError::ReviewSideRequired)
    ));
    assert!(requests.lock().unwrap().is_empty());

    let selected = importer
        .import(
            &bare,
            RequestedReviewSide::Selected {
                review_side: ReviewSide::Black,
            },
            &RequestedEloProfile::FromImportedMetadata,
        )
        .await
        .unwrap();
    let ImportProvenance::Lichess {
        side_qualified_url,
        canonical_url,
        ..
    } = selected.provenance
    else {
        panic!("bare Lichess URL should retain Lichess provenance")
    };
    assert_eq!(side_qualified_url, "https://lichess.org/Synthet1/black");
    assert_eq!(canonical_url, "https://lichess.org/Synthet1");

    let qualified = GameInputSource::LichessUrl {
        url: "https://lichess.org/Synthet1Demo/black".to_string(),
    };
    let preselected = importer
        .import(
            &qualified,
            RequestedReviewSide::FromQualifiedUrl,
            &RequestedEloProfile::FromImportedMetadata,
        )
        .await
        .unwrap();
    assert_eq!(preselected.review_side, ReviewSide::Black);

    let overridden = importer
        .import(
            &qualified,
            RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
            &RequestedEloProfile::FromImportedMetadata,
        )
        .await
        .unwrap();
    assert_eq!(overridden.review_side, ReviewSide::White);
    assert_eq!(overridden.elo_profile.rating.value(), 1245);
}

#[tokio::test]
async fn missing_or_unsupported_rating_metadata_requires_a_player_supplied_profile() {
    for replacement in [None, Some("99")] {
        let raw = String::from_utf8(canonical_raw_export()).unwrap();
        let rating =
            replacement.map_or(String::new(), |rating| format!("[BlackElo \"{rating}\"]\n"));
        let body = raw.replace("[BlackElo \"1246\"]\n", &rating).into_bytes();
        let client = FakeLichessClient::new(body);
        let importer = GameImporter::new(client);
        let source = GameInputSource::LichessUrl {
            url: "https://lichess.org/Synthet1Demo/black".to_string(),
        };

        let metadata_result = importer
            .import(
                &source,
                RequestedReviewSide::FromQualifiedUrl,
                &RequestedEloProfile::FromImportedMetadata,
            )
            .await;
        assert!(matches!(
            metadata_result,
            Err(GameImportError::EloProfileRequired)
        ));

        let supplied = importer
            .import(
                &source,
                RequestedReviewSide::FromQualifiedUrl,
                &RequestedEloProfile::PlayerProvided {
                    rating: EloRating::try_from(1400).unwrap(),
                },
            )
            .await
            .unwrap();
        assert_eq!(supplied.elo_profile.rating.value(), 1400);
        assert_eq!(supplied.elo_profile.source, EloSource::PlayerProvided);
        assert_eq!(supplied.game.black.rating, RatingMetadata::Absent);
    }
}

#[tokio::test]
async fn pasted_and_local_pgn_build_equivalent_typed_game_material() {
    let private_marker = "PRIVATE_PGN_COMMENT_MUST_NOT_LEAVE_THE_IMPORT_BOUNDARY";
    let pgn = fs::read_to_string(canonical_fixture_root().join("lichess-export.pgn"))
        .unwrap()
        .replacen("1. e4", &format!("1. e4 {{{private_marker}}}"), 1);
    let client = FakeLichessClient::with_canonical_capture();
    let requests = client.requests.clone();
    let importer = GameImporter::new(client);
    let elo = RequestedEloProfile::PlayerProvided {
        rating: EloRating::try_from(1246).unwrap(),
    };
    let local_path = temporary_pgn_path();
    fs::write(&local_path, &pgn).unwrap();

    for side in [ReviewSide::White, ReviewSide::Black, ReviewSide::Both] {
        let review_side = RequestedReviewSide::Selected { review_side: side };
        let pasted = importer
            .import(
                &GameInputSource::PastedPgn { pgn: pgn.clone() },
                review_side,
                &elo,
            )
            .await
            .unwrap();
        let local = importer
            .import(
                &GameInputSource::LocalPgnFile {
                    path: local_path.to_string_lossy().into_owned(),
                },
                review_side,
                &elo,
            )
            .await
            .unwrap();

        assert_eq!(pasted.game.moves, local.game.moves);
        assert_eq!(
            pasted.game.final_position_ref,
            local.game.final_position_ref
        );
        assert_eq!(pasted.game.outcome, local.game.outcome);
        assert_eq!(pasted.review_side, side);
        assert_eq!(pasted.review_side, local.review_side);
        assert_eq!(pasted.elo_profile, local.elo_profile);
        assert!(!serde_json::to_string(&local)
            .unwrap()
            .contains(private_marker));
        assert!(matches!(
            pasted.provenance,
            ImportProvenance::PastedPgn { .. }
        ));
        assert!(matches!(
            local.provenance,
            ImportProvenance::LocalPgn { .. }
        ));

        assert_eq!(pasted.game.white.name, MetadataText::Absent);
        assert_eq!(pasted.game.black.name, MetadataText::Absent);
        assert_eq!(pasted.game.event, MetadataText::Absent);
        assert_eq!(pasted.game.site, MetadataText::Absent);
        assert!(matches!(
            local.game.white.name,
            MetadataText::Present { .. }
        ));
        let expected_opening = OpeningMetadata::Present {
            eco: "A00".to_string(),
            name: "Saragossa Opening".to_string(),
            provenance: OpeningIdentificationProvenance::Service {
                provider: OpeningServiceProvider::Lichess,
                attribution: OpeningServiceAttribution::PgnUrl {
                    canonical_url: "https://lichess.org/Synthet1".to_string(),
                },
            },
        };
        assert_eq!(pasted.game.opening, expected_opening);
        assert_eq!(local.game.opening, expected_opening);
    }
    fs::remove_file(local_path).unwrap();
    assert!(requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn pgn_opening_metadata_requires_exactly_one_supported_service_url() {
    let base = fs::read_to_string(canonical_fixture_root().join("lichess-export.pgn")).unwrap();
    let importer = GameImporter::new(FakeLichessClient::with_canonical_capture());
    let review_side = RequestedReviewSide::Selected {
        review_side: ReviewSide::Black,
    };
    let elo = RequestedEloProfile::PlayerProvided {
        rating: EloRating::try_from(1246).unwrap(),
    };

    let attributed = base
        .replacen(
            "[Site \"https://lichess.org/Synthet1\"]",
            "[Site \"Local\"]\n[Link \"https://www.chess.com/game/live/100000000001\"]",
            1,
        )
        .replacen(
            "[Opening \"Saragossa Opening\"]",
            "[Opening \"Trusted Header\"]",
            1,
        );
    let attributed = importer
        .import(
            &GameInputSource::PastedPgn { pgn: attributed },
            review_side,
            &elo,
        )
        .await
        .unwrap();
    assert_eq!(
        attributed.game.opening,
        OpeningMetadata::Present {
            eco: "A00".to_string(),
            name: "Trusted Header".to_string(),
            provenance: OpeningIdentificationProvenance::Service {
                provider: OpeningServiceProvider::ChessCom,
                attribution: OpeningServiceAttribution::PgnUrl {
                    canonical_url: "https://www.chess.com/game/live/100000000001".to_string(),
                },
            },
        }
    );

    for pgn in [
        base.replacen(
            "[Site \"https://lichess.org/Synthet1\"]",
            "[Site \"Local\"]",
            1,
        ),
        base.replacen(
            "[Site \"https://lichess.org/Synthet1\"]",
            "[Site \"https://lichess.org/Synthet1\"]\n[Link \"https://www.chess.com/game/live/100000000001\"]",
            1,
        ),
    ] {
        let pgn = pgn.replacen(
            "[Opening \"Saragossa Opening\"]",
            "[Opening \"Untrusted Header\"]",
            1,
        );
        let imported = importer
            .import(
                &GameInputSource::PastedPgn { pgn },
                review_side,
                &elo,
            )
            .await
            .unwrap();
        assert!(matches!(
            imported.game.opening,
            OpeningMetadata::Present {
                provenance: OpeningIdentificationProvenance::Catalog {
                    catalog_version: OpeningCatalogVersion::V2026_04_16,
                    ..
                },
                ..
            }
        ));
        assert!(!serde_json::to_string(&imported)
            .unwrap()
            .contains("Untrusted Header"));
    }
}

#[tokio::test]
async fn incomplete_lichess_opening_metadata_falls_back_to_the_pinned_catalog() {
    let raw = String::from_utf8(canonical_raw_export()).unwrap();
    let body = raw
        .replace("[ECO \"A00\"]\n", "")
        .replace("[Opening \"Saragossa Opening\"]\n", "")
        .into_bytes();
    let importer = GameImporter::new(FakeLichessClient::new(body));

    let snapshot = importer
        .import(
            &GameInputSource::LichessUrl {
                url: "https://lichess.org/Synthet1/black".to_string(),
            },
            RequestedReviewSide::FromQualifiedUrl,
            &RequestedEloProfile::FromImportedMetadata,
        )
        .await
        .expect("incomplete opening metadata falls back without failing import");

    assert!(matches!(
        snapshot.game.opening,
        OpeningMetadata::Present {
            provenance: OpeningIdentificationProvenance::Catalog {
                catalog_version: OpeningCatalogVersion::V2026_04_16,
                ..
            },
            ..
        }
    ));
}

#[tokio::test]
async fn mutated_canonical_exports_return_the_typed_reviewability_failure() {
    let raw = String::from_utf8(canonical_raw_export()).unwrap();
    let mut oversized = canonical_raw_export();
    oversized.resize(512 * 1024 + 1, b' ');
    let cases = [
        (
            raw.replacen("[Result \"0-1\"]", "[Result \"*\"]", 1)
                .into_bytes(),
            LichessImportError::OngoingGame,
        ),
        (
            raw.replacen("[Result \"0-1\"]", "[Result \"*\"]", 1)
                .replacen("[Termination \"Normal\"]", "[Termination \"Aborted\"]", 1)
                .into_bytes(),
            LichessImportError::AbortedGame,
        ),
        (
            raw.replacen("[Variant \"Standard\"]", "[Variant \"Chess960\"]", 1)
                .into_bytes(),
            LichessImportError::UnsupportedVariant,
        ),
        (
            raw.replacen("[GameId \"Synthet1\"]", "[GameId \"abcdefgh\"]", 1)
                .into_bytes(),
            LichessImportError::MalformedResponse,
        ),
        (
            [canonical_raw_export(), canonical_raw_export()].concat(),
            LichessImportError::InvalidPgn,
        ),
        (
            raw.replacen("1. c3", "1. not-a-move", 1).into_bytes(),
            LichessImportError::InvalidPgn,
        ),
        (oversized, LichessImportError::ResponseTooLarge),
    ];

    for (body, expected) in cases {
        let importer = GameImporter::new(FakeLichessClient::new(body));
        let error = importer
            .import(
                &canonical_lichess_source(),
                RequestedReviewSide::FromQualifiedUrl,
                &RequestedEloProfile::FromImportedMetadata,
            )
            .await
            .expect_err("mutated export should not be reviewable");

        assert!(
            matches!(error, GameImportError::Lichess(ref actual) if *actual == expected),
            "unexpected error: {error}"
        );
    }
}

#[tokio::test]
async fn missing_and_private_games_are_rejected_before_import_work() {
    for (status, expected_reason) in [
        (404, CommandRejectionReason::GameNotFound),
        (403, CommandRejectionReason::PrivateGame),
    ] {
        let importer = GameImporter::new(FakeLichessClient::failure(LichessExportError::Status {
            code: status,
            retry_after_seconds: None,
        }));
        let error = importer
            .import(
                &canonical_lichess_source(),
                RequestedReviewSide::FromQualifiedUrl,
                &RequestedEloProfile::FromImportedMetadata,
            )
            .await
            .unwrap_err();
        let terminal = error.terminal();

        assert_eq!(
            terminal.event(),
            &ReviewSessionEvent::Rejected {
                operation: OperationKind::GameImport,
                reason: expected_reason,
                recovery: RejectionRecovery::CorrectInput,
            }
        );
        assert_eq!(
            terminal.player_message(),
            "This link must point to one public, completed standard-chess Game."
        );
    }
}

#[tokio::test]
async fn first_fetch_and_cache_hit_expose_the_stable_import_progress() {
    let importer = GameImporter::new(FakeLichessClient::with_canonical_capture());
    let first_progress = Arc::new(Mutex::new(Vec::new()));
    let first_events = Arc::clone(&first_progress);

    importer
        .import_with_progress(
            &canonical_lichess_source(),
            RequestedReviewSide::FromQualifiedUrl,
            &RequestedEloProfile::FromImportedMetadata,
            move |stage| first_events.lock().unwrap().push(stage),
        )
        .await
        .unwrap();

    assert_eq!(
        *first_progress.lock().unwrap(),
        vec![
            ImportProgressStage::ValidatingSource,
            ImportProgressStage::WaitingForLichess,
            ImportProgressStage::FetchingGame,
            ImportProgressStage::ValidatingGame,
            ImportProgressStage::BuildingSnapshot,
        ]
    );

    let cached_progress = Arc::new(Mutex::new(Vec::new()));
    let cached_events = Arc::clone(&cached_progress);
    importer
        .import_with_progress(
            &canonical_lichess_source(),
            RequestedReviewSide::FromQualifiedUrl,
            &RequestedEloProfile::FromImportedMetadata,
            move |stage| cached_events.lock().unwrap().push(stage),
        )
        .await
        .unwrap();

    assert_eq!(
        *cached_progress.lock().unwrap(),
        vec![
            ImportProgressStage::ValidatingSource,
            ImportProgressStage::BuildingSnapshot,
        ]
    );
}

#[test]
fn terminal_mapping_uses_existing_wire_reasons_and_exact_player_copy() {
    let retry = GameImportError::from(LichessImportError::RateLimited {
        retry_after_seconds: 75,
        retry_at: "2026-07-16T12:00:00Z".parse().unwrap(),
    })
    .terminal();
    assert_eq!(
        retry.event(),
        &ReviewSessionEvent::Unavailable {
            operation: OperationKind::GameImport,
            reason: ProviderUnavailableReason::RateLimited {
                retry_after_seconds: 75,
            },
            retry: RetryDirective::RetryAfter { seconds: 75 },
        }
    );
    assert_eq!(
        retry.player_message(),
        "Lichess asked us to slow down. Try this Game again after 2026-07-16T12:00:00Z."
    );

    let ongoing = GameImportError::from(LichessImportError::OngoingGame).terminal();
    assert_eq!(
        ongoing.event(),
        &ReviewSessionEvent::Rejected {
            operation: OperationKind::GameImport,
            reason: CommandRejectionReason::OngoingGame,
            recovery: RejectionRecovery::None,
        }
    );
    assert_eq!(
        ongoing.player_message(),
        "This link must point to one public, completed standard-chess Game."
    );
}

#[derive(Clone)]
struct FakeLichessClient {
    response: Result<LichessExportResponse, LichessExportError>,
    requests: Arc<Mutex<Vec<LichessExportRequest>>>,
}

impl FakeLichessClient {
    fn with_canonical_capture() -> Self {
        Self::new(canonical_raw_export())
    }

    fn new(body: Vec<u8>) -> Self {
        Self {
            response: Ok(LichessExportResponse {
                body,
                content_type: LICHESS_PGN_MEDIA_TYPE.to_string(),
                captured_at: "2026-09-03T00:00:00Z".parse().unwrap(),
            }),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn failure(error: LichessExportError) -> Self {
        Self {
            response: Err(error),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl LichessExportClient for FakeLichessClient {
    fn export<'a>(
        &'a self,
        request: &'a LichessExportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LichessExportResponse, LichessExportError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            self.response.clone()
        })
    }
}

#[derive(Clone)]
struct FakeChessComClient {
    response: Result<ChessComGameResponse, ChessComGameFetchError>,
    requests: Arc<Mutex<Vec<ChessComGameRequest>>>,
}

impl FakeChessComClient {
    fn lorenzo_game() -> Self {
        let body = serde_json::to_vec(&serde_json::json!({
            "game": {
                "id": 1403674481_u64,
                "initialSetup": "",
                "isFinished": true,
                "isVsComputer": true,
                "gameEndReason": "resigned",
                "moveList": "gvZJow7Rfo5Qeg0Klt6EpxEZmuKCtCJCvB!TbsQBuB86sCTCoCZxCoxogoRJnvJBdB7BcuBAks9IuIAIfe?7ac7lofILvDljcdYIiqWOsAXHdJLJAJjrforqe0qioxIAJRil013V12lRxFAs20Hz0ezreuskurk~",
                "pgnHeaders": {
                    "Event": "Play vs Bot",
                    "Site": "Chess.com",
                    "Date": "2026.05.23",
                    "White": "synthetic-white",
                    "Black": "Lorenzo-BOT",
                    "Result": "0-1",
                    "ECO": "A07",
                    "Opening": "King's Indian Attack",
                    "WhiteElo": 636,
                    "BlackElo": 1800,
                    "TimeControl": "?",
                    "EndDate": "2026.05.23",
                    "Termination": "Lorenzo-BOT won by resignation",
                    "SetUp": "1",
                    "FEN": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
                },
                "plyCount": 80,
                "type": "chess"
            },
            "players": {
                "top": { "isComputer": true },
                "bottom": { "isComputer": false }
            }
        }))
        .unwrap();
        Self {
            response: Ok(ChessComGameResponse {
                body,
                content_type: CHESS_COM_JSON_MEDIA_TYPE.to_string(),
                captured_at: "2026-07-30T17:24:58Z".parse().unwrap(),
            }),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn live_pvp_game() -> Self {
        let body = serde_json::to_vec(&serde_json::json!({
            "game": {
                "id": 100000000001_u64,
                "initialSetup": "",
                "isFinished": true,
                "isLiveGame": true,
                "gameEndReason": "resigned",
                "moveList": "gv1Tow0KlB5Qmu9zbszsjs!0fo8!egZRcj6EdeKCvlXHoCTLCQ0QBJQ0sA",
                "pgnHeaders": {
                    "Event": "Live Chess",
                    "Site": "Chess.com",
                    "Date": "2025.04.12",
                    "White": "synthetic-white",
                    "Black": "nbank22",
                    "Result": "1-0",
                    "ECO": "A04",
                    "WhiteElo": 681,
                    "BlackElo": 605,
                    "TimeControl": "600",
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
                captured_at: "2026-07-30T19:56:00Z".parse().unwrap(),
            }),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl ChessComGameClient for FakeChessComClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ChessComGameRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<ChessComGameResponse, ChessComGameFetchError>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            self.response.clone()
        })
    }
}

fn fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
    serde_json::from_slice(&fs::read(contract_fixture_root().join(name)).unwrap()).unwrap()
}

fn canonical_raw_export() -> Vec<u8> {
    fs::read(canonical_fixture_root().join("lichess-export.raw.pgn")).unwrap()
}

fn canonical_lichess_source() -> GameInputSource {
    GameInputSource::LichessUrl {
        url: "https://lichess.org/Synthet1Demo/black".to_string(),
    }
}

fn contract_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/coach-engine-sdk/fixtures")
}

fn canonical_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/shared-assets/fixtures/Synthet1")
}

fn temporary_pgn_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "chenchess-review-session-import-{}.pgn",
        std::process::id()
    ))
}
