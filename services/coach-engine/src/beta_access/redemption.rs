use super::{invitation::InvitationCode, *};

impl BetaAccessRuntime {
    pub(crate) async fn redeem(
        &self,
        identity: BetaAccessRedemptionIdentity,
        code: &str,
        source_ip: IpAddr,
        now: DateTime<Utc>,
    ) -> Result<BetaAccessRedemptionResult, BetaAccessInvitationError> {
        let enabled = self
            .enabled()
            .ok_or(BetaAccessInvitationError::Unavailable)?;
        let issuer = enabled
            .invitation
            .as_ref()
            .ok_or(BetaAccessInvitationError::Unavailable)?;
        let code = InvitationCode::parse(code);
        let lookup_id = match (&identity, &code) {
            (BetaAccessRedemptionIdentity::Verified { .. }, Some(code)) => {
                Some(issuer.lookup_id(code))
            }
            _ => None,
        };
        let player_rate_key = enabled.hasher.identifier(
            b"redemption-player",
            identity.player_id().as_str().as_bytes(),
        );
        let source_ip = source_ip.to_string();
        let ip_rate_key = enabled
            .hasher
            .identifier(b"redemption-source-ip", source_ip.as_bytes());
        let target = enabled
            .store
            .redemption_target(BetaAccessRedemptionAttempt {
                ip_rate_key,
                lookup_id,
                now,
                player_rate_key,
            })
            .await?;
        if matches!(&target, BetaAccessRedemptionTarget::RateLimited) {
            return Ok(BetaAccessRedemptionResult::RateLimited);
        }
        let BetaAccessRedemptionIdentity::Verified { email, player_id } = identity else {
            return Ok(BetaAccessRedemptionResult::VerificationRequired);
        };
        let stored = match target {
            BetaAccessRedemptionTarget::Candidate(stored) => stored,
            BetaAccessRedemptionTarget::Invalid => return Ok(BetaAccessRedemptionResult::Invalid),
            BetaAccessRedemptionTarget::Revoked => return Ok(BetaAccessRedemptionResult::Revoked),
            BetaAccessRedemptionTarget::AlreadyHandled => {
                return Ok(BetaAccessRedemptionResult::AlreadyHandled)
            }
            BetaAccessRedemptionTarget::RateLimited => {
                return Ok(BetaAccessRedemptionResult::RateLimited)
            }
        };
        let Some(code) = code else {
            return Ok(BetaAccessRedemptionResult::Invalid);
        };
        if !issuer.verify(&stored, &stored.email, &code) {
            return Ok(BetaAccessRedemptionResult::Invalid);
        }
        if stored.email != email {
            return Ok(BetaAccessRedemptionResult::WrongAccount);
        }
        let candidate = BetaAccessRedemptionCandidate::from(stored.as_ref());
        match enabled
            .store
            .commit_redemption(candidate, player_id, now)
            .await?
        {
            BetaAccessRedemptionCommit::Granted => Ok(BetaAccessRedemptionResult::Granted),
            BetaAccessRedemptionCommit::Revoked => Ok(BetaAccessRedemptionResult::Revoked),
            BetaAccessRedemptionCommit::AlreadyHandled => {
                Ok(BetaAccessRedemptionResult::AlreadyHandled)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BetaAccessRedemptionResult {
    Granted,
    WrongAccount,
    VerificationRequired,
    Revoked,
    Invalid,
    AlreadyHandled,
    RateLimited,
}

pub(crate) enum BetaAccessRedemptionIdentity {
    VerificationRequired {
        player_id: PlayerId,
    },
    Verified {
        player_id: PlayerId,
        email: NormalizedEmail,
    },
}

impl BetaAccessRedemptionIdentity {
    fn player_id(&self) -> &PlayerId {
        match self {
            Self::VerificationRequired { player_id } | Self::Verified { player_id, .. } => {
                player_id
            }
        }
    }
}

pub(super) struct BetaAccessRedemptionAttempt {
    pub(super) ip_rate_key: String,
    pub(super) lookup_id: Option<String>,
    pub(super) now: DateTime<Utc>,
    pub(super) player_rate_key: String,
}

pub(super) enum BetaAccessRedemptionTarget {
    Candidate(Box<StoredInvitation>),
    Invalid,
    Revoked,
    AlreadyHandled,
    RateLimited,
}

pub(super) struct BetaAccessRedemptionCandidate {
    pub(super) authenticator: String,
    pub(super) email: NormalizedEmail,
    pub(super) invitation_id: String,
    pub(super) lookup_id: String,
    pub(super) request_id: String,
}

impl From<&StoredInvitation> for BetaAccessRedemptionCandidate {
    fn from(invitation: &StoredInvitation) -> Self {
        Self {
            authenticator: invitation.authenticator.clone(),
            email: invitation.email.clone(),
            invitation_id: invitation.id.clone(),
            lookup_id: invitation.lookup_id.clone(),
            request_id: invitation.request_id.clone(),
        }
    }
}

pub(super) enum BetaAccessRedemptionCommit {
    Granted,
    Revoked,
    AlreadyHandled,
}
