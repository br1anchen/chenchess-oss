use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    critical_moment_comment::CriticalMomentCommentAuthoringProvenance,
    firestore::{codec::DurablePayload, FirestoreDatabase, FirestoreError, FirestoreWrite},
    review_durability::path::hashed_path_segment,
    review_session_contract::{
        CriticalMomentComment, CriticalMomentId, GameImportId, IdempotencyKey,
    },
    review_session_processor::ProcessorPrincipal,
};

/// Named on the durable layout so the Review Session purge cannot reach it.
pub(crate) const REVIEW_ANNOTATIONS_COLLECTION: &str = "reviewAnnotations";
pub(crate) const REVIEW_ANNOTATION_COMMENTS_COLLECTION: &str = "comments";

/// Everything one Player has published on one imported Game.
///
/// `GameImportId` is `game-import:{review_key}:{owner_key}`, so it already
/// carries both the Player and the reviewed Game. The owner is carried beside
/// it because it selects the Firestore Player subtree, and that subtree is the
/// authorization boundary: a caller cannot address another Player's annotations
/// because it cannot address another Player's subtree.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReviewAnnotationAddress {
    pub owner: ProcessorPrincipal,
    pub game_import_id: GameImportId,
}

/// One published Review Moment Comment, durable beyond the conversation that
/// wrote it.
///
/// Only a comment that passed the Grounding Gate is ever recorded. A Gate
/// rejection opens a bounded retry inside one conversation and is ephemeral by
/// construction, so nothing short of a finished comment reaches durability.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewMomentAnnotation {
    pub moment_id: CriticalMomentId,
    pub idempotency_key: IdempotencyKey,
    pub comment: CriticalMomentComment,
    pub authoring_provenance: CriticalMomentCommentAuthoringProvenance,
    pub published_at: DateTime<Utc>,
}

impl ReviewMomentAnnotation {
    /// What one logical write is addressed by inside a review: the Review
    /// Moment it annotates and the Player's key for that write.
    fn identity(&self) -> (&CriticalMomentId, &IdempotencyKey) {
        (&self.moment_id, &self.idempotency_key)
    }
}

/// Every annotation stored at one address, ordered oldest first.
///
/// Annotations are append-only, so which comment a Player currently sees is a
/// property of the set rather than of any single record. Answering that here
/// keeps the ordering rule in one place instead of at every call site.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReviewAnnotations {
    entries: Vec<ReviewMomentAnnotation>,
}

impl ReviewAnnotations {
    fn new(entries: Vec<ReviewMomentAnnotation>) -> Self {
        let mut annotations = Self { entries };
        annotations.sort();
        annotations
    }

    /// Orders by publication time, breaking ties on the idempotency key.
    ///
    /// The time is the Coach Engine's own, not a caller's, and the tie-break
    /// makes "newest" the same answer for every reader. Two conversations
    /// publishing on one Review Moment within the same instant is the accepted
    /// testing-only edge case, not a guarantee about which one wins.
    fn sort(&mut self) {
        self.entries.sort_by(|left, right| {
            left.published_at
                .cmp(&right.published_at)
                .then_with(|| left.idempotency_key.cmp(&right.idempotency_key))
        });
    }

    fn insert(&mut self, annotation: ReviewMomentAnnotation) {
        if self
            .for_key(&annotation.moment_id, &annotation.idempotency_key)
            .is_some()
        {
            return;
        }

        self.entries.push(annotation);
        self.sort();
    }

    /// The comment a Player currently sees on one Review Moment.
    pub fn active(&self, moment_id: &CriticalMomentId) -> Option<&ReviewMomentAnnotation> {
        self.entries
            .iter()
            .rev()
            .find(|entry| &entry.moment_id == moment_id)
    }

    /// The annotation one logical write already produced, if it has.
    pub fn for_key(
        &self,
        moment_id: &CriticalMomentId,
        idempotency_key: &IdempotencyKey,
    ) -> Option<&ReviewMomentAnnotation> {
        self.entries
            .iter()
            .find(|entry| entry.identity() == (moment_id, idempotency_key))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub type ReviewAnnotationStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ReviewAnnotationStoreError>> + Send + 'a>>;

/// The durable, append-only home of published Review Moment Comments.
///
/// There is no replace and no delete: an annotation is only ever added, and a
/// replayed idempotency key returns the annotation that key already wrote. A
/// Player's own notes therefore accumulate across conversations instead of
/// being overwritten by whichever chat wrote last.
pub trait ReviewAnnotationStore: Send + Sync {
    /// Records one logical publication, returning the authoritative annotation.
    ///
    /// Replaying a key that already published returns that key's original
    /// annotation rather than writing a second one, so an accidental double-tap
    /// or a retried request costs one comment.
    fn append<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
        annotation: ReviewMomentAnnotation,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewMomentAnnotation>;

    fn read<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewAnnotations>;

    /// Removes every annotation of one review.
    ///
    /// Deleting a Game re-importable at the same address has to take its
    /// comments too: left behind, they would come back attached to a review the
    /// Player published nothing against. Deleting what is already gone
    /// succeeds.
    fn delete<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ()>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReviewAnnotationStoreError {
    #[error("Review Moment annotation persistence is misconfigured: {0}")]
    Configuration(String),
    #[error("Review Moment annotation persistence is unavailable")]
    Unavailable,
    #[error("a stored Review Moment annotation is unreadable")]
    InvalidRecord,
}

impl ReviewAnnotationStoreError {
    pub(crate) fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::Configuration(_) => "configuration",
            Self::Unavailable => "unavailable",
            Self::InvalidRecord => "invalid-record",
        }
    }
}

impl From<FirestoreError> for ReviewAnnotationStoreError {
    fn from(error: FirestoreError) -> Self {
        match error {
            FirestoreError::Configuration(message) => Self::Configuration(message),
            FirestoreError::InvalidDocument => Self::InvalidRecord,
            FirestoreError::Transport | FirestoreError::Unavailable | FirestoreError::Conflict => {
                Self::Unavailable
            }
        }
    }
}

/// One review's annotations, read once and appended through.
///
/// A Review Session reads the whole Game Import's annotations when it opens and
/// then answers every Review Moment from that snapshot, so restoring a Player's
/// own notes costs one round trip per review rather than one per moment. A
/// comment published elsewhere while this conversation is open is deliberately
/// not pulled in: cross-conversation activity is the host model's to surface,
/// not the widget's to reconcile.
pub struct ReviewAnnotationLog {
    store: Arc<dyn ReviewAnnotationStore>,
    address: ReviewAnnotationAddress,
    annotations: Mutex<ReviewAnnotations>,
}

impl ReviewAnnotationLog {
    pub async fn load(
        store: Arc<dyn ReviewAnnotationStore>,
        address: ReviewAnnotationAddress,
    ) -> Result<Self, ReviewAnnotationStoreError> {
        let annotations = store.read(&address).await?;
        Ok(Self {
            store,
            address,
            annotations: Mutex::new(annotations),
        })
    }

    pub async fn active(&self, moment_id: &CriticalMomentId) -> Option<ReviewMomentAnnotation> {
        self.annotations.lock().await.active(moment_id).cloned()
    }

    /// Publishes one annotation durably and adopts whatever the store made
    /// authoritative.
    pub async fn publish(
        &self,
        annotation: ReviewMomentAnnotation,
    ) -> Result<ReviewMomentAnnotation, ReviewAnnotationStoreError> {
        let stored = self.store.append(&self.address, annotation).await?;
        self.annotations.lock().await.insert(stored.clone());
        Ok(stored)
    }

    pub fn address(&self) -> &ReviewAnnotationAddress {
        &self.address
    }

    pub async fn adopt(&self, annotation: ReviewMomentAnnotation) {
        self.annotations.lock().await.insert(annotation);
    }
}

#[derive(Default)]
pub struct InMemoryReviewAnnotationStore {
    annotations: Mutex<BTreeMap<ReviewAnnotationAddress, Vec<ReviewMomentAnnotation>>>,
}

impl ReviewAnnotationStore for InMemoryReviewAnnotationStore {
    fn append<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
        annotation: ReviewMomentAnnotation,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewMomentAnnotation> {
        Box::pin(async move {
            let mut annotations = self.annotations.lock().await;
            let entries = annotations.entry(address.clone()).or_default();
            match entries
                .iter()
                .find(|entry| entry.identity() == annotation.identity())
            {
                Some(existing) => Ok(existing.clone()),
                None => {
                    entries.push(annotation.clone());
                    Ok(annotation)
                }
            }
        })
    }

    fn read<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewAnnotations> {
        Box::pin(async move {
            Ok(ReviewAnnotations::new(
                self.annotations
                    .lock()
                    .await
                    .get(address)
                    .cloned()
                    .unwrap_or_default(),
            ))
        })
    }

    fn delete<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ()> {
        Box::pin(async move {
            self.annotations.lock().await.remove(address);
            Ok(())
        })
    }
}

pub(crate) fn review_annotation_store(
    database: FirestoreDatabase,
) -> Arc<dyn ReviewAnnotationStore> {
    Arc::new(FirestoreReviewAnnotationStore { database })
}

struct FirestoreReviewAnnotationStore {
    database: FirestoreDatabase,
}

/// The stored form of one annotation.
///
/// `publishedAt` is promoted to a Firestore timestamp because it is the
/// ordering field; everything else rides in the queryless payload. The document
/// records no Review Session, no revision, and no purge time — a Player's own
/// note outlives every conversation that could have carried one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewAnnotationDocument {
    published_at: DateTime<Utc>,
    payload: DurablePayload<ReviewAnnotationPayload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewAnnotationPayload {
    moment_id: CriticalMomentId,
    idempotency_key: IdempotencyKey,
    comment: CriticalMomentComment,
    authoring_provenance: CriticalMomentCommentAuthoringProvenance,
}

impl ReviewAnnotationDocument {
    fn from_annotation(annotation: &ReviewMomentAnnotation) -> Self {
        Self {
            published_at: annotation.published_at,
            payload: DurablePayload::new(ReviewAnnotationPayload {
                moment_id: annotation.moment_id.clone(),
                idempotency_key: annotation.idempotency_key.clone(),
                comment: annotation.comment.clone(),
                authoring_provenance: annotation.authoring_provenance.clone(),
            }),
        }
    }

    fn into_annotation(self) -> ReviewMomentAnnotation {
        let payload = self.payload.into_inner();
        ReviewMomentAnnotation {
            moment_id: payload.moment_id,
            idempotency_key: payload.idempotency_key,
            comment: payload.comment,
            authoring_provenance: payload.authoring_provenance,
            published_at: self.published_at,
        }
    }
}

pub(crate) fn annotation_create_write(
    database: &FirestoreDatabase,
    address: &ReviewAnnotationAddress,
    annotation: &ReviewMomentAnnotation,
) -> Result<FirestoreWrite, ReviewAnnotationStoreError> {
    let path = annotation_path(address, annotation)?;
    let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
    let document = ReviewAnnotationDocument::from_annotation(annotation);
    Ok(database.create_write(
        &path_refs,
        &document,
        &[("publishedAt", document.published_at)],
    )?)
}

pub(crate) async fn read_annotation_document(
    database: &FirestoreDatabase,
    address: &ReviewAnnotationAddress,
    annotation: &ReviewMomentAnnotation,
) -> Result<Option<ReviewMomentAnnotation>, ReviewAnnotationStoreError> {
    let path = annotation_path(address, annotation)?;
    let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(database
        .get_document::<ReviewAnnotationDocument>(&path_refs)
        .await?
        .map(ReviewAnnotationDocument::into_annotation))
}

impl FirestoreReviewAnnotationStore {
    async fn append_annotation(
        &self,
        address: &ReviewAnnotationAddress,
        annotation: ReviewMomentAnnotation,
    ) -> Result<ReviewMomentAnnotation, ReviewAnnotationStoreError> {
        let write = annotation_create_write(&self.database, address, &annotation)?;
        match self.database.commit(vec![write]).await {
            Ok(()) => Ok(annotation),
            // The key already published here. Append-only means the earlier
            // record wins, so the replay reads back what it originally wrote.
            Err(FirestoreError::Conflict) => {
                read_annotation_document(&self.database, address, &annotation)
                    .await?
                    .ok_or(ReviewAnnotationStoreError::Unavailable)
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn read_annotations(
        &self,
        address: &ReviewAnnotationAddress,
    ) -> Result<ReviewAnnotations, ReviewAnnotationStoreError> {
        let path = review_annotations_path(address)?;
        let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
        Ok(ReviewAnnotations::new(
            self.database
                .list_documents::<ReviewAnnotationDocument>(&path_refs)
                .await?
                .into_iter()
                .map(|(_, document)| document.into_annotation())
                .collect(),
        ))
    }

    async fn delete_annotations(
        &self,
        address: &ReviewAnnotationAddress,
    ) -> Result<(), ReviewAnnotationStoreError> {
        /* The comments are a subcollection of the review's annotation document,
        so the review document is what has to go recursively; deleting the
        comments alone would leave the parent to answer a later read. */
        let mut path = review_annotations_path(address)?;
        path.pop();
        let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
        self.database
            .recursive_delete_document(&path_refs)
            .await
            .map_err(Into::into)
    }
}

impl ReviewAnnotationStore for FirestoreReviewAnnotationStore {
    fn append<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
        annotation: ReviewMomentAnnotation,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewMomentAnnotation> {
        Box::pin(self.append_annotation(address, annotation))
    }

    fn read<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewAnnotations> {
        Box::pin(self.read_annotations(address))
    }

    fn delete<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ()> {
        Box::pin(self.delete_annotations(address))
    }
}

/// Roots every annotation in the Player subtree that account deletion removes,
/// so erasure holds by construction rather than by remembering this store.
fn review_annotations_path(
    address: &ReviewAnnotationAddress,
) -> Result<Vec<String>, ReviewAnnotationStoreError> {
    let ProcessorPrincipal::Player(player_id) = &address.owner else {
        return Err(ReviewAnnotationStoreError::Configuration(
            "Local Coach annotations use in-memory durability".to_string(),
        ));
    };
    let mut path = crate::account_deletion::application_data_document_path(player_id).to_vec();
    path.push(REVIEW_ANNOTATIONS_COLLECTION.to_string());
    path.push(hashed_path_segment(address.game_import_id.as_str()));
    path.push(REVIEW_ANNOTATION_COMMENTS_COLLECTION.to_string());
    Ok(path)
}

fn annotation_path(
    address: &ReviewAnnotationAddress,
    annotation: &ReviewMomentAnnotation,
) -> Result<Vec<String>, ReviewAnnotationStoreError> {
    let mut path = review_annotations_path(address)?;
    path.push(hashed_path_segment(
        serde_json_canonicalizer::to_vec(&annotation.identity())
            .expect("a Review Moment annotation identity has a canonical representation"),
    ));
    Ok(path)
}

#[cfg(test)]
#[path = "review_annotation_store/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "review_annotation_store/firestore_tests.rs"]
mod firestore_tests;
