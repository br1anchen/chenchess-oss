use super::*;

/// Counts what the cache is asked to do, and can fail one write on demand.
#[derive(Default)]
pub(super) struct TrackingCheckpointStore {
    inner: InMemoryReviewAnalysisCache,
    pub(super) seeds: AtomicUsize,
    pub(super) loads: AtomicUsize,
    pub(super) replace_attempts: AtomicUsize,
    pub(super) fail_next_replace: AtomicBool,
}

impl ReviewAnalysisCacheStore for TrackingCheckpointStore {
    fn seed<'a>(&'a self, entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            self.seeds.fetch_add(1, Ordering::SeqCst);
            self.inner.seed(entries).await
        })
    }

    fn load<'a>(
        &'a self,
        game_import_id: &'a GameImportId,
        game: &'a ReviewSessionGame,
        now: chrono::DateTime<Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>> {
        Box::pin(async move {
            self.loads.fetch_add(1, Ordering::SeqCst);
            self.inner.load(game_import_id, game, now).await
        })
    }

    fn replace_moment<'a>(
        &'a self,
        mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            self.replace_attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail_next_replace.swap(false, Ordering::SeqCst) {
                return Err(ReviewAnalysisCacheError::Unavailable);
            }
            self.inner.replace_moment(mutation).await
        })
    }
}

/// A cache whose reads always fail.
///
/// Everything a read would have returned is recomputable, so a review still
/// opens: this is what proves cached analysis is an optimization and never a
/// dependency.
#[derive(Default)]
pub(super) struct UnreadableCheckpointStore {
    inner: InMemoryReviewAnalysisCache,
    pub(super) loads: AtomicUsize,
}

impl ReviewAnalysisCacheStore for UnreadableCheckpointStore {
    fn seed<'a>(&'a self, entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a> {
        self.inner.seed(entries)
    }

    fn load<'a>(
        &'a self,
        _game_import_id: &'a GameImportId,
        _game: &'a ReviewSessionGame,
        _now: chrono::DateTime<Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>> {
        Box::pin(async move {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Err(ReviewAnalysisCacheError::Unavailable)
        })
    }

    fn replace_moment<'a>(
        &'a self,
        mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a> {
        self.inner.replace_moment(mutation)
    }
}
