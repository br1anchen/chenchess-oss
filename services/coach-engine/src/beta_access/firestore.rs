use std::{collections::BTreeMap, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::firestore::{FirestoreDatabase, FirestoreError, FirestoreTransaction, FirestoreWrite};
use crate::review_session_contract::PlayerId;

use super::{
    invitation::{
        InvitationDeliveryAttempt, InvitationDeliveryStatus, InvitationStatus, StoredInvitation,
    },
    opaque_request_id, BeginRetryFuture, BetaAccessAdminRequest,
    BetaAccessAuthorizationRevokeResult, BetaAccessAuthorizationStatus, BetaAccessGrantCommit,
    BetaAccessGrantTarget, BetaAccessInvitationTarget, BetaAccessRedemptionAttempt,
    BetaAccessRedemptionCandidate, BetaAccessRequest, BetaAccessRequestStatus,
    BetaAccessRetryCommit, BetaAccessRevokeResult, BetaAccessStore, BetaAccessStoreError,
    BetaAccessStoreOutcome, BetaAccessSubmission, CommitGrantFuture, CommitRedemptionFuture,
    GrantTargetFuture, HasAccessFuture, InvitationTargetFuture, ListFuture, NormalizedEmail,
    RateLimitState, RecordDeliveryFuture, RedemptionTargetFuture, RevokeAccessFuture, RevokeFuture,
    SubmitFuture, EMAIL_ATTEMPT_LIMIT, IP_ATTEMPT_LIMIT,
};

mod grant;
mod redemption;

const ACCESS_REQUESTS_COLLECTION: &str = "betaAccessRequests";
const INVITATIONS_COLLECTION: &str = "betaInvitations";
const RATE_LIMITS_COLLECTION: &str = "betaAccessRateLimits";
const SCHEMA_VERSION: u8 = 1;
const MAX_TRANSACTION_ATTEMPTS: u8 = 4;

fn opaque_identifier(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn beta_access_store(database: FirestoreDatabase) -> Arc<dyn BetaAccessStore> {
    Arc::new(FirestoreBetaAccessStore { database })
}

struct FirestoreBetaAccessStore {
    database: FirestoreDatabase,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BetaAccessRequestDocument {
    schema_version: u8,
    email: String,
    status: BetaAccessRequestStatus,
    created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delivery_status: Option<InvitationDeliveryStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delivery_retryable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    invitation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    invitation_status: Option<InvitationStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    access_status: Option<BetaAccessAuthorizationStatus>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RateLimitDocument {
    schema_version: u8,
    attempts: u16,
    window_started_at: DateTime<Utc>,
    purge_at: DateTime<Utc>,
}

impl RateLimitDocument {
    fn into_state(self) -> Result<RateLimitState, BetaAccessStoreError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let state = RateLimitState {
            attempts: self.attempts,
            window_started_at: self.window_started_at,
            purge_at: self.purge_at,
        };
        if !state.has_valid_shape() {
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        Ok(state)
    }

    fn from_state(state: RateLimitState) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            attempts: state.attempts,
            window_started_at: state.window_started_at,
            purge_at: state.purge_at,
        }
    }
}

impl BetaAccessRequestDocument {
    fn pending(submission: &BetaAccessSubmission) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            email: submission.email.as_str().to_string(),
            status: BetaAccessRequestStatus::Pending,
            created_at: submission.now,
            delivery_status: None,
            delivery_retryable: None,
            invitation_id: None,
            invitation_status: None,
            access_status: None,
        }
    }

    fn matches(&self, email: &NormalizedEmail) -> bool {
        self.schema_version == SCHEMA_VERSION
            && NormalizedEmail::parse(&self.email).ok().as_ref() == Some(email)
            && self.valid_invitation_projection()
    }

    fn into_request(self, id: String) -> Result<BetaAccessRequest, BetaAccessStoreError> {
        let email =
            NormalizedEmail::parse(&self.email).map_err(|_| BetaAccessStoreError::InvalidRecord)?;
        if self.schema_version != SCHEMA_VERSION
            || email.as_str() != self.email
            || !opaque_request_id(&id)
            || !self.valid_invitation_projection()
        {
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let access_status = match self.invitation_status {
            Some(InvitationStatus::Redeemed) => Some(
                self.access_status
                    .unwrap_or(BetaAccessAuthorizationStatus::Active),
            ),
            Some(InvitationStatus::Issued | InvitationStatus::Revoked) | None => None,
        };
        Ok(BetaAccessRequest {
            id,
            email,
            status: self.status,
            created_at: self.created_at,
            delivery_status: self.delivery_status,
            delivery_retryable: self.delivery_retryable,
            invitation_status: self.invitation_status,
            access_status,
        })
    }

    fn valid_invitation_projection(&self) -> bool {
        match self.status {
            BetaAccessRequestStatus::Pending => {
                self.invitation_id.is_none()
                    && self.invitation_status.is_none()
                    && self.delivery_status.is_none()
                    && self.delivery_retryable.is_none()
                    && self.access_status.is_none()
            }
            BetaAccessRequestStatus::Granted => {
                self.invitation_id.as_deref().is_some_and(|id| {
                    id.len() == 32
                        && id
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }) && self.invitation_status.is_some()
                    && match self.delivery_status {
                        Some(InvitationDeliveryStatus::Failed) => self.delivery_retryable.is_some(),
                        Some(
                            InvitationDeliveryStatus::Pending | InvitationDeliveryStatus::Sent,
                        ) => self.delivery_retryable.is_none(),
                        None => false,
                    }
                    && match self.invitation_status {
                        Some(InvitationStatus::Redeemed) => true,
                        Some(InvitationStatus::Issued | InvitationStatus::Revoked) | None => {
                            self.access_status.is_none()
                        }
                    }
            }
        }
    }
}

impl BetaAccessStore for FirestoreBetaAccessStore {
    fn begin_retry<'a>(
        &'a self,
        invitation_id: &'a str,
        request_id: &'a str,
        expected_attempt: u32,
    ) -> BeginRetryFuture<'a> {
        Box::pin(async move {
            for transaction_attempt in 1..=MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                match self
                    .begin_retry_in_transaction(
                        invitation_id,
                        request_id,
                        expected_attempt,
                        transaction,
                    )
                    .await
                {
                    Err(BetaAccessStoreError::Conflict)
                        if transaction_attempt < MAX_TRANSACTION_ATTEMPTS =>
                    {
                        continue;
                    }
                    result => return result,
                }
            }
            Err(BetaAccessStoreError::Conflict)
        })
    }

    fn grant_target<'a>(&'a self, request_id: &'a str) -> GrantTargetFuture<'a> {
        Box::pin(async move {
            if !opaque_request_id(request_id) {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            let document = self
                .database
                .get_document::<BetaAccessRequestDocument>(&[
                    ACCESS_REQUESTS_COLLECTION,
                    request_id,
                ])
                .await?
                .ok_or(BetaAccessStoreError::NotFound)?;
            let request = document.into_request(request_id.to_string())?;
            Ok(match request.status {
                BetaAccessRequestStatus::Pending => BetaAccessGrantTarget::Pending(request.email),
                BetaAccessRequestStatus::Granted => BetaAccessGrantTarget::AlreadyGranted,
            })
        })
    }

    fn has_access<'a>(&'a self, player_id: &'a PlayerId) -> HasAccessFuture<'a> {
        Box::pin(async move { self.player_has_access(player_id).await })
    }

    fn invitation_target<'a>(&'a self, request_id: &'a str) -> InvitationTargetFuture<'a> {
        Box::pin(async move {
            if !opaque_request_id(request_id) {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            let transaction = self.database.begin_transaction().await?;
            let result = self
                .invitation_target_in_transaction(request_id, &transaction)
                .await;
            self.database.rollback_transaction(transaction).await?;
            result
        })
    }

    fn commit_grant<'a>(&'a self, invitation: StoredInvitation) -> CommitGrantFuture<'a> {
        Box::pin(async move {
            for attempt in 1..=MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                match self
                    .commit_grant_in_transaction(&invitation, transaction)
                    .await
                {
                    Err(BetaAccessStoreError::Conflict) if attempt < MAX_TRANSACTION_ATTEMPTS => {
                        continue;
                    }
                    result => return result,
                }
            }
            Err(BetaAccessStoreError::Conflict)
        })
    }

    fn commit_redemption<'a>(
        &'a self,
        candidate: BetaAccessRedemptionCandidate,
        player_id: PlayerId,
        now: DateTime<Utc>,
    ) -> CommitRedemptionFuture<'a> {
        Box::pin(async move {
            for attempt in 1..=MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                match self
                    .commit_redemption_in_transaction(&candidate, &player_id, now, transaction)
                    .await
                {
                    Err(BetaAccessStoreError::Conflict) if attempt < MAX_TRANSACTION_ATTEMPTS => {
                        continue;
                    }
                    result => return result,
                }
            }
            Err(BetaAccessStoreError::Conflict)
        })
    }

    fn list(&self) -> ListFuture<'_> {
        Box::pin(async move {
            let request_documents = self
                .database
                .list_documents::<BetaAccessRequestDocument>(&[ACCESS_REQUESTS_COLLECTION])
                .await?;
            let requests = request_documents
                .into_iter()
                .map(|(id, document)| {
                    let invitation_id = document.invitation_id.clone();
                    document
                        .into_request(id)
                        .map(|request| (request, invitation_id))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !requests
                .iter()
                .any(|(request, _)| request.invitation_status == Some(InvitationStatus::Redeemed))
            {
                return requests
                    .into_iter()
                    .map(|(request, _)| BetaAccessAdminRequest::new(request, None))
                    .collect();
            }
            let invitation_documents = self
                .database
                .list_documents::<StoredInvitation>(&[INVITATIONS_COLLECTION])
                .await?;
            let invitations = invitation_documents.into_iter().collect::<BTreeMap<_, _>>();
            requests
                .into_iter()
                .map(|(request, invitation_id)| {
                    let invitation =
                        if request.invitation_status == Some(InvitationStatus::Redeemed) {
                            let invitation_id = invitation_id
                                .as_deref()
                                .ok_or(BetaAccessStoreError::InvalidRecord)?;
                            let invitation = invitations
                                .get(invitation_id)
                                .ok_or(BetaAccessStoreError::InvalidRecord)?;
                            if invitation.id != invitation_id {
                                return Err(BetaAccessStoreError::InvalidRecord);
                            }
                            Some(invitation)
                        } else {
                            None
                        };
                    BetaAccessAdminRequest::new(request, invitation)
                })
                .collect()
        })
    }

    fn record_delivery<'a>(
        &'a self,
        invitation_id: &'a str,
        request_id: &'a str,
        delivery_attempt: u32,
        attempt: InvitationDeliveryAttempt,
    ) -> RecordDeliveryFuture<'a> {
        Box::pin(async move {
            for transaction_attempt in 1..=MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                match self
                    .record_delivery_in_transaction(
                        invitation_id,
                        request_id,
                        delivery_attempt,
                        &attempt,
                        transaction,
                    )
                    .await
                {
                    Err(BetaAccessStoreError::Conflict)
                        if transaction_attempt < MAX_TRANSACTION_ATTEMPTS =>
                    {
                        continue;
                    }
                    result => return result,
                }
            }
            Err(BetaAccessStoreError::Conflict)
        })
    }

    fn redemption_target<'a>(
        &'a self,
        attempt: BetaAccessRedemptionAttempt,
    ) -> RedemptionTargetFuture<'a> {
        Box::pin(async move {
            for transaction_attempt in 1..=MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                match self
                    .redemption_target_in_transaction(&attempt, transaction)
                    .await
                {
                    Err(BetaAccessStoreError::Conflict)
                        if transaction_attempt < MAX_TRANSACTION_ATTEMPTS =>
                    {
                        continue;
                    }
                    result => return result,
                }
            }
            Err(BetaAccessStoreError::Conflict)
        })
    }

    fn revoke<'a>(&'a self, request_id: &'a str) -> RevokeFuture<'a> {
        Box::pin(async move {
            for transaction_attempt in 1..=MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                match self.revoke_in_transaction(request_id, transaction).await {
                    Err(BetaAccessStoreError::Conflict)
                        if transaction_attempt < MAX_TRANSACTION_ATTEMPTS =>
                    {
                        continue;
                    }
                    result => return result,
                }
            }
            Err(BetaAccessStoreError::Conflict)
        })
    }

    fn revoke_access<'a>(&'a self, request_id: &'a str) -> RevokeAccessFuture<'a> {
        Box::pin(async move {
            for transaction_attempt in 1..=MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                match self
                    .revoke_access_in_transaction(request_id, transaction)
                    .await
                {
                    Err(BetaAccessStoreError::Conflict)
                        if transaction_attempt < MAX_TRANSACTION_ATTEMPTS =>
                    {
                        continue;
                    }
                    result => return result,
                }
            }
            Err(BetaAccessStoreError::Conflict)
        })
    }

    fn submit<'a>(&'a self, submission: BetaAccessSubmission) -> SubmitFuture<'a> {
        Box::pin(async move {
            for attempt in 1..=MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                match self.submit_in_transaction(&submission, transaction).await {
                    Err(BetaAccessStoreError::Conflict) if attempt < MAX_TRANSACTION_ATTEMPTS => {
                        continue;
                    }
                    result => return result,
                }
            }
            Err(BetaAccessStoreError::Conflict)
        })
    }
}

impl FirestoreBetaAccessStore {
    async fn invitation_target_in_transaction(
        &self,
        request_id: &str,
        transaction: &FirestoreTransaction,
    ) -> Result<BetaAccessInvitationTarget, BetaAccessStoreError> {
        let document = self
            .database
            .get_document_in_transaction::<BetaAccessRequestDocument>(
                &[ACCESS_REQUESTS_COLLECTION, request_id],
                transaction,
            )
            .await?
            .ok_or(BetaAccessStoreError::NotFound)?;
        let request = document.clone().into_request(request_id.to_string())?;
        if request.status == BetaAccessRequestStatus::Pending {
            return Ok(BetaAccessInvitationTarget::NotIssued);
        }
        let invitation_id = document
            .invitation_id
            .ok_or(BetaAccessStoreError::InvalidRecord)?;
        let invitation = self
            .database
            .get_document_in_transaction::<StoredInvitation>(
                &[INVITATIONS_COLLECTION, invitation_id.as_str()],
                transaction,
            )
            .await?
            .ok_or(BetaAccessStoreError::InvalidRecord)?;
        if !invitation.valid_shape()
            || invitation.id != invitation_id
            || invitation.request_id != request_id
            || invitation.email != request.email
            || Some(invitation.status) != request.invitation_status
            || Some(invitation.delivery_status) != request.delivery_status
            || invitation.delivery_retryable != request.delivery_retryable
        {
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        Ok(BetaAccessInvitationTarget::Invitation(Box::new(invitation)))
    }

    async fn begin_retry_in_transaction(
        &self,
        invitation_id: &str,
        request_id: &str,
        expected_attempt: u32,
        transaction: FirestoreTransaction,
    ) -> Result<BetaAccessRetryCommit, BetaAccessStoreError> {
        if !opaque_request_id(request_id) {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let request_path = [ACCESS_REQUESTS_COLLECTION, request_id];
        let invitation_path = [INVITATIONS_COLLECTION, invitation_id];
        let (Some(mut request), Some(mut invitation)) = (
            self.database
                .get_document_in_transaction::<BetaAccessRequestDocument>(
                    &request_path,
                    &transaction,
                )
                .await?,
            self.database
                .get_document_in_transaction::<StoredInvitation>(&invitation_path, &transaction)
                .await?,
        ) else {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        };
        request.clone().into_request(request_id.to_string())?;
        if !invitation.valid_shape()
            || invitation.id != invitation_id
            || invitation.request_id != request_id
            || request.invitation_id.as_deref() != Some(invitation_id)
            || request.invitation_status != Some(invitation.status)
            || request.delivery_status != Some(invitation.delivery_status)
            || request.delivery_retryable != invitation.delivery_retryable
        {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        match invitation.status {
            InvitationStatus::Revoked => {
                self.database.rollback_transaction(transaction).await?;
                return Ok(BetaAccessRetryCommit::Revoked);
            }
            InvitationStatus::Redeemed => {
                self.database.rollback_transaction(transaction).await?;
                return Ok(BetaAccessRetryCommit::Redeemed);
            }
            InvitationStatus::Issued => {}
        }
        if invitation.delivery_attempt != expected_attempt
            || invitation.delivery_status != InvitationDeliveryStatus::Failed
            || invitation.delivery_retryable != Some(true)
        {
            self.database.rollback_transaction(transaction).await?;
            return Ok(BetaAccessRetryCommit::NotRetryable);
        }
        let delivery_attempt = invitation
            .delivery_attempt
            .checked_add(1)
            .ok_or(BetaAccessStoreError::InvalidRecord)?;
        invitation.delivery_attempt = delivery_attempt;
        invitation.delivery_status = InvitationDeliveryStatus::Pending;
        invitation.delivery_retryable = None;
        invitation.provider_message_id = None;
        request.delivery_status = Some(InvitationDeliveryStatus::Pending);
        request.delivery_retryable = None;
        let writes = vec![
            self.database.update_write(
                &request_path,
                &request,
                &[("createdAt", request.created_at)],
            )?,
            self.database.update_write(
                &invitation_path,
                &invitation,
                &[("createdAt", invitation.created_at)],
            )?,
        ];
        self.database
            .commit_transaction(transaction, writes)
            .await?;
        Ok(BetaAccessRetryCommit::Started { delivery_attempt })
    }

    async fn commit_grant_in_transaction(
        &self,
        invitation: &StoredInvitation,
        transaction: FirestoreTransaction,
    ) -> Result<BetaAccessGrantCommit, BetaAccessStoreError> {
        if !invitation.valid_shape() || !opaque_request_id(&invitation.request_id) {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let request_path = [ACCESS_REQUESTS_COLLECTION, invitation.request_id.as_str()];
        let Some(mut request) = self
            .database
            .get_document_in_transaction::<BetaAccessRequestDocument>(&request_path, &transaction)
            .await?
        else {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::NotFound);
        };
        let request_projection = request
            .clone()
            .into_request(invitation.request_id.clone())?;
        if request_projection.status == BetaAccessRequestStatus::Granted {
            self.database.rollback_transaction(transaction).await?;
            return Ok(BetaAccessGrantCommit::AlreadyGranted);
        }
        if request_projection.email != invitation.email {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        request.status = BetaAccessRequestStatus::Granted;
        request.delivery_status = Some(InvitationDeliveryStatus::Pending);
        request.delivery_retryable = None;
        request.invitation_id = Some(invitation.id.clone());
        request.invitation_status = Some(InvitationStatus::Issued);
        let invitation_path = [INVITATIONS_COLLECTION, invitation.id.as_str()];
        let writes = vec![
            self.database.update_write(
                &request_path,
                &request,
                &[("createdAt", request.created_at)],
            )?,
            self.database.create_write(
                &invitation_path,
                invitation,
                &[("createdAt", invitation.created_at)],
            )?,
            self.invitation_lookup_create_write(invitation)?,
        ];
        self.database
            .commit_transaction(transaction, writes)
            .await?;
        Ok(BetaAccessGrantCommit::Issued)
    }

    async fn record_delivery_in_transaction(
        &self,
        invitation_id: &str,
        request_id: &str,
        delivery_attempt: u32,
        attempt: &InvitationDeliveryAttempt,
        transaction: FirestoreTransaction,
    ) -> Result<(), BetaAccessStoreError> {
        if !opaque_request_id(request_id) {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let request_path = [ACCESS_REQUESTS_COLLECTION, request_id];
        let invitation_path = [INVITATIONS_COLLECTION, invitation_id];
        let (Some(mut request), Some(mut invitation)) = (
            self.database
                .get_document_in_transaction::<BetaAccessRequestDocument>(
                    &request_path,
                    &transaction,
                )
                .await?,
            self.database
                .get_document_in_transaction::<StoredInvitation>(&invitation_path, &transaction)
                .await?,
        ) else {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        };
        request.clone().into_request(request_id.to_string())?;
        if !invitation.valid_shape()
            || invitation.request_id != request_id
            || invitation.delivery_attempt != delivery_attempt
            || request.invitation_id.as_deref() != Some(invitation_id)
            || request.invitation_status != Some(invitation.status)
            || request.delivery_status != Some(InvitationDeliveryStatus::Pending)
            || request.delivery_retryable.is_some()
            || invitation.delivery_status != InvitationDeliveryStatus::Pending
        {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let status = attempt.status();
        let (delivery_retryable, provider_message_id) = attempt.metadata();
        invitation.delivery_status = status;
        invitation.delivery_retryable = delivery_retryable;
        invitation.provider_message_id = provider_message_id.map(str::to_owned);
        request.delivery_status = Some(status);
        request.delivery_retryable = delivery_retryable;
        if !invitation.valid_shape() {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let writes = vec![
            self.database.update_write(
                &request_path,
                &request,
                &[("createdAt", request.created_at)],
            )?,
            self.database.update_write(
                &invitation_path,
                &invitation,
                &[("createdAt", invitation.created_at)],
            )?,
        ];
        self.database
            .commit_transaction(transaction, writes)
            .await
            .map_err(Into::into)
    }

    async fn revoke_in_transaction(
        &self,
        request_id: &str,
        transaction: FirestoreTransaction,
    ) -> Result<BetaAccessRevokeResult, BetaAccessStoreError> {
        if !opaque_request_id(request_id) {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let request_path = [ACCESS_REQUESTS_COLLECTION, request_id];
        let Some(mut request) = self
            .database
            .get_document_in_transaction::<BetaAccessRequestDocument>(&request_path, &transaction)
            .await?
        else {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::NotFound);
        };
        let projection = request.clone().into_request(request_id.to_string())?;
        if projection.status == BetaAccessRequestStatus::Pending {
            self.database.rollback_transaction(transaction).await?;
            return Ok(BetaAccessRevokeResult::NotIssued);
        }
        let invitation_id = request
            .invitation_id
            .clone()
            .ok_or(BetaAccessStoreError::InvalidRecord)?;
        let invitation_path = [INVITATIONS_COLLECTION, invitation_id.as_str()];
        let Some(mut invitation) = self
            .database
            .get_document_in_transaction::<StoredInvitation>(&invitation_path, &transaction)
            .await?
        else {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        };
        if !invitation.valid_shape()
            || invitation.id != invitation_id
            || invitation.request_id != request_id
            || invitation.email != projection.email
            || request.invitation_status != Some(invitation.status)
            || request.delivery_status != Some(invitation.delivery_status)
            || request.delivery_retryable != invitation.delivery_retryable
        {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        match invitation.status {
            InvitationStatus::Revoked => {
                self.database.rollback_transaction(transaction).await?;
                return Ok(BetaAccessRevokeResult::AlreadyRevoked);
            }
            InvitationStatus::Redeemed => {
                self.database.rollback_transaction(transaction).await?;
                return Ok(BetaAccessRevokeResult::AlreadyRedeemed);
            }
            InvitationStatus::Issued => {}
        }
        invitation.status = InvitationStatus::Revoked;
        request.invitation_status = Some(InvitationStatus::Revoked);
        let writes = vec![
            self.database.update_write(
                &request_path,
                &request,
                &[("createdAt", request.created_at)],
            )?,
            self.database.update_write(
                &invitation_path,
                &invitation,
                &[("createdAt", invitation.created_at)],
            )?,
        ];
        self.database
            .commit_transaction(transaction, writes)
            .await?;
        Ok(BetaAccessRevokeResult::Revoked)
    }

    async fn revoke_access_in_transaction(
        &self,
        request_id: &str,
        transaction: FirestoreTransaction,
    ) -> Result<BetaAccessAuthorizationRevokeResult, BetaAccessStoreError> {
        if !opaque_request_id(request_id) {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let request_path = [ACCESS_REQUESTS_COLLECTION, request_id];
        let Some(mut request) = self
            .database
            .get_document_in_transaction::<BetaAccessRequestDocument>(&request_path, &transaction)
            .await?
        else {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::NotFound);
        };
        let projection = request.clone().into_request(request_id.to_string())?;
        if projection.invitation_status != Some(InvitationStatus::Redeemed) {
            self.database.rollback_transaction(transaction).await?;
            return Ok(BetaAccessAuthorizationRevokeResult::NotGranted);
        }
        let invitation_id = request
            .invitation_id
            .clone()
            .ok_or(BetaAccessStoreError::InvalidRecord)?;
        let invitation = self
            .database
            .get_document_in_transaction::<StoredInvitation>(
                &[INVITATIONS_COLLECTION, invitation_id.as_str()],
                &transaction,
            )
            .await?
            .ok_or(BetaAccessStoreError::InvalidRecord)?;
        let player_id = invitation
            .redeemed_by
            .as_ref()
            .ok_or(BetaAccessStoreError::InvalidRecord)?;
        if !invitation.valid_shape()
            || invitation.id != invitation_id
            || invitation.request_id != request_id
            || invitation.email != projection.email
            || invitation.status != InvitationStatus::Redeemed
            || request.invitation_status != Some(invitation.status)
            || request.delivery_status != Some(invitation.delivery_status)
            || request.delivery_retryable != invitation.delivery_retryable
        {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let grant_path = grant::BetaAccessGrantPath::new(player_id);
        let grant_path = grant_path.segments();
        let grant = self
            .database
            .get_document_in_transaction::<grant::BetaAccessGrantDocument>(
                &grant_path,
                &transaction,
            )
            .await?;
        if projection.access_status == Some(BetaAccessAuthorizationStatus::Revoked) {
            self.database.rollback_transaction(transaction).await?;
            return match grant {
                None => Ok(BetaAccessAuthorizationRevokeResult::AlreadyRevoked),
                Some(_) => Err(BetaAccessStoreError::InvalidRecord),
            };
        }
        request.access_status = Some(BetaAccessAuthorizationStatus::Revoked);
        let request_write = self.database.update_write(
            &request_path,
            &request,
            &[("createdAt", request.created_at)],
        )?;
        let Some(grant) = grant else {
            self.database
                .commit_transaction(transaction, vec![request_write])
                .await?;
            return Ok(BetaAccessAuthorizationRevokeResult::AlreadyRevoked);
        };
        if !grant.grants_invitation(&invitation_id) {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        self.database
            .commit_transaction(
                transaction,
                vec![request_write, self.database.delete_write(&grant_path)?],
            )
            .await?;
        Ok(BetaAccessAuthorizationRevokeResult::Revoked)
    }

    async fn submit_in_transaction(
        &self,
        submission: &BetaAccessSubmission,
        transaction: FirestoreTransaction,
    ) -> Result<BetaAccessStoreOutcome, BetaAccessStoreError> {
        let request_path = [
            ACCESS_REQUESTS_COLLECTION,
            submission.email_rate_key.as_str(),
        ];
        let email_limit_id = format!("email-{}", submission.email_rate_key);
        let ip_limit_id = format!("ip-{}", submission.ip_rate_key);
        let email_limit_path = [RATE_LIMITS_COLLECTION, email_limit_id.as_str()];
        let ip_limit_path = [RATE_LIMITS_COLLECTION, ip_limit_id.as_str()];

        let request = self
            .database
            .get_document_in_transaction::<BetaAccessRequestDocument>(&request_path, &transaction)
            .await?;
        if request
            .as_ref()
            .is_some_and(|request| !request.matches(&submission.email))
        {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let email_limit = self
            .database
            .get_document_in_transaction::<RateLimitDocument>(&email_limit_path, &transaction)
            .await?;
        let ip_limit = self
            .database
            .get_document_in_transaction::<RateLimitDocument>(&ip_limit_path, &transaction)
            .await?;
        let (email_limit_state, email_allowed) = RateLimitState::consume(
            email_limit.map(RateLimitDocument::into_state).transpose()?,
            EMAIL_ATTEMPT_LIMIT,
            submission.now,
        );
        let (ip_limit_state, ip_allowed) = RateLimitState::consume(
            ip_limit.map(RateLimitDocument::into_state).transpose()?,
            IP_ATTEMPT_LIMIT,
            submission.now,
        );
        if !email_allowed || !ip_allowed {
            self.database.rollback_transaction(transaction).await?;
            return Ok(BetaAccessStoreOutcome::RateLimited);
        }

        let mut writes = vec![
            self.rate_limit_write(&email_limit_path, email_limit.is_some(), email_limit_state)?,
            self.rate_limit_write(&ip_limit_path, ip_limit.is_some(), ip_limit_state)?,
        ];
        let outcome = if request.is_some() {
            BetaAccessStoreOutcome::Duplicate
        } else {
            let document = BetaAccessRequestDocument::pending(submission);
            writes.push(self.database.create_write(
                &request_path,
                &document,
                &[("createdAt", document.created_at)],
            )?);
            BetaAccessStoreOutcome::Recorded
        };
        self.database
            .commit_transaction(transaction, writes)
            .await?;
        Ok(outcome)
    }

    fn rate_limit_write(
        &self,
        path: &[&str],
        exists: bool,
        state: RateLimitState,
    ) -> Result<FirestoreWrite, BetaAccessStoreError> {
        let document = RateLimitDocument::from_state(state);
        let timestamps = [
            ("windowStartedAt", document.window_started_at),
            ("purgeAt", document.purge_at),
        ];
        if exists {
            self.database
                .update_write(path, &document, &timestamps)
                .map_err(Into::into)
        } else {
            self.database
                .create_write(path, &document, &timestamps)
                .map_err(Into::into)
        }
    }
}

impl From<FirestoreError> for BetaAccessStoreError {
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
