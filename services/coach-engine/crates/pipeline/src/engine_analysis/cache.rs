use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Instant,
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalysisOutput, EngineAnalyzer,
    EngineCacheIdentity, EngineMultiPvOutput, EngineProvenance, IndexedEngineAnalysisError,
    TimedEngineAnalysis,
};
use crate::{request_single_flight::SingleFlight, review_session_contract::CanonicalPosition};

use store::{CacheKey, EngineCache, DEFAULT_MAX_BYTES};

use batch::BatchPlan;

mod batch;
mod store;

#[cfg(test)]
mod tests;

pub struct ExactEngineCache {
    inner: Arc<dyn EngineAnalyzer>,
    cache: Mutex<EngineCache>,
    in_flight: SingleFlight<CacheKey>,
    batch_in_flight: SingleFlight<String>,
}

impl ExactEngineCache {
    pub fn new(inner: Arc<dyn EngineAnalyzer>) -> Self {
        Self::with_max_bytes(inner, DEFAULT_MAX_BYTES)
    }

    fn with_max_bytes(inner: Arc<dyn EngineAnalyzer>, max_bytes: usize) -> Self {
        Self {
            inner,
            cache: Mutex::new(EngineCache::new(max_bytes)),
            in_flight: SingleFlight::default(),
            batch_in_flight: SingleFlight::default(),
        }
    }

    fn lookup(&self, key: &CacheKey) -> Option<EngineAnalysisOutput> {
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
    }

    fn insert(&self, identity: &EngineCacheIdentity, key: CacheKey, output: EngineAnalysisOutput) {
        let Some(provenance) = output
            .provenance
            .as_ref()
            .filter(|provenance| identity.matches(provenance))
        else {
            tracing::warn!(
                provider = self.inner.provider_name(),
                "engine cache skipped an analysis without matching provenance"
            );
            return;
        };
        debug_assert!(!provenance.version.is_empty());
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, output);
    }
}

/// The canonical FEN to analyze and the key to file the result under.
///
/// A position the Position Snapshot builder cannot accept is not cacheable: it
/// has no content identity to key on. The provider rejects it a moment later
/// with its own error, so the request passes through uncached rather than
/// inventing a key from the raw string.
fn cacheable_position(
    identity: &EngineCacheIdentity,
    position: &str,
) -> Option<(String, CacheKey)> {
    match CanonicalPosition::from_fen(position) {
        Ok(canonical) => Some((
            canonical.fen,
            CacheKey::new(identity, &canonical.content_id),
        )),
        Err(error) => {
            tracing::debug!(
                error = %error,
                "engine cache skipped a position without a content identity"
            );
            None
        }
    }
}

impl EngineAnalyzer for ExactEngineCache {
    fn provider_name(&self) -> &'static str {
        self.inner.provider_name()
    }

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
            let Some(identity) = self.inner.cache_identity() else {
                return self.inner.analyze_with_provenance(input).await;
            };
            let Some((canonical_fen, key)) = cacheable_position(&identity, input.position) else {
                return self.inner.analyze_with_provenance(input).await;
            };
            // The engine is given the canonical position the key names, never
            // the caller's spelling of it, so a shared entry is always the
            // analysis of the position it is filed under.
            let canonical_input = EngineAnalysisInput {
                position: &canonical_fen,
            };
            let started_at = Instant::now();
            let mut duplicate_collapsed = false;
            loop {
                if let Some(output) = self.lookup(&key) {
                    tracing::info!(
                        event = "coach_engine_cache_lookup",
                        provider = self.inner.provider_name(),
                        cache_key = key.digest(),
                        cache_hit = true,
                        duplicate_collapsed,
                        wait_milliseconds = started_at.elapsed().as_millis(),
                        "exact engine analysis cache lookup"
                    );
                    return Ok(output);
                }
                match self.in_flight.register(key.clone()) {
                    Ok(_leader) => {
                        let output = self.inner.analyze_with_provenance(canonical_input).await?;
                        self.insert(&identity, key.clone(), output.clone());
                        tracing::info!(
                            event = "coach_engine_cache_lookup",
                            provider = self.inner.provider_name(),
                            cache_key = key.digest(),
                            cache_hit = false,
                            duplicate_collapsed,
                            wait_milliseconds = started_at.elapsed().as_millis(),
                            "exact engine analysis cache lookup"
                        );
                        return Ok(output);
                    }
                    Err(waiter) => {
                        duplicate_collapsed = true;
                        waiter.wait().await;
                    }
                }
            }
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        self.inner.provenance()
    }

    fn cache_identity(&self) -> Option<EngineCacheIdentity> {
        self.inner.cache_identity()
    }

    fn supports_multi_pv(&self) -> bool {
        self.inner.supports_multi_pv()
    }

    fn analyze_multi_pv<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
        variation_count: u8,
    ) -> Pin<Box<dyn Future<Output = Result<EngineMultiPvOutput, EngineAnalysisError>> + Send + 'a>>
    {
        self.inner.analyze_multi_pv(input, variation_count)
    }

    fn analyze_positions(
        self: Arc<Self>,
        positions: Vec<String>,
        concurrency: usize,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<TimedEngineAnalysis>, IndexedEngineAnalysisError>>
                + Send,
        >,
    > {
        Box::pin(async move {
            let Some(identity) = self.inner.cache_identity() else {
                return self
                    .inner
                    .clone()
                    .analyze_positions(positions, concurrency)
                    .await;
            };
            let position_count = positions.len();
            let started = Instant::now();
            let mut duplicate_collapsed = false;
            loop {
                let mut plan = BatchPlan::new(positions.clone(), &identity, &self.cache);
                let missing_positions = plan.positions_to_analyze();
                if missing_positions.is_empty() {
                    tracing::info!(
                        event = "coach_engine_cache_batch_completion",
                        provider = self.inner.provider_name(),
                        position_count,
                        cache_hits = plan.cache_hits(),
                        cache_misses = plan.cache_misses(),
                        coalesced = plan.coalesced(),
                        duplicate_collapsed,
                        wall_milliseconds = started.elapsed().as_millis(),
                        "exact engine analysis cache batch"
                    );
                    return plan.finish();
                }
                let flight_key = batch_flight_key(&identity, &plan.flight_material());
                match self.batch_in_flight.register(flight_key) {
                    Ok(_leader) => {
                        let analyzed = self
                            .inner
                            .clone()
                            .analyze_positions(missing_positions, concurrency)
                            .await
                            .map_err(|error| plan.original_error(error))?;
                        plan.merge(analyzed, |key, output| {
                            self.insert(&identity, key, output);
                        })?;
                        tracing::info!(
                            event = "coach_engine_cache_batch_completion",
                            provider = self.inner.provider_name(),
                            position_count,
                            cache_hits = plan.cache_hits(),
                            cache_misses = plan.cache_misses(),
                            coalesced = plan.coalesced(),
                            duplicate_collapsed,
                            wall_milliseconds = started.elapsed().as_millis(),
                            "exact engine analysis cache batch"
                        );
                        return plan.finish();
                    }
                    Err(waiter) => {
                        duplicate_collapsed = true;
                        waiter.wait().await;
                    }
                }
            }
        })
    }
}

fn batch_flight_key(identity: &EngineCacheIdentity, positions: &[&str]) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct BatchKey<'a> {
        schema: &'static str,
        identity: &'a EngineCacheIdentity,
        positions: &'a [&'a str],
    }

    let encoded = serde_json::to_vec(&BatchKey {
        schema: "exact-engine-analysis-batch-flight/v1",
        identity,
        positions,
    })
    .expect("engine batch flight keys are serializable");
    format!("{:x}", Sha256::digest(encoded))
}
