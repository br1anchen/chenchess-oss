use std::{collections::HashMap, sync::Mutex, time::Duration};

use super::{
    cacheable_position,
    store::{CacheKey, EngineCache},
    EngineAnalysisError, EngineAnalysisOutput, EngineCacheIdentity, IndexedEngineAnalysisError,
    TimedEngineAnalysis,
};

pub(super) struct BatchPlan {
    resolved: Vec<Option<TimedEngineAnalysis>>,
    misses: Vec<PendingAnalysis>,
    cache_hits: usize,
    coalesced: usize,
}

impl BatchPlan {
    pub(super) fn new(
        positions: Vec<String>,
        identity: &EngineCacheIdentity,
        cache: &Mutex<EngineCache>,
    ) -> Self {
        let mut resolved = (0..positions.len()).map(|_| None).collect::<Vec<_>>();
        let mut misses = Vec::<PendingAnalysis>::new();
        let mut miss_by_key = HashMap::<CacheKey, usize>::new();
        let mut cache_hits = 0usize;
        let mut coalesced = 0usize;

        for (index, position) in positions.into_iter().enumerate() {
            // An uncacheable position — one with no Position Snapshot content
            // identity — is neither a hit nor coalesced with anything. It is
            // analyzed as the caller spelled it and its result is never stored.
            let Some((canonical_fen, key)) = cacheable_position(identity, &position) else {
                misses.push(PendingAnalysis {
                    position,
                    key: None,
                    result_indices: vec![index],
                });
                continue;
            };
            if let Some(output) = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&key)
            {
                resolved[index] = Some(TimedEngineAnalysis {
                    analysis: output.analysis,
                    provenance: output.provenance,
                    duration: Duration::ZERO,
                });
                cache_hits += 1;
                continue;
            }
            if let Some(miss_index) = miss_by_key.get(&key).copied() {
                misses[miss_index].result_indices.push(index);
                coalesced += 1;
                continue;
            }
            miss_by_key.insert(key.clone(), misses.len());
            // The canonical FEN replaces the caller's spelling: the engine
            // analyzes the position the key names, so a stored entry is always
            // the analysis of the position it is filed under.
            misses.push(PendingAnalysis {
                position: canonical_fen,
                key: Some(key),
                result_indices: vec![index],
            });
        }

        Self {
            resolved,
            misses,
            cache_hits,
            coalesced,
        }
    }

    pub(super) fn positions_to_analyze(&self) -> Vec<String> {
        self.misses
            .iter()
            .map(|miss| miss.position.clone())
            .collect()
    }

    /// Identifies this batch's outstanding work for single-flight collapsing. A
    /// miss contributes its content key so two concurrent batches naming the
    /// same positions collapse whatever FEN spelling each used; an uncacheable
    /// position has no content key and contributes its raw string, which only
    /// ever collapses with an identical request.
    pub(super) fn flight_material(&self) -> Vec<&str> {
        self.misses
            .iter()
            .map(|miss| {
                miss.key
                    .as_ref()
                    .map_or(miss.position.as_str(), CacheKey::digest)
            })
            .collect()
    }

    pub(super) fn original_error(
        &self,
        error: IndexedEngineAnalysisError,
    ) -> IndexedEngineAnalysisError {
        IndexedEngineAnalysisError {
            index: self
                .misses
                .get(error.index)
                .and_then(|miss| miss.result_indices.first())
                .copied()
                .unwrap_or(error.index),
            error: error.error,
        }
    }

    pub(super) fn merge(
        &mut self,
        analyzed: Vec<TimedEngineAnalysis>,
        mut cache_result: impl FnMut(CacheKey, EngineAnalysisOutput),
    ) -> Result<(), IndexedEngineAnalysisError> {
        if analyzed.len() != self.misses.len() {
            let index = self
                .misses
                .get(analyzed.len().min(self.misses.len().saturating_sub(1)))
                .and_then(|miss| miss.result_indices.first())
                .copied()
                .unwrap_or(0);
            return Err(IndexedEngineAnalysisError {
                index,
                error: EngineAnalysisError::Protocol(format!(
                    "engine batch returned {} results for {} positions",
                    analyzed.len(),
                    self.misses.len()
                )),
            });
        }

        for (miss, timed) in self.misses.iter().zip(analyzed) {
            if let Some(key) = miss.key.clone() {
                cache_result(
                    key,
                    EngineAnalysisOutput {
                        analysis: timed.analysis.clone(),
                        provenance: timed.provenance.clone(),
                    },
                );
            }
            for (duplicate_index, result_index) in miss.result_indices.iter().copied().enumerate() {
                self.resolved[result_index] = Some(TimedEngineAnalysis {
                    analysis: timed.analysis.clone(),
                    provenance: timed.provenance.clone(),
                    duration: if duplicate_index == 0 {
                        timed.duration
                    } else {
                        Duration::ZERO
                    },
                });
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<Vec<TimedEngineAnalysis>, IndexedEngineAnalysisError> {
        self.resolved
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                result.ok_or_else(|| IndexedEngineAnalysisError {
                    index,
                    error: EngineAnalysisError::Protocol(
                        "engine batch omitted a requested position".to_owned(),
                    ),
                })
            })
            .collect()
    }

    pub(super) fn cache_hits(&self) -> usize {
        self.cache_hits
    }

    pub(super) fn cache_misses(&self) -> usize {
        self.misses.len()
    }

    pub(super) fn coalesced(&self) -> usize {
        self.coalesced
    }
}

struct PendingAnalysis {
    position: String,
    /// Absent when the position has no Position Snapshot content identity, which
    /// is what keeps an uncacheable result out of the store.
    key: Option<CacheKey>,
    result_indices: Vec<usize>,
}
