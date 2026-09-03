use chrono::{DateTime, Utc};

use crate::{
    daily_coaching::{
        runs::DailyCoachingRunConnection, state::ProfileHealthObservation, DailyCoachingDocument,
        DailyCoachingOwnerKey, DailyCoachingProvider,
    },
    profile_game_feed::{ProfileGameFeedError, ProfileGameFetchError, RecentProfileGameCount},
    review_session_contract::{PlayerId, RetryDirective},
};

use super::{DailyCoachingLifecycle, DailyCoachingTickError};

impl DailyCoachingLifecycle {
    pub(in crate::daily_coaching) async fn check_profile(
        &self,
        player_id: &PlayerId,
        provider: DailyCoachingProvider,
        expected_identity_username: &str,
        now: DateTime<Utc>,
    ) -> Result<ProfileCheckResult, DailyCoachingTickError> {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        let state = self.state_store.bind_player(&owner_key, player_id).await?;
        let Some(connection) = state.connection_for_identity(provider, expected_identity_username)
        else {
            return Ok(ProfileCheckResult::Stale);
        };
        let result = self
            .profile_feed
            .latest(
                connection.canonical_url(),
                RecentProfileGameCount::try_from(1)
                    .expect("one is a valid recent profile Game count"),
            )
            .await;
        let (observation, outcome) = match result {
            Ok(_) => (
                ProfileHealthObservation::Reachable,
                ProfileCheckResult::Reachable,
            ),
            Err(ProfileGameFeedError::Fetch(ProfileGameFetchError::Status {
                code: 404, ..
            })) => (
                ProfileHealthObservation::ProfileUnavailable,
                ProfileCheckResult::ProfileUnavailable,
            ),
            Err(error) => {
                return Ok(ProfileCheckResult::ProviderUnavailable(
                    profile_feed_retry_directive(&error),
                ));
            }
        };
        let Some(state) = self
            .state_store
            .observe_profile_health(
                &owner_key,
                provider,
                expected_identity_username,
                observation,
                now,
            )
            .await?
        else {
            return Ok(ProfileCheckResult::Stale);
        };
        if matches!(outcome, ProfileCheckResult::ProfileUnavailable) {
            let notice = state
                .profile_unavailable_notice(provider, expected_identity_username)
                .ok_or(DailyCoachingTickError::InvalidState)?;
            self.operator
                .record_profile_unavailable(
                    state
                        .player_id()
                        .ok_or(DailyCoachingTickError::InvalidState)?,
                    state.owner_key(),
                    &notice,
                )
                .await
                .map_err(|_| DailyCoachingTickError::Operator)?;
            self.redrive_profile_unavailable_email(&state, now).await?;
        }
        if matches!(outcome, ProfileCheckResult::Reachable) {
            self.promote(player_id, now).await?;
        }
        Ok(outcome)
    }

    pub(super) async fn observe_profile_feed<T>(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        connection: &DailyCoachingRunConnection,
        result: Result<T, ProfileGameFeedError>,
        now: DateTime<Utc>,
    ) -> Result<Option<T>, DailyCoachingTickError> {
        let (observation, value) = match result {
            Ok(value) => (ProfileHealthObservation::Reachable, Some(value)),
            Err(ProfileGameFeedError::Fetch(ProfileGameFetchError::Status {
                code: 404, ..
            })) => (ProfileHealthObservation::ProfileUnavailable, None),
            Err(error) => return Err(DailyCoachingTickError::Feed(error)),
        };
        let state = self
            .state_store
            .observe_profile_health(
                owner_key,
                connection.provider(),
                connection.identity_username(),
                observation,
                now,
            )
            .await?;
        if observation == ProfileHealthObservation::ProfileUnavailable {
            if let Some(state) = state {
                let notice = state
                    .profile_unavailable_notice(
                        connection.provider(),
                        connection.identity_username(),
                    )
                    .ok_or(DailyCoachingTickError::InvalidState)?;
                self.operator
                    .record_profile_unavailable(
                        state
                            .player_id()
                            .ok_or(DailyCoachingTickError::InvalidState)?,
                        state.owner_key(),
                        &notice,
                    )
                    .await
                    .map_err(|_| DailyCoachingTickError::Operator)?;
                self.redrive_profile_unavailable_email(&state, now).await?;
            }
        }
        Ok(value)
    }

    pub(super) async fn redrive_profile_unavailable_email(
        &self,
        state: &DailyCoachingDocument,
        now: DateTime<Utc>,
    ) -> Result<(), DailyCoachingTickError> {
        if let Some(player_id) = state.player_id() {
            for notice in state.all_profile_unavailable_notices() {
                self.operator
                    .record_profile_unavailable(player_id, state.owner_key(), &notice)
                    .await
                    .map_err(|_| DailyCoachingTickError::Operator)?;
            }
        }
        if !self.email.is_available() || !state.is_enabled() {
            return Ok(());
        }
        for notice in state.profile_unavailable_notices() {
            let Some(lease) = self
                .email
                .begin_profile_unavailable_delivery(state.owner_key(), &notice, now)
                .await
                .map_err(|_| DailyCoachingTickError::Email)?
            else {
                continue;
            };
            self.email
                .deliver_claimed_profile_unavailable(state.owner_key(), &notice, lease)
                .await
                .map_err(|_| DailyCoachingTickError::Email)?;
        }
        Ok(())
    }
}

pub(in crate::daily_coaching) enum ProfileCheckResult {
    Reachable,
    ProfileUnavailable,
    ProviderUnavailable(RetryDirective),
    Stale,
}

fn profile_feed_retry_directive(error: &ProfileGameFeedError) -> RetryDirective {
    match error {
        ProfileGameFeedError::Fetch(ProfileGameFetchError::Status {
            retry_after_seconds: Some(seconds),
            ..
        }) if *seconds > 0 => RetryDirective::RetryAfter { seconds: *seconds },
        _ => RetryDirective::RetryAllowed,
    }
}
