use std::{
    collections::VecDeque,
    fs,
    future::Future,
    path::Path,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex as StdMutex,
    },
};

use tokio::sync::Semaphore;

use super::*;
use crate::lichess::{LichessExportRequest, LICHESS_PGN_MEDIA_TYPE};

#[tokio::test]
async fn retries_only_connection_and_the_three_transient_gateway_statuses() {
    for retryable in [
        LichessExportError::Connection,
        status(502),
        status(503),
        status(504),
    ] {
        let client = ScriptedClient::new([
            ScriptedCall::immediate(Err(retryable)),
            ScriptedCall::immediate(Ok(canonical_response())),
        ]);
        let gateway = LichessImportGateway::with_policy(client.clone(), test_policy());

        gateway
            .import(&canonical_source(), &|_| {})
            .await
            .expect("the one allowed retry should recover");

        assert_eq!(client.call_count(), 2);
    }

    for not_retryable in [
        LichessExportError::Timeout,
        LichessExportError::Transport("broken stream".to_string()),
        status(400),
        status(500),
        LichessExportError::ResponseTooLarge {
            limit_bytes: LICHESS_MAX_RESPONSE_BYTES,
        },
    ] {
        let client = ScriptedClient::new([
            ScriptedCall::immediate(Err(not_retryable)),
            ScriptedCall::immediate(Ok(canonical_response())),
        ]);
        let gateway = LichessImportGateway::with_policy(client.clone(), test_policy());

        gateway
            .import(&canonical_source(), &|_| {})
            .await
            .expect_err("this failure must not be retried");

        assert_eq!(client.call_count(), 1);
    }
}

#[tokio::test]
async fn identical_in_flight_imports_share_one_leader_even_if_callers_differ() {
    let gate = Arc::new(Semaphore::new(0));
    let client = ScriptedClient::new([ScriptedCall::gated(
        Ok(canonical_response()),
        Arc::clone(&gate),
    )]);
    let gateway = Arc::new(LichessImportGateway::with_policy(
        client.clone(),
        test_policy(),
    ));
    let first = spawn_import(Arc::clone(&gateway), canonical_source());
    wait_for_calls(&client, 1).await;
    let second = spawn_import(Arc::clone(&gateway), canonical_source());
    tokio::task::yield_now().await;
    gate.add_permits(1);

    let first = first.await.unwrap().unwrap();
    let second = second.await.unwrap().unwrap();

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(client.call_count(), 1);
}

#[tokio::test]
async fn leader_fetch_survives_the_first_callers_cancellation() {
    let gate = Arc::new(Semaphore::new(0));
    let client = ScriptedClient::new([ScriptedCall::gated(
        Ok(canonical_response()),
        Arc::clone(&gate),
    )]);
    let gateway = Arc::new(LichessImportGateway::with_policy(
        client.clone(),
        test_policy(),
    ));
    let first = spawn_import(Arc::clone(&gateway), canonical_source());
    wait_for_calls(&client, 1).await;
    first.abort();
    let replacement = spawn_import(Arc::clone(&gateway), canonical_source());
    gate.add_permits(1);

    replacement.await.unwrap().unwrap();

    assert_eq!(client.call_count(), 1);
}

#[tokio::test]
async fn different_keys_never_overlap_outbound_requests() {
    let gate = Arc::new(Semaphore::new(0));
    let client = ScriptedClient::new([
        ScriptedCall::gated(Ok(canonical_response()), Arc::clone(&gate)),
        ScriptedCall::immediate(Err(LichessExportError::Transport("second key".to_string()))),
    ]);
    let gateway = Arc::new(LichessImportGateway::with_policy(
        client.clone(),
        test_policy(),
    ));
    let first = spawn_import(Arc::clone(&gateway), canonical_source());
    wait_for_calls(&client, 1).await;
    let second = spawn_import(
        Arc::clone(&gateway),
        LichessGameUrl::parse("https://lichess.org/abcdefgh").unwrap(),
    );
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(client.call_count(), 1);
    gate.add_permits(1);

    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap_err();

    assert_eq!(client.max_active(), 1);
    assert_eq!(client.call_count(), 2);
}

#[tokio::test]
async fn validated_game_is_reused_from_the_exact_key_cache() {
    let client = ScriptedClient::new([ScriptedCall::immediate(Ok(canonical_response()))]);
    let gateway = LichessImportGateway::with_policy(client.clone(), test_policy());

    let first = gateway.import(&canonical_source(), &|_| {}).await.unwrap();
    let second = gateway.import(&canonical_source(), &|_| {}).await.unwrap();

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(client.call_count(), 1);
}

#[test]
fn cache_enforces_ttl_lru_entry_count_and_memory_budget() {
    let source = canonical_source();
    let game = Arc::new(prepare_lichess_game(&source, canonical_response()).unwrap());
    let now = Instant::now();
    let first = CacheKey::new("first001");
    let second = CacheKey::new("second02");
    let third = CacheKey::new("third003");
    let mut lru = GameCache::new(Duration::from_secs(10), 2, usize::MAX);
    lru.insert(first.clone(), Arc::clone(&game), now);
    lru.insert(second.clone(), Arc::clone(&game), now);
    assert!(lru.get(&first, now).is_some());
    lru.insert(third.clone(), Arc::clone(&game), now);
    assert!(lru.entries.contains_key(&first));
    assert!(!lru.entries.contains_key(&second));
    assert!(lru.entries.contains_key(&third));

    let weight = game.cache_weight();
    let mut bounded = GameCache::new(Duration::from_secs(10), 3, weight * 2 - 1);
    bounded.insert(first.clone(), Arc::clone(&game), now);
    bounded.insert(second, Arc::clone(&game), now);
    assert!(!bounded.entries.contains_key(&first));
    assert_eq!(bounded.entries.len(), 1);

    let mut expiring = GameCache::new(Duration::from_millis(5), 3, usize::MAX);
    expiring.insert(third.clone(), game, now);
    assert!(expiring
        .get(&third, now + Duration::from_millis(5))
        .is_none());
}

#[tokio::test]
async fn invalid_response_never_enters_the_game_cache() {
    let invalid = LichessExportResponse {
        body: b"not a PGN".to_vec(),
        content_type: LICHESS_PGN_MEDIA_TYPE.to_string(),
        captured_at: captured_at(),
    };
    let client = ScriptedClient::new([
        ScriptedCall::immediate(Ok(invalid.clone())),
        ScriptedCall::immediate(Ok(invalid)),
    ]);
    let gateway = LichessImportGateway::with_policy(client.clone(), test_policy());

    gateway
        .import(&canonical_source(), &|_| {})
        .await
        .unwrap_err();
    wait_for_idle(&gateway).await;
    gateway
        .import(&canonical_source(), &|_| {})
        .await
        .unwrap_err();

    assert_eq!(client.call_count(), 2);
}

#[tokio::test]
async fn cooldown_blocks_every_key_without_a_probe() {
    let client = ScriptedClient::new([ScriptedCall::immediate(Err(status(429)))]);
    let gateway = LichessImportGateway::with_policy(client.clone(), test_policy());

    assert!(matches!(
        gateway.import(&canonical_source(), &|_| {}).await,
        Err(LichessImportError::RateLimited { .. })
    ));
    assert!(matches!(
        gateway
            .import(
                &LichessGameUrl::parse("https://lichess.org/abcdefgh").unwrap(),
                &|_| {}
            )
            .await,
        Err(LichessImportError::RateLimited { .. })
    ));

    assert_eq!(client.call_count(), 1);
}

#[tokio::test]
async fn queued_key_rechecks_cooldown_after_the_active_request_gets_429() {
    let gate = Arc::new(Semaphore::new(0));
    let client = ScriptedClient::new([ScriptedCall::gated(Err(status(429)), Arc::clone(&gate))]);
    let gateway = Arc::new(LichessImportGateway::with_policy(
        client.clone(),
        test_policy(),
    ));
    let first = spawn_import(Arc::clone(&gateway), canonical_source());
    wait_for_calls(&client, 1).await;
    let queued = spawn_import(
        Arc::clone(&gateway),
        LichessGameUrl::parse("https://lichess.org/abcdefgh").unwrap(),
    );
    gate.add_permits(1);

    assert!(matches!(
        first.await.unwrap(),
        Err(LichessImportError::RateLimited { .. })
    ));
    assert!(matches!(
        queued.await.unwrap(),
        Err(LichessImportError::RateLimited { .. })
    ));
    assert_eq!(client.call_count(), 1);
}

#[tokio::test]
async fn repeated_rate_limits_double_the_local_cooldown_to_the_policy_cap() {
    let client = ScriptedClient::new([
        ScriptedCall::immediate(Err(status(429))),
        ScriptedCall::immediate(Err(status(429))),
        ScriptedCall::immediate(Err(status(429))),
    ]);
    let mut policy = test_policy();
    policy.cooldown_min = Duration::from_millis(20);
    policy.cooldown_max = Duration::from_millis(40);
    let gateway = LichessImportGateway::with_policy(client.clone(), policy);

    gateway
        .import(&canonical_source(), &|_| {})
        .await
        .unwrap_err();
    assert_eq!(cooldown_delay(&gateway).await, Duration::from_millis(20));
    tokio::time::sleep(Duration::from_millis(25)).await;
    gateway
        .import(&canonical_source(), &|_| {})
        .await
        .unwrap_err();
    assert_eq!(cooldown_delay(&gateway).await, Duration::from_millis(40));
    tokio::time::sleep(Duration::from_millis(45)).await;
    gateway
        .import(&canonical_source(), &|_| {})
        .await
        .unwrap_err();
    assert_eq!(cooldown_delay(&gateway).await, Duration::from_millis(40));

    assert_eq!(client.call_count(), 3);
}

#[tokio::test]
async fn retry_after_longer_than_the_local_backoff_cap_is_honored() {
    let client = ScriptedClient::new([ScriptedCall::immediate(Err(LichessExportError::Status {
        code: 429,
        retry_after_seconds: Some(1),
    }))]);
    let mut policy = test_policy();
    policy.cooldown_min = Duration::from_millis(20);
    policy.cooldown_max = Duration::from_millis(40);
    let gateway = LichessImportGateway::with_policy(client, policy);

    gateway
        .import(&canonical_source(), &|_| {})
        .await
        .unwrap_err();

    assert_eq!(cooldown_delay(&gateway).await, Duration::from_secs(1));
}

#[tokio::test]
async fn first_fetch_reports_waiting_fetching_and_validating_in_order() {
    let client = ScriptedClient::new([ScriptedCall::immediate(Ok(canonical_response()))]);
    let gateway = LichessImportGateway::with_policy(client, test_policy());
    let progress = StdMutex::new(Vec::new());

    gateway
        .import(&canonical_source(), &|stage| {
            progress.lock().unwrap().push(stage)
        })
        .await
        .unwrap();

    assert_eq!(
        *progress.lock().unwrap(),
        vec![
            ImportProgressStage::WaitingForLichess,
            ImportProgressStage::FetchingGame,
            ImportProgressStage::ValidatingGame,
        ]
    );
}

#[derive(Clone)]
struct ScriptedClient {
    shared: Arc<ScriptedClientState>,
}

struct ScriptedClientState {
    calls: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
    script: StdMutex<VecDeque<ScriptedCall>>,
}

#[derive(Clone)]
struct ScriptedCall {
    result: Result<LichessExportResponse, LichessExportError>,
    gate: Option<Arc<Semaphore>>,
}

impl ScriptedCall {
    fn immediate(result: Result<LichessExportResponse, LichessExportError>) -> Self {
        Self { result, gate: None }
    }

    fn gated(
        result: Result<LichessExportResponse, LichessExportError>,
        gate: Arc<Semaphore>,
    ) -> Self {
        Self {
            result,
            gate: Some(gate),
        }
    }
}

impl ScriptedClient {
    fn new(script: impl IntoIterator<Item = ScriptedCall>) -> Self {
        Self {
            shared: Arc::new(ScriptedClientState {
                calls: AtomicUsize::new(0),
                active: AtomicUsize::new(0),
                max_active: AtomicUsize::new(0),
                script: StdMutex::new(script.into_iter().collect()),
            }),
        }
    }

    fn call_count(&self) -> usize {
        self.shared.calls.load(Ordering::SeqCst)
    }

    fn max_active(&self) -> usize {
        self.shared.max_active.load(Ordering::SeqCst)
    }
}

impl LichessExportClient for ScriptedClient {
    fn export<'a>(
        &'a self,
        _request: &'a LichessExportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LichessExportResponse, LichessExportError>> + Send + 'a>>
    {
        Box::pin(async move {
            let call = self
                .shared
                .script
                .lock()
                .unwrap()
                .pop_front()
                .expect("script must cover every outbound request");
            self.shared.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.shared.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.shared.max_active.fetch_max(active, Ordering::SeqCst);
            let _active = ActiveCall(Arc::clone(&self.shared));
            if let Some(gate) = call.gate {
                gate.acquire().await.unwrap().forget();
            }
            call.result
        })
    }
}

struct ActiveCall(Arc<ScriptedClientState>);

impl Drop for ActiveCall {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

fn spawn_import(
    gateway: Arc<LichessImportGateway<ScriptedClient>>,
    source: LichessGameUrl,
) -> tokio::task::JoinHandle<Result<Arc<PreparedLichessGame>, LichessImportError>> {
    tokio::spawn(async move { gateway.import(&source, &|_| {}).await })
}

async fn wait_for_calls(client: &ScriptedClient, expected: usize) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while client.call_count() < expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("scripted call did not start");
}

async fn wait_for_idle(gateway: &LichessImportGateway<ScriptedClient>) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if gateway.inner.state.lock().await.in_flight.is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("gateway leader did not finish");
}

async fn cooldown_delay(gateway: &LichessImportGateway<ScriptedClient>) -> Duration {
    gateway
        .inner
        .state
        .lock()
        .await
        .cooldown
        .as_ref()
        .unwrap()
        .delay
}

fn test_policy() -> GatewayPolicy {
    GatewayPolicy {
        retry_min: Duration::ZERO,
        retry_max: Duration::ZERO,
        cooldown_min: Duration::from_secs(1),
        cooldown_max: Duration::from_secs(2),
        cache_ttl: Duration::from_secs(60),
        cache_max_games: 8,
        cache_max_bytes: 4 * 1024 * 1024,
    }
}

fn status(code: u16) -> LichessExportError {
    LichessExportError::Status {
        code,
        retry_after_seconds: None,
    }
}

fn canonical_source() -> LichessGameUrl {
    LichessGameUrl::parse("https://lichess.org/Synthet1").unwrap()
}

fn canonical_response() -> LichessExportResponse {
    LichessExportResponse {
        body: fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages/shared-assets/fixtures/Synthet1/lichess-export.raw.pgn"),
        )
        .unwrap(),
        content_type: LICHESS_PGN_MEDIA_TYPE.to_string(),
        captured_at: captured_at(),
    }
}

fn captured_at() -> DateTime<Utc> {
    "2026-09-03T00:00:00Z".parse().unwrap()
}
