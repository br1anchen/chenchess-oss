use std::{future::Future, pin::Pin, sync::Arc, time::Instant};

use anyhow::Context;
use tokio::sync::Semaphore;

use super::{
    EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalysisOutput, EngineAnalyzer,
    EngineCacheIdentity, EngineMultiPvOutput, EngineProvenance, IndexedEngineAnalysisError,
    TimedEngineAnalysis,
};

const DEFAULT_WORKERS: usize = 8;
const MAXIMUM_WORKERS: usize = 8;

pub struct EngineWorkerLimit {
    inner: Arc<dyn EngineAnalyzer>,
    workers: usize,
    permits: Arc<Semaphore>,
}

impl EngineWorkerLimit {
    pub fn from_env(inner: Arc<dyn EngineAnalyzer>) -> anyhow::Result<Self> {
        let configured = std::env::var("STOCKFISH_WORKERS").ok();
        let workers = configured_workers(configured.as_deref())?;
        Ok(Self::new_with_workers(inner, workers))
    }

    #[cfg(test)]
    fn new(inner: Arc<dyn EngineAnalyzer>, workers: usize) -> Self {
        assert!((1..=MAXIMUM_WORKERS).contains(&workers));
        Self::new_with_workers(inner, workers)
    }

    pub fn workers(&self) -> usize {
        self.workers
    }

    fn new_with_workers(inner: Arc<dyn EngineAnalyzer>, workers: usize) -> Self {
        Self {
            inner,
            workers,
            permits: Arc::new(Semaphore::new(workers)),
        }
    }
}

fn configured_workers(configured: Option<&str>) -> anyhow::Result<usize> {
    let workers = match configured {
        Some(configured) => configured
            .parse::<usize>()
            .context("STOCKFISH_WORKERS must be a whole number between 1 and 8")?,
        None => DEFAULT_WORKERS,
    };
    anyhow::ensure!(
        (1..=MAXIMUM_WORKERS).contains(&workers),
        "STOCKFISH_WORKERS must be between 1 and 8"
    );
    Ok(workers)
}

impl EngineAnalyzer for EngineWorkerLimit {
    fn provider_name(&self) -> &'static str {
        self.inner.provider_name()
    }

    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        let permits = self.permits.clone();
        let inner = self.inner.clone();
        Box::pin(async move {
            let _permit = permits.acquire_owned().await.map_err(|_| {
                EngineAnalysisError::Protocol("engine worker permit pool closed".to_owned())
            })?;
            inner.analyze(input).await
        })
    }

    fn analyze_with_provenance<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysisOutput, EngineAnalysisError>> + Send + 'a>>
    {
        let permits = self.permits.clone();
        let inner = self.inner.clone();
        Box::pin(async move {
            let _permit = permits.acquire_owned().await.map_err(|_| {
                EngineAnalysisError::Protocol("engine worker permit pool closed".to_owned())
            })?;
            inner.analyze_with_provenance(input).await
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
        let permits = self.permits.clone();
        let inner = self.inner.clone();
        Box::pin(async move {
            let _permit = permits.acquire_owned().await.map_err(|_| {
                EngineAnalysisError::Protocol("engine worker permit pool closed".to_owned())
            })?;
            inner.analyze_multi_pv(input, variation_count).await
        })
    }

    fn analyze_positions(
        self: Arc<Self>,
        positions: Vec<String>,
        requested_concurrency: usize,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<TimedEngineAnalysis>, IndexedEngineAnalysisError>>
                + Send,
        >,
    > {
        Box::pin(async move {
            let concurrency = requested_concurrency.min(self.workers).min(positions.len());
            if concurrency == 0 {
                return self.inner.clone().analyze_positions(positions, 0).await;
            }
            let queued_at = Instant::now();
            let _permits = self
                .permits
                .clone()
                .acquire_many_owned(concurrency as u32)
                .await
                .map_err(|_| IndexedEngineAnalysisError {
                    index: 0,
                    error: EngineAnalysisError::Protocol(
                        "engine worker permit pool closed".to_owned(),
                    ),
                })?;
            tracing::info!(
                event = "coach_engine_worker_admission_completion",
                requested_concurrency,
                configured_workers = self.workers,
                effective_concurrency = concurrency,
                position_count = positions.len(),
                queue_wait_milliseconds = queued_at.elapsed().as_millis(),
                "engine worker limit applied"
            );
            self.inner
                .clone()
                .analyze_positions(positions, concurrency)
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::Semaphore;

    use super::*;

    struct ConcurrencyProbe {
        observed: Arc<AtomicUsize>,
    }

    struct BlockingConcurrencyProbe {
        calls: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
        release: Arc<Semaphore>,
    }

    impl EngineAnalyzer for ConcurrencyProbe {
        fn analyze<'a>(
            &'a self,
            _input: EngineAnalysisInput<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
        {
            unreachable!("the worker limit test uses batched analysis")
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
            self.observed.store(concurrency, Ordering::Relaxed);
            Box::pin(async move {
                Ok(positions
                    .into_iter()
                    .map(|_| TimedEngineAnalysis {
                        analysis: EngineAnalysis {
                            best_move: "e2e4".to_owned(),
                            evaluation: super::super::PositionEvaluation::Centipawns(0),
                            principal_variation: vec!["e2e4".to_owned()],
                            depth: 16,
                        },
                        provenance: None,
                        duration: std::time::Duration::ZERO,
                    })
                    .collect())
            })
        }
    }

    impl EngineAnalyzer for BlockingConcurrencyProbe {
        fn analyze<'a>(
            &'a self,
            _input: EngineAnalysisInput<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
        {
            unreachable!("the worker limit test uses batched analysis")
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
                self.calls.fetch_add(1, Ordering::Release);
                let active = self.active.fetch_add(concurrency, Ordering::AcqRel) + concurrency;
                self.peak.fetch_max(active, Ordering::AcqRel);
                let permit = self.release.acquire().await.unwrap();
                permit.forget();
                self.active.fetch_sub(concurrency, Ordering::AcqRel);
                Ok(analyses_for(positions))
            })
        }
    }

    #[tokio::test]
    async fn configured_workers_cap_requested_batch_concurrency() {
        let observed = Arc::new(AtomicUsize::new(0));
        let analyzer = Arc::new(EngineWorkerLimit::new(
            Arc::new(ConcurrencyProbe {
                observed: observed.clone(),
            }),
            4,
        ));

        analyzer
            .analyze_positions(vec!["fen".to_owned(); 8], 8)
            .await
            .unwrap();

        assert_eq!(observed.load(Ordering::Relaxed), 4);
    }

    #[tokio::test]
    async fn configured_workers_are_shared_across_concurrent_batches() {
        let calls = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Semaphore::new(0));
        let analyzer = Arc::new(EngineWorkerLimit::new(
            Arc::new(BlockingConcurrencyProbe {
                calls: calls.clone(),
                active,
                peak: peak.clone(),
                release: release.clone(),
            }),
            2,
        ));

        let first = tokio::spawn(
            analyzer
                .clone()
                .analyze_positions(vec!["first".to_owned(); 2], 2),
        );
        wait_for_calls(&calls, 1).await;
        let second = tokio::spawn(
            analyzer
                .clone()
                .analyze_positions(vec!["second".to_owned(); 2], 2),
        );
        tokio::task::yield_now().await;
        assert_eq!(calls.load(Ordering::Acquire), 1);

        release.add_permits(1);
        wait_for_calls(&calls, 2).await;
        release.add_permits(1);

        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(peak.load(Ordering::Acquire), 2);
    }

    #[test]
    fn hosted_worker_setting_is_validated_against_the_certified_range() {
        assert_eq!(configured_workers(None).unwrap(), DEFAULT_WORKERS);
        assert_eq!(configured_workers(Some("4")).unwrap(), 4);
        assert!(configured_workers(Some("0")).is_err());
        assert!(configured_workers(Some("9")).is_err());
        assert!(configured_workers(Some("four")).is_err());
    }

    fn analyses_for(positions: Vec<String>) -> Vec<TimedEngineAnalysis> {
        positions
            .into_iter()
            .map(|_| TimedEngineAnalysis {
                analysis: EngineAnalysis {
                    best_move: "e2e4".to_owned(),
                    evaluation: super::super::PositionEvaluation::Centipawns(0),
                    principal_variation: vec!["e2e4".to_owned()],
                    depth: 16,
                },
                provenance: None,
                duration: std::time::Duration::ZERO,
            })
            .collect()
    }

    async fn wait_for_calls(calls: &AtomicUsize, expected: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while calls.load(Ordering::Acquire) != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the expected engine batch should start");
    }
}
