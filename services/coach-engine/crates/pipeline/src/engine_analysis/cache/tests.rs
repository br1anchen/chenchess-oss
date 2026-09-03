use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use tokio::sync::Semaphore;

use super::{
    batch::BatchPlan,
    store::{cache_weight, payload_digest, CacheKey, EngineCache},
    *,
};
use crate::{
    engine_analysis::PositionEvaluation,
    review_session_contract::{CanonicalPosition, PositionContentId},
};

/// Positions are real FENs because the cache keys on Position Snapshot content:
/// a placeholder string has no content identity and is deliberately not cached.
const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const AFTER_E4: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
const AFTER_D4: &str = "rnbqkbnr/pppppppp/8/8/3P4/8/PPP1PPPP/RNBQKBNR b KQkq d3 0 1";

fn content(fen: &str) -> PositionContentId {
    CanonicalPosition::from_fen(fen)
        .expect("the test position is a valid FEN")
        .content_id
}

fn identity(binary: &str) -> EngineCacheIdentity {
    EngineCacheIdentity {
        provider: "Stockfish".to_owned(),
        binary_sha256: binary.to_owned(),
        depth: 16,
        threads: 1,
        hash_mib: 16,
    }
}

fn output(binary: &str, best_move: &str) -> EngineAnalysisOutput {
    EngineAnalysisOutput {
        analysis: EngineAnalysis {
            best_move: best_move.to_owned(),
            evaluation: PositionEvaluation::Centipawns(12),
            principal_variation: vec![best_move.to_owned()],
            depth: 16,
        },
        provenance: Some(EngineProvenance {
            version: "18".to_owned(),
            binary_sha256: binary.to_owned(),
            depth: 16,
            threads: 1,
            hash_mib: 16,
        }),
    }
}

#[test]
fn cache_key_covers_position_content_and_engine_identity() {
    assert_ne!(
        CacheKey::new(&identity("a"), &content(START)),
        CacheKey::new(&identity("a"), &content(AFTER_E4))
    );
    assert_ne!(
        CacheKey::new(&identity("a"), &content(START)),
        CacheKey::new(&identity("b"), &content(START))
    );
}

#[test]
fn digest_invalid_entries_are_discarded() {
    let key = CacheKey::new(&identity("a"), &content(START));
    let mut cache = EngineCache::new(4096);
    cache.insert(key.clone(), output("a", "e2e4"));
    cache.entries.get_mut(&key).unwrap().payload_digest = "invalid".to_owned();

    assert!(cache.get(&key).is_none());
    assert!(cache.entries.is_empty());
    assert_eq!(cache.used_bytes, 0);
}

#[test]
fn cache_evicts_the_least_recently_used_entry_to_its_byte_budget() {
    let first = CacheKey::new(&identity("a"), &content(START));
    let second = CacheKey::new(&identity("a"), &content(AFTER_E4));
    let third = CacheKey::new(&identity("a"), &content(AFTER_D4));
    let sample = output("a", "e2e4");
    let entry_bytes = cache_weight(&first, &sample, &payload_digest(&first, &sample));
    let mut cache = EngineCache::new(entry_bytes * 2);
    cache.insert(first.clone(), sample.clone());
    cache.insert(second.clone(), output("a", "d2d4"));
    assert!(cache.get(&first).is_some());
    cache.insert(third.clone(), output("a", "g1f3"));

    assert!(cache.get(&first).is_some());
    assert!(cache.get(&second).is_none());
    assert!(cache.get(&third).is_some());
    assert!(cache.used_bytes <= cache.max_bytes);
}

#[test]
fn batch_cardinality_mismatch_is_a_provider_error_instead_of_a_panic() {
    let cache = Mutex::new(EngineCache::new(4096));
    let mut plan = BatchPlan::new(vec![START.to_owned()], &identity("a"), &cache);

    let error = plan.merge(Vec::new(), |_, _| {}).unwrap_err();

    assert_eq!(error.index, 0);
    assert!(matches!(error.error, EngineAnalysisError::Protocol(_)));
}

struct FailingAnalyzer {
    calls: Arc<AtomicUsize>,
}

struct CountingAnalyzer {
    calls: Arc<AtomicUsize>,
}

impl EngineAnalyzer for CountingAnalyzer {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move { Ok(self.analyze_with_provenance(input).await?.analysis) })
    }

    fn analyze_with_provenance<'a>(
        &'a self,
        _input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysisOutput, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(output("a", "e2e4"))
        })
    }

    fn cache_identity(&self) -> Option<EngineCacheIdentity> {
        Some(identity("a"))
    }
}

/// The Sicilian after 1.e4 c5 2.Nf3, as two different reviews reach it. Same
/// board, same rights, same halfmove clock; different move number, and one of
/// them records an en passant square nothing can capture. Neither difference
/// changes what Stockfish will say, so neither may cost a second compute.
const SICILIAN_IN_ONE_REVIEW: &str =
    "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b Kkq - 1 3";
const SICILIAN_IN_ANOTHER_REVIEW: &str =
    "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b Kkq e3 1 17";

/// Records the positions it was handed, so a test can assert the engine sees the
/// canonical position its cache key names.
#[derive(Default)]
struct RecordingAnalyzer {
    positions: Mutex<Vec<String>>,
}

impl EngineAnalyzer for RecordingAnalyzer {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move { Ok(self.analyze_with_provenance(input).await?.analysis) })
    }

    fn analyze_with_provenance<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysisOutput, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.record(input.position);
            Ok(output("a", "e2e4"))
        })
    }

    fn cache_identity(&self) -> Option<EngineCacheIdentity> {
        Some(identity("a"))
    }

    fn analyze_positions(
        self: Arc<Self>,
        positions: Vec<String>,
        _concurrency: usize,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<TimedEngineAnalysis>, IndexedEngineAnalysisError>>
                + Send,
        >,
    > {
        Box::pin(async move {
            Ok(positions
                .into_iter()
                .map(|position| {
                    self.record(&position);
                    let output = output("a", "e2e4");
                    TimedEngineAnalysis {
                        analysis: output.analysis,
                        provenance: output.provenance,
                        duration: std::time::Duration::ZERO,
                    }
                })
                .collect())
        })
    }
}

impl RecordingAnalyzer {
    fn record(&self, position: &str) {
        self.positions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(position.to_owned());
    }

    fn recorded(&self) -> Vec<String> {
        self.positions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[tokio::test]
async fn the_engine_is_given_the_canonical_position_its_key_names() {
    let analyzer = Arc::new(RecordingAnalyzer::default());
    let cache = ExactEngineCache::with_max_bytes(analyzer.clone(), 4096);

    // This spelling records an en passant square nothing can capture, so it is
    // not the canonical position the key identifies. Sharing the entry is only
    // sound if the engine saw the canonical position.
    cache
        .analyze_with_provenance(EngineAnalysisInput {
            position: SICILIAN_IN_ANOTHER_REVIEW,
        })
        .await
        .unwrap();

    let canonical = CanonicalPosition::from_fen(SICILIAN_IN_ANOTHER_REVIEW).unwrap();
    assert!(!canonical.fen.contains("e3"));
    assert_eq!(analyzer.recorded(), vec![canonical.fen]);
}

#[tokio::test]
async fn a_batch_gives_the_engine_canonical_positions() {
    let analyzer = Arc::new(RecordingAnalyzer::default());
    let cache = Arc::new(ExactEngineCache::with_max_bytes(analyzer.clone(), 4096));

    cache
        .analyze_positions(vec![SICILIAN_IN_ANOTHER_REVIEW.to_owned()], 1)
        .await
        .unwrap();

    let canonical = CanonicalPosition::from_fen(SICILIAN_IN_ANOTHER_REVIEW).unwrap();
    assert_eq!(analyzer.recorded(), vec![canonical.fen]);
}

#[tokio::test]
async fn the_same_position_from_two_reviews_is_one_engine_compute() {
    let calls = Arc::new(AtomicUsize::new(0));
    let analyzer = ExactEngineCache::with_max_bytes(
        Arc::new(CountingAnalyzer {
            calls: calls.clone(),
        }),
        4096,
    );

    let first = analyzer
        .analyze_with_provenance(EngineAnalysisInput {
            position: SICILIAN_IN_ONE_REVIEW,
        })
        .await
        .unwrap();
    let second = analyzer
        .analyze_with_provenance(EngineAnalysisInput {
            position: SICILIAN_IN_ANOTHER_REVIEW,
        })
        .await
        .unwrap();

    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(first.analysis, second.analysis);
}

#[test]
fn a_batch_coalesces_one_position_reached_by_two_reviews() {
    let cache = Mutex::new(EngineCache::new(4096));

    let plan = BatchPlan::new(
        vec![
            SICILIAN_IN_ONE_REVIEW.to_owned(),
            SICILIAN_IN_ANOTHER_REVIEW.to_owned(),
        ],
        &identity("a"),
        &cache,
    );

    assert_eq!(plan.cache_misses(), 1);
    assert_eq!(plan.coalesced(), 1);
}

#[tokio::test]
async fn a_position_without_content_identity_passes_through_uncached() {
    let calls = Arc::new(AtomicUsize::new(0));
    let analyzer = ExactEngineCache::with_max_bytes(
        Arc::new(CountingAnalyzer {
            calls: calls.clone(),
        }),
        4096,
    );

    for _ in 0..2 {
        analyzer
            .analyze_with_provenance(EngineAnalysisInput {
                position: "not-a-fen",
            })
            .await
            .unwrap();
    }

    // The provider decides what to do with an unusable position; the cache
    // neither answers for it nor stores anything under a guessed key.
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(analyzer
        .cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entries
        .is_empty());
}

struct BlockingBatchAnalyzer {
    calls: Arc<AtomicUsize>,
    release: Arc<Semaphore>,
}

struct LoadAnalyzer {
    calls: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    slots: Arc<Semaphore>,
}

impl EngineAnalyzer for FailingAnalyzer {
    fn analyze<'a>(
        &'a self,
        _input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Err(EngineAnalysisError::Protocol("scripted failure".to_owned()))
        })
    }

    fn cache_identity(&self) -> Option<EngineCacheIdentity> {
        Some(identity("a"))
    }
}

impl EngineAnalyzer for BlockingBatchAnalyzer {
    fn analyze<'a>(
        &'a self,
        _input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        unreachable!("the cache test uses batched analysis")
    }

    fn cache_identity(&self) -> Option<EngineCacheIdentity> {
        Some(identity("a"))
    }

    fn analyze_positions(
        self: Arc<Self>,
        positions: Vec<String>,
        _concurrency: usize,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<TimedEngineAnalysis>, IndexedEngineAnalysisError>>
                + Send,
        >,
    > {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::Release);
            let permit = self.release.acquire().await.expect("test pool is open");
            permit.forget();
            Ok(positions
                .into_iter()
                .map(|_| {
                    let output = output("a", "e2e4");
                    TimedEngineAnalysis {
                        analysis: output.analysis,
                        provenance: output.provenance,
                        duration: std::time::Duration::ZERO,
                    }
                })
                .collect())
        })
    }
}

impl EngineAnalyzer for LoadAnalyzer {
    fn analyze<'a>(
        &'a self,
        _input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        unreachable!("the load test uses batched analysis")
    }

    fn cache_identity(&self) -> Option<EngineCacheIdentity> {
        Some(identity("a"))
    }

    fn analyze_positions(
        self: Arc<Self>,
        positions: Vec<String>,
        _concurrency: usize,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<TimedEngineAnalysis>, IndexedEngineAnalysisError>>
                + Send,
        >,
    > {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::Release);
            let _permit = self.slots.clone().acquire_owned().await.unwrap();
            let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
            self.peak.fetch_max(active, Ordering::AcqRel);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            self.active.fetch_sub(1, Ordering::AcqRel);
            Ok(positions
                .into_iter()
                .map(|_| {
                    let output = output("a", "e2e4");
                    TimedEngineAnalysis {
                        analysis: output.analysis,
                        provenance: output.provenance,
                        duration: std::time::Duration::from_millis(10),
                    }
                })
                .collect())
        })
    }
}

#[tokio::test]
async fn failed_analysis_never_enters_the_cache() {
    let calls = Arc::new(AtomicUsize::new(0));
    let analyzer = ExactEngineCache::with_max_bytes(
        Arc::new(FailingAnalyzer {
            calls: calls.clone(),
        }),
        4096,
    );

    for _ in 0..2 {
        assert!(analyzer
            .analyze_with_provenance(EngineAnalysisInput { position: START })
            .await
            .is_err());
    }

    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(analyzer
        .cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entries
        .is_empty());
}

#[tokio::test]
async fn concurrent_equivalent_batches_collapse_to_one_provider_request() {
    let calls = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Semaphore::new(0));
    let analyzer = Arc::new(ExactEngineCache::with_max_bytes(
        Arc::new(BlockingBatchAnalyzer {
            calls: calls.clone(),
            release: release.clone(),
        }),
        4096,
    ));
    let positions = vec![START.to_owned(), AFTER_E4.to_owned()];

    let first = tokio::spawn(analyzer.clone().analyze_positions(positions.clone(), 2));
    wait_for_calls(&calls, 1).await;
    let second = tokio::spawn(analyzer.clone().analyze_positions(positions.clone(), 2));
    tokio::task::yield_now().await;
    assert_eq!(calls.load(Ordering::Acquire), 1);
    release.add_permits(1);

    assert_eq!(first.await.unwrap().unwrap().len(), 2);
    assert_eq!(second.await.unwrap().unwrap().len(), 2);
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(
        analyzer
            .clone()
            .analyze_positions(positions, 2)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn concurrent_batches_spelling_one_position_differently_collapse() {
    let calls = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Semaphore::new(0));
    let analyzer = Arc::new(ExactEngineCache::with_max_bytes(
        Arc::new(BlockingBatchAnalyzer {
            calls: calls.clone(),
            release: release.clone(),
        }),
        4096,
    ));

    // Neither batch can find the other's result in the store, because neither has
    // finished. Collapsing them is the single-flight key's job, so it has to name
    // the position by content and not by the caller's spelling of it.
    let first = tokio::spawn(
        analyzer
            .clone()
            .analyze_positions(vec![SICILIAN_IN_ONE_REVIEW.to_owned()], 1),
    );
    wait_for_calls(&calls, 1).await;
    let second = tokio::spawn(
        analyzer
            .clone()
            .analyze_positions(vec![SICILIAN_IN_ANOTHER_REVIEW.to_owned()], 1),
    );
    tokio::task::yield_now().await;
    assert_eq!(calls.load(Ordering::Acquire), 1);
    release.add_permits(1);

    assert_eq!(first.await.unwrap().unwrap().len(), 1);
    assert_eq!(second.await.unwrap().unwrap().len(), 1);
    assert_eq!(calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn cancelled_leader_releases_waiters_to_retry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Semaphore::new(0));
    let analyzer = Arc::new(ExactEngineCache::with_max_bytes(
        Arc::new(BlockingBatchAnalyzer {
            calls: calls.clone(),
            release: release.clone(),
        }),
        4096,
    ));
    let positions = vec![START.to_owned()];

    let first = tokio::spawn(analyzer.clone().analyze_positions(positions.clone(), 1));
    wait_for_calls(&calls, 1).await;
    let second = tokio::spawn(analyzer.clone().analyze_positions(positions, 1));
    tokio::task::yield_now().await;
    first.abort();
    wait_for_calls(&calls, 2).await;
    release.add_permits(1);

    assert_eq!(second.await.unwrap().unwrap().len(), 1);
}

/// The latency claim is about queueing, not about this machine.
///
/// Time is paused, so every duration below is the virtual time the runtime
/// advanced through while tasks waited on the provider pool — the queueing the
/// cache exists to remove, and nothing else. Measured against the wall clock
/// this compared two samples taken minutes apart on a shared machine, and a
/// busy second sample failed a claim about single-flight that was never in
/// question.
#[tokio::test(start_paused = true)]
async fn bounded_player_load_collapses_duplicates_without_worsening_p95() {
    let baseline_calls = Arc::new(AtomicUsize::new(0));
    let baseline_peak = Arc::new(AtomicUsize::new(0));
    let baseline = Arc::new(LoadAnalyzer {
        calls: baseline_calls.clone(),
        active: Arc::new(AtomicUsize::new(0)),
        peak: baseline_peak.clone(),
        slots: Arc::new(Semaphore::new(2)),
    });
    let baseline_durations = run_load(baseline).await;

    let cached_calls = Arc::new(AtomicUsize::new(0));
    let cached_peak = Arc::new(AtomicUsize::new(0));
    let cached = Arc::new(ExactEngineCache::with_max_bytes(
        Arc::new(LoadAnalyzer {
            calls: cached_calls.clone(),
            active: Arc::new(AtomicUsize::new(0)),
            peak: cached_peak.clone(),
            slots: Arc::new(Semaphore::new(2)),
        }),
        4096,
    ));
    let cached_durations = run_load(cached).await;

    assert_eq!(baseline_calls.load(Ordering::Acquire), 24);
    assert_eq!(cached_calls.load(Ordering::Acquire), 1);
    assert!(baseline_peak.load(Ordering::Acquire) <= 2);
    assert!(cached_peak.load(Ordering::Acquire) <= 2);
    assert!(
        p95(&cached_durations) <= p95(&baseline_durations),
        "single-flight should improve or preserve bounded-load p95"
    );
}

async fn wait_for_calls(calls: &AtomicUsize, expected: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while calls.load(Ordering::Acquire) != expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the expected provider call should start");
}

async fn run_load<A: EngineAnalyzer>(analyzer: Arc<A>) -> Vec<std::time::Duration> {
    let requests = (0..24)
        .map(|_| {
            let analyzer = analyzer.clone();
            tokio::spawn(async move {
                // The paused clock, so a request's duration is the time it spent
                // queued for the provider rather than the time this machine
                // happened to take.
                let started_at = tokio::time::Instant::now();
                analyzer
                    .analyze_positions(vec![START.to_owned(), AFTER_E4.to_owned()], 2)
                    .await
                    .unwrap();
                started_at.elapsed()
            })
        })
        .collect::<Vec<_>>();
    let mut durations = Vec::with_capacity(requests.len());
    for request in requests {
        durations.push(request.await.unwrap());
    }
    durations
}

fn p95(durations: &[std::time::Duration]) -> std::time::Duration {
    let mut ordered = durations.to_vec();
    ordered.sort_unstable();
    ordered[(ordered.len() * 95).div_ceil(100).saturating_sub(1)]
}
