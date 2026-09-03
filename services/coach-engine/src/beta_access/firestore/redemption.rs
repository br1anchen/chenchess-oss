use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::review_session_contract::PlayerId;

use super::grant::{BetaAccessGrantDocument, BetaAccessGrantPath};
use super::*;
use crate::beta_access::{
    BetaAccessRedemptionCommit, BetaAccessRedemptionTarget, REDEMPTION_IP_ATTEMPT_LIMIT,
    REDEMPTION_PLAYER_ATTEMPT_LIMIT,
};

pub(super) const INVITATION_LOOKUPS_COLLECTION: &str = "betaInvitationLookups";

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InvitationLookupDocument {
    schema_version: u8,
    invitation_id: String,
    created_at: DateTime<Utc>,
}

impl InvitationLookupDocument {
    fn new(invitation: &StoredInvitation) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            invitation_id: invitation.id.clone(),
            created_at: invitation.created_at,
        }
    }

    fn valid_for(&self, lookup_id: &str) -> bool {
        self.schema_version == SCHEMA_VERSION
            && opaque_identifier(lookup_id, 64)
            && opaque_identifier(&self.invitation_id, 32)
    }
}

impl FirestoreBetaAccessStore {
    pub(super) fn invitation_lookup_create_write(
        &self,
        invitation: &StoredInvitation,
    ) -> Result<FirestoreWrite, BetaAccessStoreError> {
        let lookup = InvitationLookupDocument::new(invitation);
        self.database
            .create_write(
                &[INVITATION_LOOKUPS_COLLECTION, invitation.lookup_id.as_str()],
                &lookup,
                &[("createdAt", lookup.created_at)],
            )
            .map_err(Into::into)
    }

    pub(super) async fn redemption_target_in_transaction(
        &self,
        attempt: &BetaAccessRedemptionAttempt,
        transaction: FirestoreTransaction,
    ) -> Result<BetaAccessRedemptionTarget, BetaAccessStoreError> {
        let player_limit_id = format!("redemption-player-{}", attempt.player_rate_key);
        let ip_limit_id = format!("redemption-ip-{}", attempt.ip_rate_key);
        let player_limit_path = [RATE_LIMITS_COLLECTION, player_limit_id.as_str()];
        let ip_limit_path = [RATE_LIMITS_COLLECTION, ip_limit_id.as_str()];
        let player_limit = self
            .database
            .get_document_in_transaction::<RateLimitDocument>(&player_limit_path, &transaction)
            .await?;
        let ip_limit = self
            .database
            .get_document_in_transaction::<RateLimitDocument>(&ip_limit_path, &transaction)
            .await?;
        let (player_state, player_allowed) = RateLimitState::consume(
            player_limit
                .map(RateLimitDocument::into_state)
                .transpose()?,
            REDEMPTION_PLAYER_ATTEMPT_LIMIT,
            attempt.now,
        );
        let (ip_state, ip_allowed) = RateLimitState::consume(
            ip_limit.map(RateLimitDocument::into_state).transpose()?,
            REDEMPTION_IP_ATTEMPT_LIMIT,
            attempt.now,
        );
        if !player_allowed || !ip_allowed {
            self.database.rollback_transaction(transaction).await?;
            return Ok(BetaAccessRedemptionTarget::RateLimited);
        }

        let writes = vec![
            self.rate_limit_write(&player_limit_path, player_limit.is_some(), player_state)?,
            self.rate_limit_write(&ip_limit_path, ip_limit.is_some(), ip_state)?,
        ];
        let outcome = match attempt.lookup_id.as_deref() {
            Some(lookup_id) => {
                self.lookup_redemption_target(lookup_id, &transaction)
                    .await?
            }
            None => BetaAccessRedemptionTarget::Invalid,
        };
        self.database
            .commit_transaction(transaction, writes)
            .await?;
        Ok(outcome)
    }

    async fn lookup_redemption_target(
        &self,
        lookup_id: &str,
        transaction: &FirestoreTransaction,
    ) -> Result<BetaAccessRedemptionTarget, BetaAccessStoreError> {
        if !opaque_identifier(lookup_id, 64) {
            return Ok(BetaAccessRedemptionTarget::Invalid);
        }
        let Some(lookup) = self
            .database
            .get_document_in_transaction::<InvitationLookupDocument>(
                &[INVITATION_LOOKUPS_COLLECTION, lookup_id],
                transaction,
            )
            .await?
        else {
            return Ok(BetaAccessRedemptionTarget::Invalid);
        };
        if !lookup.valid_for(lookup_id) {
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let invitation = self
            .database
            .get_document_in_transaction::<StoredInvitation>(
                &[INVITATIONS_COLLECTION, lookup.invitation_id.as_str()],
                transaction,
            )
            .await?
            .ok_or(BetaAccessStoreError::InvalidRecord)?;
        let request = self
            .database
            .get_document_in_transaction::<BetaAccessRequestDocument>(
                &[ACCESS_REQUESTS_COLLECTION, invitation.request_id.as_str()],
                transaction,
            )
            .await?
            .ok_or(BetaAccessStoreError::InvalidRecord)?;
        let projection = request
            .clone()
            .into_request(invitation.request_id.clone())?;
        if !invitation.valid_shape()
            || invitation.id != lookup.invitation_id
            || invitation.lookup_id != lookup_id
            || invitation.created_at != lookup.created_at
            || projection.email != invitation.email
            || projection.status != BetaAccessRequestStatus::Granted
            || request.invitation_id.as_deref() != Some(invitation.id.as_str())
            || request.invitation_status != Some(invitation.status)
            || request.delivery_status != Some(invitation.delivery_status)
            || request.delivery_retryable != invitation.delivery_retryable
        {
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        Ok(match invitation.status {
            InvitationStatus::Issued => BetaAccessRedemptionTarget::Candidate(Box::new(invitation)),
            InvitationStatus::Revoked => BetaAccessRedemptionTarget::Revoked,
            InvitationStatus::Redeemed => BetaAccessRedemptionTarget::AlreadyHandled,
        })
    }

    pub(super) async fn commit_redemption_in_transaction(
        &self,
        candidate: &BetaAccessRedemptionCandidate,
        player_id: &PlayerId,
        now: DateTime<Utc>,
        transaction: FirestoreTransaction,
    ) -> Result<BetaAccessRedemptionCommit, BetaAccessStoreError> {
        let lookup_path = [INVITATION_LOOKUPS_COLLECTION, candidate.lookup_id.as_str()];
        let invitation_path = [INVITATIONS_COLLECTION, candidate.invitation_id.as_str()];
        let request_path = [ACCESS_REQUESTS_COLLECTION, candidate.request_id.as_str()];
        let grant_path = BetaAccessGrantPath::new(player_id);
        let grant_path = grant_path.segments();
        let (Some(lookup), Some(mut invitation), Some(mut request)) = (
            self.database
                .get_document_in_transaction::<InvitationLookupDocument>(&lookup_path, &transaction)
                .await?,
            self.database
                .get_document_in_transaction::<StoredInvitation>(&invitation_path, &transaction)
                .await?,
            self.database
                .get_document_in_transaction::<BetaAccessRequestDocument>(
                    &request_path,
                    &transaction,
                )
                .await?,
        ) else {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        };
        let grant = self
            .database
            .get_document_in_transaction::<BetaAccessGrantDocument>(&grant_path, &transaction)
            .await?;
        if grant.as_ref().is_some_and(|grant| !grant.valid_shape()) {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let request_projection = request.clone().into_request(candidate.request_id.clone())?;
        if !lookup.valid_for(&candidate.lookup_id)
            || lookup.invitation_id != candidate.invitation_id
            || !invitation.valid_shape()
            || invitation.id != candidate.invitation_id
            || invitation.authenticator != candidate.authenticator
            || invitation.email != candidate.email
            || invitation.lookup_id != candidate.lookup_id
            || invitation.request_id != candidate.request_id
            || invitation.created_at != lookup.created_at
            || request_projection.email != candidate.email
            || request_projection.status != BetaAccessRequestStatus::Granted
            || request.invitation_id.as_deref() != Some(candidate.invitation_id.as_str())
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
                return Ok(BetaAccessRedemptionCommit::Revoked);
            }
            InvitationStatus::Redeemed => {
                self.database.rollback_transaction(transaction).await?;
                return Ok(BetaAccessRedemptionCommit::AlreadyHandled);
            }
            InvitationStatus::Issued => {}
        }
        if grant.is_some() {
            self.database.rollback_transaction(transaction).await?;
            return Ok(BetaAccessRedemptionCommit::AlreadyHandled);
        }
        invitation.status = InvitationStatus::Redeemed;
        invitation.redeemed_at = Some(now);
        invitation.redeemed_by = Some(player_id.clone());
        request.invitation_status = Some(InvitationStatus::Redeemed);
        request.access_status = Some(BetaAccessAuthorizationStatus::Active);
        if !invitation.valid_shape() {
            self.database.rollback_transaction(transaction).await?;
            return Err(BetaAccessStoreError::InvalidRecord);
        }
        let grant = BetaAccessGrantDocument::new(candidate.invitation_id.clone(), now);
        let writes = vec![
            self.database.update_write(
                &invitation_path,
                &invitation,
                &[("createdAt", invitation.created_at), ("redeemedAt", now)],
            )?,
            self.database.update_write(
                &request_path,
                &request,
                &[("createdAt", request.created_at)],
            )?,
            self.database
                .create_write(&grant_path, &grant, &[("grantedAt", grant.granted_at)])?,
        ];
        self.database
            .commit_transaction(transaction, writes)
            .await?;
        Ok(BetaAccessRedemptionCommit::Granted)
    }
}
