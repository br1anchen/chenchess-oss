use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    firestore::{
        codec::DurablePayload, FirestoreDatabase, FirestoreError, FirestoreVersionedDocument,
        FirestoreVersionedDocumentAtPath, FirestoreWrite,
    },
    review_durability::path::hashed_path_segment,
    review_session_contract::PlayerId,
};

use super::{
    hosted::{holds_when_preference_off, preference_allows_outbox, writes_without_preference},
    model::{
        FeedbackAnchor, QualityCaptureContent, QualityCaptureDraft, QualityCaptureId,
        QUALITY_CAPTURE_SCHEMA_VERSION,
    },
    preference_for, QualityCapturePreferenceStore, QualityCaptureStoreError, RetentionPreference,
    StoreFuture, CURRENT_DISCLOSURE_VERSION,
};
use crate::evaluation_fingerprint::CaptureTrigger;

const USERS_COLLECTION: &str = "users";
const QUALITY_OUTBOX_COLLECTION: &str = "qualityOutbox";
const HELD_COLLECTION: &str = "heldQualityCaptures";
const CAPTURES_COLLECTION: &str = "captures";
const WITHDRAWALS_COLLECTION: &str = "withdrawals";
const EXPORT_BATCH_SIZE: u16 = 50;
const PENDING_EXPORT_STATUS: &str = "pendingExport";
const WITHDRAWAL_PENDING_STATUS: &str = "withdrawalPending";

#[derive(Clone)]
pub(crate) struct FirestoreQualityCaptureStore {
    application: FirestoreDatabase,
    quality: Option<FirestoreDatabase>,
}

impl FirestoreQualityCaptureStore {
    pub(crate) fn new(application: FirestoreDatabase, quality: FirestoreDatabase) -> Self {
        Self {
            application,
            quality: Some(quality),
        }
    }

    pub(crate) fn preference_only(application: FirestoreDatabase) -> Self {
        Self {
            application,
            quality: None,
        }
    }

    fn quality(&self) -> Result<&FirestoreDatabase, QualityCaptureStoreError> {
        self.quality.as_ref().ok_or_else(|| {
            QualityCaptureStoreError::Configuration(
                "quality export credential is not configured".to_string(),
            )
        })
    }

    pub(super) async fn process_due(&self) -> Result<(), QualityCaptureStoreError> {
        if self.quality.is_none() {
            return Ok(());
        }
        let now = Utc::now();
        let (pending, withdrawals) = tokio::try_join!(
            self.application
                .query_due_collection_group::<QualityOutboxDocument>(
                    QUALITY_OUTBOX_COLLECTION,
                    PENDING_EXPORT_STATUS,
                    now,
                    EXPORT_BATCH_SIZE,
                ),
            self.application
                .query_due_collection_group::<QualityOutboxDocument>(
                    QUALITY_OUTBOX_COLLECTION,
                    WITHDRAWAL_PENDING_STATUS,
                    now,
                    EXPORT_BATCH_SIZE,
                )
        )?;
        for outbox in pending {
            if let Err(error) = self.export_one(outbox, now).await {
                tracing::error!(
                    category = error.diagnostic_category(),
                    "quality capture export failed"
                );
            }
        }

        for outbox in withdrawals {
            if let Err(error) = self.withdraw_one(outbox).await {
                tracing::error!(
                    category = error.diagnostic_category(),
                    "quality capture withdrawal failed"
                );
            }
        }
        Ok(())
    }

    pub(crate) async fn withdraw_all_for_account_deletion(
        &self,
        player_id: &PlayerId,
    ) -> Result<(), QualityCaptureStoreError> {
        self.set_preference(player_id, false).await?;
        let user_id = user_document_id(player_id);
        let documents = self
            .application
            .list_documents::<QualityOutboxDocument>(&[
                USERS_COLLECTION,
                &user_id,
                QUALITY_OUTBOX_COLLECTION,
            ])
            .await?;
        for (document_id, _) in documents {
            let path = vec![
                USERS_COLLECTION.to_string(),
                user_id.clone(),
                QUALITY_OUTBOX_COLLECTION.to_string(),
                document_id,
            ];
            let refs = path_refs(&path);
            let Some(mut versioned) = self
                .application
                .get_versioned_document::<QualityOutboxDocument>(&refs)
                .await?
            else {
                continue;
            };
            if versioned.value.status != QualityOutboxStatus::WithdrawalPending {
                let withdrawal = versioned.value.into_withdrawal();
                self.application
                    .commit(vec![self.application.update_write_at(
                        &refs,
                        &withdrawal,
                        &[
                            ("createdAt", withdrawal.created_at),
                            ("nextAttemptAt", withdrawal.next_attempt_at),
                            ("purgeAt", withdrawal.purge_at),
                        ],
                        versioned.update_time,
                    )?])
                    .await?;
                let Some(updated) = self
                    .application
                    .get_versioned_document::<QualityOutboxDocument>(&refs)
                    .await?
                else {
                    continue;
                };
                versioned = updated;
            }
            self.withdraw_one(FirestoreVersionedDocumentAtPath {
                path,
                value: versioned.value,
                update_time: versioned.update_time,
            })
            .await?;
        }
        Ok(())
    }

    async fn export_one(
        &self,
        versioned: FirestoreVersionedDocumentAtPath<QualityOutboxDocument>,
        now: DateTime<Utc>,
    ) -> Result<(), QualityCaptureStoreError> {
        validate_outbox_path(&versioned.path)?;
        let mut outbox = versioned.value;
        outbox.validate_pending()?;
        let capture_document_id = capture_document_id(&outbox.capture_id);
        let capture = outbox.capture_document()?;
        let capture_path = [CAPTURES_COLLECTION, capture_document_id.as_str()];
        let quality = self.quality()?;
        match quality
            .create_document(
                &[CAPTURES_COLLECTION],
                &capture_document_id,
                &capture,
                &[
                    ("createdAt", capture.created_at),
                    ("purgeAt", capture.purge_at),
                ],
            )
            .await
        {
            Ok(()) => {}
            Err(FirestoreError::Conflict) => {
                let existing = quality
                    .get_document::<QualityCaptureDocument>(&capture_path)
                    .await?
                    .ok_or(QualityCaptureStoreError::Conflict)?;
                if existing != capture {
                    outbox.block_digest_conflict();
                    self.update_outbox(&versioned.path, outbox, versioned.update_time)
                        .await?;
                    return Err(QualityCaptureStoreError::Conflict);
                }
            }
            Err(error) => {
                outbox.schedule_retry(now);
                let update_result = self
                    .update_outbox(&versioned.path, outbox, versioned.update_time)
                    .await;
                if let Err(update_error) = update_result {
                    tracing::warn!(
                        category = update_error.diagnostic_category(),
                        "quality capture retry could not be scheduled"
                    );
                }
                return Err(error.into());
            }
        }

        outbox.mark_exported();
        match self
            .update_outbox(&versioned.path, outbox, versioned.update_time)
            .await
        {
            Ok(()) => Ok(()),
            Err(QualityCaptureStoreError::Conflict) => {
                self.compensate_if_withdrawn(&versioned.path, &capture_path, &capture)
                    .await
            }
            Err(error) => Err(error),
        }
    }

    async fn compensate_if_withdrawn(
        &self,
        outbox_path: &[String],
        capture_path: &[&str],
        capture: &QualityCaptureDocument,
    ) -> Result<(), QualityCaptureStoreError> {
        let path = path_refs(outbox_path);
        let current = self
            .application
            .get_document::<QualityOutboxDocument>(&path)
            .await?;
        match current {
            Some(outbox)
                if outbox.status == QualityOutboxStatus::Exported
                    && outbox.content_digest == capture.content_digest =>
            {
                Ok(())
            }
            Some(outbox) if outbox.status == QualityOutboxStatus::PendingExport => Ok(()),
            Some(_) | None => {
                let quality = self.quality()?;
                let existing = quality
                    .get_document::<QualityCaptureDocument>(capture_path)
                    .await?;
                if existing.as_ref() == Some(capture) {
                    quality
                        .commit(vec![quality.delete_write(capture_path)?])
                        .await?;
                }
                Ok(())
            }
        }
    }

    async fn withdraw_one(
        &self,
        versioned: FirestoreVersionedDocumentAtPath<QualityOutboxDocument>,
    ) -> Result<(), QualityCaptureStoreError> {
        validate_outbox_path(&versioned.path)?;
        let outbox = versioned.value;
        outbox.validate_withdrawal()?;
        let quality = self.quality()?;
        let capture_document_id = capture_document_id(&outbox.capture_id);
        let capture_path = [CAPTURES_COLLECTION, capture_document_id.as_str()];
        let existing = quality
            .get_document::<QualityCaptureDocument>(&capture_path)
            .await?;
        let matching_capture = existing
            .as_ref()
            .is_some_and(|capture| capture.content_digest == outbox.content_digest);

        let mut quality_writes = Vec::with_capacity(2);
        if matching_capture {
            quality_writes.push(quality.delete_write(&capture_path)?);
        }
        if outbox.admitted {
            let withdrawal_path = [WITHDRAWALS_COLLECTION, capture_document_id.as_str()];
            let existing_withdrawal = quality
                .get_document::<QualityWithdrawalDocument>(&withdrawal_path)
                .await?;
            match existing_withdrawal {
                Some(withdrawal)
                    if withdrawal.schema_version == QUALITY_CAPTURE_SCHEMA_VERSION
                        && withdrawal.content_digest == outbox.content_digest => {}
                Some(_) => return Err(QualityCaptureStoreError::Conflict),
                None => {
                    let withdrawn_at = Utc::now();
                    quality_writes.push(quality.create_write(
                        &withdrawal_path,
                        &QualityWithdrawalDocument {
                            schema_version: QUALITY_CAPTURE_SCHEMA_VERSION,
                            content_digest: outbox.content_digest.clone(),
                            withdrawn_at,
                        },
                        &[("withdrawnAt", withdrawn_at)],
                    )?);
                }
            }
        }
        if !quality_writes.is_empty() {
            match quality.commit(quality_writes).await {
                Ok(()) => {}
                Err(FirestoreError::Conflict) if outbox.admitted => {
                    let withdrawal = quality
                        .get_document::<QualityWithdrawalDocument>(&[
                            WITHDRAWALS_COLLECTION,
                            &capture_document_id,
                        ])
                        .await?
                        .ok_or(QualityCaptureStoreError::Conflict)?;
                    if withdrawal.content_digest != outbox.content_digest {
                        return Err(QualityCaptureStoreError::Conflict);
                    }
                    if matching_capture {
                        quality
                            .commit(vec![quality.delete_write(&capture_path)?])
                            .await?;
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }

        let path = path_refs(&versioned.path);
        match self
            .application
            .commit(vec![self
                .application
                .delete_write_at(&path, versioned.update_time)?])
            .await
        {
            Ok(()) | Err(FirestoreError::Conflict) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    async fn update_outbox(
        &self,
        path: &[String],
        outbox: QualityOutboxDocument,
        update_time: String,
    ) -> Result<(), QualityCaptureStoreError> {
        let path = path_refs(path);
        self.application
            .commit(vec![self.application.update_write_at(
                &path,
                &outbox,
                &[
                    ("createdAt", outbox.created_at),
                    ("nextAttemptAt", outbox.next_attempt_at),
                    ("purgeAt", outbox.purge_at),
                ],
                update_time,
            )?])
            .await
            .map_err(Into::into)
    }

    async fn queue_withdrawals(
        &self,
        player_id: &PlayerId,
    ) -> Result<usize, QualityCaptureStoreError> {
        let user_id = user_document_id(player_id);
        let documents = self
            .application
            .list_documents::<QualityOutboxDocument>(&[
                USERS_COLLECTION,
                &user_id,
                QUALITY_OUTBOX_COLLECTION,
            ])
            .await?;
        let mut queued = 0;
        for (document_id, _) in documents {
            let path = [
                USERS_COLLECTION,
                user_id.as_str(),
                QUALITY_OUTBOX_COLLECTION,
                document_id.as_str(),
            ];
            let Some(versioned) = self
                .application
                .get_versioned_document::<QualityOutboxDocument>(&path)
                .await?
            else {
                continue;
            };
            if versioned.value.status == QualityOutboxStatus::WithdrawalPending {
                continue;
            }
            let outbox = versioned.value.into_withdrawal();
            let write = self.application.update_write_at(
                &path,
                &outbox,
                &[
                    ("createdAt", outbox.created_at),
                    ("nextAttemptAt", outbox.next_attempt_at),
                    ("purgeAt", outbox.purge_at),
                ],
                versioned.update_time,
            )?;
            match self.application.commit(vec![write]).await {
                Ok(()) => queued += 1,
                Err(FirestoreError::Conflict) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(queued)
    }
}

impl QualityCapturePreferenceStore for FirestoreQualityCaptureStore {
    fn preference<'a>(&'a self, player_id: &'a PlayerId) -> StoreFuture<'a, RetentionPreference> {
        Box::pin(async move {
            let user_id = user_document_id(player_id);
            let document = self
                .application
                .get_document::<QualityPreferenceDocument>(&[USERS_COLLECTION, &user_id])
                .await?;
            let Some(document) = document else {
                return Ok(preference_for(None, 0));
            };
            document.validate()?;
            Ok(preference_for(
                document.acknowledged().then_some(document.capture_enabled),
                0,
            ))
        })
    }

    fn set_preference<'a>(
        &'a self,
        player_id: &'a PlayerId,
        enabled: bool,
    ) -> StoreFuture<'a, RetentionPreference> {
        Box::pin(async move {
            let now = Utc::now();
            let user_id = user_document_id(player_id);
            let path = [USERS_COLLECTION, user_id.as_str()];
            let existing = self
                .application
                .get_versioned_document::<QualityPreferenceDocument>(&path)
                .await?;
            if let Some(existing) = &existing {
                existing.value.validate()?;
            }
            let document = QualityPreferenceDocument {
                schema_version: QUALITY_CAPTURE_SCHEMA_VERSION,
                created_at: existing
                    .as_ref()
                    .map_or(now, |existing| existing.value.created_at),
                updated_at: now,
                capture_enabled: enabled,
                acknowledged_disclosure_version: CURRENT_DISCLOSURE_VERSION,
            };
            let timestamps = [
                ("createdAt", document.created_at),
                ("updatedAt", document.updated_at),
            ];
            let write = match existing {
                Some(existing) => self.application.update_write_at(
                    &path,
                    &document,
                    &timestamps,
                    existing.update_time,
                )?,
                None => self
                    .application
                    .create_write(&path, &document, &timestamps)?,
            };
            self.application.commit(vec![write]).await?;
            let queued = if enabled {
                0
            } else {
                self.queue_withdrawals(player_id).await?
            };
            Ok(preference_for(Some(enabled), queued))
        })
    }
}

pub(super) async fn prepare_outbox_writes(
    database: &FirestoreDatabase,
    player_id: &PlayerId,
    capture: &QualityCaptureDraft,
) -> Result<Vec<FirestoreWrite>, QualityCaptureStoreError> {
    if !database.is_application() || !capture.has_valid_shape() {
        return Err(QualityCaptureStoreError::InvalidRecord);
    }
    let user_id = user_document_id(player_id);
    let user_path = [USERS_COLLECTION, user_id.as_str()];
    let preference = database
        .get_versioned_document::<QualityPreferenceDocument>(&user_path)
        .await?;
    if let Some(preference) = &preference {
        preference.value.validate()?;
    }
    let allowed = preference.as_ref().is_some_and(|preference| {
        preference_allows_outbox(&preference_for(
            preference
                .value
                .acknowledged()
                .then_some(preference.value.capture_enabled),
            0,
        ))
    });
    if writes_without_preference(capture) || allowed {
        return outbox_writes(database, player_id, capture, preference.as_ref());
    }
    if holds_when_preference_off(capture) {
        return held_writes(database, player_id, capture, preference.as_ref());
    }
    Ok(Vec::new())
}

pub(super) async fn take_held(
    database: &FirestoreDatabase,
    player_id: &PlayerId,
) -> Option<QualityCaptureDraft> {
    let user_id = user_document_id(player_id);
    let documents = database
        .list_documents::<HeldQualityCaptureDocument>(&[
            USERS_COLLECTION,
            user_id.as_str(),
            HELD_COLLECTION,
        ])
        .await
        .ok()?;
    documents
        .into_iter()
        .next()
        .map(|(_, document)| document.into_draft())
}

/// The Player's most recent exported generation, as the join a Review Feedback
/// Report is written against. A withdrawn row has dropped its digest and is
/// never an anchor.
pub(super) async fn feedback_anchor(
    database: &FirestoreDatabase,
    player_id: &PlayerId,
) -> Option<FeedbackAnchor> {
    let user_id = user_document_id(player_id);
    let documents = database
        .list_documents::<QualityOutboxDocument>(&[
            USERS_COLLECTION,
            user_id.as_str(),
            QUALITY_OUTBOX_COLLECTION,
        ])
        .await
        .ok()?;
    documents
        .into_iter()
        .filter_map(|(_, document)| {
            if !matches!(
                document.status,
                QualityOutboxStatus::PendingExport | QualityOutboxStatus::Exported
            ) {
                return None;
            }
            let fingerprint_digest = document.fingerprint_digest?;
            Some((
                document.created_at,
                FeedbackAnchor {
                    capture_id: document.capture_id,
                    fingerprint_digest,
                },
            ))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, anchor)| anchor)
}

fn outbox_writes(
    database: &FirestoreDatabase,
    player_id: &PlayerId,
    capture: &QualityCaptureDraft,
    preference: Option<&FirestoreVersionedDocument<QualityPreferenceDocument>>,
) -> Result<Vec<FirestoreWrite>, QualityCaptureStoreError> {
    let user_id = user_document_id(player_id);
    let capture_document_id = capture_document_id(&capture.capture_id);
    let outbox_path = [
        USERS_COLLECTION,
        user_id.as_str(),
        QUALITY_OUTBOX_COLLECTION,
        capture_document_id.as_str(),
    ];
    let mut writes = Vec::with_capacity(2);
    if let Some(preference) = preference {
        writes.push(database.update_write_at(
            &[USERS_COLLECTION, user_id.as_str()],
            &preference.value,
            &[
                ("createdAt", preference.value.created_at),
                ("updatedAt", preference.value.updated_at),
            ],
            preference.update_time.clone(),
        )?);
    }
    writes.push(database.create_write(
        &outbox_path,
        &QualityOutboxDocument::pending(capture.clone()),
        &[
            ("createdAt", capture.created_at),
            ("nextAttemptAt", capture.created_at),
            ("purgeAt", capture.purge_at),
        ],
    )?);
    Ok(writes)
}

fn held_writes(
    database: &FirestoreDatabase,
    player_id: &PlayerId,
    capture: &QualityCaptureDraft,
    preference: Option<&FirestoreVersionedDocument<QualityPreferenceDocument>>,
) -> Result<Vec<FirestoreWrite>, QualityCaptureStoreError> {
    let user_id = user_document_id(player_id);
    let capture_document_id = capture_document_id(&capture.capture_id);
    let held_path = [
        USERS_COLLECTION,
        user_id.as_str(),
        HELD_COLLECTION,
        capture_document_id.as_str(),
    ];
    let mut writes = Vec::with_capacity(2);
    if let Some(preference) = preference {
        writes.push(database.update_write_at(
            &[USERS_COLLECTION, user_id.as_str()],
            &preference.value,
            &[
                ("createdAt", preference.value.created_at),
                ("updatedAt", preference.value.updated_at),
            ],
            preference.update_time.clone(),
        )?);
    }
    writes.push(database.create_write(
        &held_path,
        &HeldQualityCaptureDocument::from_draft(capture),
        &[
            ("createdAt", capture.created_at),
            ("purgeAt", capture.purge_at),
        ],
    )?);
    Ok(writes)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualityPreferenceDocument {
    schema_version: u8,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    capture_enabled: bool,
    acknowledged_disclosure_version: u16,
}

impl QualityPreferenceDocument {
    fn acknowledged(&self) -> bool {
        self.acknowledged_disclosure_version == CURRENT_DISCLOSURE_VERSION
    }

    fn validate(&self) -> Result<(), QualityCaptureStoreError> {
        if self.schema_version != QUALITY_CAPTURE_SCHEMA_VERSION
            || self.updated_at < self.created_at
            || self.acknowledged_disclosure_version > CURRENT_DISCLOSURE_VERSION
        {
            Err(QualityCaptureStoreError::InvalidRecord)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum QualityOutboxStatus {
    PendingExport,
    Exported,
    WithdrawalPending,
    DigestConflict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualityOutboxDocument {
    schema_version: u8,
    status: QualityOutboxStatus,
    next_attempt_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    purge_at: DateTime<Utc>,
    capture_id: QualityCaptureId,
    content_digest: crate::review_session_contract::ArtifactDigest,
    /// Evaluation Fingerprint of a Language Layer generation, kept past export
    /// so late feedback can still be anchored to it. The payload leaves on
    /// export and the quality database is write-only from here, so without this
    /// a consented Player's feedback has nothing to point at. It is a digest
    /// over model, pin, and axes — thousands of captures share one, and it
    /// carries nothing about the Player.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fingerprint_digest: Option<crate::review_session_contract::ArtifactDigest>,
    admitted: bool,
    attempt_count: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    destination_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload: Option<DurablePayload<QualityCapturePayload>>,
}

impl QualityOutboxDocument {
    fn pending(capture: QualityCaptureDraft) -> Self {
        let fingerprint_digest = capture
            .feedback_anchor()
            .map(|anchor| anchor.fingerprint_digest);
        Self {
            schema_version: capture.schema_version,
            status: QualityOutboxStatus::PendingExport,
            next_attempt_at: capture.created_at,
            created_at: capture.created_at,
            purge_at: capture.purge_at,
            capture_id: capture.capture_id,
            content_digest: capture.content_digest,
            fingerprint_digest,
            admitted: false,
            attempt_count: 0,
            destination_ref: None,
            payload: Some(DurablePayload::new(QualityCapturePayload {
                case_key: capture.case_key,
                content: capture.content,
            })),
        }
    }

    fn validate_common(&self) -> Result<(), QualityCaptureStoreError> {
        if self.schema_version != QUALITY_CAPTURE_SCHEMA_VERSION
            || self.purge_at <= self.created_at
            || self.next_attempt_at < self.created_at
            || self
                .destination_ref
                .as_ref()
                .is_some_and(|reference| reference != &capture_document_id(&self.capture_id))
        {
            Err(QualityCaptureStoreError::InvalidRecord)
        } else {
            Ok(())
        }
    }

    fn validate_pending(&self) -> Result<(), QualityCaptureStoreError> {
        self.validate_common()?;
        let Some(payload) = self.payload.as_ref() else {
            return Err(QualityCaptureStoreError::InvalidRecord);
        };
        let payload = payload.clone().into_inner();
        if self.status != QualityOutboxStatus::PendingExport
            || self.destination_ref.is_some()
            || self.admitted
            || !QualityCaptureDraft::material_has_valid_shape(
                self.schema_version,
                self.created_at,
                self.purge_at,
                &payload.case_key,
                &self.content_digest,
                &payload.content,
            )
        {
            Err(QualityCaptureStoreError::InvalidRecord)
        } else {
            Ok(())
        }
    }

    fn validate_withdrawal(&self) -> Result<(), QualityCaptureStoreError> {
        self.validate_common()?;
        if self.status != QualityOutboxStatus::WithdrawalPending || self.payload.is_some() {
            Err(QualityCaptureStoreError::InvalidRecord)
        } else {
            Ok(())
        }
    }

    fn capture_document(&self) -> Result<QualityCaptureDocument, QualityCaptureStoreError> {
        let payload = self
            .payload
            .clone()
            .ok_or(QualityCaptureStoreError::InvalidRecord)?;
        let inner = payload.clone().into_inner();
        let axes = language_layer_index_axes(&inner.content);
        Ok(QualityCaptureDocument {
            schema_version: self.schema_version,
            created_at: self.created_at,
            purge_at: self.purge_at,
            content_digest: self.content_digest.clone(),
            fingerprint_digest: axes.as_ref().map(|axes| axes.fingerprint_digest.clone()),
            environment: axes.as_ref().map(|axes| axes.environment),
            capture_origin: axes.as_ref().map(|axes| axes.capture_origin),
            capture_trigger: axes.as_ref().map(|axes| axes.capture_trigger),
            capture_outcome: axes.as_ref().map(|axes| axes.capture_outcome),
            payload,
        })
    }

    fn mark_exported(&mut self) {
        self.status = QualityOutboxStatus::Exported;
        self.next_attempt_at = self.purge_at;
        self.destination_ref = Some(capture_document_id(&self.capture_id));
        self.payload = None;
    }

    fn schedule_retry(&mut self, now: DateTime<Utc>) {
        self.attempt_count = self.attempt_count.saturating_add(1);
        let exponent = self.attempt_count.min(6);
        let delay_minutes = 1_i64 << exponent;
        self.next_attempt_at = now
            .checked_add_signed(TimeDelta::minutes(delay_minutes))
            .unwrap_or(self.purge_at)
            .min(self.purge_at);
    }

    fn block_digest_conflict(&mut self) {
        self.status = QualityOutboxStatus::DigestConflict;
        self.next_attempt_at = self.purge_at;
        self.destination_ref = Some(capture_document_id(&self.capture_id));
        self.payload = None;
    }

    fn into_withdrawal(mut self) -> Self {
        self.status = QualityOutboxStatus::WithdrawalPending;
        self.next_attempt_at = Utc::now();
        self.destination_ref = Some(capture_document_id(&self.capture_id));
        self.payload = None;
        self.fingerprint_digest = None;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualityCapturePayload {
    case_key: crate::review_session_contract::ArtifactDigest,
    content: QualityCaptureContent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualityCaptureDocument {
    schema_version: u8,
    created_at: DateTime<Utc>,
    purge_at: DateTime<Utc>,
    content_digest: crate::review_session_contract::ArtifactDigest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fingerprint_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    environment: Option<crate::evaluation_fingerprint::EvaluationEnvironment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_origin: Option<crate::evaluation_fingerprint::CaptureOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_trigger: Option<CaptureTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_outcome: Option<crate::evaluation_fingerprint::CaptureOutcome>,
    payload: DurablePayload<QualityCapturePayload>,
}

struct LanguageLayerIndexAxes {
    fingerprint_digest: String,
    environment: crate::evaluation_fingerprint::EvaluationEnvironment,
    capture_origin: crate::evaluation_fingerprint::CaptureOrigin,
    capture_trigger: CaptureTrigger,
    capture_outcome: crate::evaluation_fingerprint::CaptureOutcome,
}

fn language_layer_index_axes(content: &QualityCaptureContent) -> Option<LanguageLayerIndexAxes> {
    match content {
        QualityCaptureContent::LanguageLayerGeneration {
            fingerprint,
            observations,
            ..
        } => Some(LanguageLayerIndexAxes {
            fingerprint_digest: fingerprint.digest.as_str().to_string(),
            environment: fingerprint.axes.environment,
            capture_origin: fingerprint.axes.capture_origin,
            capture_trigger: observations.capture_trigger,
            capture_outcome: observations.capture_outcome,
        }),
        QualityCaptureContent::GameAnalysis { .. }
        | QualityCaptureContent::CoachingResponse { .. }
        | QualityCaptureContent::FeedbackAnnotation { .. } => None,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HeldQualityCaptureDocument {
    schema_version: u8,
    created_at: DateTime<Utc>,
    purge_at: DateTime<Utc>,
    capture_id: QualityCaptureId,
    content_digest: crate::review_session_contract::ArtifactDigest,
    payload: DurablePayload<QualityCapturePayload>,
}

impl HeldQualityCaptureDocument {
    fn into_draft(self) -> QualityCaptureDraft {
        let payload = self.payload.into_inner();
        QualityCaptureDraft {
            schema_version: self.schema_version,
            capture_id: self.capture_id,
            created_at: self.created_at,
            purge_at: self.purge_at,
            case_key: payload.case_key,
            content_digest: self.content_digest,
            content: payload.content,
        }
    }

    fn from_draft(capture: &QualityCaptureDraft) -> Self {
        Self {
            schema_version: capture.schema_version,
            created_at: capture.created_at,
            purge_at: capture.purge_at,
            capture_id: capture.capture_id.clone(),
            content_digest: capture.content_digest.clone(),
            payload: DurablePayload::new(QualityCapturePayload {
                case_key: capture.case_key.clone(),
                content: capture.content.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualityWithdrawalDocument {
    schema_version: u8,
    content_digest: crate::review_session_contract::ArtifactDigest,
    withdrawn_at: DateTime<Utc>,
}

fn user_document_id(player_id: &PlayerId) -> String {
    hashed_path_segment(player_id.as_str())
}

fn capture_document_id(capture_id: &QualityCaptureId) -> String {
    hashed_path_segment(capture_id.as_str())
}

fn validate_outbox_path(path: &[String]) -> Result<(), QualityCaptureStoreError> {
    if path.len() == 4
        && path[0] == USERS_COLLECTION
        && is_lower_hex_digest(&path[1])
        && path[2] == QUALITY_OUTBOX_COLLECTION
        && is_lower_hex_digest(&path[3])
    {
        Ok(())
    } else {
        Err(QualityCaptureStoreError::InvalidRecord)
    }
}

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn path_refs(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}

impl From<FirestoreError> for QualityCaptureStoreError {
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
#[path = "firestore/tests.rs"]
mod tests;
