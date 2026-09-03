use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Instant,
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    HumanMoveCacheIdentity, HumanMoveInput, HumanMoveModel, HumanMoveModelError,
    HumanMovePrediction,
};
use crate::{
    request_single_flight::SingleFlight,
    review_session_contract::{CanonicalPosition, PositionContentId},
};

const CACHE_SCHEMA: &str = "exact-human-move-cache/v1";
const DEFAULT_MAX_BYTES: usize = 64 * 1024 * 1024;
const ENTRY_OVERHEAD_BYTES: usize = 512;

pub struct ExactHumanMoveCache {
    inner: Arc<dyn HumanMoveModel>,
    cache: Mutex<HumanMoveCache>,
    in_flight: SingleFlight<CacheKey>,
}

impl ExactHumanMoveCache {
    pub fn new(inner: Arc<dyn HumanMoveModel>) -> Self {
        Self::with_max_bytes(inner, DEFAULT_MAX_BYTES)
    }

    fn with_max_bytes(inner: Arc<dyn HumanMoveModel>, max_bytes: usize) -> Self {
        Self {
            inner,
            cache: Mutex::new(HumanMoveCache::new(max_bytes)),
            in_flight: SingleFlight::default(),
        }
    }

    fn lookup(&self, key: &CacheKey) -> Option<HumanMovePrediction> {
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
    }

    fn insert(&self, key: CacheKey, prediction: HumanMovePrediction) {
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, prediction);
    }
}

impl HumanMoveModel for ExactHumanMoveCache {
    fn provider_name(&self) -> &'static str {
        self.inner.provider_name()
    }

    fn predict<'a>(
        &'a self,
        input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        Box::pin(async move {
            let Some(identity) = self.inner.cache_identity() else {
                return self.inner.predict(input).await;
            };
            // A position the Position Snapshot builder cannot accept has no
            // content identity to key on. The model rejects it on its own, so
            // the request passes through uncached rather than keying on the raw
            // string.
            let Ok(canonical) = CanonicalPosition::from_fen(input.position).inspect_err(|error| {
                tracing::debug!(
                    error = %error,
                    "human move cache skipped a position without a content identity"
                );
            }) else {
                return self.inner.predict(input).await;
            };
            let key = CacheKey::new(&identity, &canonical.content_id, input);
            // The model is given the canonical position the key names, never the
            // caller's spelling of it, so a shared entry is always the
            // prediction for the position it is filed under.
            let canonical_input = HumanMoveInput {
                position: &canonical.fen,
                ..input
            };
            let started_at = Instant::now();
            let mut duplicate_collapsed = false;
            loop {
                if let Some(prediction) = self.lookup(&key) {
                    tracing::info!(
                        event = "coach_human_move_cache_lookup",
                        provider = self.inner.provider_name(),
                        cache_key = key.digest(),
                        cache_hit = true,
                        duplicate_collapsed,
                        wait_milliseconds = started_at.elapsed().as_millis(),
                        "exact human move prediction cache lookup"
                    );
                    return Ok(prediction);
                }
                match self.in_flight.register(key.clone()) {
                    Ok(_leader) => {
                        let prediction = self.inner.predict(canonical_input).await?;
                        self.insert(key.clone(), prediction.clone());
                        tracing::info!(
                            event = "coach_human_move_cache_lookup",
                            provider = self.inner.provider_name(),
                            cache_key = key.digest(),
                            cache_hit = false,
                            duplicate_collapsed,
                            wait_milliseconds = started_at.elapsed().as_millis(),
                            "exact human move prediction cache lookup"
                        );
                        return Ok(prediction);
                    }
                    Err(waiter) => {
                        duplicate_collapsed = true;
                        waiter.wait().await;
                    }
                }
            }
        })
    }

    fn cache_identity(&self) -> Option<HumanMoveCacheIdentity> {
        self.inner.cache_identity()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CacheKey(String);

impl CacheKey {
    /// The key is the model identity, the Position Snapshot content id, and the
    /// two request parameters the prediction depends on. The Elo is the review
    /// strength the model plays at, not a Player attribute, so the key stays
    /// identity-free: no Player, Game Import, Review, or Review Session material
    /// enters it.
    ///
    fn new(
        identity: &HumanMoveCacheIdentity,
        content_id: &PositionContentId,
        input: HumanMoveInput<'_>,
    ) -> Self {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct KeyMaterial<'a> {
            schema: &'static str,
            identity: &'a HumanMoveCacheIdentity,
            position_content_id: &'a PositionContentId,
            player_elo: u16,
            candidate_limit: u8,
        }

        let material = serde_json::to_vec(&KeyMaterial {
            schema: CACHE_SCHEMA,
            identity,
            position_content_id: content_id,
            player_elo: input.elo.rating(),
            candidate_limit: input.limit,
        })
        .expect("human move cache keys are serializable");
        Self(format!("{:x}", Sha256::digest(material)))
    }

    fn digest(&self) -> &str {
        &self.0
    }
}

struct HumanMoveCache {
    entries: HashMap<CacheKey, CacheEntry>,
    recency: VecDeque<(CacheKey, u64)>,
    max_bytes: usize,
    used_bytes: usize,
    access_clock: u64,
}

struct CacheEntry {
    prediction: HumanMovePrediction,
    last_access: u64,
    bytes: usize,
}

impl HumanMoveCache {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            recency: VecDeque::new(),
            max_bytes,
            used_bytes: 0,
            access_clock: 0,
        }
    }

    fn get(&mut self, key: &CacheKey) -> Option<HumanMovePrediction> {
        self.access_clock = self.access_clock.wrapping_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.last_access = self.access_clock;
        self.recency.push_back((key.clone(), self.access_clock));
        let prediction = entry.prediction.clone();
        self.compact_recency();
        Some(prediction)
    }

    fn insert(&mut self, key: CacheKey, prediction: HumanMovePrediction) {
        let bytes = cache_weight(&key, &prediction);
        if bytes > self.max_bytes || self.max_bytes == 0 {
            return;
        }
        if let Some(replaced) = self.entries.remove(&key) {
            self.used_bytes = self.used_bytes.saturating_sub(replaced.bytes);
        }
        self.access_clock = self.access_clock.wrapping_add(1);
        self.used_bytes += bytes;
        self.recency.push_back((key.clone(), self.access_clock));
        self.entries.insert(
            key,
            CacheEntry {
                prediction,
                last_access: self.access_clock,
                bytes,
            },
        );
        self.evict_to_budget();
        self.compact_recency();
    }

    fn evict_to_budget(&mut self) {
        while self.used_bytes > self.max_bytes {
            let Some((key, generation)) = self.recency.pop_front() else {
                break;
            };
            if self
                .entries
                .get(&key)
                .is_none_or(|entry| entry.last_access != generation)
            {
                continue;
            }
            let removed = self
                .entries
                .remove(&key)
                .expect("the current least-recently-used entry remains present");
            self.used_bytes = self.used_bytes.saturating_sub(removed.bytes);
        }
        if self.entries.is_empty() {
            self.recency.clear();
        }
    }

    fn compact_recency(&mut self) {
        let maximum_records = self.entries.len().saturating_mul(4).saturating_add(64);
        if self.recency.len() <= maximum_records {
            return;
        }
        self.recency.retain(|(key, generation)| {
            self.entries
                .get(key)
                .is_some_and(|entry| entry.last_access == *generation)
        });
    }
}

fn cache_weight(key: &CacheKey, prediction: &HumanMovePrediction) -> usize {
    serde_json::to_vec(prediction)
        .expect("human move cache payloads are serializable")
        .len()
        .saturating_add(key.0.len())
        .saturating_add(ENTRY_OVERHEAD_BYTES)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::Semaphore;

    use super::*;
    use crate::domain::{EloProfile, HumanMoveCandidate};

    struct BlockingModel {
        calls: Arc<AtomicUsize>,
        release: Arc<Semaphore>,
    }

    impl HumanMoveModel for BlockingModel {
        fn predict<'a>(
            &'a self,
            _input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::Release);
                let permit = self.release.acquire().await.expect("test pool is open");
                permit.forget();
                Ok(prediction())
            })
        }

        fn cache_identity(&self) -> Option<HumanMoveCacheIdentity> {
            Some(identity("model-a"))
        }
    }

    struct FailingModel {
        calls: Arc<AtomicUsize>,
    }

    /// Records the position it was handed, so a test can assert the model sees
    /// the canonical position its cache key names.
    #[derive(Default)]
    struct RecordingModel {
        positions: Mutex<Vec<String>>,
    }

    impl HumanMoveModel for RecordingModel {
        fn predict<'a>(
            &'a self,
            input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async move {
                self.positions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(input.position.to_owned());
                Ok(prediction())
            })
        }

        fn cache_identity(&self) -> Option<HumanMoveCacheIdentity> {
            Some(identity("model-a"))
        }
    }

    impl HumanMoveModel for FailingModel {
        fn predict<'a>(
            &'a self,
            _input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::Relaxed);
                Err(HumanMoveModelError::InvalidResponse(
                    "scripted failure".to_owned(),
                ))
            })
        }

        fn cache_identity(&self) -> Option<HumanMoveCacheIdentity> {
            Some(identity("model-a"))
        }
    }

    /// Positions are real FENs because the cache keys on Position Snapshot
    /// content: a placeholder string has no content identity and is
    /// deliberately not cached.
    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    const AFTER_E4: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";

    /// The Sicilian after 1.e4 c5 2.Nf3, as two different reviews reach it. Same
    /// board, same rights, same halfmove clock; different move number, and one
    /// of them records an en passant square nothing can capture. The model is
    /// handed a FEN and nothing else, so neither difference may cost a second
    /// prediction.
    const SICILIAN_IN_ONE_REVIEW: &str =
        "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b Kkq - 1 3";
    const SICILIAN_IN_ANOTHER_REVIEW: &str =
        "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b Kkq e3 1 17";

    fn key(model: &str, position: &'static str, elo: u16, limit: u8) -> CacheKey {
        let canonical =
            CanonicalPosition::from_fen(position).expect("the test position is a valid FEN");
        CacheKey::new(
            &identity(model),
            &canonical.content_id,
            input(position, elo, limit),
        )
    }

    #[test]
    fn key_covers_position_content_elo_limit_and_model_identity() {
        assert_ne!(
            key("model-a", START, 1200, 5),
            key("model-a", AFTER_E4, 1200, 5)
        );
        assert_ne!(
            key("model-a", START, 1200, 5),
            key("model-a", START, 1300, 5)
        );
        assert_ne!(
            key("model-a", START, 1200, 5),
            key("model-a", START, 1200, 3)
        );
        assert_ne!(
            key("model-a", START, 1200, 5),
            key("model-b", START, 1200, 5)
        );
    }

    #[test]
    fn the_same_position_from_two_reviews_is_one_key() {
        assert_eq!(
            key("model-a", SICILIAN_IN_ONE_REVIEW, 1200, 5),
            key("model-a", SICILIAN_IN_ANOTHER_REVIEW, 1200, 5)
        );
    }

    #[tokio::test]
    async fn the_model_is_given_the_canonical_position_its_key_names() {
        let model = Arc::new(RecordingModel::default());
        let cache = ExactHumanMoveCache::with_max_bytes(model.clone(), 4096);

        // This spelling records an en passant square nothing can capture, so it
        // is not the canonical position the key identifies. Sharing the entry is
        // only sound if the model saw the canonical position.
        cache
            .predict(input(SICILIAN_IN_ANOTHER_REVIEW, 1200, 5))
            .await
            .unwrap();

        let canonical = CanonicalPosition::from_fen(SICILIAN_IN_ANOTHER_REVIEW).unwrap();
        assert!(!canonical.fen.contains("e3"));
        assert_eq!(model.positions.lock().unwrap().as_slice(), [canonical.fen]);
    }

    #[tokio::test]
    async fn concurrent_predictions_spelling_one_position_differently_collapse() {
        let calls = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Semaphore::new(0));
        let cache = Arc::new(ExactHumanMoveCache::with_max_bytes(
            Arc::new(BlockingModel {
                calls: calls.clone(),
                release: release.clone(),
            }),
            4096,
        ));

        // Neither request can find the other's result in the store, because
        // neither has finished. Collapsing them is the single-flight key's job,
        // so it has to name the position by content and not by its spelling.
        let first = tokio::spawn({
            let cache = cache.clone();
            async move { cache.predict(input(SICILIAN_IN_ONE_REVIEW, 1200, 5)).await }
        });
        wait_for_calls(&calls, 1).await;
        let second = tokio::spawn({
            let cache = cache.clone();
            async move {
                cache
                    .predict(input(SICILIAN_IN_ANOTHER_REVIEW, 1200, 5))
                    .await
            }
        });
        tokio::task::yield_now().await;
        assert_eq!(calls.load(Ordering::Acquire), 1);
        release.add_permits(1);

        assert_eq!(first.await.unwrap().unwrap(), prediction());
        assert_eq!(second.await.unwrap().unwrap(), prediction());
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn a_position_without_content_identity_passes_through_uncached() {
        let model = Arc::new(RecordingModel::default());
        let cache = ExactHumanMoveCache::with_max_bytes(model.clone(), 4096);

        for _ in 0..2 {
            cache.predict(input("not-a-fen", 1200, 5)).await.unwrap();
        }

        // The model decides what to do with an unusable position; the cache
        // neither answers for it nor stores anything under a guessed key.
        assert_eq!(model.positions.lock().unwrap().len(), 2);
        assert!(cache
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entries
            .is_empty());
    }

    #[tokio::test]
    async fn concurrent_equivalent_predictions_collapse_to_one_provider_request() {
        let calls = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Semaphore::new(0));
        let cache = Arc::new(ExactHumanMoveCache::with_max_bytes(
            Arc::new(BlockingModel {
                calls: calls.clone(),
                release: release.clone(),
            }),
            4096,
        ));

        let requests = (0..8)
            .map(|_| {
                let cache = cache.clone();
                tokio::spawn(async move { cache.predict(input(START, 1200, 5)).await })
            })
            .collect::<Vec<_>>();
        wait_for_calls(&calls, 1).await;
        tokio::task::yield_now().await;
        assert_eq!(calls.load(Ordering::Acquire), 1);
        release.add_permits(1);

        for request in requests {
            assert_eq!(request.await.unwrap().unwrap(), prediction());
        }
        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert_eq!(
            cache.predict(input(START, 1200, 5)).await.unwrap(),
            prediction()
        );
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn failed_predictions_are_not_cached() {
        let calls = Arc::new(AtomicUsize::new(0));
        let cache = ExactHumanMoveCache::with_max_bytes(
            Arc::new(FailingModel {
                calls: calls.clone(),
            }),
            4096,
        );

        for _ in 0..2 {
            assert!(cache.predict(input(START, 1200, 5)).await.is_err());
        }
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert!(cache
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entries
            .is_empty());
    }

    fn identity(model: &str) -> HumanMoveCacheIdentity {
        HumanMoveCacheIdentity {
            provider: "Maia".to_owned(),
            package: "maia2==0.11.0".to_owned(),
            model: model.to_owned(),
            image: "image".to_owned(),
            model_digest: "model-digest".to_owned(),
            config_digest: "config-digest".to_owned(),
        }
    }

    fn input(position: &'static str, elo: u16, limit: u8) -> HumanMoveInput<'static> {
        HumanMoveInput {
            position,
            elo: EloProfile::try_from(elo).unwrap(),
            limit,
        }
    }

    fn prediction() -> HumanMovePrediction {
        HumanMovePrediction {
            candidates: vec![HumanMoveCandidate {
                uci: "e2e4".to_owned(),
                probability: 0.5,
                rank: 1,
            }],
            win_probability: Some(0.51),
        }
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
}
