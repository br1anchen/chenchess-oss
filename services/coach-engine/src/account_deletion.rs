use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    deployment::DeploymentEnvironment,
    firestore::{FirestoreDatabase, FirestoreError},
    review_durability::path::hashed_path_segment,
    review_session_contract::PlayerId,
};

use firebase::FirebaseIdentityAdmin;
use firestore::AccountDeletionPersistence;
use oauth::OAuthGrantRevoker;

mod firebase;
mod firestore;
mod oauth;

pub const ACCOUNT_DELETION_CONFIRMATION: &str =
    "DELETE MY CHEN CHESS ACCOUNT IN STAGING AND PRODUCTION";
const RECENT_AUTHENTICATION_SECONDS: u64 = 5 * 60;
const RECOVERY_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct AccountDeletionRuntime {
    mode: AccountDeletionMode,
}

#[derive(Clone)]
enum AccountDeletionMode {
    Disabled,
    MarkerOnly(FirestoreDatabase),
    Production {
        local_application: FirestoreDatabase,
        deletion: Arc<ProductionAccountDeletion>,
    },
}

impl AccountDeletionRuntime {
    pub fn disabled() -> Self {
        Self {
            mode: AccountDeletionMode::Disabled,
        }
    }

    #[cfg(test)]
    pub(crate) fn marker_only(local_application: FirestoreDatabase) -> Self {
        Self {
            mode: AccountDeletionMode::MarkerOnly(local_application),
        }
    }

    pub(crate) async fn ensure_player_active(
        &self,
        player_id: &PlayerId,
    ) -> Result<(), AccountDeletionError> {
        let application = match &self.mode {
            AccountDeletionMode::Disabled => return Ok(()),
            AccountDeletionMode::MarkerOnly(application)
            | AccountDeletionMode::Production {
                local_application: application,
                ..
            } => application,
        };
        let document_id = marker_document_id(player_id);
        let Some(marker) = application
            .get_document::<AccountDeletionMarker>(&["deletedUsers", &document_id])
            .await?
        else {
            return Ok(());
        };
        marker.validate_for(player_id)?;
        if marker
            .purge_at
            .is_some_and(|purge_at| purge_at <= Utc::now())
        {
            Ok(())
        } else {
            Err(AccountDeletionError::AccountDeleting)
        }
    }

    pub async fn delete_account(
        &self,
        player_id: PlayerId,
        authenticated_at: u64,
        confirmation: &str,
    ) -> Result<(), AccountDeletionError> {
        let AccountDeletionMode::Production { deletion, .. } = &self.mode else {
            return Err(AccountDeletionError::UnavailableInEnvironment);
        };
        let now = unix_timestamp()?;
        validate_deletion_admission(authenticated_at, confirmation, now)?;
        deletion.run(&player_id).await
    }

    pub fn spawn_recovery(&self) {
        let AccountDeletionMode::Production { deletion, .. } = &self.mode else {
            return;
        };
        let deletion = deletion.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(RECOVERY_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if let Err(error) = deletion.resume_incomplete().await {
                    tracing::error!(
                        category = error.diagnostic_category(),
                        "account deletion recovery pass failed"
                    );
                }
            }
        });
    }
}

pub async fn configured_account_deletion_runtime() -> anyhow::Result<AccountDeletionRuntime> {
    let deployment_environment =
        DeploymentEnvironment::parse(&required_env("DEPLOYMENT_ENVIRONMENT")?)?;
    let local_application = FirestoreDatabase::from_env()?;
    match deployment_environment {
        DeploymentEnvironment::Staging => {
            for name in [
                crate::firestore::ACCOUNT_LIFECYCLE_SERVICE_ACCOUNT_ENV,
                "COACH_ACCOUNT_LIFECYCLE_INTERNAL_TOKEN",
                "COACH_OAUTH_INTERNAL_BASE_URL",
            ] {
                anyhow::ensure!(
                    std::env::var_os(name).is_none(),
                    "{name} must be absent in staging"
                );
            }
            Ok(AccountDeletionRuntime {
                mode: AccountDeletionMode::MarkerOnly(local_application),
            })
        }
        DeploymentEnvironment::Production => {
            let (staging, production) = FirestoreDatabase::account_lifecycle_pair_from_env()?;
            let quality = FirestoreDatabase::quality_from_env()?;
            let persistence = AccountDeletionPersistence::new(staging, production.clone(), quality);
            let firebase = FirebaseIdentityAdmin::from_env()?;
            let oauth = OAuthGrantRevoker::from_env()?;
            Ok(AccountDeletionRuntime {
                mode: AccountDeletionMode::Production {
                    local_application,
                    deletion: Arc::new(ProductionAccountDeletion {
                        persistence,
                        firebase,
                        oauth,
                    }),
                },
            })
        }
    }
}

struct ProductionAccountDeletion {
    persistence: AccountDeletionPersistence,
    firebase: FirebaseIdentityAdmin,
    oauth: OAuthGrantRevoker,
}

impl ProductionAccountDeletion {
    async fn run(&self, player_id: &PlayerId) -> Result<(), AccountDeletionError> {
        let mut marker = self.persistence.ensure_markers(player_id).await?;
        if marker.phase < AccountDeletionPhase::CapturesWithdrawn {
            self.persistence.withdraw_captures(player_id).await?;
            marker = self
                .persistence
                .advance(player_id, AccountDeletionPhase::CapturesWithdrawn)
                .await?;
        }
        if marker.phase < AccountDeletionPhase::ApplicationDataDeleted {
            self.persistence.delete_application_data(player_id).await?;
            marker = self
                .persistence
                .advance(player_id, AccountDeletionPhase::ApplicationDataDeleted)
                .await?;
        }
        if marker.phase < AccountDeletionPhase::OAuthGrantsRevoked {
            self.oauth.revoke_all(player_id).await?;
            marker = self
                .persistence
                .advance(player_id, AccountDeletionPhase::OAuthGrantsRevoked)
                .await?;
        }
        if marker.phase < AccountDeletionPhase::FirebaseTokensRevoked {
            self.firebase.revoke_refresh_tokens(player_id).await?;
            marker = self
                .persistence
                .advance(player_id, AccountDeletionPhase::FirebaseTokensRevoked)
                .await?;
        }
        if marker.phase < AccountDeletionPhase::FirebaseIdentityDeleted {
            self.firebase.delete_identity(player_id).await?;
            self.persistence.complete(player_id).await?;
        }
        Ok(())
    }

    async fn resume_incomplete(&self) -> Result<(), AccountDeletionError> {
        for player_id in self.persistence.incomplete_players().await? {
            if let Err(error) = self.run(&player_id).await {
                tracing::warn!(
                    category = error.diagnostic_category(),
                    "account deletion recovery could not advance one saga"
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum AccountDeletionPhase {
    MarkersWritten,
    CapturesWithdrawn,
    ApplicationDataDeleted,
    OAuthGrantsRevoked,
    FirebaseTokensRevoked,
    FirebaseIdentityDeleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AccountDeletionMarker {
    schema_version: u8,
    player_id: PlayerId,
    started_at: DateTime<Utc>,
    phase: AccountDeletionPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    purge_at: Option<DateTime<Utc>>,
}

impl AccountDeletionMarker {
    fn started(player_id: PlayerId, started_at: DateTime<Utc>) -> Self {
        Self {
            schema_version: 1,
            player_id,
            started_at,
            phase: AccountDeletionPhase::MarkersWritten,
            completed_at: None,
            purge_at: None,
        }
    }

    fn validate_for(&self, player_id: &PlayerId) -> Result<(), AccountDeletionError> {
        if self.schema_version != 1
            || &self.player_id != player_id
            || self.completed_at.is_some() != self.purge_at.is_some()
            || self
                .completed_at
                .zip(self.purge_at)
                .is_some_and(|(completed, purge)| {
                    completed < self.started_at
                        || completed.checked_add_signed(chrono::TimeDelta::hours(2)) != Some(purge)
                })
            || (self.phase == AccountDeletionPhase::FirebaseIdentityDeleted)
                != self.completed_at.is_some()
        {
            return Err(AccountDeletionError::InvalidRecord);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AccountDeletionError {
    #[error("account deletion requires the exact cross-environment confirmation")]
    ConfirmationRequired,
    #[error("account deletion requires a recent Firebase authentication")]
    RecentAuthenticationRequired,
    #[error("account deletion is unavailable in this environment")]
    UnavailableInEnvironment,
    #[error("the Player account is being deleted")]
    AccountDeleting,
    #[error("account deletion persistence is misconfigured: {0}")]
    Configuration(String),
    #[error("account deletion transport failed")]
    Transport,
    #[error("account deletion dependency is unavailable")]
    Unavailable,
    #[error("account deletion persistence conflicted")]
    Conflict,
    #[error("account deletion persistence returned an invalid record")]
    InvalidRecord,
}

impl AccountDeletionError {
    pub(crate) const fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::ConfirmationRequired => "confirmation_required",
            Self::RecentAuthenticationRequired => "recent_authentication_required",
            Self::UnavailableInEnvironment => "unavailable_in_environment",
            Self::AccountDeleting => "account_deleting",
            Self::Configuration(_) => "configuration",
            Self::Transport => "transport",
            Self::Unavailable => "unavailable",
            Self::Conflict => "conflict",
            Self::InvalidRecord => "invalid_record",
        }
    }
}

impl From<FirestoreError> for AccountDeletionError {
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

fn marker_document_id(player_id: &PlayerId) -> String {
    hashed_path_segment(player_id.as_str())
}

/// The Player subtree that account deletion removes recursively.
///
/// Every durable Player-owned record is stored beneath it, which is what makes
/// erasure structural rather than a list of stores somebody has to remember to
/// extend.
pub(crate) fn application_data_document_path(player_id: &PlayerId) -> [String; 2] {
    application_data_document_path_for_owner(&player_subtree_owner(player_id))
}

/// The opaque segment naming one Player's subtree.
///
/// A Review Share Grant is resolved by a caller who has no Player identity at
/// all, so the token has to name the subtree the grant lives in. Deriving that
/// segment here keeps the layout in one place: a store addressing the subtree
/// by owner segment and account deletion removing it agree because they call
/// the same two functions.
pub(crate) fn player_subtree_owner(player_id: &PlayerId) -> String {
    marker_document_id(player_id)
}

pub(crate) fn application_data_document_path_for_owner(owner_segment: &str) -> [String; 2] {
    [
        firestore::USERS_COLLECTION.to_string(),
        owner_segment.to_string(),
    ]
}

fn required_env(name: &str) -> Result<String, AccountDeletionError> {
    let value = std::env::var(name)
        .map_err(|_| AccountDeletionError::Configuration(format!("{name} is required")))?;
    if value.trim().is_empty() {
        return Err(AccountDeletionError::Configuration(format!(
            "{name} must not be empty"
        )));
    }
    Ok(value)
}

fn validate_deletion_admission(
    authenticated_at: u64,
    confirmation: &str,
    now: u64,
) -> Result<(), AccountDeletionError> {
    if confirmation != ACCOUNT_DELETION_CONFIRMATION {
        return Err(AccountDeletionError::ConfirmationRequired);
    }
    if authenticated_at > now
        || now.saturating_sub(authenticated_at) > RECENT_AUTHENTICATION_SECONDS
    {
        return Err(AccountDeletionError::RecentAuthenticationRequired);
    }
    Ok(())
}

fn unix_timestamp() -> Result<u64, AccountDeletionError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AccountDeletionError::Unavailable)
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> PlayerId {
        PlayerId::try_from("firebase-player".to_string()).unwrap()
    }

    #[test]
    fn deletion_marker_progress_is_monotonic_and_completion_expires_in_two_hours() {
        let started_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let mut marker = AccountDeletionMarker::started(player(), started_at);
        marker.validate_for(&player()).unwrap();

        for phase in [
            AccountDeletionPhase::CapturesWithdrawn,
            AccountDeletionPhase::ApplicationDataDeleted,
            AccountDeletionPhase::OAuthGrantsRevoked,
            AccountDeletionPhase::FirebaseTokensRevoked,
        ] {
            assert!(marker.phase < phase);
            marker.phase = phase;
            marker.validate_for(&player()).unwrap();
        }

        marker.phase = AccountDeletionPhase::FirebaseIdentityDeleted;
        marker.completed_at = Some(started_at);
        marker.purge_at = Some(started_at + chrono::TimeDelta::hours(2));
        marker.validate_for(&player()).unwrap();
        marker.purge_at = Some(started_at + chrono::TimeDelta::hours(3));
        assert!(matches!(
            marker.validate_for(&player()),
            Err(AccountDeletionError::InvalidRecord)
        ));
        marker.completed_at = Some(DateTime::<Utc>::MAX_UTC);
        marker.purge_at = Some(DateTime::<Utc>::MAX_UTC);
        assert!(matches!(
            marker.validate_for(&player()),
            Err(AccountDeletionError::InvalidRecord)
        ));
    }

    #[test]
    fn deletion_admission_requires_exact_confirmation_and_recent_authentication() {
        let now = 10_000;
        validate_deletion_admission(
            now - RECENT_AUTHENTICATION_SECONDS,
            ACCOUNT_DELETION_CONFIRMATION,
            now,
        )
        .unwrap();
        assert!(matches!(
            validate_deletion_admission(now, "wrong", now),
            Err(AccountDeletionError::ConfirmationRequired)
        ));
        for authenticated_at in [now + 1, now - RECENT_AUTHENTICATION_SECONDS - 1] {
            assert!(matches!(
                validate_deletion_admission(authenticated_at, ACCOUNT_DELETION_CONFIRMATION, now),
                Err(AccountDeletionError::RecentAuthenticationRequired)
            ));
        }
    }

    #[tokio::test]
    async fn nonproduction_runtime_rejects_the_deletion_surface_before_confirmation() {
        let error = AccountDeletionRuntime::disabled()
            .delete_account(player(), u64::MAX, "wrong")
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            AccountDeletionError::UnavailableInEnvironment
        ));
    }
}
