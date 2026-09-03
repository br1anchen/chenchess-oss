use super::*;

use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};

use crate::engine_analysis::{
    EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer, EngineCacheIdentity,
    EngineProvenance, ExactEngineCache, PositionEvaluation,
};
use crate::opening_analysis::OpeningAnalysisRuntime;
use crate::review_session_processor::{ControllableTrafficClock, PlayerTrafficPolicy};
use coach_engine_pipeline::evaluation_recording::{
    PINNED_STOCKFISH_BINARY_DIGEST, PINNED_STOCKFISH_DEPTH, PINNED_STOCKFISH_HASH_MIB,
    PINNED_STOCKFISH_THREADS, PINNED_STOCKFISH_VERSION,
};

const AMAR_REF: &str = "A00-amar-opening-b2ca";

#[tokio::test]
async fn resolve_returns_the_catalog_row_for_a_known_address() {
    let application = application(Arc::new(FakeProfileValidator::default()));

    let resolved = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/resolve",
        json!({ "openingLineRef": AMAR_REF }),
    )
    .await;
    assert_eq!(resolved.0, StatusCode::OK);
    assert_eq!(
        resolved.1,
        json!({
            "outcome": "resolved",
            "line": {
                "eco": "A00",
                "name": "Amar Opening",
                "path": "1. Nh3",
                "openingLineRef": AMAR_REF,
            },
        })
    );

    let unknown = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/resolve",
        json!({ "openingLineRef": "A00-amar-opening-0000" }),
    )
    .await;
    assert_eq!(unknown.0, StatusCode::OK);
    assert_eq!(unknown.1, json!({ "outcome": "unknownOpeningLine" }));
}

#[tokio::test]
async fn the_same_line_evaluated_twice_returns_identical_results_and_hits_the_cache() {
    let engine = Arc::new(CountingEngine::default());
    let application = application_with_opening_analysis(cached_runtime(engine.clone()));
    let body = json!({
        "openingLineRef": AMAR_REF,
        "continuation": [
            { "kind": "san", "san": "d5" },
            { "kind": "san", "san": "g3" },
        ],
    });

    let first = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        body.clone(),
    )
    .await;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(first.1["outcome"], "analyzed");
    assert_eq!(first.1["verdict"], json!({ "kind": "completed" }));
    assert_eq!(first.1["plies"].as_array().map(Vec::len), Some(2));
    let analyses_after_first = engine.analysis_count();
    assert_eq!(analyses_after_first, 3, "root plus two continuation plies");

    let second = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        body,
    )
    .await;
    assert_eq!(second.1, first.1);
    assert_eq!(
        engine.analysis_count(),
        analyses_after_first,
        "the second evaluation must be served from the cache"
    );
}

#[tokio::test]
async fn a_transposition_into_a_cached_position_hits_the_same_cache_entry() {
    let engine = Arc::new(CountingEngine::default());
    let application = application_with_opening_analysis(cached_runtime(engine.clone()));

    let first = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        json!({
            "openingLineRef": AMAR_REF,
            "continuation": [
                { "kind": "san", "san": "e6" },
                { "kind": "san", "san": "g3" },
                { "kind": "san", "san": "d5" },
            ],
        }),
    )
    .await;
    assert_eq!(first.1["outcome"], "analyzed");
    let analyses_after_first = engine.analysis_count();
    assert_eq!(
        analyses_after_first, 4,
        "root plus three continuation plies"
    );

    // The other move order reaches the same final position: only the two
    // intermediate positions are new, the destination is one cache entry.
    let transposed = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        json!({
            "openingLineRef": AMAR_REF,
            "continuation": [
                { "kind": "san", "san": "d5" },
                { "kind": "san", "san": "g3" },
                { "kind": "san", "san": "e6" },
            ],
        }),
    )
    .await;
    assert_eq!(transposed.1["outcome"], "analyzed");
    assert_eq!(
        engine.analysis_count(),
        analyses_after_first + 2,
        "the transposed destination must reuse the cached entry"
    );
    assert_eq!(
        transposed.1["plies"][2]["resultingFen"],
        first.1["plies"][2]["resultingFen"],
    );
}

#[tokio::test]
async fn a_second_player_reuses_the_identity_free_cache() {
    let engine = Arc::new(CountingEngine::default());
    let application = application_with_opening_analysis(cached_runtime(engine.clone()));
    let body = json!({
        "openingLineRef": AMAR_REF,
        "continuation": [{ "kind": "san", "san": "d5" }],
    });

    let first = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        body.clone(),
    )
    .await;
    assert_eq!(first.1["outcome"], "analyzed");
    let analyses_after_first = engine.analysis_count();

    let other_player = firebase_token("another-player");
    let second = request_with_token(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        body,
        &other_player,
    )
    .await;
    assert_eq!(second.1["plies"], first.1["plies"]);
    assert_eq!(
        engine.analysis_count(),
        analyses_after_first,
        "no cache entry carries an owner, so another Player hits the same entries"
    );
}

#[tokio::test]
async fn a_continuation_past_twelve_plies_returns_the_evaluated_prefix() {
    let engine = Arc::new(CountingEngine::default());
    let application = application_with_opening_analysis(cached_runtime(engine.clone()));
    let continuation: Vec<Value> = [
        "d5", "g3", "e5", "Bg2", "Nf6", "O-O", "Bc5", "d3", "O-O", "Nc3", "c6", "a3", "h6",
    ]
    .iter()
    .map(|san| json!({ "kind": "san", "san": san }))
    .collect();

    let analyzed = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        json!({ "openingLineRef": AMAR_REF, "continuation": continuation }),
    )
    .await;
    assert_eq!(analyzed.0, StatusCode::OK);
    assert_eq!(analyzed.1["outcome"], "analyzed");
    assert_eq!(
        analyzed.1["verdict"],
        json!({ "kind": "plyLimitReached", "index": 12 })
    );
    assert_eq!(analyzed.1["plies"].as_array().map(Vec::len), Some(12));
}

#[tokio::test]
async fn an_illegal_continuation_move_keeps_the_evaluated_prefix() {
    let engine = Arc::new(CountingEngine::default());
    let application = application_with_opening_analysis(cached_runtime(engine));

    let analyzed = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        json!({
            "openingLineRef": AMAR_REF,
            "continuation": [
                { "kind": "san", "san": "d5" },
                { "kind": "san", "san": "Ke4" },
            ],
        }),
    )
    .await;
    assert_eq!(analyzed.1["outcome"], "analyzed");
    assert_eq!(
        analyzed.1["verdict"],
        json!({ "kind": "illegalMove", "index": 1 })
    );
    assert_eq!(analyzed.1["plies"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn an_unknown_address_is_a_typed_outcome_and_spends_no_engine_compute() {
    let engine = Arc::new(CountingEngine::default());
    let application = application_with_opening_analysis(cached_runtime(engine.clone()));

    let unknown = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        json!({ "openingLineRef": "Z99-not-a-line-ffff", "continuation": [] }),
    )
    .await;
    assert_eq!(unknown.0, StatusCode::OK);
    assert_eq!(unknown.1, json!({ "outcome": "unknownOpeningLine" }));
    assert_eq!(engine.analysis_count(), 0);
}

#[tokio::test]
async fn the_per_player_rate_limit_trips_and_recovers() {
    let engine = Arc::new(CountingEngine::default());
    let clock = Arc::new(ControllableTrafficClock::new(1_000));
    let runtime = cached_runtime(engine)
        .with_traffic(Arc::new(PlayerTrafficPolicy::v1_with_clock(clock.clone())));
    let application = application_with_opening_analysis(runtime);
    let body = json!({ "openingLineRef": AMAR_REF, "continuation": [] });

    for _ in 0..10 {
        let admitted = request(
            &application,
            Method::POST,
            "/api/v1/opening-lines/analysis",
            body.clone(),
        )
        .await;
        assert_eq!(admitted.1["outcome"], "analyzed");
    }
    let tripped = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        body.clone(),
    )
    .await;
    assert_eq!(tripped.0, StatusCode::OK);
    assert_eq!(tripped.1["outcome"], "rateLimited");
    assert_eq!(tripped.1["retry"]["kind"], "retryAfter");

    clock.advance_ms(61_000);
    let recovered = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        body,
    )
    .await;
    assert_eq!(recovered.1["outcome"], "analyzed");
}

#[tokio::test]
async fn analysis_refuses_an_unauthenticated_request() {
    let engine = Arc::new(CountingEngine::default());
    let application = application_with_opening_analysis(cached_runtime(engine.clone()));

    let response = application
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/opening-lines/analysis")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "openingLineRef": AMAR_REF, "continuation": [] }).to_string(),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(engine.analysis_count(), 0);
}

#[tokio::test]
async fn a_missing_engine_is_unavailable_with_a_retry_directive() {
    let application = application_with_opening_analysis(OpeningAnalysisRuntime::disabled());

    let unavailable = request(
        &application,
        Method::POST,
        "/api/v1/opening-lines/analysis",
        json!({ "openingLineRef": AMAR_REF, "continuation": [] }),
    )
    .await;
    assert_eq!(unavailable.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        unavailable.1,
        json!({ "outcome": "unavailable", "retry": { "kind": "retryAllowed" } })
    );
}

#[tokio::test]
async fn played_openings_is_an_authed_typed_read() {
    let application = application(Arc::new(FakeProfileValidator::default()));

    let empty = request(
        &application,
        Method::GET,
        "/api/v1/openings/played",
        Value::Null,
    )
    .await;
    assert_eq!(empty.0, StatusCode::OK);
    assert_eq!(empty.1, json!({ "openings": [] }));

    let unauthenticated = application
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/openings/played")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
}

fn cached_runtime(engine: Arc<CountingEngine>) -> OpeningAnalysisRuntime {
    OpeningAnalysisRuntime::new(Some(Arc::new(ExactEngineCache::new(engine))))
}

/// Depth-pinned deterministic engine that counts how many positions it was
/// actually asked to analyze — the cache-hit oracle.
#[derive(Default)]
struct CountingEngine {
    analyses: AtomicUsize,
}

impl CountingEngine {
    fn analysis_count(&self) -> usize {
        self.analyses.load(Ordering::SeqCst)
    }
}

impl EngineAnalyzer for CountingEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        self.analyses.fetch_add(1, Ordering::SeqCst);
        let best_move = first_legal_uci(input.position);
        Box::pin(async move {
            Ok(EngineAnalysis {
                best_move: best_move.clone(),
                evaluation: PositionEvaluation::Centipawns(50),
                principal_variation: vec![best_move],
                depth: PINNED_STOCKFISH_DEPTH,
            })
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        Some(pinned_provenance())
    }

    fn cache_identity(&self) -> Option<EngineCacheIdentity> {
        Some(EngineCacheIdentity::from_provenance(
            self.provider_name(),
            &pinned_provenance(),
        ))
    }
}

fn pinned_provenance() -> EngineProvenance {
    EngineProvenance {
        version: PINNED_STOCKFISH_VERSION.to_string(),
        binary_sha256: PINNED_STOCKFISH_BINARY_DIGEST
            .strip_prefix("sha256:")
            .expect("the pinned digest carries its prefix")
            .to_string(),
        depth: PINNED_STOCKFISH_DEPTH,
        threads: PINNED_STOCKFISH_THREADS,
        hash_mib: PINNED_STOCKFISH_HASH_MIB,
    }
}

fn first_legal_uci(fen: &str) -> String {
    let position: Chess = Fen::from_ascii(fen.as_bytes())
        .expect("analysis positions are canonical FENs")
        .into_position(CastlingMode::Standard)
        .expect("analysis positions are legal");
    let chess_move = position
        .legal_moves()
        .into_iter()
        .next()
        .expect("analysis positions have a legal move");
    UciMove::from_standard(&chess_move).to_string()
}
