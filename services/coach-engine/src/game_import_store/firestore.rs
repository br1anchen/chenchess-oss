use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    engine_analysis::EngineProvenance,
    firestore::{codec::DurablePayload, FirestoreDatabase, FirestoreError},
    imported_games::ImportedGameCard,
    quality_capture::QualityCaptureAppender,
    review_durability::{game_import_id, path::hashed_path_segment},
    review_session_contract::{GameImportId, GameReview, GameReviewCriticalMoment, ImportedGame},
    review_session_game_identity::ReviewSessionGameIdentity,
    review_session_processor::ProcessorPrincipal,
};

use super::{
    DeletedImportedGame, GameImportLookup, GameImportRecord, GameImportReference,
    GameImportReferenceLookup, GameImportStore, GameImportStoreError, GameImportStoreFuture,
};

const USERS_COLLECTION: &str = "users";
const GAME_IMPORTS_COLLECTION: &str = "gameImports";
const IMPORTED_GAMES_COLLECTION: &str = "importedGames";

pub(crate) fn game_import_store(
    database: FirestoreDatabase,
    quality_capture: QualityCaptureAppender,
) -> Arc<dyn GameImportStore> {
    Arc::new(FirestoreGameImportStore {
        database,
        quality_capture,
    })
}

struct FirestoreGameImportStore {
    database: FirestoreDatabase,
    quality_capture: QualityCaptureAppender,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GameImportDocument {
    schema_version: u8,
    created_at: DateTime<Utc>,
    payload: DurablePayload<GameImportPayload>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GameImportPayload {
    imported_game: ImportedGame,
    frozen_review: GameReview,
    player_selected_moments: std::collections::BTreeMap<u16, GameReviewCriticalMoment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    engine_provenance: Option<EngineProvenance>,
}

impl GameImportDocument {
    fn from_record(record: &GameImportRecord) -> Self {
        Self {
            schema_version: record.schema_version,
            created_at: record.created_at,
            payload: DurablePayload::new(GameImportPayload {
                imported_game: record.imported_game.clone(),
                frozen_review: record.frozen_review.clone(),
                player_selected_moments: record.player_selected_moments.clone(),
                engine_provenance: record.engine_provenance.clone(),
            }),
        }
    }

    fn into_record(
        self,
        owner: ProcessorPrincipal,
        game_import_id: GameImportId,
    ) -> Result<GameImportRecord, GameImportStoreError> {
        let payload = self.payload.into_inner();
        let record = GameImportRecord {
            schema_version: self.schema_version,
            game_import_id,
            owner,
            created_at: self.created_at,
            imported_game: payload.imported_game,
            frozen_review: payload.frozen_review,
            player_selected_moments: payload.player_selected_moments,
            engine_provenance: payload.engine_provenance,
        };
        if record.has_valid_shape() {
            Ok(record)
        } else {
            Err(GameImportStoreError::InvalidRecord)
        }
    }
}

fn listed_game_import_record(
    owner: &ProcessorPrincipal,
    document_id: &str,
    document: GameImportDocument,
) -> Option<GameImportRecord> {
    let identity = ReviewSessionGameIdentity::from_import(&document.payload.as_ref().imported_game);
    let game_import_id = game_import_id(owner, &identity);
    if hashed_path_segment(game_import_id.as_str()) != document_id {
        return None;
    }
    document.into_record(owner.clone(), game_import_id).ok()
}

fn owner_document_id(owner: &ProcessorPrincipal) -> Result<String, GameImportStoreError> {
    match owner {
        ProcessorPrincipal::Player(player_id) => Ok(hashed_path_segment(player_id.as_str())),
        ProcessorPrincipal::LocalCoach => Err(GameImportStoreError::Configuration(
            "Local Coach imports use in-memory durability".to_string(),
        )),
    }
}

impl GameImportStore for FirestoreGameImportStore {
    fn create<'a>(&'a self, record: GameImportRecord) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async move {
            if !record.has_valid_shape() {
                return Err(GameImportStoreError::InvalidRecord);
            }
            let owner_document_id = owner_document_id(&record.owner)?;
            let document_id = hashed_path_segment(record.game_import_id.as_str());
            let path = [
                USERS_COLLECTION,
                owner_document_id.as_str(),
                GAME_IMPORTS_COLLECTION,
                document_id.as_str(),
            ];
            let timestamps = [("createdAt", record.created_at)];
            let business_write = || {
                self.database.create_write(
                    &path,
                    &GameImportDocument::from_record(&record),
                    &timestamps,
                )
            };
            let mut writes = vec![business_write()?];
            let quality_writes = self
                .quality_capture
                .prepare_game_analysis_writes(&record.owner, &record)
                .await;
            let capture_was_gated = !quality_writes.is_empty();
            writes.extend(quality_writes);
            match self.database.commit(writes).await {
                Ok(()) => Ok(()),
                Err(FirestoreError::Conflict) if capture_was_gated => self
                    .database
                    .commit(vec![business_write()?])
                    .await
                    .map_err(Into::into),
                Err(error) => Err(error.into()),
            }
        })
    }

    fn create_with_imported_game_card<'a>(
        &'a self,
        record: GameImportRecord,
        card: ImportedGameCard,
    ) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async move {
            if !record.has_valid_shape()
                || !card.is_valid()
                || record.game_import_id != card.game_import_id
            {
                return Err(GameImportStoreError::InvalidRecord);
            }
            let owner_document_id = owner_document_id(&record.owner)?;
            let import_document_id = hashed_path_segment(record.game_import_id.as_str());
            let import_path = [
                USERS_COLLECTION,
                owner_document_id.as_str(),
                GAME_IMPORTS_COLLECTION,
                import_document_id.as_str(),
            ];
            let card_path = [
                USERS_COLLECTION,
                owner_document_id.as_str(),
                IMPORTED_GAMES_COLLECTION,
                card.imported_game_key.as_str(),
            ];
            let business_writes = || {
                Ok::<_, FirestoreError>(vec![
                    self.database.create_write(
                        &import_path,
                        &GameImportDocument::from_record(&record),
                        &[("createdAt", record.created_at)],
                    )?,
                    self.database.upsert_write(
                        &card_path,
                        &card,
                        &[("importedAt", card.imported_at), ("endedAt", card.ended_at)],
                    )?,
                ])
            };
            let mut writes = business_writes()?;
            let quality_writes = self
                .quality_capture
                .prepare_game_analysis_writes(&record.owner, &record)
                .await;
            let capture_was_gated = !quality_writes.is_empty();
            writes.extend(quality_writes);
            match self.database.commit(writes).await {
                Ok(()) => Ok(()),
                Err(FirestoreError::Conflict) if capture_was_gated => self
                    .database
                    .commit(business_writes()?)
                    .await
                    .map_err(Into::into),
                Err(error) => Err(error.into()),
            }
        })
    }

    fn delete_imported_game<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        deleted: DeletedImportedGame,
    ) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async move {
            let owner_document_id = owner_document_id(owner)?;
            let import_document_ids = deleted
                .game_import_ids
                .iter()
                .map(|game_import_id| hashed_path_segment(game_import_id.as_str()))
                .collect::<Vec<_>>();
            let mut writes = Vec::with_capacity(import_document_ids.len() + 1);
            for import_document_id in &import_document_ids {
                writes.push(self.database.delete_write(&[
                    USERS_COLLECTION,
                    owner_document_id.as_str(),
                    GAME_IMPORTS_COLLECTION,
                    import_document_id.as_str(),
                ])?);
            }
            /* The card and its Game Imports go in one commit, the way the
            import wrote them, so no read ever sees a card whose review is
            already gone. */
            writes.push(self.database.delete_write(&[
                USERS_COLLECTION,
                owner_document_id.as_str(),
                IMPORTED_GAMES_COLLECTION,
                deleted.imported_game_key.as_str(),
            ])?);
            self.database.commit(writes).await.map_err(Into::into)
        })
    }

    fn upsert_imported_game_card<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        card: ImportedGameCard,
    ) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async move {
            if !card.is_valid() {
                return Err(GameImportStoreError::InvalidRecord);
            }
            let owner_document_id = owner_document_id(owner)?;
            let path = [
                USERS_COLLECTION,
                owner_document_id.as_str(),
                IMPORTED_GAMES_COLLECTION,
                card.imported_game_key.as_str(),
            ];
            let write = self.database.upsert_write(
                &path,
                &card,
                &[("importedAt", card.imported_at), ("endedAt", card.ended_at)],
            )?;
            self.database.commit(vec![write]).await.map_err(Into::into)
        })
    }

    fn list_imported_game_cards<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<ImportedGameCard>> {
        Box::pin(async move {
            let owner_document_id = owner_document_id(owner)?;
            let cards = self
                .database
                .list_valid_documents::<ImportedGameCard>(&[
                    USERS_COLLECTION,
                    owner_document_id.as_str(),
                    IMPORTED_GAMES_COLLECTION,
                ])
                .await?;
            Ok(cards
                .into_iter()
                .filter_map(|(document_id, card)| {
                    (document_id == card.imported_game_key && card.is_valid()).then_some(card)
                })
                .collect())
        })
    }

    fn list_game_import_records<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<GameImportRecord>> {
        Box::pin(async move {
            let owner_document_id = owner_document_id(owner)?;
            let documents = self
                .database
                .list_valid_documents::<GameImportDocument>(&[
                    USERS_COLLECTION,
                    owner_document_id.as_str(),
                    GAME_IMPORTS_COLLECTION,
                ])
                .await?;
            Ok(documents
                .into_iter()
                .filter_map(|(document_id, document)| {
                    listed_game_import_record(owner, &document_id, document)
                })
                .collect())
        })
    }

    fn find<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        game_import_id: &'a GameImportId,
    ) -> GameImportStoreFuture<'a, GameImportLookup> {
        Box::pin(async move {
            let owner_document_id = owner_document_id(owner)?;
            let document_id = hashed_path_segment(game_import_id.as_str());
            /* A document from a retired schema is NotFound, not an error: the
            caller then rejects the address as an unknown Game Import
            instead of reporting persistence as unavailable. */
            let document = match self
                .database
                .get_document::<GameImportDocument>(&[
                    USERS_COLLECTION,
                    &owner_document_id,
                    GAME_IMPORTS_COLLECTION,
                    &document_id,
                ])
                .await
            {
                Ok(Some(document)) => document,
                Ok(None) | Err(FirestoreError::InvalidDocument) => {
                    return Ok(GameImportLookup::NotFound)
                }
                Err(error) => return Err(error.into()),
            };
            let Ok(record) = document.into_record(owner.clone(), game_import_id.clone()) else {
                return Ok(GameImportLookup::NotFound);
            };
            Ok(GameImportLookup::Found(Box::new(record)))
        })
    }

    fn retain_for_review_session<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async move {
            let owner_document_id = owner_document_id(owner)?;
            let document_id = hashed_path_segment(reference.game_import_id.as_str());
            let document = match self
                .database
                .get_document::<GameImportDocument>(&[
                    USERS_COLLECTION,
                    &owner_document_id,
                    GAME_IMPORTS_COLLECTION,
                    &document_id,
                ])
                .await
            {
                Ok(Some(document)) => document,
                Ok(None) | Err(FirestoreError::InvalidDocument) => {
                    return Ok(GameImportReferenceLookup::NotFound)
                }
                Err(error) => return Err(error.into()),
            };
            let Ok(record) = document.into_record(owner.clone(), reference.game_import_id.clone())
            else {
                return Ok(GameImportReferenceLookup::NotFound);
            };
            if !record.matches_reference(reference) {
                return Err(GameImportStoreError::InvalidRecord);
            }
            Ok(GameImportReferenceLookup::Found(Box::new(record)))
        })
    }

    fn resolve_review_session_reference<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async move {
            let owner_document_id = owner_document_id(owner)?;
            let document_id = hashed_path_segment(reference.game_import_id.as_str());
            let document = match self
                .database
                .get_document::<GameImportDocument>(&[
                    USERS_COLLECTION,
                    &owner_document_id,
                    GAME_IMPORTS_COLLECTION,
                    &document_id,
                ])
                .await
            {
                Ok(Some(document)) => document,
                Ok(None) | Err(FirestoreError::InvalidDocument) => {
                    return Ok(GameImportReferenceLookup::NotFound)
                }
                Err(error) => return Err(error.into()),
            };
            let Ok(record) = document.into_record(owner.clone(), reference.game_import_id.clone())
            else {
                return Ok(GameImportReferenceLookup::NotFound);
            };
            if !record.matches_reference(reference) {
                return Err(GameImportStoreError::InvalidRecord);
            }
            Ok(GameImportReferenceLookup::Found(Box::new(record)))
        })
    }
}

impl From<FirestoreError> for GameImportStoreError {
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
        extract::{Path, Query, State},
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };
    use serde_json::Value;
    use tokio::sync::Mutex;

    use super::*;
    use crate::imported_games::ImportedGameCard;
    use crate::review_durability::game_import_id;
    use crate::review_session_contract::{
        OperationCompletion, PlayerId, ReviewSessionEvent, ReviewSessionEventEnvelope,
    };
    use crate::review_session_game_identity::ReviewSessionGameIdentity;

    #[test]
    fn listed_game_import_reconstructs_the_id_from_provenance() {
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let mut record = fixture_record(created_at);
        record.game_import_id = game_import_id(
            &record.owner,
            &ReviewSessionGameIdentity::from_import(&record.imported_game),
        );
        let document_id = hashed_path_segment(record.game_import_id.as_str());

        let restored = listed_game_import_record(
            &record.owner,
            &document_id,
            GameImportDocument::from_record(&record),
        )
        .unwrap();
        assert_eq!(restored, record);
        assert!(listed_game_import_record(
            &record.owner,
            "not-the-document",
            GameImportDocument::from_record(&record),
        )
        .is_none());
    }

    #[tokio::test]
    async fn firestore_adapter_commits_durable_player_owned_data_then_restores_it() {
        let state = Arc::new(Mutex::new(None::<Value>));
        let application = Router::new()
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-staging/documents/users/:owner_id/gameImports",
                post(create_document),
            )
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-staging/documents/users/:owner_id/gameImports/:id",
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
        let store = FirestoreGameImportStore {
            database: FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
            quality_capture: QualityCaptureAppender::Inert,
        };
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let record = fixture_record(created_at);
        let owner = record.owner.clone();
        let game_import_id = record.game_import_id.clone();
        let reference = record.reference();

        store.create(record.clone()).await.unwrap();
        let stored = state.lock().await.as_ref().unwrap().clone();
        let serialized = serde_json::to_string(&stored).unwrap();
        assert!(!serialized.contains("\"purgeAt\""));
        assert!(!serialized.contains("\"expiresAt\""));
        assert!(stored["fields"]["owner"].is_null());
        assert!(stored["fields"]["gameImportId"].is_null());
        assert!(!serialized.contains("original pasted PGN"));
        let GameImportLookup::Found(restored) = store.find(&owner, &game_import_id).await.unwrap()
        else {
            panic!("the committed Firestore record should be restorable")
        };
        assert_eq!(*restored, record);
        let other = ProcessorPrincipal::Player(
            PlayerId::try_from("firebase-player-b".to_string()).unwrap(),
        );
        assert!(matches!(
            store.find(&other, &game_import_id).await.unwrap(),
            GameImportLookup::NotFound
        ));
        let unknown = GameImportId::try_from("game-import:fixture:unknown".to_string()).unwrap();
        assert!(matches!(
            store.find(&owner, &unknown).await.unwrap(),
            GameImportLookup::NotFound
        ));

        assert!(matches!(
            store
                .retain_for_review_session(&owner, &reference)
                .await
                .unwrap(),
            GameImportReferenceLookup::Found(_)
        ));
        assert!(matches!(
            store
                .resolve_review_session_reference(&owner, &reference)
                .await
                .unwrap(),
            GameImportReferenceLookup::Found(_)
        ));

        server.abort();
    }

    #[tokio::test]
    async fn production_import_commits_the_business_record_and_quality_outbox_together() {
        let commits = Arc::new(Mutex::new(Vec::<Value>::new()));
        let application = Router::new()
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-production/documents/users/:id",
                get(read_enabled_quality_preference),
            )
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-production/documents:commit",
                post(record_commit),
            )
            .with_state(commits.clone());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, application).await });
        let database =
            FirestoreDatabase::production_emulator("chenchess-test", address.to_string()).unwrap();
        let store = FirestoreGameImportStore {
            database: database.clone(),
            quality_capture: QualityCaptureAppender::for_application(database),
        };
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let mut record = fixture_record(created_at);
        record.engine_provenance = Some(EngineProvenance {
            version: "Stockfish 18".to_string(),
            binary_sha256: "a".repeat(64),
            depth: 16,
            threads: 1,
            hash_mib: 16,
        });
        store
            .create(record)
            .await
            .expect("the atomic import commit should succeed");

        let locked = commits.lock().await;
        assert_eq!(locked.len(), 1);
        let writes = locked[0]["writes"].as_array().unwrap();
        assert_eq!(writes.len(), 3);
        assert!(writes.iter().any(|write| write["update"]["name"]
            .as_str()
            .is_some_and(|name| name.contains("/gameImports/"))));
        assert!(writes.iter().any(|write| write["update"]["name"]
            .as_str()
            .is_some_and(|name| name.contains("/qualityOutbox/"))));
        assert!(writes.iter().any(
            |write| write["update"]["name"]
                .as_str()
                .is_some_and(|name| name.ends_with(&format!(
                    "/users/{}",
                    hashed_path_segment("firebase-player-a")
                )))
        ));
        drop(locked);

        store
            .create(fixture_record(created_at + chrono::TimeDelta::minutes(1)))
            .await
            .expect("missing capture provenance must not fail the business import");
        assert_eq!(
            commits.lock().await.last().unwrap()["writes"]
                .as_array()
                .unwrap()
                .len(),
            1,
            "an uncapturable result must commit only the business record"
        );
        server.abort();
    }

    #[tokio::test]
    async fn manual_import_commits_the_review_and_imported_game_card_together() {
        let commits = Arc::new(Mutex::new(Vec::<Value>::new()));
        let application = Router::new()
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-staging/documents:commit",
                post(record_commit),
            )
            .with_state(commits.clone());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, application).await });
        let store = FirestoreGameImportStore {
            database: FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
            quality_capture: QualityCaptureAppender::Inert,
        };
        let created_at = "2026-08-12T11:00:00Z".parse().unwrap();
        let record = fixture_record(created_at);
        let card = ImportedGameCard::new(
            record.game_import_id.clone(),
            &record.imported_game,
            r#"[Date "2026.08.12"]
[Time "10:00:00"]
[TimeControl "600+5"]

1. e4 e5 2. Nf3 Nc6 *"#,
            0,
            created_at,
        )
        .unwrap();

        store
            .create_with_imported_game_card(record, card)
            .await
            .unwrap();

        let locked = commits.lock().await;
        assert_eq!(locked.len(), 1);
        let writes = locked[0]["writes"].as_array().unwrap();
        assert_eq!(writes.len(), 2);
        assert!(writes.iter().any(|write| write["update"]["name"]
            .as_str()
            .is_some_and(|name| name.contains("/gameImports/"))));
        assert!(writes.iter().any(|write| write["update"]["name"]
            .as_str()
            .is_some_and(|name| name.contains("/importedGames/"))));
        server.abort();
    }

    async fn create_document(
        State(state): State<Arc<Mutex<Option<Value>>>>,
        Path(owner_id): Path<String>,
        Query(query): Query<BTreeMap<String, String>>,
        Json(document): Json<Value>,
    ) -> Result<(StatusCode, Json<Value>), StatusCode> {
        let expected_id = hashed_path_segment("game-import:fixture:store");
        if owner_id != hashed_path_segment("firebase-player-a")
            || query.get("documentId") != Some(&expected_id)
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        *state.lock().await = Some(document.clone());
        Ok((StatusCode::CREATED, Json(document)))
    }

    async fn read_document(
        State(state): State<Arc<Mutex<Option<Value>>>>,
        Path((owner_id, id)): Path<(String, String)>,
    ) -> Result<Json<Value>, StatusCode> {
        if owner_id != hashed_path_segment("firebase-player-a")
            || id != hashed_path_segment("game-import:fixture:store")
        {
            return Err(StatusCode::NOT_FOUND);
        }
        state
            .lock()
            .await
            .clone()
            .map(|mut document| {
                document["updateTime"] =
                    Value::String("2026-07-26T10:00:01.000000000Z".to_string());
                Json(document)
            })
            .ok_or(StatusCode::NOT_FOUND)
    }

    async fn commit_documents(
        State(state): State<Arc<Mutex<Option<Value>>>>,
        Json(commit): Json<Value>,
    ) -> Result<Json<Value>, StatusCode> {
        let update = commit["writes"]
            .as_array()
            .and_then(|writes| writes.first())
            .map(|write| write["update"].clone())
            .filter(|update| update["fields"].is_object())
            .ok_or(StatusCode::BAD_REQUEST)?;
        *state.lock().await = Some(update);
        Ok(Json(serde_json::json!({})))
    }

    async fn read_enabled_quality_preference(Path(id): Path<String>) -> Json<Value> {
        Json(serde_json::json!({
            "name": format!(
                "projects/chenchess-test/databases/coach-app-production/documents/users/{id}"
            ),
            "updateTime": "2026-07-26T09:59:59Z",
            "fields": {
                "schemaVersion": { "integerValue": "1" },
                "createdAt": { "timestampValue": "2026-07-26T09:00:00Z" },
                "updatedAt": { "timestampValue": "2026-07-26T09:00:00Z" },
                "captureEnabled": { "booleanValue": true },
                "acknowledgedDisclosureVersion": { "integerValue": "1" }
            }
        }))
    }

    async fn record_commit(
        State(state): State<Arc<Mutex<Vec<Value>>>>,
        Json(commit): Json<Value>,
    ) -> Json<Value> {
        state.lock().await.push(commit);
        Json(serde_json::json!({}))
    }

    fn fixture_record(created_at: DateTime<Utc>) -> GameImportRecord {
        let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/events.json"
        )))
        .unwrap();
        let review = events
            .into_iter()
            .find_map(|event| match event.event {
                ReviewSessionEvent::Completed { result } => match *result {
                    OperationCompletion::GameImported { review, .. } => Some(*review),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let snapshot = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
        )))
        .unwrap();
        GameImportRecord::new(
            GameImportId::try_from("game-import:fixture:store".to_string()).unwrap(),
            ProcessorPrincipal::Player(
                PlayerId::try_from("firebase-player-a".to_string()).unwrap(),
            ),
            snapshot,
            review,
            Vec::new(),
            None,
            created_at,
        )
    }
}
