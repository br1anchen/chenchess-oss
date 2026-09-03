use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    engine_analysis::EngineProvenance,
    firestore::{codec::DurablePayload, FirestoreDatabase, FirestoreError},
    review_session_contract::{
        EloRating, GameReview, GameReviewCriticalMoment, ImportedGame, ReviewSide,
    },
    review_session_game_identity::ReviewSessionGameIdentity,
};

use super::{
    game_analysis_document_id, GameAnalysisRecord, GameAnalysisStore, GameAnalysisStoreError,
    GameAnalysisStoreFuture, IdentityFreeGame,
};

const GAME_ANALYSIS_COLLECTION: &str = "gameAnalysis";

pub(crate) fn game_analysis_store(database: FirestoreDatabase) -> Arc<dyn GameAnalysisStore> {
    Arc::new(FirestoreGameAnalysisStore { database })
}

struct FirestoreGameAnalysisStore {
    database: FirestoreDatabase,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GameAnalysisDocument {
    schema_version: u8,
    generation: u32,
    created_at: DateTime<Utc>,
    purge_at: DateTime<Utc>,
    hard_expires_at: DateTime<Utc>,
    payload: DurablePayload<GameAnalysisPayload>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GameAnalysisPayload {
    identity_free_game: IdentityFreeGame,
    review_side: ReviewSide,
    resolved_elo: EloRating,
    review: GameReview,
    player_selected_moments: std::collections::BTreeMap<u16, GameReviewCriticalMoment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    engine_provenance: Option<EngineProvenance>,
}

impl GameAnalysisDocument {
    fn from_record(record: &GameAnalysisRecord) -> Self {
        Self {
            schema_version: record.schema_version,
            generation: record.generation,
            created_at: record.created_at,
            purge_at: record.purge_at,
            hard_expires_at: record.hard_expires_at,
            payload: DurablePayload::new(GameAnalysisPayload {
                identity_free_game: record.identity_free_game.clone(),
                review_side: record.review_side,
                resolved_elo: record.resolved_elo,
                review: record.review.clone(),
                player_selected_moments: record.player_selected_moments.clone(),
                engine_provenance: record.engine_provenance.clone(),
            }),
        }
    }

    fn into_record(self) -> GameAnalysisRecord {
        let payload = self.payload.into_inner();
        GameAnalysisRecord {
            schema_version: self.schema_version,
            generation: self.generation,
            created_at: self.created_at,
            purge_at: self.purge_at,
            hard_expires_at: self.hard_expires_at,
            identity_free_game: payload.identity_free_game,
            review_side: payload.review_side,
            resolved_elo: payload.resolved_elo,
            review: payload.review,
            player_selected_moments: payload.player_selected_moments,
            engine_provenance: payload.engine_provenance,
        }
    }
}

impl GameAnalysisStore for FirestoreGameAnalysisStore {
    fn find<'a>(
        &'a self,
        identity: &'a ReviewSessionGameIdentity,
        imported: &'a ImportedGame,
        now: DateTime<Utc>,
    ) -> GameAnalysisStoreFuture<'a, Option<Box<GameAnalysisRecord>>> {
        Box::pin(async move {
            let document_id = game_analysis_document_id(identity);
            let Some(versioned) = self
                .database
                .get_versioned_document::<GameAnalysisDocument>(&[
                    GAME_ANALYSIS_COLLECTION,
                    &document_id,
                ])
                .await?
            else {
                return Ok(None);
            };
            let mut record = versioned.value.into_record();
            if !record.is_usable(imported, now) {
                return Ok(None);
            }
            let prior_purge_at = record.purge_at;
            record.refresh(now);
            if record.purge_at != prior_purge_at {
                let write = self.database.update_write_at(
                    &[GAME_ANALYSIS_COLLECTION, &document_id],
                    &GameAnalysisDocument::from_record(&record),
                    &[
                        ("createdAt", record.created_at),
                        ("purgeAt", record.purge_at),
                        ("hardExpiresAt", record.hard_expires_at),
                    ],
                    versioned.update_time,
                )?;
                match self.database.commit(vec![write]).await {
                    Ok(()) | Err(FirestoreError::Conflict) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(Some(Box::new(record)))
        })
    }

    fn put<'a>(
        &'a self,
        identity: &'a ReviewSessionGameIdentity,
        record: GameAnalysisRecord,
    ) -> GameAnalysisStoreFuture<'a, ()> {
        Box::pin(async move {
            let document_id = game_analysis_document_id(identity);
            let write = self.database.upsert_write(
                &[GAME_ANALYSIS_COLLECTION, &document_id],
                &GameAnalysisDocument::from_record(&record),
                &[
                    ("createdAt", record.created_at),
                    ("purgeAt", record.purge_at),
                    ("hardExpiresAt", record.hard_expires_at),
                ],
            )?;
            self.database.commit(vec![write]).await.map_err(Into::into)
        })
    }
}

impl From<FirestoreError> for GameAnalysisStoreError {
    fn from(error: FirestoreError) -> Self {
        match error {
            FirestoreError::Configuration(message) => Self::Configuration(message),
            FirestoreError::Transport => Self::Transport,
            FirestoreError::Unavailable => Self::Unavailable,
            FirestoreError::Conflict => Self::Conflict,
            FirestoreError::InvalidDocument => Self::InvalidRecord,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::{
        extract::{Path, State},
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };
    use serde_json::Value;
    use tokio::sync::Mutex;

    use super::*;
    use crate::review_session_contract::{
        GameReview, ImportedGame, OperationCompletion, ReviewSessionEvent,
        ReviewSessionEventEnvelope, ReviewSide,
    };

    #[tokio::test]
    async fn firestore_analysis_commits_ttl_data_then_restores_without_an_owner() {
        let state = Arc::new(Mutex::new(BTreeMap::<String, Value>::new()));
        let application = Router::new()
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-staging/documents/gameAnalysis/:id",
                get(read_document),
            )
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-staging/documents:commit",
                post(commit_documents),
            )
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, application).await });
        let store = FirestoreGameAnalysisStore {
            database: FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
        };
        let created_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = fixture_import();
        let identity = ReviewSessionGameIdentity::from_import(&imported);
        let record =
            GameAnalysisRecord::new(&imported, fixture_review(), Vec::new(), None, created_at);

        store.put(&identity, record.clone()).await.unwrap();
        let locked = state.lock().await;
        let (document_id, document) = locked.iter().next().unwrap();
        assert_eq!(document_id, &game_analysis_document_id(&identity));
        assert!(!document_id.contains(identity.as_str()));
        assert_eq!(document["fields"].as_object().unwrap().len(), 6);
        assert!(document["fields"]["payload"]["stringValue"].is_string());
        assert!(document["fields"]["review"].is_null());
        let stored = serde_json::to_string(document).unwrap();
        drop(locked);
        assert!(stored.contains("\"purgeAt\":{\"timestampValue\""));
        // The shared document must stay free of anything that identifies who
        // paid for this analysis.
        assert!(!stored.contains("owner"));
        assert!(!stored.contains("firebase-player"));
        for forbidden in [
            "synthetic-white",
            "synthetic-white",
            "rated rapid game",
            "https://lichess.org/Synthet1",
            "canonicalUrl",
            "sideQualifiedUrl",
        ] {
            assert!(!stored.contains(forbidden), "analysis contains {forbidden}");
        }

        let restored = store
            .find(&identity, &imported, created_at)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(*restored, record);
        let mut other_import = imported.clone();
        other_import.review_side = ReviewSide::White;
        let other_identity = ReviewSessionGameIdentity::from_import(&other_import);
        assert!(store
            .find(&other_identity, &other_import, created_at)
            .await
            .unwrap()
            .is_none());
        assert!(store
            .find(
                &identity,
                &imported,
                created_at
                    + chrono::TimeDelta::hours(super::super::GAME_ANALYSIS_SLIDING_LIFETIME_HOURS,),
            )
            .await
            .unwrap()
            .is_none());

        server.abort();
    }

    async fn read_document(
        State(state): State<Arc<Mutex<BTreeMap<String, Value>>>>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, StatusCode> {
        state
            .lock()
            .await
            .get(&id)
            .cloned()
            .map(|mut document| {
                document["updateTime"] =
                    Value::String("2026-08-01T10:00:01.000000000Z".to_string());
                Json(document)
            })
            .ok_or(StatusCode::NOT_FOUND)
    }

    async fn commit_documents(
        State(state): State<Arc<Mutex<BTreeMap<String, Value>>>>,
        Json(commit): Json<Value>,
    ) -> Result<Json<Value>, StatusCode> {
        let update = commit["writes"]
            .as_array()
            .and_then(|writes| writes.first())
            .map(|write| write["update"].clone())
            .filter(|update| update["fields"].is_object())
            .ok_or(StatusCode::BAD_REQUEST)?;
        let id = update["name"]
            .as_str()
            .and_then(|name| name.rsplit('/').next())
            .ok_or(StatusCode::BAD_REQUEST)?
            .to_string();
        state.lock().await.insert(id, update);
        Ok(Json(serde_json::json!({})))
    }

    fn fixture_review() -> GameReview {
        let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/events.json"
        )))
        .unwrap();
        events
            .into_iter()
            .find_map(|event| match event.event {
                ReviewSessionEvent::Completed { result } => match *result {
                    OperationCompletion::GameImported { review, .. } => Some(*review),
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    }

    fn fixture_import() -> ImportedGame {
        serde_json::from_str::<ImportedGame>(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
        )))
        .unwrap()
    }
}
