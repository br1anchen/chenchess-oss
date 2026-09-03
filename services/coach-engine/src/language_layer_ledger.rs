//! Language Layer admission and the spend ledger that records every attempt.
//!
//! #371 and ADR 0050 /
//! ADR 0051: admission runs before any provider request. A denial spends
//! nothing. The operational record and the spend counters commit in the same
//! transaction. Authoring stays on #372 / #374.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Utc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::evaluation_fingerprint::CaptureOutcome;
use crate::evaluation_fingerprint::EvaluationStepObservation;
use crate::evaluation_fingerprint::PinVerificationVerdict;
use crate::language_layer_provider::{CompletionAttempt, CompletionOutcome, RateLimitDelaySource};
use crate::operating_limits::{COACH_TURN_DEADLINE_SECONDS, HOST_TURN_DEADLINE_SECONDS};
use crate::pin_verification::{PinMismatchReport, PinVerificationCause, PinVerificationJudgement};
use crate::retry_after::MAX_HONORED_RETRY_AFTER;
use crate::review_session_contract::PlayerId;

mod firestore;
pub(crate) use firestore::FirestoreLanguageLayerLedger;

/// One hosted call and its single retry, in micros of a dollar.
pub const OPERATION_CEILING_MICROS: i64 = 5_000;
/// Worst-case billed cost of one operation, in micros of a dollar.
pub const OPERATION_WORST_CASE_MICROS: i64 = 4_500;
/// Model calls in one HostTurn, including the corrective retry.
pub const HOST_TURN_MAX_STEPS: u8 = 3;
/// HostTurn envelope reserved before step 1: three counted calls plus one
/// per-turn transport retry, each at the operation ceiling.
///
/// The Review Session ceiling stays [`REVIEW_SESSION_CEILING_MICROS`], so a
/// turn can start only while committed spend is at most 5_000. Unused
/// reservation is released at end.
pub const HOST_TURN_ENVELOPE_MICROS: i64 =
    (HOST_TURN_MAX_STEPS as i64 + 1) * OPERATION_CEILING_MICROS;
/// One Review Session, in micros of a dollar.
pub const REVIEW_SESSION_CEILING_MICROS: i64 = 25_000;
/// One Player over a rolling 30 calendar days, in micros of a dollar.
pub const PLAYER_CEILING_MICROS: i64 = 500_000;
/// Global calendar-month ceiling, in micros of a dollar.
pub const GLOBAL_CEILING_MICROS: i64 = 25_000_000;

/// Player-visible first-open wait. Sized to that wait, not to
/// [`crate::operating_limits::COACH_TURN_DEADLINE_SECONDS`].
pub const COMMENT_AUTHORING_DEADLINE_SECONDS: u64 = 10;

/// Conservative admission, timeout, and backoff values. None of these is a
/// fingerprint axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageLayerAdmissionConfig {
    pub max_concurrent_provider_calls: usize,
    pub provider_attempt_timeout_ceiling: Duration,
    pub comment_authoring_deadline: Duration,
    pub coach_turn_authoring_deadline: Duration,
    pub host_turn_authoring_deadline: Duration,
    pub rate_shaped_retry_delay: Duration,
}

impl LanguageLayerAdmissionConfig {
    pub fn conservative_defaults() -> Self {
        Self {
            max_concurrent_provider_calls: 4,
            provider_attempt_timeout_ceiling: Duration::from_secs(20),
            comment_authoring_deadline: Duration::from_secs(COMMENT_AUTHORING_DEADLINE_SECONDS),
            coach_turn_authoring_deadline: Duration::from_secs(COACH_TURN_DEADLINE_SECONDS),
            host_turn_authoring_deadline: Duration::from_secs(HOST_TURN_DEADLINE_SECONDS),
            rate_shaped_retry_delay: Duration::from_millis(1000),
        }
    }

    /// Concurrent admissions can overshoot a ceiling by at most
    /// `(cap − 1) ×` [`OPERATION_CEILING_MICROS`].
    pub fn concurrent_overshoot_bound_micros(&self) -> i64 {
        (self.max_concurrent_provider_calls as i64).saturating_sub(1) * OPERATION_CEILING_MICROS
    }

    /// Per-attempt provider wait, capped by the named timeout ceiling.
    pub fn slot_wait(&self, remaining_deadline: Duration) -> Duration {
        remaining_deadline.min(self.provider_attempt_timeout_ceiling)
    }

    pub fn retry_delay(&self) -> Duration {
        self.rate_shaped_retry_delay
    }

    /// Advertised provider delay, or the 1 s floor when the signal is missing
    /// or already elapsed. Consecutive 429 escalation is applied by
    /// [`ProviderConcurrency`], not here.
    pub(crate) fn rate_limit_cooldown(&self, retry_after: Option<Duration>) -> Duration {
        match retry_after {
            Some(wait) if !wait.is_zero() => wait,
            _ => self.rate_shaped_retry_delay,
        }
    }
}

impl Default for LanguageLayerAdmissionConfig {
    fn default() -> Self {
        Self::conservative_defaults()
    }
}

/// The hosted Language Layer tasks of the Task Contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedTask {
    Comment,
    HostTurn,
}

impl HostedTask {
    pub fn fallback(self) -> HostedFallback {
        match self {
            Self::Comment => HostedFallback::SafeRendering,
            Self::HostTurn => HostedFallback::Unavailable,
        }
    }
}

/// Asymmetric degradation when admission denies or the provider fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedFallback {
    SafeRendering,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetDecision {
    Admitted,
    Denied,
    /// A provider cooldown denied the call. Not a budget event.
    ProviderCooldown,
}

impl BudgetDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Denied => "denied",
            Self::ProviderCooldown => "providerCooldown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialReason {
    ReviewSessionCeiling,
    PlayerCeiling,
    GlobalCeiling,
    ConcurrencyUnavailable,
    /// The provider asked this replica to wait. Distinct from the withdrawn
    /// availability kill switch.
    ProviderCooldown,
}

impl DenialReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReviewSessionCeiling => "reviewSessionCeiling",
            Self::PlayerCeiling => "playerCeiling",
            Self::GlobalCeiling => "globalCeiling",
            Self::ConcurrencyUnavailable => "concurrencyUnavailable",
            Self::ProviderCooldown => "providerCooldown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptErrorClass {
    Cancelled,
    TimedOut,
    DeadlineExhausted,
    RateLimited,
}

impl AttemptErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timedOut",
            Self::DeadlineExhausted => "deadlineExhausted",
            Self::RateLimited => "rateLimited",
        }
    }

    pub fn from_completion(outcome: &CompletionOutcome) -> Option<Self> {
        match outcome {
            CompletionOutcome::TimedOut => Some(Self::TimedOut),
            CompletionOutcome::DeadlineExhausted => Some(Self::DeadlineExhausted),
            CompletionOutcome::RateLimited { .. } => Some(Self::RateLimited),
            _ => None,
        }
    }
}

/// One settled attempt, including denials and billed failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageLayerOperationalRecord {
    pub request_id: String,
    pub player_id: PlayerId,
    pub settled_at: DateTime<Utc>,
    pub latency: Duration,
    pub cost_micros: i64,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub budget_decision: BudgetDecision,
    pub denial_reason: Option<DenialReason>,
    pub error_class: Option<AttemptErrorClass>,
    pub pin_verification: PinVerificationVerdict,
    pub pin_cause: Option<PinVerificationCause>,
    pub fingerprint_digest: String,
    pub capture_outcome: Option<CaptureOutcome>,
    /// Honoured provider cooldown on a 429 attempt or a cooldown denial.
    pub provider_cooldown: Option<Duration>,
    /// HostTurn per-step observations. Empty for Comment.
    pub steps: Vec<EvaluationStepObservation>,
}

/// Process-local Review Session spend. A session that loses residency is a
/// new session and re-meters from zero.
#[derive(Debug)]
pub struct ReviewSessionSpend {
    state: Mutex<SpendState>,
}

#[derive(Debug, Default)]
struct SpendState {
    committed: i64,
    reserved: i64,
}

impl ReviewSessionSpend {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SpendState::default()),
        }
    }

    /// Committed billed spend plus any outstanding HostTurn reservation.
    pub fn spent_micros(&self) -> i64 {
        let state = self.lock();
        state.committed.saturating_add(state.reserved)
    }

    /// Billed spend only. HostTurn reservations are not included.
    pub fn committed_micros(&self) -> i64 {
        self.lock().committed
    }

    /// Outstanding HostTurn reservation. Billed spend is not included.
    pub fn reserved_micros(&self) -> i64 {
        self.lock().reserved
    }

    /// Saturating add. Negative values are ignored. Consumes reservation first.
    pub fn record(&self, cost_micros: i64) {
        if cost_micros <= 0 {
            return;
        }
        let mut state = self.lock();
        let from_reserve = state.reserved.min(cost_micros);
        state.reserved -= from_reserve;
        state.committed = state.committed.saturating_add(cost_micros);
    }

    /// Reserve micros against the Review Session ceiling. Returns false and
    /// spends nothing when the envelope does not fit.
    pub fn try_reserve(&self, micros: i64) -> bool {
        if micros <= 0 {
            return true;
        }
        let mut state = self.lock();
        if state
            .committed
            .saturating_add(state.reserved)
            .saturating_add(micros)
            > REVIEW_SESSION_CEILING_MICROS
        {
            return false;
        }
        state.reserved = state.reserved.saturating_add(micros);
        true
    }

    /// Release up to `micros` of outstanding reservation. Saturates at zero.
    pub fn release_reservation(&self, micros: i64) {
        if micros <= 0 {
            return;
        }
        let mut state = self.lock();
        let released = state.reserved.min(micros);
        state.reserved -= released;
    }

    fn lock(&self) -> MutexGuard<'_, SpendState> {
        self.state
            .lock()
            .expect("review session spend lock must not be poisoned")
    }
}

impl Default for ReviewSessionSpend {
    fn default() -> Self {
        Self::new()
    }
}

pub trait CeilingAlert: Send + Sync {
    fn global_ceiling_tripped(&self);
}

pub struct TracingCeilingAlert;

impl CeilingAlert for TracingCeilingAlert {
    fn global_ceiling_tripped(&self) {
        tracing::error!(
            alert_class = "language-layer-global-ceiling-trip",
            "language layer global monthly ceiling reached; admission denies until the month turns or the ceiling is raised"
        );
    }
}

pub trait PinMismatchAlert: Send + Sync {
    fn pin_mismatched(&self, report: &PinMismatchReport);
}

pub struct TracingPinMismatchAlert;

impl PinMismatchAlert for TracingPinMismatchAlert {
    fn pin_mismatched(&self, report: &PinMismatchReport) {
        tracing::error!(
            alert_class = "language-layer-pin-mismatch",
            pinned_model = report.pinned_model.as_str(),
            pinned_provider = report.pinned_provider_family.as_str(),
            observed_model = report.observed_permaslug.as_deref(),
            observed_provider = report.observed_provider.as_deref(),
            observed_provider_family = report.observed_provider_family.as_deref(),
            served_endpoint = report.served_endpoint.as_deref(),
            served_region = report.served_region.as_deref(),
            routed_service_tier = report.routed_service_tier.as_deref(),
            "language layer pin mismatch; observed route recorded"
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    #[error("language layer ledger is unavailable")]
    Unavailable,
}

pub type LedgerFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, LedgerError>> + Send + 'a>>;

pub trait LanguageLayerLedger: Send + Sync {
    fn player_rolling_30_day(
        &self,
        player_id: &PlayerId,
        as_of: DateTime<Utc>,
    ) -> LedgerFuture<'_, i64>;

    fn global_calendar_month(&self, as_of: DateTime<Utc>) -> LedgerFuture<'_, i64>;

    fn settle(&self, record: LanguageLayerOperationalRecord) -> LedgerFuture<'_, ()>;

    fn records(&self) -> LedgerFuture<'_, Vec<LanguageLayerOperationalRecord>>;
}

struct MemoryState {
    player_days: HashMap<String, HashMap<String, i64>>,
    global_months: HashMap<String, i64>,
    records: Vec<LanguageLayerOperationalRecord>,
}

/// In-memory ledger. One mutex is the transaction: counters and the
/// operational record commit together.
pub struct MemoryLanguageLayerLedger {
    state: Mutex<MemoryState>,
}

impl MemoryLanguageLayerLedger {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MemoryState {
                player_days: HashMap::new(),
                global_months: HashMap::new(),
                records: Vec::new(),
            }),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, MemoryState>, LedgerError> {
        self.state.lock().map_err(|_| LedgerError::Unavailable)
    }
}

impl Default for MemoryLanguageLayerLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageLayerLedger for MemoryLanguageLayerLedger {
    fn player_rolling_30_day(
        &self,
        player_id: &PlayerId,
        as_of: DateTime<Utc>,
    ) -> LedgerFuture<'_, i64> {
        let player_id = player_id.clone();
        Box::pin(async move {
            let state = self.lock()?;
            let days = state.player_days.get(player_id.as_str());
            let Some(days) = days else {
                return Ok(0);
            };
            let end = as_of.date_naive();
            let mut total = 0i64;
            for offset in 0..30 {
                let Some(day) = end.checked_sub_signed(TimeDelta::days(offset)) else {
                    break;
                };
                if let Some(spent) = days.get(&day.format("%Y-%m-%d").to_string()) {
                    total = total.saturating_add(*spent);
                }
            }
            Ok(total)
        })
    }

    fn global_calendar_month(&self, as_of: DateTime<Utc>) -> LedgerFuture<'_, i64> {
        Box::pin(async move {
            let state = self.lock()?;
            Ok(state
                .global_months
                .get(&month_key(as_of))
                .copied()
                .unwrap_or(0))
        })
    }

    fn settle(&self, record: LanguageLayerOperationalRecord) -> LedgerFuture<'_, ()> {
        Box::pin(async move {
            let mut state = self.lock()?;
            if record.budget_decision == BudgetDecision::Admitted && record.cost_micros > 0 {
                let player = record.player_id.as_str().to_string();
                let day = day_key(record.settled_at);
                let month = month_key(record.settled_at);
                let day_spend = state
                    .player_days
                    .entry(player)
                    .or_default()
                    .entry(day)
                    .or_insert(0);
                *day_spend = day_spend.saturating_add(record.cost_micros);
                let month_spend = state.global_months.entry(month).or_insert(0);
                *month_spend = month_spend.saturating_add(record.cost_micros);
            }
            state.records.push(record);
            Ok(())
        })
    }

    fn records(&self) -> LedgerFuture<'_, Vec<LanguageLayerOperationalRecord>> {
        Box::pin(async move { Ok(self.lock()?.records.clone()) })
    }
}

/// Engine-wide in-flight cap and the process-local provider cooldown. A
/// caller waits for a slot only within the remaining task deadline. A 429
/// from any Player suppresses hosted attempts for every Player on this
/// replica until the advertised window ends. Consecutive 429s double the
/// floor up to [`MAX_HONORED_RETRY_AFTER`].
pub struct ProviderConcurrency {
    slots: std::sync::Arc<Semaphore>,
    state: Mutex<CooldownState>,
}

struct CooldownState {
    until: Option<Instant>,
    consecutive_429s: u32,
    honoured: Option<Duration>,
}

impl ProviderConcurrency {
    pub fn new(max_concurrent_provider_calls: usize) -> Self {
        Self {
            slots: std::sync::Arc::new(Semaphore::new(max_concurrent_provider_calls)),
            state: Mutex::new(CooldownState {
                until: None,
                consecutive_429s: 0,
                honoured: None,
            }),
        }
    }

    pub async fn acquire(
        &self,
        remaining_deadline: Duration,
    ) -> Result<OwnedSemaphorePermit, DenialReason> {
        match tokio::time::timeout(remaining_deadline, self.slots.clone().acquire_owned()).await {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(_)) | Err(_) => Err(DenialReason::ConcurrencyUnavailable),
        }
    }

    pub fn cooldown_remaining(&self) -> Option<Duration> {
        let state = self.lock_state();
        state
            .until
            .and_then(|deadline| deadline.checked_duration_since(Instant::now()))
            .filter(|wait| !wait.is_zero())
    }

    pub fn honoured_cooldown(&self) -> Option<Duration> {
        self.lock_state().honoured
    }

    pub fn honor_rate_limit(
        &self,
        retry_after: Option<Duration>,
        source: RateLimitDelaySource,
        config: &LanguageLayerAdmissionConfig,
    ) -> Duration {
        let mut state = self.lock_state();
        state.consecutive_429s = state.consecutive_429s.saturating_add(1);
        let escalated_floor =
            escalated_rate_limit_floor(config.rate_shaped_retry_delay, state.consecutive_429s);
        let advertised = config.rate_limit_cooldown(retry_after);
        let wait = advertised.max(escalated_floor).min(MAX_HONORED_RETRY_AFTER);
        let logged_source = if retry_after.is_some_and(|wait| !wait.is_zero()) {
            source
        } else {
            RateLimitDelaySource::Unspecified
        };
        tracing::warn!(
            honoured_ms = wait.as_millis() as u64,
            source = logged_source.as_str(),
            consecutive_429s = state.consecutive_429s,
            "language layer provider cooldown opened"
        );
        state.honoured = Some(wait);
        if let Some(deadline) = Instant::now().checked_add(wait) {
            match state.until {
                Some(existing) if existing >= deadline => {}
                _ => state.until = Some(deadline),
            }
        }
        wait
    }

    pub fn note_non_rate_limited(&self) {
        let mut state = self.lock_state();
        state.consecutive_429s = 0;
        if state
            .until
            .and_then(|deadline| deadline.checked_duration_since(Instant::now()))
            .filter(|wait| !wait.is_zero())
            .is_none()
        {
            state.until = None;
            state.honoured = None;
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, CooldownState> {
        self.state
            .lock()
            .expect("provider cooldown lock must not be poisoned")
    }
}

fn escalated_rate_limit_floor(floor: Duration, consecutive_429s: u32) -> Duration {
    let shift = consecutive_429s.saturating_sub(1).min(31);
    floor
        .checked_mul(1u32 << shift)
        .unwrap_or(MAX_HONORED_RETRY_AFTER)
        .min(MAX_HONORED_RETRY_AFTER)
}

#[derive(Clone, Copy)]
pub struct AdmissionRequest<'a> {
    pub player_id: &'a PlayerId,
    pub session: &'a ReviewSessionSpend,
    pub remaining_deadline: Duration,
    pub as_of: DateTime<Utc>,
}

/// `Admitted` holds the in-flight provider slot until it is dropped.
pub enum Admission {
    Admitted(OwnedSemaphorePermit),
    Denied(DenialReason),
}

/// Envelope reservation against the Review Session, Player, and global
/// ceilings. Denial spends nothing and reserves nothing.
pub enum HostTurnEnvelopeAdmission {
    Admitted(HostTurnEnvelope),
    Denied(DenialReason),
}

impl Admission {
    pub fn is_admitted(&self) -> bool {
        matches!(self, Self::Admitted(_))
    }
}

impl std::fmt::Debug for Admission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admitted(_) => f.write_str("Admitted"),
            Self::Denied(reason) => f.debug_tuple("Denied").field(reason).finish(),
        }
    }
}

impl std::fmt::Debug for HostTurnEnvelopeAdmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admitted(_) => f.write_str("Admitted"),
            Self::Denied(reason) => f.debug_tuple("Denied").field(reason).finish(),
        }
    }
}

pub struct AttemptContext {
    pub player_id: PlayerId,
    pub task: HostedTask,
    pub remaining_deadline: Duration,
    pub as_of: DateTime<Utc>,
    pub fingerprint_digest: String,
    pub cancelled: bool,
    /// Set after the provider has been entered. Bills; stamps Cancelled.
    pub in_flight_cancelled: bool,
    pub pin: PinVerificationJudgement,
}

#[derive(Debug, Clone)]
pub enum HostedAttemptOutcome {
    Settled {
        record: LanguageLayerOperationalRecord,
        attempt: Option<Box<CompletionAttempt>>,
    },
    Denied {
        reason: DenialReason,
        fallback: HostedFallback,
        record: LanguageLayerOperationalRecord,
    },
}

/// Next Language Layer request id. The suffix is a UUID so a process restart
/// cannot reuse `ll-1` and no-op settle against an earlier generation.
pub fn next_request_id() -> String {
    format!("ll-{}", Uuid::new_v4().simple())
}

pub fn cost_micros_from_dollars(cost: Option<f64>) -> i64 {
    let Some(dollars) = cost.filter(|value| value.is_finite()) else {
        return 0;
    };
    let micros = (dollars * 1_000_000.0).round();
    if micros <= 0.0 {
        0
    } else if micros >= i64::MAX as f64 {
        i64::MAX
    } else {
        micros as i64
    }
}

/// Admission before any provider request: ADR 0050 ceilings, then the
/// provider cooldown, then a concurrency slot. Counter reads use only the
/// ledger methods. An exhausted budget is recorded as the budget reason
/// even while a cooldown is open; the cooldown still prevents the call.
pub async fn admit(
    request: &AdmissionRequest<'_>,
    config: &LanguageLayerAdmissionConfig,
    ledger: &(impl LanguageLayerLedger + ?Sized),
    concurrency: &ProviderConcurrency,
) -> Result<Admission, LedgerError> {
    if request
        .session
        .spent_micros()
        .saturating_add(OPERATION_WORST_CASE_MICROS)
        > REVIEW_SESSION_CEILING_MICROS
    {
        return Ok(Admission::Denied(DenialReason::ReviewSessionCeiling));
    }
    let player_spent = ledger
        .player_rolling_30_day(request.player_id, request.as_of)
        .await?;
    if player_spent.saturating_add(OPERATION_WORST_CASE_MICROS) > PLAYER_CEILING_MICROS {
        return Ok(Admission::Denied(DenialReason::PlayerCeiling));
    }
    let global_spent = ledger.global_calendar_month(request.as_of).await?;
    if global_spent.saturating_add(OPERATION_WORST_CASE_MICROS) > GLOBAL_CEILING_MICROS {
        return Ok(Admission::Denied(DenialReason::GlobalCeiling));
    }
    if concurrency.cooldown_remaining().is_some() {
        return Ok(Admission::Denied(DenialReason::ProviderCooldown));
    }
    match concurrency
        .acquire(config.slot_wait(request.remaining_deadline))
        .await
    {
        Ok(permit) => Ok(Admission::Admitted(permit)),
        Err(reason) => Ok(Admission::Denied(reason)),
    }
}

/// Reserve the HostTurn envelope against the Review Session, Player, and
/// global ceilings. Denial spends nothing and reserves nothing.
pub async fn admit_host_turn_envelope(
    request: &AdmissionRequest<'_>,
    session: Arc<ReviewSessionSpend>,
    ledger: &(impl LanguageLayerLedger + ?Sized),
    concurrency: &ProviderConcurrency,
) -> Result<HostTurnEnvelopeAdmission, LedgerError> {
    if !session.try_reserve(HOST_TURN_ENVELOPE_MICROS) {
        return Ok(HostTurnEnvelopeAdmission::Denied(
            DenialReason::ReviewSessionCeiling,
        ));
    }
    let player_spent = ledger
        .player_rolling_30_day(request.player_id, request.as_of)
        .await?;
    if player_spent.saturating_add(HOST_TURN_ENVELOPE_MICROS) > PLAYER_CEILING_MICROS {
        session.release_reservation(HOST_TURN_ENVELOPE_MICROS);
        return Ok(HostTurnEnvelopeAdmission::Denied(
            DenialReason::PlayerCeiling,
        ));
    }
    let global_spent = ledger.global_calendar_month(request.as_of).await?;
    if global_spent.saturating_add(HOST_TURN_ENVELOPE_MICROS) > GLOBAL_CEILING_MICROS {
        session.release_reservation(HOST_TURN_ENVELOPE_MICROS);
        return Ok(HostTurnEnvelopeAdmission::Denied(
            DenialReason::GlobalCeiling,
        ));
    }
    if concurrency.cooldown_remaining().is_some() {
        session.release_reservation(HOST_TURN_ENVELOPE_MICROS);
        return Ok(HostTurnEnvelopeAdmission::Denied(
            DenialReason::ProviderCooldown,
        ));
    }
    Ok(HostTurnEnvelopeAdmission::Admitted(HostTurnEnvelope {
        reserved: HOST_TURN_ENVELOPE_MICROS,
        session,
    }))
}

/// Concurrency and remaining Player/global room for one HostTurn step after
/// the envelope is already reserved. Leftover reserved micros must still
/// cover one worst-case attempt; committed spend still cannot pass the
/// Review Session ceiling.
pub async fn admit_host_turn_step(
    request: &AdmissionRequest<'_>,
    config: &LanguageLayerAdmissionConfig,
    ledger: &(impl LanguageLayerLedger + ?Sized),
    concurrency: &ProviderConcurrency,
) -> Result<Admission, LedgerError> {
    if request.session.reserved_micros() < OPERATION_WORST_CASE_MICROS
        || request
            .session
            .committed_micros()
            .saturating_add(OPERATION_WORST_CASE_MICROS)
            > REVIEW_SESSION_CEILING_MICROS
    {
        return Ok(Admission::Denied(DenialReason::ReviewSessionCeiling));
    }
    let player_spent = ledger
        .player_rolling_30_day(request.player_id, request.as_of)
        .await?;
    if player_spent.saturating_add(OPERATION_WORST_CASE_MICROS) > PLAYER_CEILING_MICROS {
        return Ok(Admission::Denied(DenialReason::PlayerCeiling));
    }
    let global_spent = ledger.global_calendar_month(request.as_of).await?;
    if global_spent.saturating_add(OPERATION_WORST_CASE_MICROS) > GLOBAL_CEILING_MICROS {
        return Ok(Admission::Denied(DenialReason::GlobalCeiling));
    }
    if concurrency.cooldown_remaining().is_some() {
        return Ok(Admission::Denied(DenialReason::ProviderCooldown));
    }
    match concurrency
        .acquire(config.slot_wait(request.remaining_deadline))
        .await
    {
        Ok(permit) => Ok(Admission::Admitted(permit)),
        Err(reason) => Ok(Admission::Denied(reason)),
    }
}

/// Token that a HostTurn envelope is reserved on the session spend.
///
/// Drop releases exactly the micros this token reserved. `record` may have
/// already consumed part of that reservation; release saturates at zero.
pub struct HostTurnEnvelope {
    reserved: i64,
    session: Arc<ReviewSessionSpend>,
}

impl HostTurnEnvelope {
    pub fn release(self) {
        drop(self);
    }
}

impl Drop for HostTurnEnvelope {
    fn drop(&mut self) {
        self.session.release_reservation(self.reserved);
    }
}

pub enum HostedAttemptStart {
    Denied {
        reason: DenialReason,
        fallback: HostedFallback,
        record: LanguageLayerOperationalRecord,
    },
    Settled(HostedAttemptOutcome),
    Open(OpenHostedAttempt),
}

pub struct OpenHostedAttempt {
    pub attempt: CompletionAttempt,
    pub provider_cooldown: Option<Duration>,
}

/// Admit, then call `provider` only when admitted. A denial settles a cost-0
/// operational record and spends nothing. Successful completions stay open so
/// Pin Verification can settle the same operational record.
pub async fn begin_hosted_attempt<P, Fut>(
    context: &AttemptContext,
    config: &LanguageLayerAdmissionConfig,
    ledger: &(impl LanguageLayerLedger + ?Sized),
    session: &ReviewSessionSpend,
    concurrency: &ProviderConcurrency,
    alert: &(impl CeilingAlert + ?Sized),
    provider: P,
) -> Result<HostedAttemptStart, LedgerError>
where
    P: FnOnce() -> Fut,
    Fut: Future<Output = CompletionAttempt>,
{
    let request = AdmissionRequest {
        player_id: &context.player_id,
        session,
        remaining_deadline: context.remaining_deadline,
        as_of: context.as_of,
    };
    let admission = admit(&request, config, ledger, concurrency).await?;
    match admission {
        Admission::Denied(reason) => {
            let record = denied_record(context, reason, concurrency.honoured_cooldown());
            ledger.settle(record.clone()).await?;
            if reason == DenialReason::GlobalCeiling {
                alert.global_ceiling_tripped();
            }
            Ok(HostedAttemptStart::Denied {
                reason,
                fallback: context.task.fallback(),
                record,
            })
        }
        Admission::Admitted(permit) => {
            if context.cancelled {
                drop(permit);
                concurrency.note_non_rate_limited();
                let record = cancelled_without_provider(context);
                ledger.settle(record.clone()).await?;
                return Ok(HostedAttemptStart::Settled(HostedAttemptOutcome::Settled {
                    record,
                    attempt: None,
                }));
            }
            let attempt = provider().await;
            let provider_cooldown = match attempt.outcome {
                CompletionOutcome::RateLimited {
                    retry_after,
                    source,
                } => Some(concurrency.honor_rate_limit(retry_after, source, config)),
                _ => {
                    concurrency.note_non_rate_limited();
                    None
                }
            };
            drop(permit);
            Ok(HostedAttemptStart::Open(OpenHostedAttempt {
                attempt,
                provider_cooldown,
            }))
        }
    }
}

pub async fn finish_hosted_attempt(
    context: &AttemptContext,
    ledger: &(impl LanguageLayerLedger + ?Sized),
    session: &ReviewSessionSpend,
    alert: &(impl CeilingAlert + ?Sized),
    attempt: CompletionAttempt,
    provider_cooldown: Option<Duration>,
) -> Result<HostedAttemptOutcome, LedgerError> {
    let cost_micros = cost_micros_from_dollars(attempt.cost);
    if cost_micros > 0 {
        session.record(cost_micros);
    }
    let record = admitted_record(context, &attempt, cost_micros, provider_cooldown);
    ledger.settle(record.clone()).await?;
    if ledger.global_calendar_month(context.as_of).await? >= GLOBAL_CEILING_MICROS {
        alert.global_ceiling_tripped();
    }
    Ok(HostedAttemptOutcome::Settled {
        record,
        attempt: Some(Box::new(attempt)),
    })
}

/// Admit, then call `provider` only when admitted. A denial settles a cost-0
/// operational record and spends nothing.
pub async fn attempt_hosted<P, Fut>(
    context: &AttemptContext,
    config: &LanguageLayerAdmissionConfig,
    ledger: &(impl LanguageLayerLedger + ?Sized),
    session: &ReviewSessionSpend,
    concurrency: &ProviderConcurrency,
    alert: &(impl CeilingAlert + ?Sized),
    provider: P,
) -> Result<HostedAttemptOutcome, LedgerError>
where
    P: FnOnce() -> Fut,
    Fut: Future<Output = CompletionAttempt>,
{
    match begin_hosted_attempt(
        context,
        config,
        ledger,
        session,
        concurrency,
        alert,
        provider,
    )
    .await?
    {
        HostedAttemptStart::Denied {
            reason,
            fallback,
            record,
        } => Ok(HostedAttemptOutcome::Denied {
            reason,
            fallback,
            record,
        }),
        HostedAttemptStart::Settled(outcome) => Ok(outcome),
        HostedAttemptStart::Open(open) => {
            finish_hosted_attempt(
                context,
                ledger,
                session,
                alert,
                open.attempt,
                open.provider_cooldown,
            )
            .await
        }
    }
}

fn cancelled_without_provider(context: &AttemptContext) -> LanguageLayerOperationalRecord {
    language_layer_record(LanguageLayerRecordInput {
        player_id: context.player_id.clone(),
        settled_at: context.as_of,
        latency: Duration::ZERO,
        cost_micros: 0,
        prompt_tokens: None,
        completion_tokens: None,
        budget_decision: BudgetDecision::Admitted,
        denial_reason: None,
        error_class: Some(AttemptErrorClass::Cancelled),
        fingerprint_digest: context.fingerprint_digest.clone(),
        capture_outcome: None,
        provider_cooldown: None,
        steps: Vec::new(),
        pin: PinVerificationJudgement::NotApplicable,
    })
}

fn denied_record(
    context: &AttemptContext,
    reason: DenialReason,
    provider_cooldown: Option<Duration>,
) -> LanguageLayerOperationalRecord {
    let (budget_decision, capture_outcome, provider_cooldown) = match reason {
        DenialReason::ProviderCooldown => (
            BudgetDecision::ProviderCooldown,
            CaptureOutcome::ProviderCooldown,
            provider_cooldown,
        ),
        DenialReason::ReviewSessionCeiling
        | DenialReason::PlayerCeiling
        | DenialReason::GlobalCeiling
        | DenialReason::ConcurrencyUnavailable => {
            (BudgetDecision::Denied, CaptureOutcome::BudgetRefused, None)
        }
    };
    language_layer_record(LanguageLayerRecordInput {
        player_id: context.player_id.clone(),
        settled_at: context.as_of,
        latency: Duration::ZERO,
        cost_micros: 0,
        prompt_tokens: None,
        completion_tokens: None,
        budget_decision,
        denial_reason: Some(reason),
        error_class: None,
        fingerprint_digest: context.fingerprint_digest.clone(),
        capture_outcome: Some(capture_outcome),
        provider_cooldown,
        steps: Vec::new(),
        pin: PinVerificationJudgement::NotApplicable,
    })
}

fn admitted_record(
    context: &AttemptContext,
    attempt: &CompletionAttempt,
    cost_micros: i64,
    provider_cooldown: Option<Duration>,
) -> LanguageLayerOperationalRecord {
    language_layer_record(LanguageLayerRecordInput {
        player_id: context.player_id.clone(),
        settled_at: context.as_of,
        latency: attempt.latency,
        cost_micros,
        prompt_tokens: attempt.prompt_tokens,
        completion_tokens: attempt.completion_tokens,
        budget_decision: BudgetDecision::Admitted,
        denial_reason: None,
        error_class: if context.in_flight_cancelled {
            Some(AttemptErrorClass::Cancelled)
        } else {
            AttemptErrorClass::from_completion(&attempt.outcome)
        },
        fingerprint_digest: context.fingerprint_digest.clone(),
        capture_outcome: None,
        provider_cooldown,
        steps: Vec::new(),
        pin: context.pin.clone(),
    })
}

pub(crate) struct LanguageLayerRecordInput {
    pub player_id: PlayerId,
    pub settled_at: DateTime<Utc>,
    pub latency: Duration,
    pub cost_micros: i64,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub budget_decision: BudgetDecision,
    pub denial_reason: Option<DenialReason>,
    pub error_class: Option<AttemptErrorClass>,
    pub fingerprint_digest: String,
    pub capture_outcome: Option<CaptureOutcome>,
    pub provider_cooldown: Option<Duration>,
    pub steps: Vec<EvaluationStepObservation>,
    pub pin: PinVerificationJudgement,
}

pub(crate) fn language_layer_record(
    input: LanguageLayerRecordInput,
) -> LanguageLayerOperationalRecord {
    LanguageLayerOperationalRecord {
        request_id: next_request_id(),
        player_id: input.player_id,
        settled_at: input.settled_at,
        latency: input.latency,
        cost_micros: input.cost_micros,
        prompt_tokens: input.prompt_tokens,
        completion_tokens: input.completion_tokens,
        budget_decision: input.budget_decision,
        denial_reason: input.denial_reason,
        error_class: input.error_class,
        pin_verification: input.pin.as_verdict(),
        pin_cause: input.pin.cause(),
        fingerprint_digest: input.fingerprint_digest,
        capture_outcome: input.capture_outcome,
        provider_cooldown: input.provider_cooldown,
        steps: input.steps,
    }
}

fn day_key(as_of: DateTime<Utc>) -> String {
    as_of.date_naive().format("%Y-%m-%d").to_string()
}

fn month_key(as_of: DateTime<Utc>) -> String {
    as_of.format("%Y-%m").to_string()
}

#[cfg(test)]
mod firestore_tests;
#[cfg(test)]
mod tests;
