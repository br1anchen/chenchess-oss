use std::{future::Future, time::Instant};

use tokio::task::JoinSet;
use tracing::Instrument;

pub struct IndexedProviderError<E> {
    pub index: usize,
    pub error: E,
}

pub async fn collect_ordered_provider_positions<T, E, F, Fut>(
    positions: Vec<String>,
    concurrency: usize,
    analyze: F,
) -> Result<Vec<T>, IndexedProviderError<E>>
where
    T: Send + 'static,
    E: Send + 'static,
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
{
    let item_count = positions.len();
    let effective_concurrency = concurrency.min(item_count);
    let mut telemetry =
        ProviderFanoutTelemetry::new(item_count, concurrency, effective_concurrency);
    let mut results = (0..positions.len()).map(|_| None).collect::<Vec<_>>();
    let mut positions = positions.into_iter().enumerate();
    let mut in_flight = JoinSet::new();
    let mut observed_error = false;

    loop {
        while !observed_error && in_flight.len() < concurrency {
            let Some((index, position)) = positions.next() else {
                break;
            };
            let future = analyze(position);
            in_flight
                .spawn(async move { (index, future.await) }.instrument(tracing::Span::current()));
            telemetry.observe_in_flight(in_flight.len());
        }

        let Some(completed) = in_flight.join_next().await else {
            break;
        };
        let (index, result) = completed.expect("provider task should not panic");
        observed_error |= result.is_err();
        telemetry.observe_completion();
        results[index] = Some(result);
    }

    let result = collect_ordered_results(results);
    telemetry.finish(result.is_ok());
    result
}

struct ProviderFanoutTelemetry {
    completed_count: usize,
    effective_concurrency: usize,
    item_count: usize,
    peak_in_flight: usize,
    requested_concurrency: usize,
    started_at: Instant,
    status: &'static str,
}

impl ProviderFanoutTelemetry {
    fn new(item_count: usize, requested_concurrency: usize, effective_concurrency: usize) -> Self {
        Self {
            completed_count: 0,
            effective_concurrency,
            item_count,
            peak_in_flight: 0,
            requested_concurrency,
            started_at: Instant::now(),
            status: "cancelled",
        }
    }

    fn observe_in_flight(&mut self, in_flight: usize) {
        self.peak_in_flight = self.peak_in_flight.max(in_flight);
    }

    fn observe_completion(&mut self) {
        self.completed_count += 1;
    }

    fn finish(&mut self, succeeded: bool) {
        self.status = if succeeded { "succeeded" } else { "failed" };
    }
}

impl Drop for ProviderFanoutTelemetry {
    fn drop(&mut self) {
        tracing::info!(
            event = "coach_provider_fanout_completion",
            item_count = self.item_count,
            completed_count = self.completed_count,
            requested_concurrency = self.requested_concurrency,
            effective_concurrency = self.effective_concurrency,
            initial_queue_depth = self.item_count.saturating_sub(self.effective_concurrency),
            saturated = self.item_count > self.effective_concurrency,
            peak_in_flight = self.peak_in_flight,
            status = self.status,
            wall_milliseconds = self.started_at.elapsed().as_millis(),
            "bounded provider fanout metrics"
        );
    }
}

pub fn collect_ordered_results<T, E>(
    results: Vec<Option<Result<T, E>>>,
) -> Result<Vec<T>, IndexedProviderError<E>> {
    results
        .into_iter()
        .enumerate()
        .map(|(index, result)| match result {
            Some(Ok(value)) => Ok(value),
            Some(Err(error)) => Err(IndexedProviderError { index, error }),
            None => unreachable!("unstarted provider Positions follow the first provider error"),
        })
        .collect()
}
