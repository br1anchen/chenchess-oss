use std::{collections::BTreeMap, sync::Arc};

use chrono::{DateTime, Datelike, TimeDelta, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    beta_access::NormalizedEmail, firestore::FirestoreDatabase, review_session_contract::PlayerId,
};

use super::{
    runs::{
        DailyCoachingRunDocument, DailyCoachingRunOperationalCounts, DailyCoachingRunOutcome,
        DailyCoachingRunStore,
    },
    DailyCoachingDocument,
};

mod render;
mod resend;
mod store;

use render::render_operator_digest;
use resend::ResendOperatorDigestDelivery;
use store::{FirestoreOperatorDigestStore, InMemoryOperatorDigestStore, OperatorDigestStore};

const OPERATOR_EMAIL_ENV: &str = "DAILY_COACHING_OPERATOR_EMAIL";
const RESEND_API_KEY_ENV: &str = "DAILY_COACHING_RESEND_API_KEY";
const BUILT_IN_OPERATOR_EMAIL: &str = "support@example.test";
const OPERATOR_RETENTION: TimeDelta = TimeDelta::days(90);

pub(crate) type OperatorDeliveryFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<String, OperatorDigestError>> + Send + 'a>,
>;

pub(crate) trait OperatorDigestDelivery: Send + Sync {
    fn deliver<'a>(&'a self, request: OperatorDigestEmail) -> OperatorDeliveryFuture<'a>;
}

#[derive(Clone)]
pub(crate) struct OperatorDigestRuntime {
    store: Arc<dyn OperatorDigestStore>,
    delivery: OperatorDeliveryState,
    run_store: Arc<dyn DailyCoachingRunStore>,
    send_hour: u8,
}

#[derive(Clone)]
enum OperatorDeliveryState {
    Disabled,
    Enabled {
        delivery: Arc<dyn OperatorDigestDelivery>,
        recipient: NormalizedEmail,
    },
}

impl OperatorDigestRuntime {
    pub(crate) fn disabled(run_store: Arc<dyn DailyCoachingRunStore>, send_hour: u8) -> Self {
        Self {
            store: Arc::new(InMemoryOperatorDigestStore::default()),
            delivery: OperatorDeliveryState::Disabled,
            run_store,
            send_hour,
        }
    }

    pub(crate) fn configured(
        database: FirestoreDatabase,
        run_store: Arc<dyn DailyCoachingRunStore>,
        send_hour: u8,
    ) -> anyhow::Result<Self> {
        let api_key = match std::env::var(RESEND_API_KEY_ENV) {
            Ok(api_key) => api_key,
            Err(std::env::VarError::NotPresent) => {
                tracing::warn!(
                    category = "configuration",
                    "Daily Coaching Operator Digest is disabled until Resend is provisioned"
                );
                return Ok(Self::disabled(run_store, send_hour));
            }
            Err(std::env::VarError::NotUnicode(_)) => {
                anyhow::bail!("{RESEND_API_KEY_ENV} must contain valid Unicode")
            }
        };
        let recipient = match std::env::var(OPERATOR_EMAIL_ENV) {
            Ok(recipient) => recipient,
            Err(std::env::VarError::NotPresent) => BUILT_IN_OPERATOR_EMAIL.to_string(),
            Err(std::env::VarError::NotUnicode(_)) => {
                anyhow::bail!("{OPERATOR_EMAIL_ENV} must contain valid Unicode")
            }
        };
        let recipient = NormalizedEmail::parse(&recipient)
            .map_err(|_| anyhow::anyhow!("{OPERATOR_EMAIL_ENV} must be a valid email address"))?;
        Ok(Self {
            store: Arc::new(FirestoreOperatorDigestStore::new(database)),
            delivery: OperatorDeliveryState::Enabled {
                delivery: Arc::new(ResendOperatorDigestDelivery::new(api_key)?),
                recipient,
            },
            run_store,
            send_hour,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        run_store: Arc<dyn DailyCoachingRunStore>,
        delivery: Arc<dyn OperatorDigestDelivery>,
        send_hour: u8,
    ) -> Self {
        Self {
            store: Arc::new(InMemoryOperatorDigestStore::default()),
            delivery: OperatorDeliveryState::Enabled {
                delivery,
                recipient: NormalizedEmail::parse("operator@example.com").unwrap(),
            },
            run_store,
            send_hour,
        }
    }

    pub(crate) async fn deliver_due(
        &self,
        states: &[DailyCoachingDocument],
        now: DateTime<Utc>,
    ) -> Result<bool, OperatorDigestError> {
        let OperatorDeliveryState::Enabled {
            delivery,
            recipient,
        } = &self.delivery
        else {
            return Ok(false);
        };
        let window = OperatorDigestWindow::latest_due(now, self.send_hour)?;
        let Some(lease) = self.store.claim(&window, now).await? else {
            return Ok(false);
        };
        let runs = self
            .run_store
            .finished_between(window.starts_at, window.ends_at)
            .await?;
        let profile_unavailable = self
            .store
            .profile_unavailable_between(window.starts_at, window.ends_at)
            .await?;
        let degraded_providers = self
            .store
            .degraded_provider_between(window.starts_at, window.ends_at)
            .await?;
        let report = OperatorDigestReport::build(
            window.clone(),
            runs,
            states,
            profile_unavailable,
            degraded_providers,
        )?;
        let request = OperatorDigestEmail {
            digest_id: window.digest_id.clone(),
            recipient: recipient.clone(),
            rendered: render_operator_digest(&report),
        };
        match delivery.deliver(request).await {
            Ok(provider_message_id) => {
                self.store
                    .finish(&window.digest_id, lease, provider_message_id)
                    .await?;
                Ok(true)
            }
            Err(error) => {
                tracing::warn!(
                    category = "daily_coaching_operator_digest",
                    digest_id = window.digest_id,
                    %error,
                    "Daily Coaching Operator Digest will retry after its claim lease expires"
                );
                Ok(false)
            }
        }
    }

    pub(crate) async fn record_profile_unavailable(
        &self,
        player_id: &PlayerId,
        owner_key: &super::DailyCoachingOwnerKey,
        notice: &super::state::ProfileUnavailableNotice,
    ) -> Result<(), OperatorDigestError> {
        if matches!(&self.delivery, OperatorDeliveryState::Disabled) {
            return Ok(());
        }
        self.store
            .record_profile_unavailable(ProfileUnavailableEvent::new(player_id, owner_key, notice)?)
            .await
    }

    /// Records that a Coaching Digest published without one connected provider's Games.
    pub(crate) async fn record_degraded_provider(
        &self,
        player_id: &PlayerId,
        owner_key: &super::DailyCoachingOwnerKey,
        provider: super::DailyCoachingProvider,
        run_id: &str,
        reason: DegradedProviderReason,
        observed_at: DateTime<Utc>,
    ) -> Result<(), OperatorDigestError> {
        if matches!(&self.delivery, OperatorDeliveryState::Disabled) {
            return Ok(());
        }
        self.store
            .record_degraded_provider(DegradedProviderEvent::new(
                player_id,
                owner_key,
                provider,
                run_id,
                reason,
                observed_at,
            )?)
            .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperatorDigestWindow {
    pub(crate) digest_id: String,
    pub(crate) starts_at: DateTime<Utc>,
    pub(crate) ends_at: DateTime<Utc>,
}

impl OperatorDigestWindow {
    fn latest_due(now: DateTime<Utc>, send_hour: u8) -> Result<Self, OperatorDigestError> {
        let date = if now.hour() < u32::from(send_hour) {
            now.date_naive()
                .pred_opt()
                .ok_or(OperatorDigestError::InvalidState)?
        } else {
            now.date_naive()
        };
        let ends_at = Utc
            .with_ymd_and_hms(
                date.year(),
                date.month(),
                date.day(),
                u32::from(send_hour),
                0,
                0,
            )
            .single()
            .ok_or(OperatorDigestError::InvalidState)?;
        let starts_at = ends_at
            .checked_sub_signed(TimeDelta::hours(24))
            .ok_or(OperatorDigestError::InvalidState)?;
        Ok(Self {
            digest_id: format!("operator-{}-{send_hour:02}", date),
            starts_at,
            ends_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperatorDigestReport {
    pub(crate) window: OperatorDigestWindow,
    pub(crate) outcome_counts: BTreeMap<DailyCoachingRunOutcome, u32>,
    pub(crate) runs: Vec<OperatorRunReport>,
    pub(crate) profile_unavailable: Vec<ProfileUnavailableEvent>,
    pub(crate) degraded_providers: Vec<DegradedProviderEvent>,
    pub(crate) active_connections: u32,
    pub(crate) attempted_games: u32,
    pub(crate) retry_exhausted: u32,
    pub(crate) escalating_categories: Vec<&'static str>,
}

impl OperatorDigestReport {
    fn build(
        window: OperatorDigestWindow,
        runs: Vec<DailyCoachingRunDocument>,
        states: &[DailyCoachingDocument],
        mut profile_unavailable: Vec<ProfileUnavailableEvent>,
        mut degraded_providers: Vec<DegradedProviderEvent>,
    ) -> Result<Self, OperatorDigestError> {
        let mut outcome_counts = BTreeMap::new();
        let mut run_reports = Vec::with_capacity(runs.len());
        let mut attempted_games = 0_u32;
        let mut retry_exhausted = 0_u32;
        let mut repeated_takeover = false;
        for run in runs {
            let outcome = run.outcome().ok_or(OperatorDigestError::InvalidState)?;
            let finished_at = run.finished_at().ok_or(OperatorDigestError::InvalidState)?;
            if finished_at < window.starts_at || finished_at >= window.ends_at {
                return Err(OperatorDigestError::InvalidState);
            }
            let outcome_count = outcome_counts.entry(outcome).or_insert(0_u32);
            *outcome_count = outcome_count
                .checked_add(1)
                .ok_or(OperatorDigestError::InvalidState)?;
            let counts = run.operational_counts();
            attempted_games = attempted_games
                .checked_add(counts.attempted_games)
                .ok_or(OperatorDigestError::InvalidState)?;
            retry_exhausted = retry_exhausted
                .checked_add(counts.retry_exhausted)
                .ok_or(OperatorDigestError::InvalidState)?;
            repeated_takeover |= run.takeover_count() >= 2;
            run_reports.push(OperatorRunReport {
                player_id: run.player_id()?.clone(),
                run_id: run.address().run_id,
                starts_at: run.starts_at(),
                ends_at: run.ends_at(),
                finished_at,
                outcome,
                takeover_count: run.takeover_count(),
                counts,
            });
        }

        let mut active_connections = 0_u32;
        for state in states.iter().filter(|state| state.is_enabled()) {
            active_connections = active_connections
                .checked_add(
                    u32::try_from(state.connections().len())
                        .map_err(|_| OperatorDigestError::InvalidState)?,
                )
                .ok_or(OperatorDigestError::InvalidState)?;
        }
        for event in &profile_unavailable {
            event.validate()?;
            if event.entered_at < window.starts_at || event.entered_at >= window.ends_at {
                return Err(OperatorDigestError::InvalidState);
            }
        }
        run_reports.sort_by(|left, right| {
            (left.finished_at, left.player_id.as_str(), &left.run_id).cmp(&(
                right.finished_at,
                right.player_id.as_str(),
                &right.run_id,
            ))
        });
        profile_unavailable.sort_by(|left, right| {
            (left.entered_at, &left.event_id).cmp(&(right.entered_at, &right.event_id))
        });
        for event in &degraded_providers {
            event.validate()?;
            if event.observed_at < window.starts_at || event.observed_at >= window.ends_at {
                return Err(OperatorDigestError::InvalidState);
            }
        }
        degraded_providers.sort_by(|left, right| {
            (left.observed_at, &left.event_id).cmp(&(right.observed_at, &right.event_id))
        });

        let profile_unavailable_count = u32::try_from(profile_unavailable.len())
            .map_err(|_| OperatorDigestError::InvalidState)?;
        let escalating_categories = escalating_categories(
            &outcome_counts,
            profile_unavailable_count,
            active_connections,
            repeated_takeover,
            attempted_games,
            retry_exhausted,
            !degraded_providers.is_empty(),
        );

        Ok(Self {
            window,
            outcome_counts,
            runs: run_reports,
            profile_unavailable,
            degraded_providers,
            active_connections,
            attempted_games,
            retry_exhausted,
            escalating_categories,
        })
    }

    pub(crate) fn is_escalating(&self) -> bool {
        !self.escalating_categories.is_empty()
    }
}

fn escalating_categories(
    outcome_counts: &BTreeMap<DailyCoachingRunOutcome, u32>,
    profile_unavailable: u32,
    active_connections: u32,
    repeated_takeover: bool,
    attempted_games: u32,
    retry_exhausted: u32,
    degraded_provider: bool,
) -> Vec<&'static str> {
    let mut categories = Vec::new();
    if outcome_counts
        .get(&DailyCoachingRunOutcome::Skipped)
        .copied()
        .unwrap_or_default()
        > 0
    {
        categories.push("daily_coaching_skipped");
    }
    if outcome_counts
        .get(&DailyCoachingRunOutcome::Abandoned)
        .copied()
        .unwrap_or_default()
        > 0
    {
        categories.push("daily_coaching_abandoned");
    }
    if profile_unavailable >= 2_u32.max(active_connections.div_ceil(100)) {
        categories.push("daily_coaching_profile_unavailable");
    }
    if repeated_takeover {
        categories.push("daily_coaching_repeated_takeover");
    }
    if attempted_games > 0 && retry_exhausted.saturating_mul(5) >= attempted_games {
        categories.push("daily_coaching_retry_exhaustion");
    }
    // One dropped provider is enough: the digest still published and looked complete.
    if degraded_provider {
        categories.push("daily_coaching_degraded_provider");
    }
    categories
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperatorRunReport {
    pub(crate) player_id: PlayerId,
    pub(crate) run_id: String,
    pub(crate) starts_at: DateTime<Utc>,
    pub(crate) ends_at: DateTime<Utc>,
    pub(crate) finished_at: DateTime<Utc>,
    pub(crate) outcome: DailyCoachingRunOutcome,
    pub(crate) takeover_count: u32,
    pub(crate) counts: DailyCoachingRunOperationalCounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProfileUnavailableEvent {
    pub(crate) event_id: String,
    pub(crate) player_id: PlayerId,
    pub(crate) entered_at: DateTime<Utc>,
    pub(crate) purge_at: DateTime<Utc>,
}

impl ProfileUnavailableEvent {
    fn new(
        player_id: &PlayerId,
        owner_key: &super::DailyCoachingOwnerKey,
        notice: &super::state::ProfileUnavailableNotice,
    ) -> Result<Self, OperatorDigestError> {
        let provider = match notice.provider {
            super::DailyCoachingProvider::Lichess => "lichess",
            super::DailyCoachingProvider::ChessCom => "chess-com",
        };
        let event = Self {
            event_id: format!(
                "profile-unavailable-{}-{provider}-{}",
                owner_key.as_str(),
                notice.epoch
            ),
            player_id: player_id.clone(),
            entered_at: notice.entered_at,
            purge_at: notice
                .entered_at
                .checked_add_signed(OPERATOR_RETENTION)
                .ok_or(OperatorDigestError::InvalidState)?,
        };
        event.validate()?;
        Ok(event)
    }

    fn validate(&self) -> Result<(), OperatorDigestError> {
        if !self.event_id.starts_with("profile-unavailable-")
            || self.event_id.len() > 160
            || !self.event_id.is_ascii()
            || !self
                .event_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || self.entered_at.timestamp_millis() <= 0
            || self
                .entered_at
                .checked_add_signed(OPERATOR_RETENTION)
                .is_none_or(|expected| expected != self.purge_at)
        {
            Err(OperatorDigestError::InvalidState)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// One provider whose Games were dropped from an otherwise published Coaching Digest.
pub(crate) struct DegradedProviderEvent {
    pub(crate) event_id: String,
    pub(crate) player_id: PlayerId,
    pub(crate) provider: super::DailyCoachingProvider,
    pub(crate) run_id: String,
    pub(crate) reason: DegradedProviderReason,
    pub(crate) observed_at: DateTime<Utc>,
    pub(crate) purge_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
/// Why a provider dropped out of a Run, as a closed set.
///
/// Provider error text is deliberately not carried here: `reqwest` failures render the requested
/// URL, which contains the Player's provider handle, and an unexpected content type echoes a
/// provider-controlled header. Both would reach the operator email, which otherwise carries no
/// handles. The correlated Railway logs hold the detail.
pub(crate) enum DegradedProviderReason {
    InvalidProfileUrl,
    ProviderStatus,
    ProviderTimeout,
    ProviderUnreachable,
    ProviderTransport,
    ResponseTooLarge,
    ClientMisconfigured,
    UnexpectedContentType,
    MalformedResponse,
    InvalidWindow,
}

impl DegradedProviderEvent {
    pub(crate) fn new(
        player_id: &PlayerId,
        owner_key: &super::DailyCoachingOwnerKey,
        provider: super::DailyCoachingProvider,
        run_id: &str,
        reason: DegradedProviderReason,
        observed_at: DateTime<Utc>,
    ) -> Result<Self, OperatorDigestError> {
        let slug = match provider {
            super::DailyCoachingProvider::Lichess => "lichess",
            super::DailyCoachingProvider::ChessCom => "chessCom",
        };
        let event = Self {
            // One event per Player, provider, and Run window: a retried Run collapses onto it.
            event_id: format!("degraded-provider-{}-{slug}-{run_id}", owner_key.as_str()),
            player_id: player_id.clone(),
            provider,
            run_id: run_id.to_string(),
            reason,
            observed_at,
            purge_at: observed_at
                .checked_add_signed(OPERATOR_RETENTION)
                .ok_or(OperatorDigestError::InvalidState)?,
        };
        event.validate()?;
        Ok(event)
    }

    fn validate(&self) -> Result<(), OperatorDigestError> {
        if !self.event_id.starts_with("degraded-provider-")
            || self.event_id.len() > 200
            || !self.event_id.is_ascii()
            || !self
                .event_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || self.run_id.trim().is_empty()
            || self.observed_at.timestamp_millis() <= 0
            || self
                .observed_at
                .checked_add_signed(OPERATOR_RETENTION)
                .is_none_or(|expected| expected != self.purge_at)
        {
            Err(OperatorDigestError::InvalidState)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OperatorDigestEmail {
    pub(crate) digest_id: String,
    pub(crate) recipient: NormalizedEmail,
    pub(crate) rendered: RenderedOperatorDigest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedOperatorDigest {
    pub(crate) subject: String,
    pub(crate) text: String,
    pub(crate) html: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OperatorDigestError {
    #[error(transparent)]
    Run(#[from] super::runs::DailyCoachingRunStoreError),
    #[error("Daily Coaching Operator Digest persistence failed")]
    Store,
    #[error("Daily Coaching Operator Digest provider handoff failed")]
    Delivery,
    #[error("Daily Coaching Operator Digest state is invalid")]
    InvalidState,
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::daily_coaching::{
        configuration::DailyCoachingConfiguration,
        runs::InMemoryDailyCoachingRunStore,
        schedule::DailyWindow,
        state::{DailyCoachingStore, InMemoryDailyCoachingStore, StoredPlayingProfileConnection},
        DailyCoachingOwnerKey, DailyCoachingProvider,
    };

    #[derive(Default)]
    struct RecordingDelivery {
        requests: Mutex<Vec<OperatorDigestEmail>>,
    }

    impl OperatorDigestDelivery for RecordingDelivery {
        fn deliver<'a>(&'a self, request: OperatorDigestEmail) -> OperatorDeliveryFuture<'a> {
            Box::pin(async move {
                self.requests
                    .lock()
                    .expect("recording delivery is not poisoned")
                    .push(request);
                Ok("operator-provider-message-1".to_string())
            })
        }
    }

    #[tokio::test]
    async fn sends_an_empty_heartbeat_once_with_the_verdict_in_the_subject() {
        let state_store = Arc::new(InMemoryDailyCoachingStore::default());
        let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store));
        let delivery = Arc::new(RecordingDelivery::default());
        let runtime = OperatorDigestRuntime::for_test(run_store, delivery.clone(), 8);
        let now = instant("2026-08-12T08:01:00Z");

        assert!(runtime.deliver_due(&[], now).await.unwrap());
        assert!(!runtime.deliver_due(&[], now).await.unwrap());

        let requests = delivery
            .requests
            .lock()
            .expect("recording delivery is not poisoned");
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].rendered.subject,
            "[chenchess] daily coaching 2026-08-12 - OK 0 terminal Runs"
        );
        assert!(requests[0].rendered.text.contains("Published: 0"));
        assert!(!requests[0].rendered.text.contains("in progress"));
    }

    #[tokio::test]
    async fn profile_event_uses_opaque_identity_and_survives_before_delivery() {
        let state_store = Arc::new(InMemoryDailyCoachingStore::default());
        let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store));
        let delivery = Arc::new(RecordingDelivery::default());
        let runtime = OperatorDigestRuntime::for_test(run_store, delivery.clone(), 8);
        let player_id = PlayerId::try_from("player-opaque-1".to_string()).unwrap();
        let owner_key = DailyCoachingOwnerKey::for_player(&player_id);
        let entered_at = instant("2026-08-12T07:00:00Z");
        let notice = super::super::state::ProfileUnavailableNotice {
            provider: DailyCoachingProvider::Lichess,
            identity_username: "secret-provider-name".to_string(),
            epoch: 1,
            entered_at,
        };
        let event = ProfileUnavailableEvent::new(&player_id, &owner_key, &notice).unwrap();
        let stored = serde_json::to_value(&event).unwrap();
        assert_eq!(stored["enteredAt"], "2026-08-12T07:00:00Z");
        assert!(stored.get("purgeAt").is_some());
        assert_eq!(
            serde_json::from_value::<ProfileUnavailableEvent>(stored).unwrap(),
            event
        );

        runtime
            .record_profile_unavailable(&player_id, &owner_key, &notice)
            .await
            .unwrap();
        runtime
            .deliver_due(&[], instant("2026-08-12T08:01:00Z"))
            .await
            .unwrap();

        let requests = delivery
            .requests
            .lock()
            .expect("recording delivery is not poisoned");
        let rendered = &requests[0].rendered;
        assert!(rendered.text.contains("player=player-opaque-1"));
        assert!(!rendered.text.contains("secret-provider-name"));
        assert!(!rendered.text.contains("http"));
        assert!(!rendered.text.contains("@"));
    }

    #[tokio::test]
    async fn a_digest_published_without_one_provider_escalates_to_the_operator() {
        let state_store = Arc::new(InMemoryDailyCoachingStore::default());
        let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store));
        let delivery = Arc::new(RecordingDelivery::default());
        let runtime = OperatorDigestRuntime::for_test(run_store, delivery.clone(), 8);
        let player_id = PlayerId::try_from("player-opaque-3".to_string()).unwrap();
        let owner_key = DailyCoachingOwnerKey::for_player(&player_id);

        runtime
            .record_degraded_provider(
                &player_id,
                &owner_key,
                DailyCoachingProvider::Lichess,
                "daily-2026-08-11",
                DegradedProviderReason::MalformedResponse,
                instant("2026-08-12T01:00:07Z"),
            )
            .await
            .unwrap();
        assert!(runtime
            .deliver_due(&[], instant("2026-08-12T08:01:00Z"))
            .await
            .unwrap());

        let requests = delivery
            .requests
            .lock()
            .expect("recording delivery is not poisoned");
        let rendered = &requests[0].rendered;
        assert!(
            rendered.subject.contains("ALERT"),
            "a silently dropped provider must escalate: {}",
            rendered.subject
        );
        assert!(rendered
            .text
            .contains("Escalating category: daily_coaching_degraded_provider"));
        assert!(rendered.text.contains("provider=Lichess"));
        assert!(rendered.text.contains("reason=MalformedResponse"));
        assert!(rendered.text.contains("run=daily-2026-08-11"));
        assert!(rendered.text.contains("player=player-opaque-3"));
        // The operator digest never carries provider handles or Player contact details.
        assert!(!rendered.text.contains('@'));
    }

    #[tokio::test]
    async fn repeated_degraded_windows_collapse_onto_one_event() {
        let state_store = Arc::new(InMemoryDailyCoachingStore::default());
        let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store));
        let delivery = Arc::new(RecordingDelivery::default());
        let runtime = OperatorDigestRuntime::for_test(run_store, delivery.clone(), 8);
        let player_id = PlayerId::try_from("player-opaque-4".to_string()).unwrap();
        let owner_key = DailyCoachingOwnerKey::for_player(&player_id);

        for observed_at in ["2026-08-12T01:00:07Z", "2026-08-12T02:30:00Z"] {
            runtime
                .record_degraded_provider(
                    &player_id,
                    &owner_key,
                    DailyCoachingProvider::Lichess,
                    "daily-2026-08-11",
                    DegradedProviderReason::MalformedResponse,
                    instant(observed_at),
                )
                .await
                .unwrap();
        }
        runtime
            .deliver_due(&[], instant("2026-08-12T08:01:00Z"))
            .await
            .unwrap();

        let requests = delivery
            .requests
            .lock()
            .expect("recording delivery is not poisoned");
        assert_eq!(
            requests[0]
                .rendered
                .text
                .matches("provider=Lichess")
                .count(),
            1,
            "one Run window reports one degraded provider however often it retries"
        );
    }

    #[tokio::test]
    async fn skipped_run_escalates_the_exact_fixed_window() {
        let state_store = Arc::new(InMemoryDailyCoachingStore::default());
        let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
        let delivery = Arc::new(RecordingDelivery::default());
        let runtime = OperatorDigestRuntime::for_test(run_store.clone(), delivery.clone(), 8);
        let player_id = PlayerId::try_from("player-opaque-2".to_string()).unwrap();
        let owner_key = DailyCoachingOwnerKey::for_player(&player_id);
        let connected_at = instant("2026-08-10T12:00:00Z");
        state_store
            .connect_profile(
                &owner_key,
                &player_id,
                StoredPlayingProfileConnection::test(
                    DailyCoachingProvider::Lichess,
                    "ProviderName",
                ),
                "UTC".to_string(),
                connected_at,
            )
            .await
            .unwrap();
        let state = state_store.read(&owner_key).await.unwrap();
        let window = DailyWindow::resolve(
            &owner_key,
            chrono_tz::UTC,
            state.next_daily_window().unwrap(),
            &DailyCoachingConfiguration::standard(),
        )
        .unwrap();
        run_store
            .create(
                DailyCoachingRunDocument::skipped(&state, &window, window.deadline, 90).unwrap(),
            )
            .await
            .unwrap();

        runtime
            .deliver_due(&[state], instant("2026-08-12T08:01:00Z"))
            .await
            .unwrap();

        let requests = delivery
            .requests
            .lock()
            .expect("recording delivery is not poisoned");
        let rendered = &requests[0].rendered;
        assert_eq!(
            rendered.subject,
            "[chenchess] daily coaching 2026-08-12 - ALERT 1 terminal Runs"
        );
        assert!(rendered.text.contains("Skipped: 1"));
        assert!(rendered
            .text
            .contains("Escalating category: daily_coaching_skipped"));
        assert!(rendered.text.contains("player=player-opaque-2"));
        assert!(!rendered.text.contains("ProviderName"));
    }

    #[test]
    fn escalation_policy_uses_the_resolved_operational_thresholds() {
        let outcomes = BTreeMap::from([
            (DailyCoachingRunOutcome::Skipped, 1),
            (DailyCoachingRunOutcome::Abandoned, 1),
        ]);

        assert_eq!(
            escalating_categories(&outcomes, 3, 201, true, 10, 2, false),
            vec![
                "daily_coaching_skipped",
                "daily_coaching_abandoned",
                "daily_coaching_profile_unavailable",
                "daily_coaching_repeated_takeover",
                "daily_coaching_retry_exhaustion",
            ]
        );
        assert!(escalating_categories(&BTreeMap::new(), 1, 100, false, 10, 1, false).is_empty());
        assert!(escalating_categories(&BTreeMap::new(), 2, 201, false, 0, 0, false).is_empty());
    }

    #[test]
    fn schedule_uses_the_latest_completed_fixed_utc_window() {
        let window = OperatorDigestWindow::latest_due(instant("2026-08-12T07:59:59Z"), 8).unwrap();

        assert_eq!(window.digest_id, "operator-2026-08-11-08");
        assert_eq!(window.starts_at, instant("2026-08-10T08:00:00Z"));
        assert_eq!(window.ends_at, instant("2026-08-11T08:00:00Z"));
    }

    fn instant(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }
}
