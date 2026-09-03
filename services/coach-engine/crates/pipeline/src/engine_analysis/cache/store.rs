use std::collections::{HashMap, VecDeque};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::super::{EngineAnalysis, EngineAnalysisOutput, EngineCacheIdentity, EngineProvenance};
use crate::review_session_contract::PositionContentId;

const CACHE_SCHEMA: &str = "exact-engine-analysis-cache/v1";
const ENTRY_OVERHEAD_BYTES: usize = 512;
pub(super) const DEFAULT_MAX_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct CacheKey(String);

impl CacheKey {
    /// The key is the engine identity plus the Position Snapshot content id, and
    /// nothing else. No Player, Game Import, Review, or Review Session material
    /// enters it, which is what lets one entry serve every Player who reaches
    /// the position.
    pub(super) fn new(identity: &EngineCacheIdentity, position: &PositionContentId) -> Self {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct KeyMaterial<'a> {
            schema: &'static str,
            identity: &'a EngineCacheIdentity,
            position_content_id: &'a PositionContentId,
        }

        let material = serde_json::to_vec(&KeyMaterial {
            schema: CACHE_SCHEMA,
            identity,
            position_content_id: position,
        })
        .expect("engine cache keys are serializable");
        Self(format!("{:x}", Sha256::digest(material)))
    }

    pub(super) fn digest(&self) -> &str {
        &self.0
    }
}

pub(super) struct EngineCache {
    pub(super) entries: HashMap<CacheKey, CacheEntry>,
    recency: VecDeque<(CacheKey, u64)>,
    pub(super) max_bytes: usize,
    pub(super) used_bytes: usize,
    access_clock: u64,
}

pub(super) struct CacheEntry {
    output: EngineAnalysisOutput,
    pub(super) payload_digest: String,
    last_access: u64,
    bytes: usize,
}

impl EngineCache {
    pub(super) fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            recency: VecDeque::new(),
            max_bytes,
            used_bytes: 0,
            access_clock: 0,
        }
    }

    pub(super) fn get(&mut self, key: &CacheKey) -> Option<EngineAnalysisOutput> {
        let valid = self
            .entries
            .get(key)
            .is_some_and(|entry| entry.payload_digest == payload_digest(key, &entry.output));
        if !valid {
            if let Some(corrupt) = self.entries.remove(key) {
                self.used_bytes = self.used_bytes.saturating_sub(corrupt.bytes);
                tracing::warn!("discarded a digest-invalid engine cache entry");
            }
            return None;
        }
        self.access_clock = self.access_clock.wrapping_add(1);
        let entry = self
            .entries
            .get_mut(key)
            .expect("a digest-valid engine cache entry remains present");
        entry.last_access = self.access_clock;
        self.recency.push_back((key.clone(), self.access_clock));
        let output = entry.output.clone();
        self.compact_recency();
        Some(output)
    }

    pub(super) fn insert(&mut self, key: CacheKey, output: EngineAnalysisOutput) {
        let payload_digest = payload_digest(&key, &output);
        let bytes = cache_weight(&key, &output, &payload_digest);
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
                output,
                payload_digest,
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
            let is_current = self
                .entries
                .get(&key)
                .is_some_and(|entry| entry.last_access == generation);
            if !is_current {
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

pub(super) fn payload_digest(key: &CacheKey, output: &EngineAnalysisOutput) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload<'a> {
        schema: &'static str,
        key: &'a str,
        analysis: &'a EngineAnalysis,
        provenance: &'a Option<EngineProvenance>,
    }

    let payload = serde_json::to_vec(&Payload {
        schema: CACHE_SCHEMA,
        key: &key.0,
        analysis: &output.analysis,
        provenance: &output.provenance,
    })
    .expect("engine cache payloads are serializable");
    format!("{:x}", Sha256::digest(payload))
}

pub(super) fn cache_weight(key: &CacheKey, output: &EngineAnalysisOutput, digest: &str) -> usize {
    serde_json::to_vec(&(&output.analysis, &output.provenance))
        .expect("engine cache payloads are serializable")
        .len()
        .saturating_add(key.0.len())
        .saturating_add(digest.len())
        .saturating_add(ENTRY_OVERHEAD_BYTES)
}
