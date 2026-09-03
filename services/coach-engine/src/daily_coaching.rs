use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[cfg(test)]
use crate::review_session_transport::ReviewSessionCommandExecutor;
use crate::{
    beta_access::NormalizedEmail,
    deployment::DeploymentEnvironment,
    firestore::FirestoreDatabase,
    profile_game_feed::{
        ChessProfileProvider, ProfileGameClient, ProfileGameFeed, ProfileGameFetchError,
        ProfileGameRequest, ProfileGameResponse, ProfileUrlError, ProfileValidationError,
        PublicChessProfile, PublicProfileValidator, ReqwestProfileGameClient,
    },
    review_session_contract::{PlayerId, RetryDirective},
};

mod configuration;
mod dashboard;
pub(crate) mod delivery;
mod digest;
mod digested_index;
mod firestore;
mod lifecycle;
mod operator;
mod recent_profile_games;
mod reviewer;
#[cfg(test)]
pub(crate) use reviewer::{DailyGameReviewFuture, DailyGameReviewResult, DailyGameReviewer};
mod runs;
mod schedule;
pub(crate) mod selection;
mod state;

pub use dashboard::{
    CoachingHost, CoachingHostConnection, DailyCoachingDashboardState, DailyCoachingDigestDetail,
    DailyCoachingDigestSummary, DailyCoachingGameCard, DailyCoachingGameOutcome,
    DailyCoachingLeadState, DailyCoachingOpening, DailyCoachingPriority, DailyCoachingReviewSide,
    DailyCoachingTimeControlClass,
};
pub(crate) use delivery::{DigestWebhookError, WebhookHeaders};
pub(crate) use digest::DigestedGameCard;
pub(crate) use digested_index::digested_game_index;
pub use lifecycle::{DailyCoachingTickError, DailyCoachingTickReport};
use recent_profile_games::CachedRecentProfileGames;
pub use recent_profile_games::{RecentPlayingProfileGame, RecentPlayingProfileGamesOutcome};
pub(crate) use reviewer::DailyGameReviewExecutor;
pub use runs::DailyCoachingRunStoreError;
#[cfg(test)]
pub(crate) use runs::RUN_SCHEMA_VERSION;
#[cfg(test)]
pub(crate) use state::STATE_SCHEMA_VERSION;
pub(crate) use state::{
    DailyCoachingDocument, DailyCoachingOwnerKey, DailyCoachingStore, InMemoryDailyCoachingStore,
    StoreFuture, StoredPlayingProfileConnection,
};
pub use state::{DailyCoachingDomainError, DailyCoachingStoreError};

const DEFAULT_TIMEZONE_ENV: &str = "DAILY_COACHING_DEFAULT_TIMEZONE";
const BUILT_IN_DEFAULT_TIMEZONE: &str = "UTC";

fn valid_resend_provider_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum DailyCoachingProvider {
    Lichess,
    ChessCom,
}

impl From<ChessProfileProvider> for DailyCoachingProvider {
    fn from(provider: ChessProfileProvider) -> Self {
        match provider {
            ChessProfileProvider::Lichess => Self::Lichess,
            ChessProfileProvider::ChessCom => Self::ChessCom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum PlayingProfileConnectionStatus {
    Connected,
    ProfileUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayingProfileConnection {
    pub provider: DailyCoachingProvider,
    pub username: String,
    pub canonical_url: String,
    pub status: PlayingProfileConnectionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DailyCoachingSetupState {
    NotConnected,
    Connected {
        enabled: bool,
        timezone: String,
        connections: Vec<PlayingProfileConnection>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectPlayingProfileRequest {
    pub profile_url: String,
    #[serde(default)]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplacePlayingProfileRequest {
    pub expected_username: String,
    pub profile_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemovePlayingProfileRequest {
    pub expected_username: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckPlayingProfileRequest {
    pub expected_username: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetDailyCoachingEnabledRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ConnectPlayingProfileRejectionReason {
    UnparseableProfileUrl,
    UnsupportedProvider,
    ProviderAlreadyConnected,
    ProfileNotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DailyCoachingMutationRejectionReason {
    DigestEmailUnavailable,
    NoVerifiedAccountEmail,
    NoPlayingProfile,
    StalePlayingProfile,
    UnparseableProfileUrl,
    UnsupportedProvider,
    ProfileNotFound,
    ProviderMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DailyCoachingUnavailableReason {
    ProviderUnreachable,
    Persistence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "outcome",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ConnectPlayingProfileOutcome {
    Completed {
        provider: DailyCoachingProvider,
        username: String,
        canonical_url: String,
        status: PlayingProfileConnectionStatus,
    },
    Rejected {
        reason: ConnectPlayingProfileRejectionReason,
    },
    Unavailable {
        reason: DailyCoachingUnavailableReason,
        retry: RetryDirective,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "outcome",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DailyCoachingMutationOutcome {
    Completed {
        state: DailyCoachingSetupState,
    },
    Rejected {
        reason: DailyCoachingMutationRejectionReason,
    },
    Unavailable {
        reason: DailyCoachingUnavailableReason,
        retry: RetryDirective,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "outcome",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CheckPlayingProfileOutcome {
    Reachable {
        provider: DailyCoachingProvider,
    },
    ProfileUnavailable {
        provider: DailyCoachingProvider,
    },
    ProviderUnavailable {
        provider: DailyCoachingProvider,
        retry: RetryDirective,
    },
    Rejected {
        reason: DailyCoachingMutationRejectionReason,
    },
    Unavailable {
        reason: DailyCoachingUnavailableReason,
        retry: RetryDirective,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DailyCoachingDigestEmailAdminProjection {
    pub(crate) status: DailyCoachingDigestEmailAdminStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) latest_digest: Option<DailyCoachingDigestEmailAdminMetadata>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DailyCoachingDigestEmailAdminMetadata {
    coverage_date: String,
    published_at: String,
    game_count: u8,
    learning_path_count: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DailyCoachingDigestEmailAdminStatus {
    Ready,
    NoDigest,
    EmailNotConfigured,
    NoVerifiedEmail,
    EmailDisabled,
    EmailSuppressed,
    Unavailable,
}

#[derive(Clone)]
pub struct DailyCoachingRuntime {
    store: Arc<dyn DailyCoachingStore>,
    validator: Arc<dyn PublicProfileValidator>,
    default_timezone: String,
    lifecycle: lifecycle::DailyCoachingLifecycle,
    email: delivery::DigestEmailRuntime,
    /// Empty in production. Central Host's dashboard proxy injects the live
    /// OAuth grants; this field exists so tests can exercise the wire shape.
    host_connections: Vec<CoachingHostConnection>,
    /// Admission control for the recent-games read: a repeat read inside the
    /// TTL window serves the cached outcome instead of re-hitting
    /// Lichess/Chess.com. The lock is released before the provider call, so
    /// this thins sequential reads — it does not collapse a concurrent burst
    /// into one fetch. Writes sweep entries past the TTL, so the map holds
    /// only Players seen inside the window.
    recent_games_cache: Arc<Mutex<HashMap<PlayerId, CachedRecentProfileGames>>>,
}

impl DailyCoachingRuntime {
    pub fn in_memory(validator: Arc<dyn PublicProfileValidator>, default_timezone: &str) -> Self {
        let store = Arc::new(InMemoryDailyCoachingStore::default());
        Self::new(
            store.clone(),
            validator,
            default_timezone,
            Arc::new(runs::InMemoryDailyCoachingRunStore::new(store.clone())),
            Arc::new(ProfileGameFeed::new(
                Arc::new(EmptyProfileGameClient) as Arc<dyn ProfileGameClient>
            )),
            Arc::new(UnavailableDailyGameReviewer),
            configuration::DailyCoachingConfiguration::standard(),
            "in-memory-cell",
        )
        .expect("test Daily Coaching timezone should be valid")
    }

    #[cfg(test)]
    pub(crate) fn in_memory_with_pipeline(
        validator: Arc<dyn PublicProfileValidator>,
        default_timezone: &str,
        profile_client: Arc<dyn ProfileGameClient>,
        review_executor: Arc<dyn ReviewSessionCommandExecutor>,
    ) -> Self {
        let store = Arc::new(InMemoryDailyCoachingStore::default());
        Self::new(
            store.clone(),
            validator,
            default_timezone,
            Arc::new(runs::InMemoryDailyCoachingRunStore::new(store)),
            Arc::new(ProfileGameFeed::new(profile_client)),
            Arc::new(
                reviewer::CommandExecutorDailyGameReviewer::from_command_executor(review_executor),
            ),
            configuration::DailyCoachingConfiguration::standard(),
            "in-memory-pipeline-cell",
        )
        .expect("test Daily Coaching timezone should be valid")
    }

    #[cfg(test)]
    pub(crate) fn in_memory_with_reviewer(
        validator: Arc<dyn PublicProfileValidator>,
        default_timezone: &str,
        profile_client: Arc<dyn ProfileGameClient>,
        reviewer: Arc<dyn reviewer::DailyGameReviewer>,
    ) -> Self {
        let store = Arc::new(InMemoryDailyCoachingStore::default());
        Self::new(
            store.clone(),
            validator,
            default_timezone,
            Arc::new(runs::InMemoryDailyCoachingRunStore::new(store)),
            Arc::new(ProfileGameFeed::new(profile_client)),
            reviewer,
            configuration::DailyCoachingConfiguration::standard(),
            "in-memory-reviewer-cell",
        )
        .expect("test Daily Coaching timezone should be valid")
    }

    #[cfg(test)]
    pub(crate) fn in_memory_with_pipeline_and_email(
        validator: Arc<dyn PublicProfileValidator>,
        default_timezone: &str,
        profile_client: Arc<dyn ProfileGameClient>,
        review_executor: Arc<dyn ReviewSessionCommandExecutor>,
        email_store: Arc<dyn delivery::DigestEmailStore>,
        email_delivery: Arc<dyn delivery::DigestEmailDelivery>,
    ) -> Self {
        let store = Arc::new(InMemoryDailyCoachingStore::default());
        Self::new_with_email(
            store.clone(),
            validator,
            default_timezone,
            Arc::new(runs::InMemoryDailyCoachingRunStore::new(store)),
            Arc::new(ProfileGameFeed::new(profile_client)),
            Arc::new(
                reviewer::CommandExecutorDailyGameReviewer::from_command_executor(review_executor),
            ),
            configuration::DailyCoachingConfiguration::standard(),
            "in-memory-email-pipeline-cell",
            delivery::DigestEmailRuntime::for_test(email_store, email_delivery),
        )
        .expect("test Daily Coaching timezone should be valid")
    }

    pub fn disabled() -> Self {
        Self::in_memory(
            Arc::new(UnavailableProfileValidator),
            BUILT_IN_DEFAULT_TIMEZONE,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the runtime composition root receives each independently replaceable boundary"
    )]
    fn new(
        store: Arc<dyn DailyCoachingStore>,
        validator: Arc<dyn PublicProfileValidator>,
        default_timezone: &str,
        run_store: Arc<dyn runs::DailyCoachingRunStore>,
        profile_feed: Arc<ProfileGameFeed<Arc<dyn ProfileGameClient>>>,
        reviewer: Arc<dyn reviewer::DailyGameReviewer>,
        configuration: configuration::DailyCoachingConfiguration,
        holder_id: impl Into<Arc<str>>,
    ) -> Result<Self, DailyCoachingConfigurationError> {
        Self::new_with_email(
            store,
            validator,
            default_timezone,
            run_store,
            profile_feed,
            reviewer,
            configuration,
            holder_id,
            delivery::DigestEmailRuntime::disabled(),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the runtime composition root receives each independently replaceable boundary"
    )]
    fn new_with_email(
        store: Arc<dyn DailyCoachingStore>,
        validator: Arc<dyn PublicProfileValidator>,
        default_timezone: &str,
        run_store: Arc<dyn runs::DailyCoachingRunStore>,
        profile_feed: Arc<ProfileGameFeed<Arc<dyn ProfileGameClient>>>,
        reviewer: Arc<dyn reviewer::DailyGameReviewer>,
        configuration: configuration::DailyCoachingConfiguration,
        holder_id: impl Into<Arc<str>>,
        email: delivery::DigestEmailRuntime,
    ) -> Result<Self, DailyCoachingConfigurationError> {
        let default_timezone = canonical_timezone(default_timezone).ok_or_else(|| {
            DailyCoachingConfigurationError::InvalidDefaultTimezone(default_timezone.to_string())
        })?;
        let lifecycle = lifecycle::DailyCoachingLifecycle::new(
            store.clone(),
            run_store,
            profile_feed,
            reviewer,
            configuration,
            holder_id,
        )
        .with_email(email.clone());
        Ok(Self {
            store,
            validator,
            default_timezone,
            lifecycle,
            email,
            host_connections: Vec::new(),
            recent_games_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_host_connections(
        mut self,
        host_connections: Vec<CoachingHostConnection>,
    ) -> Self {
        self.host_connections = host_connections;
        self
    }

    fn with_operator(mut self, operator: operator::OperatorDigestRuntime) -> Self {
        self.lifecycle = self.lifecycle.with_operator(operator);
        self
    }

    pub async fn state(
        &self,
        player_id: &PlayerId,
    ) -> Result<DailyCoachingSetupState, DailyCoachingStoreError> {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        self.store
            .bind_player(&owner_key, player_id)
            .await
            .map(|state| state.project())
    }

    pub async fn dashboard(
        &self,
        player_id: &PlayerId,
    ) -> Result<DailyCoachingDashboardState, DailyCoachingDashboardError> {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        let state = self.store.bind_player(&owner_key, player_id).await?;
        let (latest_visible, archive) = if state.connections().is_empty() {
            (None, self.lifecycle.archive(&owner_key).await?)
        } else {
            self.lifecycle.dashboard_snapshot(&owner_key).await?
        };
        let digest_email_enabled = self
            .email
            .preference_enabled(player_id)
            .await
            .map_err(|_| DailyCoachingDashboardError::Email)?;
        Ok(dashboard::project_dashboard(state, latest_visible, archive)
            .with_digest_email_enabled(digest_email_enabled)
            .with_host_connections(self.host_connections.clone()))
    }

    pub async fn digest(
        &self,
        player_id: &PlayerId,
        digest_id: &str,
    ) -> Result<Option<DailyCoachingDigestDetail>, DailyCoachingDashboardError> {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        self.lifecycle
            .read_digest(&owner_key, digest_id)
            .await
            .map(|digest| digest.map(|(digest, cards)| dashboard::project_digest(digest, cards)))
            .map_err(Into::into)
    }

    pub(crate) fn run_store(&self) -> Arc<dyn runs::DailyCoachingRunStore> {
        self.lifecycle.run_store()
    }

    pub async fn connect(
        &self,
        player_id: &PlayerId,
        request: ConnectPlayingProfileRequest,
    ) -> ConnectPlayingProfileOutcome {
        self.connect_at(player_id, request, Utc::now()).await
    }

    pub(crate) async fn connect_with_verified_email(
        &self,
        player_id: &PlayerId,
        email: Option<&NormalizedEmail>,
        request: ConnectPlayingProfileRequest,
    ) -> ConnectPlayingProfileOutcome {
        let outcome = self.connect(player_id, request).await;
        if matches!(outcome, ConnectPlayingProfileOutcome::Completed { .. })
            && self
                .email
                .observe_verified_email(player_id, email, Utc::now())
                .await
                .is_err()
        {
            return ConnectPlayingProfileOutcome::Unavailable {
                reason: DailyCoachingUnavailableReason::Persistence,
                retry: RetryDirective::RetryAllowed,
            };
        }
        outcome
    }

    pub(crate) async fn observe_verified_email(
        &self,
        player_id: &PlayerId,
        email: Option<&NormalizedEmail>,
    ) -> Result<(), delivery::DigestEmailError> {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        let state = self
            .store
            .bind_player(&owner_key, player_id)
            .await
            .map_err(|_| delivery::DigestEmailError::Unavailable)?;
        if state.connections().is_empty() {
            return Ok(());
        }
        self.email
            .observe_verified_email(player_id, email, Utc::now())
            .await
    }

    pub(crate) async fn set_digest_email_enabled(
        &self,
        player_id: &PlayerId,
        email: Option<&NormalizedEmail>,
        enabled: bool,
    ) -> DailyCoachingMutationOutcome {
        if !self.email.is_available() {
            return DailyCoachingMutationOutcome::Rejected {
                reason: DailyCoachingMutationRejectionReason::DigestEmailUnavailable,
            };
        }
        if email.is_none() {
            return DailyCoachingMutationOutcome::Rejected {
                reason: DailyCoachingMutationRejectionReason::NoVerifiedAccountEmail,
            };
        }
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        let state = match self.store.bind_player(&owner_key, player_id).await {
            Ok(state) => state,
            Err(_) => {
                return DailyCoachingMutationOutcome::Unavailable {
                    reason: DailyCoachingUnavailableReason::Persistence,
                    retry: RetryDirective::RetryAllowed,
                };
            }
        };
        if state.connections().is_empty() {
            return DailyCoachingMutationOutcome::Rejected {
                reason: DailyCoachingMutationRejectionReason::NoPlayingProfile,
            };
        }
        if self
            .email
            .set_enabled(player_id, email, enabled, Utc::now())
            .await
            .is_err()
        {
            return DailyCoachingMutationOutcome::Unavailable {
                reason: DailyCoachingUnavailableReason::Persistence,
                retry: RetryDirective::RetryAllowed,
            };
        }
        DailyCoachingMutationOutcome::Completed {
            state: state.project(),
        }
    }

    pub(crate) async fn unsubscribe_digest_email(&self, token: &str) -> bool {
        self.email.unsubscribe(token, Utc::now()).await
    }

    pub(crate) async fn can_unsubscribe_digest_email(&self, token: &str) -> bool {
        self.email.can_unsubscribe(token).await
    }

    pub(crate) async fn ingest_digest_email_webhook(
        &self,
        headers: WebhookHeaders<'_>,
        raw_body: &[u8],
    ) -> Result<(), DigestWebhookError> {
        self.email
            .ingest_webhook(headers, raw_body, Utc::now())
            .await
    }

    pub(crate) async fn connect_at(
        &self,
        player_id: &PlayerId,
        request: ConnectPlayingProfileRequest,
        now: DateTime<Utc>,
    ) -> ConnectPlayingProfileOutcome {
        let profile = match PublicChessProfile::parse(&request.profile_url) {
            Ok(profile) => profile,
            Err(error) => return rejected_profile_url(error),
        };
        let provider = DailyCoachingProvider::from(profile.provider());
        let identity_username = profile.identity_username();
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        match self.store.bind_player(&owner_key, player_id).await {
            Ok(state) => match state.connection(provider) {
                Some(connection) if connection.identity_username() == identity_username => {
                    return connection.completed_outcome()
                }
                Some(_) => {
                    return ConnectPlayingProfileOutcome::Rejected {
                        reason: ConnectPlayingProfileRejectionReason::ProviderAlreadyConnected,
                    }
                }
                None => {}
            },
            Err(error) => return persistence_connect_outcome(error),
        }

        let validated = match self.validator.validate(&profile).await {
            Ok(validated) => validated,
            Err(error) => return validation_outcome(error),
        };
        let connection = StoredPlayingProfileConnection::from_validated(validated);
        let timezone = request
            .timezone
            .as_deref()
            .and_then(canonical_timezone)
            .unwrap_or_else(|| self.default_timezone.clone());
        match self
            .store
            .connect_profile(&owner_key, player_id, connection, timezone, now)
            .await
        {
            Ok(connection) => connection.completed_outcome(),
            Err(DailyCoachingStoreError::Domain(
                DailyCoachingDomainError::ProviderAlreadyConnected,
            )) => ConnectPlayingProfileOutcome::Rejected {
                reason: ConnectPlayingProfileRejectionReason::ProviderAlreadyConnected,
            },
            Err(error) => persistence_connect_outcome(error),
        }
    }

    pub async fn replace(
        &self,
        player_id: &PlayerId,
        provider: DailyCoachingProvider,
        request: ReplacePlayingProfileRequest,
    ) -> DailyCoachingMutationOutcome {
        let profile = match PublicChessProfile::parse(&request.profile_url) {
            Ok(profile) => profile,
            Err(error) => {
                return DailyCoachingMutationOutcome::Rejected {
                    reason: mutation_profile_url_rejection(error),
                }
            }
        };
        if DailyCoachingProvider::from(profile.provider()) != provider {
            return DailyCoachingMutationOutcome::Rejected {
                reason: DailyCoachingMutationRejectionReason::ProviderMismatch,
            };
        }
        let validated = match self.validator.validate(&profile).await {
            Ok(validated) => validated,
            Err(error) => return validation_mutation_outcome(error),
        };
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        if let Err(error) = self.store.bind_player(&owner_key, player_id).await {
            return mutation_outcome(Err(error));
        }
        let result = self
            .store
            .replace_profile(
                &owner_key,
                StoredPlayingProfileConnection::from_validated(validated),
                request.expected_username.to_ascii_lowercase(),
            )
            .await;
        mutation_outcome(result)
    }

    pub async fn remove(
        &self,
        player_id: &PlayerId,
        provider: DailyCoachingProvider,
        request: RemovePlayingProfileRequest,
    ) -> DailyCoachingMutationOutcome {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        if let Err(error) = self.store.bind_player(&owner_key, player_id).await {
            return mutation_outcome(Err(error));
        }
        let result = self
            .store
            .remove_profile(
                &owner_key,
                provider,
                request.expected_username.to_ascii_lowercase(),
            )
            .await;
        mutation_outcome(result)
    }

    pub async fn check_profile(
        &self,
        player_id: &PlayerId,
        provider: DailyCoachingProvider,
        request: CheckPlayingProfileRequest,
    ) -> CheckPlayingProfileOutcome {
        self.check_profile_at(player_id, provider, request, Utc::now())
            .await
    }

    pub(crate) async fn check_profile_at(
        &self,
        player_id: &PlayerId,
        provider: DailyCoachingProvider,
        request: CheckPlayingProfileRequest,
        now: DateTime<Utc>,
    ) -> CheckPlayingProfileOutcome {
        match self
            .lifecycle
            .check_profile(
                player_id,
                provider,
                &request.expected_username.to_ascii_lowercase(),
                now,
            )
            .await
        {
            Ok(lifecycle::ProfileCheckResult::Reachable) => {
                CheckPlayingProfileOutcome::Reachable { provider }
            }
            Ok(lifecycle::ProfileCheckResult::ProfileUnavailable) => {
                CheckPlayingProfileOutcome::ProfileUnavailable { provider }
            }
            Ok(lifecycle::ProfileCheckResult::ProviderUnavailable(retry)) => {
                CheckPlayingProfileOutcome::ProviderUnavailable { provider, retry }
            }
            Ok(lifecycle::ProfileCheckResult::Stale) => CheckPlayingProfileOutcome::Rejected {
                reason: DailyCoachingMutationRejectionReason::StalePlayingProfile,
            },
            Err(_) => CheckPlayingProfileOutcome::Unavailable {
                reason: DailyCoachingUnavailableReason::Persistence,
                retry: RetryDirective::RetryAllowed,
            },
        }
    }

    pub async fn set_enabled(
        &self,
        player_id: &PlayerId,
        enabled: bool,
    ) -> DailyCoachingMutationOutcome {
        self.set_enabled_at(player_id, enabled, Utc::now()).await
    }

    pub(crate) async fn set_enabled_at(
        &self,
        player_id: &PlayerId,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> DailyCoachingMutationOutcome {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        if let Err(error) = self.store.bind_player(&owner_key, player_id).await {
            return mutation_outcome(Err(error));
        }
        mutation_outcome(self.store.set_enabled(&owner_key, enabled, now).await)
    }

    /// Runs one Daily Coaching scheduling and recovery pass at `now`.
    pub async fn tick(
        &self,
        now: DateTime<Utc>,
    ) -> Result<DailyCoachingTickReport, DailyCoachingTickError> {
        self.lifecycle.tick(now).await
    }

    /// Promotes this player's due window without waiting for the Run to finish.
    pub async fn promote_due_window(
        &self,
        player_id: &PlayerId,
        now: DateTime<Utc>,
    ) -> Result<bool, DailyCoachingTickError> {
        self.lifecycle.promote(player_id, now).await
    }

    pub(crate) async fn start_manual_digest_run(
        &self,
        player_id: &PlayerId,
        now: DateTime<Utc>,
    ) -> Result<bool, DailyCoachingTickError> {
        self.lifecycle.start_manual_digest_run(player_id, now).await
    }

    pub(crate) async fn force_regenerate_last_digest(
        &self,
        player_id: &PlayerId,
        now: DateTime<Utc>,
    ) -> Result<bool, DailyCoachingTickError> {
        self.lifecycle
            .force_regenerate_last_digest(player_id, now)
            .await
    }

    pub(crate) async fn inspect_latest_digest_email(
        &self,
        player_id: &PlayerId,
    ) -> Result<DailyCoachingDigestEmailAdminProjection, DailyCoachingTickError> {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        let latest = self.lifecycle.archive(&owner_key).await?.into_iter().next();
        let status = if latest.is_none() {
            DailyCoachingDigestEmailAdminStatus::NoDigest
        } else {
            match self
                .email
                .readiness(&owner_key)
                .await
                .map_err(|_| DailyCoachingTickError::Email)?
            {
                delivery::DigestEmailReadiness::Ready => DailyCoachingDigestEmailAdminStatus::Ready,
                delivery::DigestEmailReadiness::NotConfigured => {
                    DailyCoachingDigestEmailAdminStatus::EmailNotConfigured
                }
                delivery::DigestEmailReadiness::NoVerifiedEmail => {
                    DailyCoachingDigestEmailAdminStatus::NoVerifiedEmail
                }
                delivery::DigestEmailReadiness::Disabled => {
                    DailyCoachingDigestEmailAdminStatus::EmailDisabled
                }
                delivery::DigestEmailReadiness::Suppressed => {
                    DailyCoachingDigestEmailAdminStatus::EmailSuppressed
                }
            }
        };
        Ok(DailyCoachingDigestEmailAdminProjection {
            status,
            latest_digest: latest.map(|latest| DailyCoachingDigestEmailAdminMetadata {
                coverage_date: latest.coverage_date.to_string(),
                published_at: latest.published_at.to_rfc3339(),
                game_count: latest.game_count,
                learning_path_count: latest.learning_path_count,
            }),
        })
    }

    /// Starts the periodic scheduler around [`Self::tick`].
    pub fn spawn_scheduler(&self) {
        self.lifecycle.spawn_scheduler();
    }
}

pub(crate) fn configured_daily_coaching_runtime(
    review_executor: Arc<dyn DailyGameReviewExecutor>,
) -> anyhow::Result<DailyCoachingRuntime> {
    let database = FirestoreDatabase::from_env()?;
    let environment = DeploymentEnvironment::parse(&std::env::var("DEPLOYMENT_ENVIRONMENT")?)?;
    let default_timezone = std::env::var(DEFAULT_TIMEZONE_ENV)
        .unwrap_or_else(|_| BUILT_IN_DEFAULT_TIMEZONE.to_string());
    let feed = Arc::new(ProfileGameFeed::new(
        Arc::new(ReqwestProfileGameClient) as Arc<dyn ProfileGameClient>
    ));
    let store = Arc::new(firestore::FirestoreDailyCoachingStore::new(
        database.clone(),
    ));
    let email =
        delivery::DigestEmailRuntime::configured(database.clone(), environment.public_origin())?;
    let run_store = Arc::new(runs::firestore::FirestoreDailyCoachingRunStore::new(
        database.clone(),
    ));
    let configuration = configuration::DailyCoachingConfiguration::from_env()?;
    let operator = operator::OperatorDigestRuntime::configured(
        database,
        run_store.clone(),
        configuration.operations.operator_digest_utc_hour,
    )?;
    DailyCoachingRuntime::new_with_email(
        store,
        feed.clone(),
        &default_timezone,
        run_store,
        feed,
        Arc::new(reviewer::CommandExecutorDailyGameReviewer::new(
            review_executor,
        )),
        configuration,
        uuid::Uuid::new_v4().to_string(),
        email,
    )
    .map(|runtime| runtime.with_operator(operator))
    .map_err(Into::into)
}

#[derive(Debug, thiserror::Error)]
/// Invalid Daily Coaching runtime configuration.
pub enum DailyCoachingConfigurationError {
    /// The configured backend fallback is not an IANA timezone.
    #[error("{DEFAULT_TIMEZONE_ENV} is not a valid IANA timezone: {0}")]
    InvalidDefaultTimezone(String),
    /// One or more lifecycle timing values are inconsistent.
    #[error(transparent)]
    Timing(#[from] configuration::DailyCoachingConfigurationError),
}

#[derive(Debug, thiserror::Error)]
pub enum DailyCoachingDashboardError {
    #[error(transparent)]
    State(#[from] DailyCoachingStoreError),
    #[error(transparent)]
    Run(#[from] DailyCoachingRunStoreError),
    #[error("Daily Coaching digest email preference is unavailable")]
    Email,
}

fn canonical_timezone(value: &str) -> Option<String> {
    value
        .parse::<Tz>()
        .ok()
        .map(|timezone| timezone.to_string())
}

fn rejected_profile_url(error: ProfileUrlError) -> ConnectPlayingProfileOutcome {
    ConnectPlayingProfileOutcome::Rejected {
        reason: match error {
            ProfileUrlError::UnparseableProfileUrl => {
                ConnectPlayingProfileRejectionReason::UnparseableProfileUrl
            }
            ProfileUrlError::UnsupportedProvider => {
                ConnectPlayingProfileRejectionReason::UnsupportedProvider
            }
        },
    }
}

fn validation_outcome(error: ProfileValidationError) -> ConnectPlayingProfileOutcome {
    match error {
        ProfileValidationError::ProfileNotFound => ConnectPlayingProfileOutcome::Rejected {
            reason: ConnectPlayingProfileRejectionReason::ProfileNotFound,
        },
        error => ConnectPlayingProfileOutcome::Unavailable {
            reason: DailyCoachingUnavailableReason::ProviderUnreachable,
            retry: retry_directive(&error),
        },
    }
}

fn validation_mutation_outcome(error: ProfileValidationError) -> DailyCoachingMutationOutcome {
    match error {
        ProfileValidationError::ProfileNotFound => DailyCoachingMutationOutcome::Rejected {
            reason: DailyCoachingMutationRejectionReason::ProfileNotFound,
        },
        error => DailyCoachingMutationOutcome::Unavailable {
            reason: DailyCoachingUnavailableReason::ProviderUnreachable,
            retry: retry_directive(&error),
        },
    }
}

fn mutation_profile_url_rejection(error: ProfileUrlError) -> DailyCoachingMutationRejectionReason {
    match error {
        ProfileUrlError::UnparseableProfileUrl => {
            DailyCoachingMutationRejectionReason::UnparseableProfileUrl
        }
        ProfileUrlError::UnsupportedProvider => {
            DailyCoachingMutationRejectionReason::UnsupportedProvider
        }
    }
}

fn retry_directive(error: &ProfileValidationError) -> RetryDirective {
    let seconds = match error {
        ProfileValidationError::ProviderUnavailable {
            retry_after_seconds,
        }
        | ProfileValidationError::Fetch(ProfileGameFetchError::Status {
            retry_after_seconds,
            ..
        }) => *retry_after_seconds,
        _ => None,
    };
    seconds
        .filter(|seconds| *seconds > 0)
        .map_or(RetryDirective::RetryAllowed, |seconds| {
            RetryDirective::RetryAfter { seconds }
        })
}

fn persistence_connect_outcome(_error: DailyCoachingStoreError) -> ConnectPlayingProfileOutcome {
    ConnectPlayingProfileOutcome::Unavailable {
        reason: DailyCoachingUnavailableReason::Persistence,
        retry: RetryDirective::RetryAllowed,
    }
}

fn mutation_outcome(
    result: Result<DailyCoachingDocument, DailyCoachingStoreError>,
) -> DailyCoachingMutationOutcome {
    match result {
        Ok(state) => DailyCoachingMutationOutcome::Completed {
            state: state.project(),
        },
        Err(DailyCoachingStoreError::Domain(error)) => DailyCoachingMutationOutcome::Rejected {
            reason: error.rejection_reason(),
        },
        Err(_) => DailyCoachingMutationOutcome::Unavailable {
            reason: DailyCoachingUnavailableReason::Persistence,
            retry: RetryDirective::RetryAllowed,
        },
    }
}

struct UnavailableProfileValidator;

impl PublicProfileValidator for UnavailableProfileValidator {
    fn validate<'a>(
        &'a self,
        _profile: &'a PublicChessProfile,
    ) -> crate::profile_game_feed::ProfileValidationFuture<'a> {
        Box::pin(async {
            Err(ProfileValidationError::ProviderUnavailable {
                retry_after_seconds: None,
            })
        })
    }
}

struct EmptyProfileGameClient;

impl ProfileGameClient for EmptyProfileGameClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        ProfileGameResponse,
                        crate::profile_game_feed::ProfileGameFetchError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            Ok(ProfileGameResponse {
                body: match request.provider() {
                    ChessProfileProvider::Lichess => Vec::new(),
                    ChessProfileProvider::ChessCom => br#"{"games":[]}"#.to_vec(),
                },
                content_type: request.accept().to_string(),
            })
        })
    }
}

struct UnavailableDailyGameReviewer;

impl reviewer::DailyGameReviewer for UnavailableDailyGameReviewer {
    fn review<'a>(
        &'a self,
        _player_id: &'a PlayerId,
        _request: &'a crate::profile_game_feed::DailyGameReviewRequest,
    ) -> reviewer::DailyGameReviewFuture<'a> {
        Box::pin(async { reviewer::DailyGameReviewResult::Terminal })
    }
}
