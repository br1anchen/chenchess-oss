//! Review Analysis Cache eviction.
//!
//! Eviction removes analysis-cache entries whose retention has run out.
//! Firestore's own `purgeAt` TTL is the primary mechanism and is declared on the
//! `moments` collection group; its deletion is best-effort and can lag by days,
//! so this is the deterministic sweep an operator can run when they need the
//! space back now, and the one place the retention window is observable.
//!
//! The job does not touch `reviewAnnotations`: a Player's published Review Moment
//! Comments are not cached analysis and are erased only with the Player subtree.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    entry::ReviewAnalysisCacheError, REVIEW_ANALYSIS_COLLECTION, REVIEW_MOMENTS_COLLECTION,
};
use crate::firestore::FirestoreDatabase;

/// Selects whether an eviction run only reports or removes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewAnalysisEvictionMode {
    /// Count every evictable document without writing.
    DryRun,
    /// Delete every evictable document.
    Apply,
}

/// Aggregate counts that do not disclose Player, Game, or review data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewAnalysisEvictionReport {
    /// Analysis-cache entries read.
    pub scanned_cache_entries: usize,
    /// Analysis-cache entries past their retention.
    pub expired_cache_entries: usize,
    /// Analysis-cache entries deleted during this invocation.
    pub removed_cache_entries: usize,
}

/// Evicts expired analysis-cache entries in the configured application database.
///
/// The explicit environment must match `DEPLOYMENT_ENVIRONMENT`. Dry-run
/// performs the same full scan as apply without writing.
///
/// # Errors
///
/// Returns [`ReviewAnalysisCacheError`] for configuration mismatches, Firestore
/// failures, or documents at unexpected paths.
pub async fn evict_review_analysis_cache_from_env(
    expected_environment: &str,
    mode: ReviewAnalysisEvictionMode,
) -> Result<ReviewAnalysisEvictionReport, ReviewAnalysisCacheError> {
    let configured_environment = std::env::var("DEPLOYMENT_ENVIRONMENT").map_err(|_| {
        ReviewAnalysisCacheError::Configuration(
            "DEPLOYMENT_ENVIRONMENT is required for review-analysis cache eviction".to_string(),
        )
    })?;
    validate_eviction_environment(configured_environment.as_str(), expected_environment)?;
    let database = FirestoreDatabase::from_env()?;
    evict_all(&database, mode, Utc::now()).await
}

fn validate_eviction_environment(
    configured_environment: &str,
    expected_environment: &str,
) -> Result<(), ReviewAnalysisCacheError> {
    if !matches!(expected_environment, "staging" | "production") {
        return Err(ReviewAnalysisCacheError::Configuration(
            "review-analysis cache eviction environment must be staging or production".to_string(),
        ));
    }
    if configured_environment != expected_environment {
        return Err(ReviewAnalysisCacheError::Configuration(
            "review-analysis cache eviction target does not match DEPLOYMENT_ENVIRONMENT"
                .to_string(),
        ));
    }
    Ok(())
}

/// Only `purgeAt` is decoded: retention is the whole decision, and an entry
/// whose payload no longer parses is still evictable on its timestamp.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetentionDocument {
    purge_at: DateTime<Utc>,
}

pub(crate) async fn evict_all(
    database: &FirestoreDatabase,
    mode: ReviewAnalysisEvictionMode,
    now: DateTime<Utc>,
) -> Result<ReviewAnalysisEvictionReport, ReviewAnalysisCacheError> {
    // `moments` is a collection group, so only cache addresses are candidates.
    let listed = database
        .list_collection_group_documents::<RetentionDocument>(REVIEW_MOMENTS_COLLECTION)
        .await?;
    let mut scanned_cache_entries = 0;
    let mut expired = Vec::new();
    for document in listed {
        if !is_analysis_cache_moment_path(&document.path) {
            continue;
        }
        scanned_cache_entries += 1;
        if now >= document.value.purge_at {
            expired.push(document.path);
        }
    }
    let mut report = ReviewAnalysisEvictionReport {
        scanned_cache_entries,
        expired_cache_entries: expired.len(),
        ..ReviewAnalysisEvictionReport::default()
    };
    if mode == ReviewAnalysisEvictionMode::DryRun {
        return Ok(report);
    }
    for path in &expired {
        let write = database.delete_write(&segments(path))?;
        database.commit(vec![write]).await?;
        report.removed_cache_entries += 1;
    }
    Ok(report)
}

fn segments(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}

fn is_analysis_cache_moment_path(path: &[String]) -> bool {
    path.len() == 4
        && path[0] == REVIEW_ANALYSIS_COLLECTION
        && path[2] == REVIEW_MOMENTS_COLLECTION
        && is_sha256_path_segment(&path[1])
        && is_sha256_path_segment(&path[3])
}

fn is_sha256_path_segment(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        extract::{OriginalUri, State},
        routing::post,
        Json, Router,
    };
    use serde_json::Value;
    use tokio::sync::Mutex;

    use super::*;

    const NOW: &str = "2026-08-09T10:00:00Z";

    #[test]
    fn eviction_environment_must_match_the_deployment() {
        assert!(validate_eviction_environment("staging", "staging").is_ok());
        assert!(validate_eviction_environment("staging", "production").is_err());
        assert!(validate_eviction_environment("local", "local").is_err());
    }

    #[test]
    fn eviction_cannot_reach_the_durable_annotation_store() {
        for collection in [
            crate::review_annotation_store::REVIEW_ANNOTATIONS_COLLECTION,
            crate::review_annotation_store::REVIEW_ANNOTATION_COMMENTS_COLLECTION,
        ] {
            assert_ne!(collection, REVIEW_MOMENTS_COLLECTION);
        }
    }

    #[tokio::test]
    async fn only_entries_past_their_retention_are_evicted() {
        let (state, database, server) = fixture_database().await;
        let now: DateTime<Utc> = NOW.parse().unwrap();

        let dry_run = evict_all(&database, ReviewAnalysisEvictionMode::DryRun, now)
            .await
            .unwrap();
        assert_eq!(
            dry_run,
            ReviewAnalysisEvictionReport {
                scanned_cache_entries: 2,
                expired_cache_entries: 1,
                ..ReviewAnalysisEvictionReport::default()
            }
        );
        assert!(state.lock().await.commits.is_empty());

        let applied = evict_all(&database, ReviewAnalysisEvictionMode::Apply, now)
            .await
            .unwrap();
        assert_eq!(applied.removed_cache_entries, 1);
        let deleted = deleted_paths(&state).await;
        assert_eq!(deleted.len(), 1);
        assert!(
            deleted[0].ends_with(&format!(
                "/reviewAnalysis/{}/moments/{}",
                "a".repeat(64),
                "e".repeat(64)
            )),
            "the live entry must survive, {deleted:?}"
        );
        server.abort();
    }

    async fn deleted_paths(state: &Arc<Mutex<EvictionServerState>>) -> Vec<String> {
        state
            .lock()
            .await
            .commits
            .iter()
            .map(|commit| commit["writes"][0]["delete"].as_str().unwrap().to_string())
            .collect()
    }

    struct EvictionServerState {
        commits: Vec<Value>,
    }

    async fn fixture_database() -> (
        Arc<Mutex<EvictionServerState>>,
        FirestoreDatabase,
        tokio::task::JoinHandle<Result<(), std::io::Error>>,
    ) {
        let state = Arc::new(Mutex::new(EvictionServerState {
            commits: Vec::new(),
        }));
        let application = Router::new()
            .fallback(post(firestore_request))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, application).await });
        let database = FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap();
        (state, database, server)
    }

    async fn firestore_request(
        OriginalUri(uri): OriginalUri,
        State(state): State<Arc<Mutex<EvictionServerState>>>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        match uri.path().rsplit(':').next() {
            Some("runQuery") => {
                let collection = body["structuredQuery"]["from"][0]["collectionId"]
                    .as_str()
                    .unwrap()
                    .to_string();
                assert!(body["structuredQuery"]["from"][0]["allDescendants"]
                    .as_bool()
                    .unwrap());
                Json(match collection.as_str() {
                    "moments" => serde_json::json!([
                        { "document": wire_document(&cache_path("e"), "2026-08-01T10:00:00Z") },
                        { "document": wire_document(&cache_path("f"), "2026-11-01T10:00:00Z") },
                    ]),
                    _ => panic!("unexpected collection group"),
                })
            }
            Some("commit") => {
                state.lock().await.commits.push(body);
                Json(serde_json::json!({}))
            }
            _ => panic!("unexpected Firestore operation"),
        }
    }

    fn cache_path(seed: &str) -> Vec<String> {
        vec![
            REVIEW_ANALYSIS_COLLECTION.to_string(),
            "a".repeat(64),
            REVIEW_MOMENTS_COLLECTION.to_string(),
            seed.repeat(64),
        ]
    }

    fn wire_document(path: &[String], purge_at: &str) -> Value {
        serde_json::json!({
            "name": format!(
                "projects/chenchess-test/databases/coach-app-staging/documents/{}",
                path.join("/"),
            ),
            "updateTime": "2026-08-06T10:00:01.000000000Z",
            "fields": { "purgeAt": { "timestampValue": purge_at } },
        })
    }
}
