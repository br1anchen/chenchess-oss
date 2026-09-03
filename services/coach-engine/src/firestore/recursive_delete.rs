use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    failure::{invalid_response_error, require_success},
    is_valid_path_segment, validate_collection_path, validate_document_path, validate_path_segment,
    FirestoreDatabase, FirestoreError,
};

const RECURSIVE_DELETE_BATCH_SIZE: usize = 400;

impl FirestoreDatabase {
    pub(crate) async fn recursive_delete_document(
        &self,
        document_path: &[&str],
    ) -> Result<(), FirestoreError> {
        validate_document_path(document_path)?;
        let root = document_path
            .iter()
            .map(|segment| (*segment).to_string())
            .collect::<Vec<_>>();
        let mut stack = vec![(root, false)];
        let mut deletes = Vec::with_capacity(RECURSIVE_DELETE_BATCH_SIZE);
        while let Some((path, expanded)) = stack.pop() {
            if expanded {
                let refs = path.iter().map(String::as_str).collect::<Vec<_>>();
                deletes.push(self.delete_write(&refs)?);
                if deletes.len() == RECURSIVE_DELETE_BATCH_SIZE {
                    self.commit(std::mem::take(&mut deletes)).await?;
                }
                continue;
            }

            stack.push((path.clone(), true));
            let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
            for collection_id in self.list_collection_ids(&path_refs).await? {
                let mut collection_path = path.clone();
                collection_path.push(collection_id);
                let collection_refs = collection_path
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                for document_id in self.list_document_ids(&collection_refs).await? {
                    let mut child_path = collection_path.clone();
                    child_path.push(document_id);
                    stack.push((child_path, false));
                }
            }
        }
        if !deletes.is_empty() {
            self.commit(deletes).await?;
        }
        Ok(())
    }

    async fn list_document_ids(
        &self,
        collection_path: &[&str],
    ) -> Result<Vec<String>, FirestoreError> {
        validate_collection_path(collection_path)?;
        let collection_name = collection_path.join("/");
        let name_prefix = format!("{}/documents/{collection_name}/", self.database_name);
        let mut document_ids = Vec::new();
        let mut page_token: Option<String> = None;
        let mut seen_page_tokens = BTreeSet::new();
        loop {
            let mut query = vec![
                ("pageSize", "100".to_string()),
                ("showMissing", "true".to_string()),
            ];
            if let Some(token) = page_token.as_ref() {
                query.push(("pageToken", token.clone()));
            }
            let request = self
                .client
                .get(format!("{}/{}", self.documents_url, collection_name))
                .query(&query);
            let response = require_success(
                "list_recursive_delete_documents",
                self.send("list_recursive_delete_documents", request)
                    .await?,
                false,
            )
            .await?;
            let page: FirestoreDocumentNameList = response.json().await.map_err(|error| {
                invalid_response_error("list_recursive_delete_documents", &error)
            })?;
            for document in page.documents {
                let document_id = document
                    .name
                    .strip_prefix(&name_prefix)
                    .filter(|suffix| is_valid_path_segment(suffix))
                    .ok_or(FirestoreError::InvalidDocument)?;
                document_ids.push(document_id.to_string());
            }
            let Some(next_page_token) = page.next_page_token else {
                break;
            };
            if next_page_token.is_empty() || !seen_page_tokens.insert(next_page_token.clone()) {
                return Err(FirestoreError::InvalidDocument);
            }
            page_token = Some(next_page_token);
        }
        Ok(document_ids)
    }

    async fn list_collection_ids(
        &self,
        document_path: &[&str],
    ) -> Result<Vec<String>, FirestoreError> {
        validate_document_path(document_path)?;
        let endpoint = format!(
            "{}/{}:listCollectionIds",
            self.documents_url,
            document_path.join("/")
        );
        let mut collection_ids = Vec::new();
        let mut page_token: Option<String> = None;
        let mut seen_page_tokens = BTreeSet::new();
        loop {
            let request = self.client.post(&endpoint).json(&ListCollectionIdsRequest {
                page_size: 100,
                page_token: page_token.as_deref(),
            });
            let response = require_success(
                "list_collection_ids",
                self.send("list_collection_ids", request).await?,
                false,
            )
            .await?;
            let page: FirestoreCollectionIdList = response
                .json()
                .await
                .map_err(|error| invalid_response_error("list_collection_ids", &error))?;
            for collection_id in page.collection_ids {
                validate_path_segment("Firestore collection ID", &collection_id)?;
                collection_ids.push(collection_id);
            }
            let Some(next_page_token) = page.next_page_token else {
                break;
            };
            if next_page_token.is_empty() || !seen_page_tokens.insert(next_page_token.clone()) {
                return Err(FirestoreError::InvalidDocument);
            }
            page_token = Some(next_page_token);
        }
        Ok(collection_ids)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListCollectionIdsRequest<'a> {
    page_size: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_token: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FirestoreCollectionIdList {
    #[serde(default)]
    collection_ids: Vec<String>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FirestoreDocumentNameList {
    #[serde(default)]
    documents: Vec<FirestoreDocumentName>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct FirestoreDocumentName {
    name: String,
}
