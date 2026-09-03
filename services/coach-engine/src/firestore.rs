use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;

use crate::deployment::DeploymentEnvironment;

use codec::FirestoreDocument;
use failure::{invalid_response_error, require_success, transport_error};
pub(crate) use service_account::{ServiceAccountTokenSource, IDENTITY_TOOLKIT_SCOPE};

pub(crate) mod codec;
mod failure;
mod recursive_delete;
mod service_account;

const FIRESTORE_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const FIRESTORE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const COACH_QUALITY_DATABASE_ID: &str = "coach-quality";
const APP_SERVICE_ACCOUNT_ENV: &str = "COACH_FIREBASE_SERVICE_ACCOUNT_JSON";
const QUALITY_SERVICE_ACCOUNT_ENV: &str = "COACH_QUALITY_SERVICE_ACCOUNT_JSON";
const QUALITY_DATABASE_ID_ENV: &str = "QUALITY_FIRESTORE_DATABASE_ID";

pub(crate) const ACCOUNT_LIFECYCLE_SERVICE_ACCOUNT_ENV: &str =
    "COACH_ACCOUNT_LIFECYCLE_SERVICE_ACCOUNT_JSON";

#[derive(Clone)]
pub(crate) struct FirestoreDatabase {
    client: Client,
    database_name: String,
    documents_url: String,
    authorization: FirestoreAuthorization,
    purpose: FirestoreDatabasePurpose,
}

/// One decode a caller asked for by name, reported when it fails.
///
/// An addressed read names one document, so failing to decode it is an event
/// and not a row to skip: the type that failed and serde's reason are what
/// name a contract change that retired a stored variant. The document's own
/// identity stays out of the log.
fn decoded<T: DeserializeOwned>(document: FirestoreDocument) -> Result<T, FirestoreError> {
    document.decode().map_err(|error| {
        tracing::error!(
            document_type = std::any::type_name::<T>(),
            error = %error.reason(),
            "Firestore document did not decode"
        );
        FirestoreError::InvalidDocument
    })
}

impl FirestoreDatabase {
    pub(crate) fn from_env() -> Result<Self, FirestoreError> {
        let project_id = required_env("FIREBASE_PROJECT_ID")?;
        let deployment_environment =
            DeploymentEnvironment::parse(&required_env("DEPLOYMENT_ENVIRONMENT")?)
                .map_err(|error| FirestoreError::Configuration(error.to_string()))?;
        let database_id = required_env("FIRESTORE_DATABASE_ID")?;
        let client = firestore_http_client()?;
        let (endpoint, authorization) = match optional_env("FIRESTORE_EMULATOR_HOST")? {
            Some(host) => {
                let endpoint = if host.starts_with("http://") || host.starts_with("https://") {
                    host
                } else {
                    format!("http://{host}")
                };
                (
                    format!("{}/v1", endpoint.trim_end_matches('/')),
                    FirestoreAuthorization::Emulator,
                )
            }
            None => {
                let service_account = std::env::var(APP_SERVICE_ACCOUNT_ENV).map_err(|_| {
                    FirestoreError::Configuration(format!(
                        "{APP_SERVICE_ACCOUNT_ENV} is required when FIREBASE_PROJECT_ID is configured"
                    ))
                })?;
                let source = ServiceAccountTokenSource::new(
                    &project_id,
                    &service_account,
                    APP_SERVICE_ACCOUNT_ENV,
                    client.clone(),
                )?;
                (
                    "https://firestore.googleapis.com/v1".to_string(),
                    FirestoreAuthorization::ServiceAccount(Arc::new(source)),
                )
            }
        };
        Self::new(
            project_id,
            database_id,
            deployment_environment,
            endpoint,
            client,
            authorization,
        )
    }

    pub(crate) fn quality_from_env() -> Result<Self, FirestoreError> {
        let project_id = required_env("FIREBASE_PROJECT_ID")?;
        let database_id = required_env(QUALITY_DATABASE_ID_ENV)?;
        if database_id != COACH_QUALITY_DATABASE_ID {
            return Err(FirestoreError::Configuration(format!(
                "{QUALITY_DATABASE_ID_ENV} must be {COACH_QUALITY_DATABASE_ID}"
            )));
        }
        let client = firestore_http_client()?;
        let (endpoint, authorization) = match optional_env("FIRESTORE_EMULATOR_HOST")? {
            Some(host) => {
                let endpoint = if host.starts_with("http://") || host.starts_with("https://") {
                    host
                } else {
                    format!("http://{host}")
                };
                (
                    format!("{}/v1", endpoint.trim_end_matches('/')),
                    FirestoreAuthorization::Emulator,
                )
            }
            None => {
                let service_account = std::env::var(QUALITY_SERVICE_ACCOUNT_ENV).map_err(|_| {
                    FirestoreError::Configuration(format!(
                        "{QUALITY_SERVICE_ACCOUNT_ENV} is required when {QUALITY_DATABASE_ID_ENV} is set"
                    ))
                })?;
                let source = ServiceAccountTokenSource::new(
                    &project_id,
                    &service_account,
                    QUALITY_SERVICE_ACCOUNT_ENV,
                    client.clone(),
                )?;
                (
                    "https://firestore.googleapis.com/v1".to_string(),
                    FirestoreAuthorization::ServiceAccount(Arc::new(source)),
                )
            }
        };
        Self::new_quality(project_id, database_id, endpoint, client, authorization)
    }

    pub(crate) fn quality_from_env_optional() -> Result<Option<Self>, FirestoreError> {
        let service_account = optional_env(QUALITY_SERVICE_ACCOUNT_ENV)?;
        let database_id = optional_env(QUALITY_DATABASE_ID_ENV)?;
        match (service_account, database_id) {
            (None, None) => Ok(None),
            (Some(_), Some(_)) => Self::quality_from_env().map(Some),
            _ => Err(FirestoreError::Configuration(format!(
                "{QUALITY_SERVICE_ACCOUNT_ENV} and {QUALITY_DATABASE_ID_ENV} must both be set or both absent"
            ))),
        }
    }

    pub(crate) fn account_lifecycle_pair_from_env() -> Result<(Self, Self), FirestoreError> {
        let project_id = required_env("FIREBASE_PROJECT_ID")?;
        let deployment_environment =
            DeploymentEnvironment::parse(&required_env("DEPLOYMENT_ENVIRONMENT")?)
                .map_err(|error| FirestoreError::Configuration(error.to_string()))?;
        if deployment_environment != DeploymentEnvironment::Production {
            return Err(FirestoreError::Configuration(
                "account deletion is available only in production".to_string(),
            ));
        }
        if optional_env("FIRESTORE_EMULATOR_HOST")?.is_some() {
            return Err(FirestoreError::Configuration(
                "production account deletion cannot use the Firestore emulator".to_string(),
            ));
        }
        let service_account = required_env(ACCOUNT_LIFECYCLE_SERVICE_ACCOUNT_ENV)?;
        let client = firestore_http_client()?;
        let authorization =
            FirestoreAuthorization::ServiceAccount(Arc::new(ServiceAccountTokenSource::new(
                &project_id,
                &service_account,
                ACCOUNT_LIFECYCLE_SERVICE_ACCOUNT_ENV,
                client.clone(),
            )?));
        let endpoint = "https://firestore.googleapis.com/v1".to_string();
        Ok((
            Self::new(
                project_id.clone(),
                DeploymentEnvironment::Staging
                    .application_database_id()
                    .to_string(),
                DeploymentEnvironment::Staging,
                endpoint.clone(),
                client.clone(),
                authorization.clone(),
            )?,
            Self::new(
                project_id,
                DeploymentEnvironment::Production
                    .application_database_id()
                    .to_string(),
                DeploymentEnvironment::Production,
                endpoint,
                client,
                authorization,
            )?,
        ))
    }

    #[cfg(test)]
    pub(crate) fn emulator(
        project_id: impl Into<String>,
        host: impl Into<String>,
    ) -> Result<Self, FirestoreError> {
        let host = host.into();
        let endpoint = if host.starts_with("http://") || host.starts_with("https://") {
            host
        } else {
            format!("http://{host}")
        };
        Self::new(
            project_id.into(),
            DeploymentEnvironment::Staging
                .application_database_id()
                .to_string(),
            DeploymentEnvironment::Staging,
            format!("{}/v1", endpoint.trim_end_matches('/')),
            firestore_http_client()?,
            FirestoreAuthorization::Emulator,
        )
    }

    #[cfg(test)]
    pub(crate) fn production_emulator(
        project_id: impl Into<String>,
        host: impl Into<String>,
    ) -> Result<Self, FirestoreError> {
        let host = host.into();
        let endpoint = if host.starts_with("http://") || host.starts_with("https://") {
            host
        } else {
            format!("http://{host}")
        };
        Self::new(
            project_id.into(),
            DeploymentEnvironment::Production
                .application_database_id()
                .to_string(),
            DeploymentEnvironment::Production,
            format!("{}/v1", endpoint.trim_end_matches('/')),
            firestore_http_client()?,
            FirestoreAuthorization::Emulator,
        )
    }

    #[cfg(test)]
    pub(crate) fn quality_emulator(
        project_id: impl Into<String>,
        host: impl Into<String>,
    ) -> Result<Self, FirestoreError> {
        let host = host.into();
        let endpoint = if host.starts_with("http://") || host.starts_with("https://") {
            host
        } else {
            format!("http://{host}")
        };
        Self::new_quality(
            project_id.into(),
            COACH_QUALITY_DATABASE_ID.to_string(),
            format!("{}/v1", endpoint.trim_end_matches('/')),
            firestore_http_client()?,
            FirestoreAuthorization::Emulator,
        )
    }

    fn new(
        project_id: String,
        database_id: String,
        deployment_environment: DeploymentEnvironment,
        endpoint: String,
        client: Client,
        authorization: FirestoreAuthorization,
    ) -> Result<Self, FirestoreError> {
        validate_path_segment("FIREBASE_PROJECT_ID", &project_id)?;
        validate_path_segment("FIRESTORE_DATABASE_ID", &database_id)?;
        let expected_database_id = deployment_environment.application_database_id();
        if database_id != expected_database_id {
            return Err(FirestoreError::Configuration(format!(
                "FIRESTORE_DATABASE_ID must be {expected_database_id} when DEPLOYMENT_ENVIRONMENT is {}",
                deployment_environment.name()
            )));
        }
        let database_name = format!("projects/{project_id}/databases/{database_id}");
        let documents_url = format!(
            "{}/{database_name}/documents",
            endpoint.trim_end_matches('/')
        );
        Ok(Self {
            client,
            database_name,
            documents_url,
            authorization,
            purpose: FirestoreDatabasePurpose::Application(deployment_environment),
        })
    }

    fn new_quality(
        project_id: String,
        database_id: String,
        endpoint: String,
        client: Client,
        authorization: FirestoreAuthorization,
    ) -> Result<Self, FirestoreError> {
        validate_path_segment("FIREBASE_PROJECT_ID", &project_id)?;
        validate_path_segment(QUALITY_DATABASE_ID_ENV, &database_id)?;
        if database_id != COACH_QUALITY_DATABASE_ID {
            return Err(FirestoreError::Configuration(format!(
                "{QUALITY_DATABASE_ID_ENV} must be {COACH_QUALITY_DATABASE_ID}"
            )));
        }
        let database_name = format!("projects/{project_id}/databases/{database_id}");
        let documents_url = format!(
            "{}/{database_name}/documents",
            endpoint.trim_end_matches('/')
        );
        Ok(Self {
            client,
            database_name,
            documents_url,
            authorization,
            purpose: FirestoreDatabasePurpose::Quality,
        })
    }

    pub(crate) fn is_application(&self) -> bool {
        matches!(self.purpose, FirestoreDatabasePurpose::Application(_))
    }

    pub(crate) async fn create_document<T: Serialize>(
        &self,
        collection_path: &[&str],
        document_id: &str,
        value: &T,
        timestamps: &[(&str, DateTime<Utc>)],
    ) -> Result<(), FirestoreError> {
        validate_collection_path(collection_path)?;
        validate_path_segment("Firestore document ID", document_id)?;
        let document = FirestoreDocument::encode(value, timestamps)?;
        let request = self
            .client
            .post(format!(
                "{}/{}",
                self.documents_url,
                collection_path.join("/")
            ))
            .query(&[("documentId", document_id)])
            .json(&document);
        require_success(
            "create_document",
            self.send("create_document", request).await?,
            true,
        )
        .await
        .map(|_| ())
    }

    pub(crate) async fn get_document<T: DeserializeOwned>(
        &self,
        document_path: &[&str],
    ) -> Result<Option<T>, FirestoreError> {
        let Some(document) = self.get_raw_document(document_path).await? else {
            return Ok(None);
        };
        decoded(document).map(Some)
    }

    pub(crate) async fn get_versioned_document<T: DeserializeOwned>(
        &self,
        document_path: &[&str],
    ) -> Result<Option<FirestoreVersionedDocument<T>>, FirestoreError> {
        let Some(document) = self.get_raw_document(document_path).await? else {
            return Ok(None);
        };
        let update_time = document
            .update_time
            .clone()
            .ok_or(FirestoreError::InvalidDocument)?;
        DateTime::parse_from_rfc3339(&update_time).map_err(|_| FirestoreError::InvalidDocument)?;
        let value = decoded(document)?;
        Ok(Some(FirestoreVersionedDocument { value, update_time }))
    }

    pub(crate) async fn begin_transaction(&self) -> Result<FirestoreTransaction, FirestoreError> {
        let request = self
            .client
            .post(format!(
                "{}/documents:beginTransaction",
                self.database_url()
            ))
            .json(&json!({ "options": { "readWrite": {} } }));
        let response = require_success(
            "begin_transaction",
            self.send("begin_transaction", request).await?,
            false,
        )
        .await?;
        let transaction: FirestoreTransactionResponse = response
            .json()
            .await
            .map_err(|error| invalid_response_error("begin_transaction", &error))?;
        if transaction.transaction.trim().is_empty() {
            return Err(FirestoreError::InvalidDocument);
        }
        Ok(FirestoreTransaction(transaction.transaction))
    }

    pub(crate) async fn get_document_in_transaction<T: DeserializeOwned>(
        &self,
        document_path: &[&str],
        transaction: &FirestoreTransaction,
    ) -> Result<Option<T>, FirestoreError> {
        let Some(document) = self
            .get_raw_document_in_transaction(document_path, Some(transaction))
            .await?
        else {
            return Ok(None);
        };
        decoded(document).map(Some)
    }

    async fn get_raw_document(
        &self,
        document_path: &[&str],
    ) -> Result<Option<FirestoreDocument>, FirestoreError> {
        self.get_raw_document_in_transaction(document_path, None)
            .await
    }

    async fn get_raw_document_in_transaction(
        &self,
        document_path: &[&str],
        transaction: Option<&FirestoreTransaction>,
    ) -> Result<Option<FirestoreDocument>, FirestoreError> {
        let name = self.document_name(document_path)?;
        let document = match transaction {
            Some(transaction) => self.batch_get_document(&name, transaction).await?,
            None => self.get_addressed_document(document_path).await?,
        };
        let Some(document) = document else {
            return Ok(None);
        };
        if document
            .name
            .as_deref()
            .is_some_and(|actual| actual != name)
        {
            return Err(FirestoreError::InvalidDocument);
        }
        Ok(Some(document))
    }

    async fn get_addressed_document(
        &self,
        document_path: &[&str],
    ) -> Result<Option<FirestoreDocument>, FirestoreError> {
        let request = self.client.get(format!(
            "{}/{}",
            self.documents_url,
            document_path.join("/")
        ));
        let response = self.send("get_document", request).await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = require_success("get_document", response, false).await?;
        response
            .json()
            .await
            .map(Some)
            .map_err(|error| invalid_response_error("get_document", &error))
    }

    /// A read inside a transaction goes through `:batchGet`, which is the
    /// documented transactional read and the only one the Firebase emulator
    /// answers: `documents.get?transaction=` never returns there, so every
    /// transactional path hangs until its client timeout (ADR 0060).
    async fn batch_get_document(
        &self,
        name: &str,
        transaction: &FirestoreTransaction,
    ) -> Result<Option<FirestoreDocument>, FirestoreError> {
        let request = self
            .client
            .post(format!("{}:batchGet", self.documents_url))
            .json(&json!({
                "documents": [name],
                "transaction": transaction.as_str(),
            }));
        let response = require_success(
            "batch_get_documents",
            self.send("batch_get_documents", request).await?,
            false,
        )
        .await?;
        let results: Vec<FirestoreBatchGetResult> = response
            .json()
            .await
            .map_err(|error| invalid_response_error("batch_get_documents", &error))?;
        // One document was asked for, so anything but one answer for it is a
        // response this caller cannot read rather than a missing document.
        let [result] = results.as_slice() else {
            return Err(FirestoreError::InvalidDocument);
        };
        match result {
            FirestoreBatchGetResult::Found { found } => Ok(Some(found.clone())),
            FirestoreBatchGetResult::Missing { missing } if missing == name => Ok(None),
            FirestoreBatchGetResult::Missing { .. } => Err(FirestoreError::InvalidDocument),
        }
    }

    pub(crate) async fn list_documents<T: DeserializeOwned>(
        &self,
        collection_path: &[&str],
    ) -> Result<Vec<(String, T)>, FirestoreError> {
        self.list_raw_documents(collection_path)
            .await?
            .into_iter()
            .map(|(id, document)| decoded(document).map(|value| (id, value)))
            .collect::<Result<_, _>>()
    }

    pub(crate) async fn list_valid_documents<T: DeserializeOwned>(
        &self,
        collection_path: &[&str],
    ) -> Result<Vec<(String, T)>, FirestoreError> {
        let mut documents = Vec::new();
        let mut dropped = 0usize;
        let mut reason = None;
        for (id, document) in self.list_raw_documents(collection_path).await? {
            match document.decode() {
                Ok(value) => documents.push((id, value)),
                Err(error) => {
                    dropped += 1;
                    reason.get_or_insert_with(|| error.reason().to_string());
                }
            }
        }
        if let Some(reason) = reason {
            /* One line for the listing rather than one per row. This caller
            drops what it cannot read by design, and a contract change that
            retires a stored variant makes every row fail at once, so a
            per-row report would flood the log on a listing every dashboard
            load repeats. */
            tracing::error!(
                document_type = std::any::type_name::<T>(),
                dropped,
                listed = dropped + documents.len(),
                error = %reason,
                "Firestore documents did not decode"
            );
        }
        Ok(documents)
    }

    pub(crate) async fn query_due_collection_group<T: DeserializeOwned>(
        &self,
        collection_id: &str,
        status: &str,
        due_at: DateTime<Utc>,
        limit: u16,
    ) -> Result<Vec<FirestoreVersionedDocumentAtPath<T>>, FirestoreError> {
        validate_path_segment("Firestore collection group", collection_id)?;
        if status.trim().is_empty() || limit == 0 || limit > 100 {
            return Err(FirestoreError::InvalidDocument);
        }
        let request = self
            .client
            .post(format!("{}/documents:runQuery", self.database_url()))
            .json(&json!({
                "structuredQuery": {
                    "from": [{
                        "collectionId": collection_id,
                        "allDescendants": true
                    }],
                    "where": {
                        "compositeFilter": {
                            "op": "AND",
                            "filters": [
                                {
                                    "fieldFilter": {
                                        "field": { "fieldPath": "status" },
                                        "op": "EQUAL",
                                        "value": { "stringValue": status }
                                    }
                                },
                                {
                                    "fieldFilter": {
                                        "field": { "fieldPath": "nextAttemptAt" },
                                        "op": "LESS_THAN_OR_EQUAL",
                                        "value": {
                                            "timestampValue": due_at.to_rfc3339_opts(
                                                chrono::SecondsFormat::Nanos,
                                                true
                                            )
                                        }
                                    }
                                }
                            ]
                        }
                    },
                    "orderBy": [{
                        "field": { "fieldPath": "nextAttemptAt" },
                        "direction": "ASCENDING"
                    }],
                    "limit": limit
                }
            }));
        let response =
            require_success("run_query", self.send("run_query", request).await?, false).await?;
        let rows: Vec<FirestoreQueryRow> = response
            .json()
            .await
            .map_err(|error| invalid_response_error("run_query", &error))?;
        rows.into_iter()
            .filter_map(|row| row.document)
            .map(|document| {
                let name = document
                    .name
                    .as_deref()
                    .ok_or(FirestoreError::InvalidDocument)?;
                let prefix = format!("{}/documents/", self.database_name);
                let path = name
                    .strip_prefix(&prefix)
                    .ok_or(FirestoreError::InvalidDocument)?
                    .split('/')
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                validate_document_path(&path.iter().map(String::as_str).collect::<Vec<_>>())?;
                let update_time = document
                    .update_time
                    .clone()
                    .ok_or(FirestoreError::InvalidDocument)?;
                DateTime::parse_from_rfc3339(&update_time)
                    .map_err(|_| FirestoreError::InvalidDocument)?;
                let value = decoded(document)?;
                Ok(FirestoreVersionedDocumentAtPath {
                    path,
                    value,
                    update_time,
                })
            })
            .collect()
    }

    pub(crate) async fn query_collection_group_timestamp_range<T: DeserializeOwned>(
        &self,
        collection_id: &str,
        status: &str,
        field_path: &str,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Vec<FirestoreVersionedDocumentAtPath<T>>, FirestoreError> {
        if status.trim().is_empty() {
            return Err(FirestoreError::InvalidDocument);
        }
        self.query_collection_group_timestamp_range_inner(
            collection_id,
            Some(status),
            field_path,
            starts_at,
            ends_at,
        )
        .await
    }

    pub(crate) async fn query_collection_group_timestamp_range_without_status<
        T: DeserializeOwned,
    >(
        &self,
        collection_id: &str,
        field_path: &str,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Vec<FirestoreVersionedDocumentAtPath<T>>, FirestoreError> {
        self.query_collection_group_timestamp_range_inner(
            collection_id,
            None,
            field_path,
            starts_at,
            ends_at,
        )
        .await
    }

    async fn query_collection_group_timestamp_range_inner<T: DeserializeOwned>(
        &self,
        collection_id: &str,
        status: Option<&str>,
        field_path: &str,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<Vec<FirestoreVersionedDocumentAtPath<T>>, FirestoreError> {
        validate_path_segment("Firestore collection group", collection_id)?;
        validate_path_segment("Firestore query field", field_path)?;
        if starts_at >= ends_at {
            return Err(FirestoreError::InvalidDocument);
        }
        let timestamp = |value: DateTime<Utc>| {
            json!({
                "timestampValue": value.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
            })
        };
        let mut filters = Vec::with_capacity(if status.is_some() { 3 } else { 2 });
        if let Some(status) = status {
            filters.push(json!({
                "fieldFilter": {
                    "field": { "fieldPath": "status" },
                    "op": "EQUAL",
                    "value": { "stringValue": status }
                }
            }));
        }
        filters.extend([
            json!({
                "fieldFilter": {
                    "field": { "fieldPath": field_path },
                    "op": "GREATER_THAN_OR_EQUAL",
                    "value": timestamp(starts_at)
                }
            }),
            json!({
                "fieldFilter": {
                    "field": { "fieldPath": field_path },
                    "op": "LESS_THAN",
                    "value": timestamp(ends_at)
                }
            }),
        ]);
        let request = self
            .client
            .post(format!("{}/documents:runQuery", self.database_url()))
            .json(&json!({
                "structuredQuery": {
                    "from": [{
                        "collectionId": collection_id,
                        "allDescendants": true
                    }],
                    "where": {
                        "compositeFilter": {
                            "op": "AND",
                            "filters": filters
                        }
                    },
                    "orderBy": [{
                        "field": { "fieldPath": field_path },
                        "direction": "ASCENDING"
                    }]
                }
            }));
        let response =
            require_success("run_query", self.send("run_query", request).await?, false).await?;
        let rows: Vec<FirestoreQueryRow> = response
            .json()
            .await
            .map_err(|error| invalid_response_error("run_query", &error))?;
        rows.into_iter()
            .filter_map(|row| row.document)
            .map(|document| self.decode_query_document(document))
            .collect()
    }

    pub(crate) async fn list_collection_group_documents<T: DeserializeOwned>(
        &self,
        collection_id: &str,
    ) -> Result<Vec<FirestoreVersionedDocumentAtPath<T>>, FirestoreError> {
        validate_path_segment("Firestore collection group", collection_id)?;
        let request = self
            .client
            .post(format!("{}/documents:runQuery", self.database_url()))
            .json(&json!({
                "structuredQuery": {
                    "from": [{
                        "collectionId": collection_id,
                        "allDescendants": true
                    }]
                }
            }));
        let response =
            require_success("run_query", self.send("run_query", request).await?, false).await?;
        let rows: Vec<FirestoreQueryRow> = response
            .json()
            .await
            .map_err(|error| invalid_response_error("run_query", &error))?;
        rows.into_iter()
            .filter_map(|row| row.document)
            .map(|document| self.decode_query_document(document))
            .collect()
    }

    fn decode_query_document<T: DeserializeOwned>(
        &self,
        document: FirestoreDocument,
    ) -> Result<FirestoreVersionedDocumentAtPath<T>, FirestoreError> {
        let name = document
            .name
            .as_deref()
            .ok_or(FirestoreError::InvalidDocument)?;
        let prefix = format!("{}/documents/", self.database_name);
        let path = name
            .strip_prefix(&prefix)
            .ok_or(FirestoreError::InvalidDocument)?
            .split('/')
            .map(str::to_string)
            .collect::<Vec<_>>();
        validate_document_path(&path.iter().map(String::as_str).collect::<Vec<_>>())?;
        let update_time = document
            .update_time
            .clone()
            .ok_or(FirestoreError::InvalidDocument)?;
        DateTime::parse_from_rfc3339(&update_time).map_err(|_| FirestoreError::InvalidDocument)?;
        let value = decoded(document)?;
        Ok(FirestoreVersionedDocumentAtPath {
            path,
            value,
            update_time,
        })
    }

    async fn list_raw_documents(
        &self,
        collection_path: &[&str],
    ) -> Result<Vec<(String, FirestoreDocument)>, FirestoreError> {
        validate_collection_path(collection_path)?;
        let collection_name = collection_path.join("/");
        let name_prefix = format!("{}/documents/{collection_name}/", self.database_name);
        let mut documents = Vec::new();
        let mut page_token: Option<String> = None;
        let mut seen_page_tokens = BTreeSet::new();
        loop {
            let mut query = vec![("pageSize", "100".to_string())];
            if let Some(token) = page_token.as_ref() {
                query.push(("pageToken", token.clone()));
            }
            let request = self
                .client
                .get(format!("{}/{}", self.documents_url, collection_name))
                .query(&query);
            let response = require_success(
                "list_documents",
                self.send("list_documents", request).await?,
                false,
            )
            .await?;
            let page: FirestoreDocumentList = response
                .json()
                .await
                .map_err(|error| invalid_response_error("list_documents", &error))?;
            for document in page.documents {
                let name = document
                    .name
                    .as_deref()
                    .ok_or(FirestoreError::InvalidDocument)?;
                let document_id = name
                    .strip_prefix(&name_prefix)
                    .filter(|suffix| is_valid_path_segment(suffix))
                    .ok_or(FirestoreError::InvalidDocument)?
                    .to_string();
                documents.push((document_id, document));
            }
            let Some(next_page_token) = page.next_page_token else {
                break;
            };
            if next_page_token.is_empty() || !seen_page_tokens.insert(next_page_token.clone()) {
                return Err(FirestoreError::InvalidDocument);
            }
            page_token = Some(next_page_token);
        }
        Ok(documents)
    }

    pub(crate) fn create_write<T: Serialize>(
        &self,
        document_path: &[&str],
        value: &T,
        timestamps: &[(&str, DateTime<Utc>)],
    ) -> Result<FirestoreWrite, FirestoreError> {
        let mut document = FirestoreDocument::encode(value, timestamps)?;
        document.name = Some(self.document_name(document_path)?);
        Ok(FirestoreWrite(FirestoreWriteBody::Update {
            update: document,
            current_document: Some(FirestorePrecondition {
                exists: Some(false),
                update_time: None,
            }),
        }))
    }

    pub(crate) fn update_write<T: Serialize>(
        &self,
        document_path: &[&str],
        value: &T,
        timestamps: &[(&str, DateTime<Utc>)],
    ) -> Result<FirestoreWrite, FirestoreError> {
        self.update_write_with_precondition(
            document_path,
            value,
            timestamps,
            FirestorePrecondition {
                exists: Some(true),
                update_time: None,
            },
        )
    }

    pub(crate) fn update_write_at<T: Serialize>(
        &self,
        document_path: &[&str],
        value: &T,
        timestamps: &[(&str, DateTime<Utc>)],
        update_time: String,
    ) -> Result<FirestoreWrite, FirestoreError> {
        if update_time.trim().is_empty() {
            return Err(FirestoreError::InvalidDocument);
        }
        self.update_write_with_precondition(
            document_path,
            value,
            timestamps,
            FirestorePrecondition {
                exists: None,
                update_time: Some(update_time),
            },
        )
    }

    /// Write a document whether or not it already exists. Used when the
    /// record's identity makes concurrent writers equivalent.
    pub(crate) fn upsert_write<T: Serialize>(
        &self,
        document_path: &[&str],
        value: &T,
        timestamps: &[(&str, DateTime<Utc>)],
    ) -> Result<FirestoreWrite, FirestoreError> {
        let mut document = FirestoreDocument::encode(value, timestamps)?;
        document.name = Some(self.document_name(document_path)?);
        Ok(FirestoreWrite(FirestoreWriteBody::Update {
            update: document,
            current_document: None,
        }))
    }

    fn update_write_with_precondition<T: Serialize>(
        &self,
        document_path: &[&str],
        value: &T,
        timestamps: &[(&str, DateTime<Utc>)],
        current_document: FirestorePrecondition,
    ) -> Result<FirestoreWrite, FirestoreError> {
        let mut document = FirestoreDocument::encode(value, timestamps)?;
        document.name = Some(self.document_name(document_path)?);
        Ok(FirestoreWrite(FirestoreWriteBody::Update {
            update: document,
            current_document: Some(current_document),
        }))
    }

    pub(crate) fn delete_write(
        &self,
        document_path: &[&str],
    ) -> Result<FirestoreWrite, FirestoreError> {
        Ok(FirestoreWrite(FirestoreWriteBody::Delete {
            delete: self.document_name(document_path)?,
            current_document: None,
        }))
    }

    pub(crate) fn delete_write_at(
        &self,
        document_path: &[&str],
        update_time: String,
    ) -> Result<FirestoreWrite, FirestoreError> {
        if update_time.trim().is_empty() {
            return Err(FirestoreError::InvalidDocument);
        }
        Ok(FirestoreWrite(FirestoreWriteBody::Delete {
            delete: self.document_name(document_path)?,
            current_document: Some(FirestorePrecondition {
                exists: None,
                update_time: Some(update_time),
            }),
        }))
    }

    pub(crate) async fn commit(&self, writes: Vec<FirestoreWrite>) -> Result<(), FirestoreError> {
        self.commit_writes(writes, None).await
    }

    pub(crate) async fn commit_transaction(
        &self,
        transaction: FirestoreTransaction,
        writes: Vec<FirestoreWrite>,
    ) -> Result<(), FirestoreError> {
        self.commit_writes(writes, Some(transaction.0)).await
    }

    async fn commit_writes(
        &self,
        writes: Vec<FirestoreWrite>,
        transaction: Option<String>,
    ) -> Result<(), FirestoreError> {
        if writes.is_empty() {
            return Err(FirestoreError::InvalidDocument);
        }
        let write_count = writes.len();
        let commit = FirestoreCommit {
            writes,
            transaction,
        };
        let request_bytes = serde_json::to_vec(&commit)
            .map_err(|_| FirestoreError::InvalidDocument)?
            .len();
        let started_at = Instant::now();
        let request = self
            .client
            .post(format!("{}/documents:commit", self.database_url()))
            .json(&commit);
        let result = require_success("commit", self.send("commit", request).await?, true)
            .await
            .map(|_| ());
        tracing::info!(
            event = "coach_firestore_commit_completion",
            firestore_operation = "commit",
            write_count,
            request_bytes,
            wall_milliseconds = started_at.elapsed().as_millis(),
            status = if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
            "firestore request metrics"
        );
        result
    }

    pub(crate) async fn rollback_transaction(
        &self,
        transaction: FirestoreTransaction,
    ) -> Result<(), FirestoreError> {
        let request = self
            .client
            .post(format!("{}/documents:rollback", self.database_url()))
            .json(&json!({ "transaction": transaction.as_str() }));
        require_success(
            "rollback_transaction",
            self.send("rollback_transaction", request).await?,
            false,
        )
        .await
        .map(|_| ())
    }

    fn database_url(&self) -> String {
        self.documents_url
            .strip_suffix("/documents")
            .expect("Firestore documents URL has a stable suffix")
            .to_string()
    }

    fn document_name(&self, path: &[&str]) -> Result<String, FirestoreError> {
        validate_document_path(path)?;
        Ok(format!(
            "{}/documents/{}",
            self.database_name,
            path.join("/")
        ))
    }

    async fn send(
        &self,
        operation: &'static str,
        request: RequestBuilder,
    ) -> Result<reqwest::Response, FirestoreError> {
        let started_at = Instant::now();
        let request = match &self.authorization {
            FirestoreAuthorization::Emulator => request,
            FirestoreAuthorization::ServiceAccount(source) => {
                request.bearer_auth(source.access_token().await?)
            }
        };
        let response = request
            .send()
            .await
            .map_err(|error| transport_error(operation, &error));
        match &response {
            Ok(response) => {
                let response_bytes = response.content_length();
                tracing::info!(
                    event = "coach_firestore_transport_completion",
                    firestore_operation = operation,
                    status_code = response.status().as_u16(),
                    response_bytes = response_bytes.unwrap_or(0),
                    response_bytes_known = response_bytes.is_some(),
                    wall_milliseconds = started_at.elapsed().as_millis(),
                    "firestore transport metrics"
                )
            }
            Err(_) => tracing::warn!(
                event = "coach_firestore_transport_completion",
                firestore_operation = operation,
                wall_milliseconds = started_at.elapsed().as_millis(),
                "firestore transport metrics"
            ),
        }
        response
    }
}

#[derive(Clone)]
enum FirestoreAuthorization {
    Emulator,
    ServiceAccount(Arc<ServiceAccountTokenSource>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FirestoreDatabasePurpose {
    Application(DeploymentEnvironment),
    Quality,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FirestoreError {
    #[error("Firestore is misconfigured: {0}")]
    Configuration(String),
    #[error("Firestore transport failed")]
    Transport,
    #[error("Firestore is unavailable")]
    Unavailable,
    #[error("Firestore document already exists")]
    Conflict,
    #[error("Firestore returned an invalid document")]
    InvalidDocument,
}

pub(crate) struct FirestoreVersionedDocument<T> {
    pub(crate) value: T,
    pub(crate) update_time: String,
}

pub(crate) struct FirestoreVersionedDocumentAtPath<T> {
    pub(crate) path: Vec<String>,
    pub(crate) value: T,
    pub(crate) update_time: String,
}

pub(crate) struct FirestoreTransaction(String);

impl FirestoreTransaction {
    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Serialize)]
#[serde(transparent)]
pub(crate) struct FirestoreWrite(FirestoreWriteBody);

#[derive(Serialize)]
#[serde(rename_all = "camelCase", untagged)]
enum FirestoreWriteBody {
    Update {
        update: FirestoreDocument,
        #[serde(rename = "currentDocument", skip_serializing_if = "Option::is_none")]
        current_document: Option<FirestorePrecondition>,
    },
    Delete {
        delete: String,
        #[serde(rename = "currentDocument", skip_serializing_if = "Option::is_none")]
        current_document: Option<FirestorePrecondition>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FirestorePrecondition {
    #[serde(skip_serializing_if = "Option::is_none")]
    exists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update_time: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FirestoreCommit {
    writes: Vec<FirestoreWrite>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transaction: Option<String>,
}

#[derive(Deserialize)]
struct FirestoreTransactionResponse {
    transaction: String,
}

/// One entry of a `:batchGet` answer: the document, or the name it did not
/// find. Untagged because the wire carries no discriminator — the present
/// field is the discriminator — and `readTime` rides along on both.
#[derive(Deserialize)]
#[serde(untagged)]
enum FirestoreBatchGetResult {
    Found { found: FirestoreDocument },
    Missing { missing: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FirestoreDocumentList {
    #[serde(default)]
    documents: Vec<FirestoreDocument>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FirestoreQueryRow {
    #[serde(default)]
    document: Option<FirestoreDocument>,
}

fn firestore_http_client() -> Result<Client, FirestoreError> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(FIRESTORE_CONNECT_TIMEOUT)
        .timeout(FIRESTORE_RESPONSE_TIMEOUT)
        .build()
        .map_err(|_| {
            FirestoreError::Configuration(
                "could not construct the bounded Firestore HTTP client".to_string(),
            )
        })
}

fn required_env(name: &str) -> Result<String, FirestoreError> {
    optional_env(name)?.ok_or_else(|| {
        FirestoreError::Configuration(format!(
            "{name} is required for durable Coach Engine persistence"
        ))
    })
}

fn optional_env(name: &str) -> Result<Option<String>, FirestoreError> {
    match std::env::var(name) {
        Ok(value) if value.trim().is_empty() => Err(FirestoreError::Configuration(format!(
            "{name} must not be empty"
        ))),
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(FirestoreError::Configuration(format!(
            "{name} is not valid text"
        ))),
    }
}

fn validate_collection_path(path: &[&str]) -> Result<(), FirestoreError> {
    if path.len().is_multiple_of(2) {
        return Err(FirestoreError::InvalidDocument);
    }
    validate_path(path)
}

fn validate_document_path(path: &[&str]) -> Result<(), FirestoreError> {
    if path.is_empty() || !path.len().is_multiple_of(2) {
        return Err(FirestoreError::InvalidDocument);
    }
    validate_path(path)
}

fn validate_path(path: &[&str]) -> Result<(), FirestoreError> {
    for segment in path {
        validate_path_segment("Firestore path segment", segment)?;
    }
    Ok(())
}

fn validate_path_segment(name: &str, value: &str) -> Result<(), FirestoreError> {
    if !is_valid_path_segment(value) {
        Err(FirestoreError::Configuration(format!(
            "{name} is not a valid Firestore path segment"
        )))
    } else {
        Ok(())
    }
}

fn is_valid_path_segment(value: &str) -> bool {
    !(value.is_empty()
        || value == "."
        || value == ".."
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || ['/', '\\', '?', '#', '%']
            .into_iter()
            .any(|character| value.contains(character)))
}

#[cfg(test)]
#[path = "firestore/tests.rs"]
mod tests;
