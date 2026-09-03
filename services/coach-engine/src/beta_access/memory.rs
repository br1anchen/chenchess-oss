use std::{collections::BTreeMap, sync::Mutex};

use super::*;

#[derive(Default)]
struct InMemoryBetaAccessState {
    access_grants: BTreeMap<PlayerId, String>,
    invitations: BTreeMap<String, StoredInvitation>,
    invitation_lookups: BTreeMap<String, String>,
    requests: BTreeMap<String, BetaAccessRequest>,
    email_limits: BTreeMap<String, RateLimitState>,
    ip_limits: BTreeMap<String, RateLimitState>,
    player_redemption_limits: BTreeMap<String, RateLimitState>,
}

#[derive(Default)]
pub(crate) struct InMemoryBetaAccessStore {
    state: Mutex<InMemoryBetaAccessState>,
}

impl InMemoryBetaAccessStore {
    pub(crate) fn request_id(&self) -> String {
        self.state
            .lock()
            .expect("in-memory beta access state is not poisoned")
            .requests
            .values()
            .next()
            .expect("test request must exist")
            .id
            .clone()
    }

    pub(crate) fn request_count(&self) -> usize {
        self.state
            .lock()
            .expect("in-memory beta access state is not poisoned")
            .requests
            .len()
    }

    pub(crate) fn serialized_invitations(&self) -> String {
        serde_json::to_string(
            &self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned")
                .invitations,
        )
        .expect("test invitations must serialize")
    }

    pub(crate) fn invitation_retryable(&self) -> Option<bool> {
        self.state
            .lock()
            .expect("in-memory beta access state is not poisoned")
            .invitations
            .values()
            .next()
            .and_then(|invitation| invitation.delivery_retryable)
    }

    pub(crate) fn invitation_count(&self) -> usize {
        self.state
            .lock()
            .expect("in-memory beta access state is not poisoned")
            .invitations
            .len()
    }

    pub(crate) fn access_grant_count(&self) -> usize {
        self.state
            .lock()
            .expect("in-memory beta access state is not poisoned")
            .access_grants
            .len()
    }

    pub(crate) fn has_access(&self, player_id: &str) -> bool {
        let player_id = PlayerId::try_from(player_id.to_string()).expect("test Player ID is valid");
        self.state
            .lock()
            .expect("in-memory beta access state is not poisoned")
            .access_grants
            .contains_key(&player_id)
    }

    pub(crate) fn remove_access_grant(&self, player_id: &str) {
        let player_id = PlayerId::try_from(player_id.to_string()).expect("test Player ID is valid");
        self.state
            .lock()
            .expect("in-memory beta access state is not poisoned")
            .access_grants
            .remove(&player_id);
    }

    pub(crate) fn mark_invitation_redeemed(&self) {
        let mut state = self
            .state
            .lock()
            .expect("in-memory beta access state is not poisoned");
        let (request_id, invitation_id, player_id) = {
            let invitation = state
                .invitations
                .values_mut()
                .next()
                .expect("test invitation must exist");
            assert_eq!(invitation.status, InvitationStatus::Issued);
            invitation.status = InvitationStatus::Redeemed;
            invitation.redeemed_at = Some(invitation.created_at);
            let player_id = PlayerId::try_from("test-redeemed-player".to_string())
                .expect("test Player ID is valid");
            invitation.redeemed_by = Some(player_id.clone());
            (
                invitation.request_id.clone(),
                invitation.id.clone(),
                player_id,
            )
        };
        let request = state
            .requests
            .get_mut(&request_id)
            .expect("test request must exist");
        request.invitation_status = Some(InvitationStatus::Redeemed);
        request.access_status = Some(BetaAccessAuthorizationStatus::Active);
        state.access_grants.insert(player_id, invitation_id);
    }

    pub(crate) fn grant(&self, email: &str) {
        let email = NormalizedEmail::parse(email).expect("test email must be valid");
        let mut state = self
            .state
            .lock()
            .expect("in-memory beta access state is not poisoned");
        let (request_id, created_at) = {
            let request = state
                .requests
                .values_mut()
                .find(|request| request.email == email)
                .expect("test request must exist");
            request.status = BetaAccessRequestStatus::Granted;
            request.delivery_status = Some(InvitationDeliveryStatus::Sent);
            request.delivery_retryable = None;
            request.invitation_status = Some(InvitationStatus::Issued);
            (request.id.clone(), request.created_at)
        };
        let invitation_id = request_id[..32].to_string();
        let lookup_id = "1".repeat(64);
        state.invitations.insert(
            invitation_id.clone(),
            StoredInvitation {
                authenticator: "0".repeat(64),
                authenticator_version: 1,
                ciphertext: "0".repeat(96),
                created_at,
                delivery_attempt: 1,
                delivery_retryable: None,
                delivery_status: InvitationDeliveryStatus::Sent,
                email,
                encryption_nonce: "0".repeat(24),
                encryption_version: 1,
                id: invitation_id.clone(),
                lookup_id: lookup_id.clone(),
                provider_message_id: Some("test-message".to_string()),
                record_version: 1,
                redeemed_at: None,
                redeemed_by: None,
                request_id,
                status: InvitationStatus::Issued,
            },
        );
        state.invitation_lookups.insert(lookup_id, invitation_id);
    }
}

impl BetaAccessStore for InMemoryBetaAccessStore {
    fn begin_retry<'a>(
        &'a self,
        invitation_id: &'a str,
        request_id: &'a str,
        expected_attempt: u32,
    ) -> BeginRetryFuture<'a> {
        Box::pin(async move {
            let mut state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            let invitation = state
                .invitations
                .get_mut(invitation_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            if invitation.request_id != request_id {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            match invitation.status {
                InvitationStatus::Revoked => return Ok(BetaAccessRetryCommit::Revoked),
                InvitationStatus::Redeemed => return Ok(BetaAccessRetryCommit::Redeemed),
                InvitationStatus::Issued => {}
            }
            if invitation.delivery_attempt != expected_attempt
                || invitation.delivery_status != InvitationDeliveryStatus::Failed
                || invitation.delivery_retryable != Some(true)
            {
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
            let request = state
                .requests
                .get_mut(request_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            request.delivery_status = Some(InvitationDeliveryStatus::Pending);
            request.delivery_retryable = None;
            Ok(BetaAccessRetryCommit::Started { delivery_attempt })
        })
    }

    fn grant_target<'a>(&'a self, request_id: &'a str) -> GrantTargetFuture<'a> {
        Box::pin(async move {
            let state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            let request = state
                .requests
                .get(request_id)
                .ok_or(BetaAccessStoreError::NotFound)?;
            Ok(match request.status {
                BetaAccessRequestStatus::Pending => {
                    BetaAccessGrantTarget::Pending(request.email.clone())
                }
                BetaAccessRequestStatus::Granted => BetaAccessGrantTarget::AlreadyGranted,
            })
        })
    }

    fn has_access<'a>(&'a self, player_id: &'a PlayerId) -> HasAccessFuture<'a> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned")
                .access_grants
                .contains_key(player_id))
        })
    }

    fn commit_grant<'a>(&'a self, invitation: StoredInvitation) -> CommitGrantFuture<'a> {
        Box::pin(async move {
            let mut state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            if !invitation.valid_shape()
                || state.invitations.contains_key(&invitation.id)
                || state.invitation_lookups.contains_key(&invitation.lookup_id)
            {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            let request = state
                .requests
                .get_mut(&invitation.request_id)
                .ok_or(BetaAccessStoreError::NotFound)?;
            if request.status == BetaAccessRequestStatus::Granted {
                return Ok(BetaAccessGrantCommit::AlreadyGranted);
            }
            if request.email != invitation.email {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            request.status = BetaAccessRequestStatus::Granted;
            request.delivery_status = Some(InvitationDeliveryStatus::Pending);
            request.delivery_retryable = None;
            request.invitation_status = Some(InvitationStatus::Issued);
            state
                .invitation_lookups
                .insert(invitation.lookup_id.clone(), invitation.id.clone());
            state.invitations.insert(invitation.id.clone(), invitation);
            Ok(BetaAccessGrantCommit::Issued)
        })
    }

    fn commit_redemption<'a>(
        &'a self,
        candidate: BetaAccessRedemptionCandidate,
        player_id: PlayerId,
        now: DateTime<Utc>,
    ) -> CommitRedemptionFuture<'a> {
        Box::pin(async move {
            let mut state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            let request = state
                .requests
                .get(&candidate.request_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            let invitation = state
                .invitations
                .get(&candidate.invitation_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            if !invitation.valid_shape()
                || invitation.authenticator != candidate.authenticator
                || invitation.email != candidate.email
                || invitation.lookup_id != candidate.lookup_id
                || invitation.request_id != candidate.request_id
                || request.email != candidate.email
                || request.status != BetaAccessRequestStatus::Granted
                || request.invitation_status != Some(invitation.status)
            {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            match invitation.status {
                InvitationStatus::Revoked => return Ok(BetaAccessRedemptionCommit::Revoked),
                InvitationStatus::Redeemed => {
                    return Ok(BetaAccessRedemptionCommit::AlreadyHandled)
                }
                InvitationStatus::Issued => {}
            }
            if state.access_grants.contains_key(&player_id) {
                return Ok(BetaAccessRedemptionCommit::AlreadyHandled);
            }
            {
                let invitation = state
                    .invitations
                    .get_mut(&candidate.invitation_id)
                    .ok_or(BetaAccessStoreError::InvalidRecord)?;
                invitation.status = InvitationStatus::Redeemed;
                invitation.redeemed_at = Some(now);
                invitation.redeemed_by = Some(player_id.clone());
            }
            let request = state
                .requests
                .get_mut(&candidate.request_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            request.invitation_status = Some(InvitationStatus::Redeemed);
            request.access_status = Some(BetaAccessAuthorizationStatus::Active);
            state
                .access_grants
                .insert(player_id, candidate.invitation_id);
            Ok(BetaAccessRedemptionCommit::Granted)
        })
    }

    fn invitation_target<'a>(&'a self, request_id: &'a str) -> InvitationTargetFuture<'a> {
        Box::pin(async move {
            let state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            let request = state
                .requests
                .get(request_id)
                .ok_or(BetaAccessStoreError::NotFound)?;
            let Some(invitation_status) = request.invitation_status else {
                return Ok(BetaAccessInvitationTarget::NotIssued);
            };
            let invitation = state
                .invitations
                .values()
                .find(|invitation| invitation.request_id == request_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            if invitation.status != invitation_status || !invitation.valid_shape() {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            Ok(BetaAccessInvitationTarget::Invitation(Box::new(
                invitation.clone(),
            )))
        })
    }

    fn list(&self) -> ListFuture<'_> {
        Box::pin(async move {
            let state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            state
                .requests
                .values()
                .cloned()
                .map(|request| {
                    let invitation = state
                        .invitations
                        .values()
                        .find(|invitation| invitation.request_id == request.id);
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
            let status = attempt.status();
            let (delivery_retryable, provider_message_id) = attempt.metadata();
            let provider_message_id = provider_message_id.map(str::to_owned);
            let mut state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            let invitation = state
                .invitations
                .get_mut(invitation_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            if invitation.request_id != request_id
                || invitation.delivery_attempt != delivery_attempt
                || invitation.delivery_status != InvitationDeliveryStatus::Pending
            {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            invitation.delivery_status = status;
            invitation.delivery_retryable = delivery_retryable;
            invitation.provider_message_id = provider_message_id;
            if !invitation.valid_shape() {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            let request = state
                .requests
                .get_mut(request_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            request.delivery_status = Some(status);
            request.delivery_retryable = delivery_retryable;
            Ok(())
        })
    }

    fn redemption_target<'a>(
        &'a self,
        attempt: BetaAccessRedemptionAttempt,
    ) -> RedemptionTargetFuture<'a> {
        Box::pin(async move {
            let mut state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            let (player_limit, player_allowed) = RateLimitState::consume(
                state
                    .player_redemption_limits
                    .get(&attempt.player_rate_key)
                    .copied(),
                REDEMPTION_PLAYER_ATTEMPT_LIMIT,
                attempt.now,
            );
            let (ip_limit, ip_allowed) = RateLimitState::consume(
                state.ip_limits.get(&attempt.ip_rate_key).copied(),
                REDEMPTION_IP_ATTEMPT_LIMIT,
                attempt.now,
            );
            if !player_allowed || !ip_allowed {
                return Ok(BetaAccessRedemptionTarget::RateLimited);
            }
            state
                .player_redemption_limits
                .insert(attempt.player_rate_key, player_limit);
            state.ip_limits.insert(attempt.ip_rate_key, ip_limit);
            let Some(lookup_id) = attempt.lookup_id else {
                return Ok(BetaAccessRedemptionTarget::Invalid);
            };
            let Some(invitation_id) = state.invitation_lookups.get(&lookup_id) else {
                return Ok(BetaAccessRedemptionTarget::Invalid);
            };
            let invitation = state
                .invitations
                .get(invitation_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            if !invitation.valid_shape() || invitation.lookup_id != lookup_id {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            let request = state
                .requests
                .get(&invitation.request_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            if request.email != invitation.email
                || request.status != BetaAccessRequestStatus::Granted
                || request.invitation_status != Some(invitation.status)
            {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            Ok(match invitation.status {
                InvitationStatus::Issued => {
                    BetaAccessRedemptionTarget::Candidate(Box::new(invitation.clone()))
                }
                InvitationStatus::Revoked => BetaAccessRedemptionTarget::Revoked,
                InvitationStatus::Redeemed => BetaAccessRedemptionTarget::AlreadyHandled,
            })
        })
    }

    fn revoke<'a>(&'a self, request_id: &'a str) -> RevokeFuture<'a> {
        Box::pin(async move {
            let mut state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            let invitation_id = match state.requests.get(request_id) {
                Some(request) if request.status == BetaAccessRequestStatus::Pending => {
                    return Ok(BetaAccessRevokeResult::NotIssued);
                }
                Some(_) => state
                    .invitations
                    .values()
                    .find(|invitation| invitation.request_id == request_id)
                    .map(|invitation| invitation.id.clone())
                    .ok_or(BetaAccessStoreError::InvalidRecord)?,
                None => return Err(BetaAccessStoreError::NotFound),
            };
            let invitation = state
                .invitations
                .get_mut(&invitation_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            let outcome = match invitation.status {
                InvitationStatus::Issued => {
                    invitation.status = InvitationStatus::Revoked;
                    BetaAccessRevokeResult::Revoked
                }
                InvitationStatus::Revoked => BetaAccessRevokeResult::AlreadyRevoked,
                InvitationStatus::Redeemed => BetaAccessRevokeResult::AlreadyRedeemed,
            };
            if outcome == BetaAccessRevokeResult::Revoked {
                state
                    .requests
                    .get_mut(request_id)
                    .ok_or(BetaAccessStoreError::InvalidRecord)?
                    .invitation_status = Some(InvitationStatus::Revoked);
            }
            Ok(outcome)
        })
    }

    fn revoke_access<'a>(&'a self, request_id: &'a str) -> RevokeAccessFuture<'a> {
        Box::pin(async move {
            let mut state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            let request = state
                .requests
                .get(request_id)
                .ok_or(BetaAccessStoreError::NotFound)?;
            if request.invitation_status != Some(InvitationStatus::Redeemed) {
                return Ok(BetaAccessAuthorizationRevokeResult::NotGranted);
            }
            let invitation = state
                .invitations
                .values()
                .find(|invitation| invitation.request_id == request_id)
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            if !invitation.valid_shape()
                || invitation.status != InvitationStatus::Redeemed
                || invitation.email != request.email
                || request.delivery_status != Some(invitation.delivery_status)
                || request.delivery_retryable != invitation.delivery_retryable
            {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            let player_id = invitation
                .redeemed_by
                .clone()
                .ok_or(BetaAccessStoreError::InvalidRecord)?;
            let invitation_id = invitation.id.clone();
            if request.access_status == Some(BetaAccessAuthorizationStatus::Revoked) {
                if state.access_grants.contains_key(&player_id) {
                    return Err(BetaAccessStoreError::InvalidRecord);
                }
                return Ok(BetaAccessAuthorizationRevokeResult::AlreadyRevoked);
            }
            match state.access_grants.get(&player_id) {
                Some(grant_invitation_id) if grant_invitation_id == &invitation_id => {
                    state.access_grants.remove(&player_id);
                    state
                        .requests
                        .get_mut(request_id)
                        .ok_or(BetaAccessStoreError::InvalidRecord)?
                        .access_status = Some(BetaAccessAuthorizationStatus::Revoked);
                    Ok(BetaAccessAuthorizationRevokeResult::Revoked)
                }
                Some(_) => Err(BetaAccessStoreError::InvalidRecord),
                None => {
                    state
                        .requests
                        .get_mut(request_id)
                        .ok_or(BetaAccessStoreError::InvalidRecord)?
                        .access_status = Some(BetaAccessAuthorizationStatus::Revoked);
                    Ok(BetaAccessAuthorizationRevokeResult::AlreadyRevoked)
                }
            }
        })
    }

    fn submit<'a>(&'a self, submission: BetaAccessSubmission) -> SubmitFuture<'a> {
        Box::pin(async move {
            let mut state = self
                .state
                .lock()
                .expect("in-memory beta access state is not poisoned");
            let (email_limit, email_allowed) = RateLimitState::consume(
                state.email_limits.get(&submission.email_rate_key).copied(),
                EMAIL_ATTEMPT_LIMIT,
                submission.now,
            );
            let (ip_limit, ip_allowed) = RateLimitState::consume(
                state.ip_limits.get(&submission.ip_rate_key).copied(),
                IP_ATTEMPT_LIMIT,
                submission.now,
            );
            state
                .email_limits
                .insert(submission.email_rate_key.clone(), email_limit);
            state.ip_limits.insert(submission.ip_rate_key, ip_limit);
            if !email_allowed || !ip_allowed {
                return Ok(BetaAccessStoreOutcome::RateLimited);
            }
            if state.requests.contains_key(&submission.email_rate_key) {
                return Ok(BetaAccessStoreOutcome::Duplicate);
            }
            state.requests.insert(
                submission.email_rate_key.clone(),
                BetaAccessRequest {
                    id: submission.email_rate_key,
                    email: submission.email,
                    status: BetaAccessRequestStatus::Pending,
                    created_at: submission.now,
                    delivery_status: None,
                    delivery_retryable: None,
                    invitation_status: None,
                    access_status: None,
                },
            );
            Ok(BetaAccessStoreOutcome::Recorded)
        })
    }
}

pub(super) struct UnavailableBetaAccessStore;

impl BetaAccessStore for UnavailableBetaAccessStore {
    fn begin_retry<'a>(
        &'a self,
        _invitation_id: &'a str,
        _request_id: &'a str,
        _expected_attempt: u32,
    ) -> BeginRetryFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn grant_target<'a>(&'a self, _request_id: &'a str) -> GrantTargetFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn has_access<'a>(&'a self, _player_id: &'a PlayerId) -> HasAccessFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn invitation_target<'a>(&'a self, _request_id: &'a str) -> InvitationTargetFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn commit_grant<'a>(&'a self, _invitation: StoredInvitation) -> CommitGrantFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn commit_redemption<'a>(
        &'a self,
        _candidate: BetaAccessRedemptionCandidate,
        _player_id: PlayerId,
        _now: DateTime<Utc>,
    ) -> CommitRedemptionFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn list(&self) -> ListFuture<'_> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn record_delivery<'a>(
        &'a self,
        _invitation_id: &'a str,
        _request_id: &'a str,
        _delivery_attempt: u32,
        _attempt: InvitationDeliveryAttempt,
    ) -> RecordDeliveryFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn redemption_target<'a>(
        &'a self,
        _attempt: BetaAccessRedemptionAttempt,
    ) -> RedemptionTargetFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn revoke<'a>(&'a self, _request_id: &'a str) -> RevokeFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn revoke_access<'a>(&'a self, _request_id: &'a str) -> RevokeAccessFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }

    fn submit<'a>(&'a self, _submission: BetaAccessSubmission) -> SubmitFuture<'a> {
        Box::pin(async { Err(BetaAccessStoreError::Unavailable) })
    }
}
