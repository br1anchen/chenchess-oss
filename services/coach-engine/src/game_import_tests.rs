use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use super::*;
use crate::{
    chess_com::{ChessComGameFetchError, ChessComGameRequest, ChessComGameResponse},
    lichess::{LichessExportError, LichessExportRequest},
};

const PGN: &str = r#"[Event "Live Chess"]
[Site "Chess.com"]
[Date "2026.08.12"]
[Round "-"]
[White "Player"]
[Black "Opponent"]
[Result "0-1"]
[WhiteElo "1200"]
[BlackElo "1300"]

1. f3 e5 2. g4 Qh4# 0-1"#;

#[tokio::test]
async fn archive_import_uses_the_carried_pgn_without_a_per_game_fetch() {
    let chess_com_calls = Arc::new(AtomicUsize::new(0));
    let importer = GameImporter::with_chess_com(
        UnusedLichess,
        CountingChessCom {
            calls: chess_com_calls.clone(),
        },
    );
    let captured_at = instant("2026-08-12T12:34:56Z");
    let response_digest = digest(b"monthly archive response");
    let request = DailyGameReviewRequest {
        source: DailyGameInputSource::ChessComArchive {
            url: "https://www.chess.com/game/daily/123456789".to_string(),
            pgn: PGN.to_string(),
            captured_at,
            response_digest: response_digest.clone(),
        },
        review_side: RequestedReviewSide::Selected {
            review_side: ReviewSide::White,
        },
        elo_profile: RequestedEloProfile::FromImportedMetadata,
        ended_at_unix_milliseconds: Some(1_786_536_896_000),
    };

    let imported = importer
        .import_daily_review_with_progress(&request, |_| {})
        .await
        .unwrap();

    assert_eq!(chess_com_calls.load(Ordering::SeqCst), 0);
    assert_eq!(imported.pgn, PGN);
    assert_eq!(
        imported.imported_game.provenance,
        ImportProvenance::ChessCom {
            canonical_game_id: CanonicalGameId::try_from("123456789".to_string()).unwrap(),
            canonical_url: "https://www.chess.com/game/daily/123456789".to_string(),
            fetch_contract_version: CHESS_COM_PUBAPI_ARCHIVE_CONTRACT_VERSION.to_string(),
            captured_at: "2026-08-12T12:34:56Z".to_string(),
            response_digest,
            pgn_digest: digest(PGN.as_bytes()),
        }
    );
}

fn digest(bytes: &[u8]) -> ArtifactDigest {
    ArtifactDigest::try_from(format!("sha256:{:x}", Sha256::digest(bytes))).unwrap()
}

fn instant(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

struct UnusedLichess;

impl LichessExportClient for UnusedLichess {
    fn export<'a>(
        &'a self,
        _request: &'a LichessExportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LichessExportResponse, LichessExportError>> + Send + 'a>>
    {
        panic!("an archive import must not fetch Lichess")
    }
}

struct CountingChessCom {
    calls: Arc<AtomicUsize>,
}

impl ChessComGameClient for CountingChessCom {
    fn fetch<'a>(
        &'a self,
        _request: &'a ChessComGameRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<ChessComGameResponse, ChessComGameFetchError>> + Send + 'a>,
    > {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(ChessComGameFetchError::Connection) })
    }
}
