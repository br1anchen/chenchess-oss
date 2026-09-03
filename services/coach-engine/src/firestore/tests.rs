use super::*;

use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use tokio::sync::Mutex;

#[test]
fn staging_database_is_accepted_for_staging() {
    let database = FirestoreDatabase::new(
        "chenchess-test".to_string(),
        DeploymentEnvironment::Staging
            .application_database_id()
            .to_string(),
        DeploymentEnvironment::Staging,
        "http://127.0.0.1:8081/v1".to_string(),
        firestore_http_client().unwrap(),
        FirestoreAuthorization::Emulator,
    )
    .unwrap();

    assert_eq!(
        database.database_name,
        "projects/chenchess-test/databases/coach-app-staging"
    );
}

#[test]
fn production_database_is_accepted_for_production() {
    let database = FirestoreDatabase::new(
        "chenchess-test".to_string(),
        DeploymentEnvironment::Production
            .application_database_id()
            .to_string(),
        DeploymentEnvironment::Production,
        "http://127.0.0.1:8081/v1".to_string(),
        firestore_http_client().unwrap(),
        FirestoreAuthorization::Emulator,
    )
    .unwrap();

    assert_eq!(
        database.database_name,
        "projects/chenchess-test/databases/coach-app-production"
    );
}

#[test]
fn quality_database_accepts_only_the_dedicated_database_id() {
    let accepted = FirestoreDatabase::new_quality(
        "chenchess-test".to_string(),
        COACH_QUALITY_DATABASE_ID.to_string(),
        "http://127.0.0.1:8081/v1".to_string(),
        firestore_http_client().unwrap(),
        FirestoreAuthorization::Emulator,
    )
    .unwrap();
    assert_eq!(
        accepted.database_name,
        "projects/chenchess-test/databases/coach-quality"
    );

    assert!(matches!(
        FirestoreDatabase::new_quality(
            "chenchess-test".to_string(),
            DeploymentEnvironment::Production
                .application_database_id()
                .to_string(),
            "http://127.0.0.1:8081/v1".to_string(),
            firestore_http_client().unwrap(),
            FirestoreAuthorization::Emulator,
        ),
        Err(FirestoreError::Configuration(_))
    ));
}

#[test]
fn default_database_is_rejected() {
    let client = firestore_http_client().unwrap();
    assert!(matches!(
        FirestoreDatabase::new(
            "chenchess-test".to_string(),
            "(default)".to_string(),
            DeploymentEnvironment::Staging,
            "http://127.0.0.1:8081/v1".to_string(),
            client,
            FirestoreAuthorization::Emulator,
        ),
        Err(FirestoreError::Configuration(_))
    ));
}

#[test]
fn production_database_is_rejected_for_staging() {
    let client = firestore_http_client().unwrap();
    assert!(matches!(
        FirestoreDatabase::new(
            "chenchess-test".to_string(),
            "coach-app-test".to_string(),
            DeploymentEnvironment::Staging,
            "http://127.0.0.1:8081/v1".to_string(),
            client,
            FirestoreAuthorization::Emulator,
        ),
        Err(FirestoreError::Configuration(_))
    ));
}

#[test]
fn unknown_deployment_environment_is_rejected() {
    assert_eq!(
        DeploymentEnvironment::parse("preview")
            .unwrap_err()
            .to_string(),
        "DEPLOYMENT_ENVIRONMENT must be staging or production"
    );
}

#[tokio::test]
async fn recursive_delete_discovers_subcollections_and_deletes_children_first() {
    let commits = Arc::new(Mutex::new(Vec::<Value>::new()));
    let application = Router::new()
        .route(
            "/v1/projects/chenchess-test/databases/coach-app-staging/documents/users/player:listCollectionIds",
            post(|| async {
                Json(serde_json::json!({
                    "collectionIds": ["children"],
                }))
            }),
        )
        .route(
            "/v1/projects/chenchess-test/databases/coach-app-staging/documents/users/player/children",
            get(|Query(query): Query<HashMap<String, String>>| async move {
                assert_eq!(query.get("showMissing").map(String::as_str), Some("true"));
                Json(serde_json::json!({
                    "documents": [{
                        "name": "projects/chenchess-test/databases/coach-app-staging/documents/users/player/children/child",
                    }],
                }))
            }),
        )
        .route(
            "/v1/projects/chenchess-test/databases/coach-app-staging/documents/users/player/children/child:listCollectionIds",
            post(|| async { Json(serde_json::json!({})) }),
        )
        .route(
            "/v1/projects/chenchess-test/databases/coach-app-staging/documents:commit",
            post(
                |State(commits): State<Arc<Mutex<Vec<Value>>>>,
                 Json(commit): Json<Value>| async move {
                    commits.lock().await.push(commit);
                    Json(serde_json::json!({}))
                },
            ),
        )
        .with_state(commits.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, application).await });
    let database = FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap();

    database
        .recursive_delete_document(&["users", "player"])
        .await
        .unwrap();

    let commits = commits.lock().await;
    let writes = commits[0]["writes"].as_array().unwrap();
    assert!(writes[0]["delete"]
        .as_str()
        .unwrap()
        .ends_with("/users/player/children/child"));
    assert!(writes[1]["delete"]
        .as_str()
        .unwrap()
        .ends_with("/users/player"));
    server.abort();
}
