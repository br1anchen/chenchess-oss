use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::evaluation_fingerprint::{
    canonical_axis_material, CaptureOutcome, EvaluationEnvironment, EvaluationFingerprint,
};
use crate::language_layer_provider::RateLimitDelaySource;
use crate::pin_record::{compiled_pin_record, fingerprint_from_pin};
use crate::retry_after::MAX_HONORED_RETRY_AFTER;

fn player() -> PlayerId {
    PlayerId::try_from("player-371".to_string()).expect("fixture player id")
}

fn fingerprint() -> EvaluationFingerprint {
    fingerprint_from_pin(&compiled_pin_record(), EvaluationEnvironment::Staging)
}

fn as_of() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-18T12:00:00Z")
        .expect("fixture timestamp")
        .with_timezone(&Utc)
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn digest() -> String {
    fingerprint().digest.as_str().to_string()
}

fn context(task: HostedTask) -> AttemptContext {
    AttemptContext {
        player_id: player(),
        task,
        remaining_deadline: Duration::from_secs(10),
        as_of: as_of(),
        fingerprint_digest: digest(),
        cancelled: false,
        in_flight_cancelled: false,
        pin: crate::pin_verification::PinVerificationJudgement::Unverified,
    }
}

fn completion(outcome: CompletionOutcome, cost: Option<f64>) -> CompletionAttempt {
    CompletionAttempt {
        latency: Duration::from_millis(12),
        http_status: Some(200),
        generation_id: Some("gen-371".into()),
        served_model: Some("google/gemini-3.5-flash-lite-20260721".into()),
        served_provider: Some("google-vertex/global".into()),
        prompt_tokens: Some(80),
        completion_tokens: Some(40),
        reasoning_tokens: Some(0),
        cost,
        finish_reason: Some("stop".into()),
        raw_content: Some("{}".into()),
        outcome,
    }
}

fn billed_seed(
    player_id: PlayerId,
    settled_at: DateTime<Utc>,
    cost_micros: i64,
) -> LanguageLayerOperationalRecord {
    LanguageLayerOperationalRecord {
        request_id: next_request_id(),
        player_id,
        settled_at,
        latency: Duration::ZERO,
        cost_micros,
        prompt_tokens: None,
        completion_tokens: None,
        budget_decision: BudgetDecision::Admitted,
        denial_reason: None,
        error_class: None,
        pin_verification: crate::evaluation_fingerprint::PinVerificationVerdict::NotApplicable,
        pin_cause: None,
        fingerprint_digest: digest(),
        capture_outcome: None,
        provider_cooldown: None,
        steps: Vec::new(),
    }
}

struct SpyProvider {
    calls: AtomicU64,
    attempt: CompletionAttempt,
}

impl SpyProvider {
    fn new(attempt: CompletionAttempt) -> Self {
        Self {
            calls: AtomicU64::new(0),
            attempt,
        }
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }

    async fn complete(&self) -> CompletionAttempt {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.attempt.clone()
    }
}

struct CountingAlert {
    trips: AtomicU64,
}

impl CountingAlert {
    fn new() -> Self {
        Self {
            trips: AtomicU64::new(0),
        }
    }

    fn trips(&self) -> u64 {
        self.trips.load(Ordering::SeqCst)
    }
}

impl CeilingAlert for CountingAlert {
    fn global_ceiling_tripped(&self) {
        self.trips.fetch_add(1, Ordering::SeqCst);
    }
}

struct Harness {
    config: LanguageLayerAdmissionConfig,
    ledger: MemoryLanguageLayerLedger,
    session: ReviewSessionSpend,
    concurrency: ProviderConcurrency,
    alert: CountingAlert,
}

impl Harness {
    fn new() -> Self {
        let config = LanguageLayerAdmissionConfig::conservative_defaults();
        let concurrency = ProviderConcurrency::new(config.max_concurrent_provider_calls);
        Self {
            config,
            ledger: MemoryLanguageLayerLedger::new(),
            session: ReviewSessionSpend::new(),
            concurrency,
            alert: CountingAlert::new(),
        }
    }

    async fn attempt(
        &self,
        context: &AttemptContext,
        provider: &SpyProvider,
    ) -> HostedAttemptOutcome {
        attempt_hosted(
            context,
            &self.config,
            &self.ledger,
            &self.session,
            &self.concurrency,
            &self.alert,
            || provider.complete(),
        )
        .await
        .expect("in-memory ledger")
    }
}

#[test]
fn a_denied_request_issues_no_provider_call_and_records_budget_refused() {
    let harness = Harness::new();
    harness.session.record(REVIEW_SESSION_CEILING_MICROS);
    let provider = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let outcome = block_on(harness.attempt(&context(HostedTask::Comment), &provider));
    match outcome {
        HostedAttemptOutcome::Denied {
            reason,
            fallback,
            record,
        } => {
            assert_eq!(reason, DenialReason::ReviewSessionCeiling);
            assert_eq!(fallback, HostedFallback::SafeRendering);
            assert_eq!(record.cost_micros, 0);
            assert_eq!(record.budget_decision, BudgetDecision::Denied);
            assert_eq!(record.budget_decision.as_str(), "denied");
            assert_eq!(record.capture_outcome, Some(CaptureOutcome::BudgetRefused));
        }
        HostedAttemptOutcome::Settled { .. } => panic!("session ceiling must deny"),
    }
    assert_eq!(provider.calls(), 0);
    assert_eq!(
        block_on(harness.ledger.player_rolling_30_day(&player(), as_of())).unwrap(),
        0
    );
}

#[test]
fn host_turn_denial_degrades_to_unavailable() {
    let harness = Harness::new();
    harness.session.record(REVIEW_SESSION_CEILING_MICROS);
    let provider = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let outcome = block_on(harness.attempt(&context(HostedTask::HostTurn), &provider));
    match outcome {
        HostedAttemptOutcome::Denied {
            reason, fallback, ..
        } => {
            assert_eq!(reason, DenialReason::ReviewSessionCeiling);
            assert_eq!(fallback, HostedFallback::Unavailable);
        }
        HostedAttemptOutcome::Settled { .. } => panic!("HostTurn must degrade"),
    }
    assert_eq!(provider.calls(), 0);
}

#[test]
fn cancelled_timed_out_and_pin_mismatched_attempts_write_records() {
    let harness = Harness::new();
    let timed = SpyProvider::new(completion(CompletionOutcome::TimedOut, Some(0.0002)));
    let exhausted = SpyProvider::new(completion(
        CompletionOutcome::DeadlineExhausted,
        Some(0.0002),
    ));
    let cancelled = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.0001)));
    let in_flight = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.0003)));
    let mismatched = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));

    let timed_out = block_on(harness.attempt(&context(HostedTask::Comment), &timed));
    let deadline = block_on(harness.attempt(&context(HostedTask::Comment), &exhausted));
    let mut cancelled_ctx = context(HostedTask::Comment);
    cancelled_ctx.cancelled = true;
    let cancelled_out = block_on(harness.attempt(&cancelled_ctx, &cancelled));
    let mut in_flight_ctx = context(HostedTask::Comment);
    in_flight_ctx.in_flight_cancelled = true;
    let in_flight_out = block_on(harness.attempt(&in_flight_ctx, &in_flight));
    let mut pin_ctx = context(HostedTask::Comment);
    pin_ctx.pin = crate::pin_verification::PinVerificationJudgement::Mismatched(
        crate::pin_verification::PinMismatchReport {
            pinned_model: "pinned/model".into(),
            pinned_provider_family: "google-vertex".into(),
            observed_permaslug: Some("other/model".into()),
            observed_provider: Some("Amazon Bedrock".into()),
            observed_provider_family: Some("amazon-bedrock".into()),
            served_endpoint: None,
            served_region: None,
            routed_service_tier: None,
        },
    );
    let pin_out = block_on(harness.attempt(&pin_ctx, &mismatched));

    match timed_out {
        HostedAttemptOutcome::Settled { record, .. } => {
            assert_eq!(record.error_class, Some(AttemptErrorClass::TimedOut));
            assert_eq!(
                record.error_class.map(AttemptErrorClass::as_str),
                Some("timedOut")
            );
            assert_eq!(
                AttemptErrorClass::from_completion(&CompletionOutcome::TimedOut),
                Some(AttemptErrorClass::TimedOut)
            );
        }
        HostedAttemptOutcome::Denied { .. } => panic!("timed out attempt is settled"),
    }
    match deadline {
        HostedAttemptOutcome::Settled { record, .. } => {
            assert_eq!(
                record.error_class,
                Some(AttemptErrorClass::DeadlineExhausted)
            );
            assert_eq!(
                record.error_class.map(AttemptErrorClass::as_str),
                Some("deadlineExhausted")
            );
            assert_eq!(
                AttemptErrorClass::from_completion(&CompletionOutcome::DeadlineExhausted),
                Some(AttemptErrorClass::DeadlineExhausted)
            );
        }
        HostedAttemptOutcome::Denied { .. } => panic!("deadline-exhausted attempt is settled"),
    }
    match cancelled_out {
        HostedAttemptOutcome::Settled { record, attempt } => {
            assert_eq!(record.error_class, Some(AttemptErrorClass::Cancelled));
            assert_eq!(
                record.error_class.map(AttemptErrorClass::as_str),
                Some("cancelled")
            );
            assert_eq!(record.budget_decision, BudgetDecision::Admitted);
            assert_eq!(record.cost_micros, 0);
            assert!(attempt.is_none());
        }
        HostedAttemptOutcome::Denied { .. } => panic!("cancelled-at-admit is settled"),
    }
    match in_flight_out {
        HostedAttemptOutcome::Settled { record, attempt } => {
            assert_eq!(record.error_class, Some(AttemptErrorClass::Cancelled));
            assert_eq!(record.budget_decision, BudgetDecision::Admitted);
            assert!(record.cost_micros > 0);
            assert!(attempt.is_some());
        }
        HostedAttemptOutcome::Denied { .. } => panic!("in-flight cancel is billed"),
    }
    match pin_out {
        HostedAttemptOutcome::Settled { record, .. } => {
            assert_eq!(record.error_class, None);
            assert_eq!(
                record.pin_verification,
                crate::evaluation_fingerprint::PinVerificationVerdict::Failed
            );
            assert_eq!(
                record.pin_cause,
                Some(crate::pin_verification::PinVerificationCause::Mismatched)
            );
            assert_eq!(record.budget_decision, BudgetDecision::Admitted);
            assert!(record.cost_micros > 0);
        }
        HostedAttemptOutcome::Denied { .. } => panic!("pin-mismatched attempt is billed"),
    }
    assert_eq!(timed.calls(), 1);
    assert_eq!(exhausted.calls(), 1);
    assert_eq!(cancelled.calls(), 0);
    assert_eq!(in_flight.calls(), 1);
    assert_eq!(mismatched.calls(), 1);
    assert_eq!(block_on(harness.ledger.records()).unwrap().len(), 5);
}

#[test]
fn spend_counters_commit_in_the_same_transaction_as_the_operational_record() {
    let harness = Harness::new();
    let provider = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let outcome = block_on(harness.attempt(&context(HostedTask::Comment), &provider));
    match outcome {
        HostedAttemptOutcome::Settled { record, .. } => {
            assert_eq!(record.cost_micros, 1000);
            assert_eq!(record.budget_decision, BudgetDecision::Admitted);
        }
        HostedAttemptOutcome::Denied { .. } => panic!("$0.001 must admit"),
    }
    assert_eq!(harness.session.spent_micros(), 1000);
    assert_eq!(
        block_on(harness.ledger.player_rolling_30_day(&player(), as_of())).unwrap(),
        1000
    );
    assert_eq!(
        block_on(harness.ledger.global_calendar_month(as_of())).unwrap(),
        1000
    );
    assert_eq!(block_on(harness.ledger.records()).unwrap().len(), 1);
    assert_eq!(provider.calls(), 1);
}

#[test]
fn player_spend_is_bounded_over_a_rolling_thirty_days() {
    let harness = Harness::new();
    let as_of = as_of();
    block_on(
        harness
            .ledger
            .settle(billed_seed(player(), as_of - TimeDelta::days(31), 400_000)),
    )
    .unwrap();
    block_on(
        harness
            .ledger
            .settle(billed_seed(player(), as_of - TimeDelta::days(1), 499_000)),
    )
    .unwrap();
    assert_eq!(
        block_on(harness.ledger.player_rolling_30_day(&player(), as_of)).unwrap(),
        499_000
    );
    let provider = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let mut ctx = context(HostedTask::Comment);
    ctx.as_of = as_of;
    let outcome = block_on(harness.attempt(&ctx, &provider));
    match outcome {
        HostedAttemptOutcome::Denied { reason, .. } => {
            assert_eq!(reason, DenialReason::PlayerCeiling);
            assert_eq!(reason.as_str(), "playerCeiling");
        }
        HostedAttemptOutcome::Settled { .. } => panic!("player ceiling must deny"),
    }
    assert_eq!(provider.calls(), 0);
}

#[test]
fn reaching_the_global_ceiling_denies_every_subsequent_call_and_alerts() {
    let harness = Harness::new();
    block_on(harness.ledger.settle(billed_seed(
        PlayerId::try_from("player-371-peer".to_string()).expect("peer"),
        as_of(),
        GLOBAL_CEILING_MICROS,
    )))
    .unwrap();
    let provider = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    for _ in 0..2 {
        let outcome = block_on(harness.attempt(&context(HostedTask::Comment), &provider));
        match outcome {
            HostedAttemptOutcome::Denied { reason, .. } => {
                assert_eq!(reason, DenialReason::GlobalCeiling);
                assert_eq!(reason.as_str(), "globalCeiling");
            }
            HostedAttemptOutcome::Settled { .. } => panic!("global ceiling must deny"),
        }
    }
    assert_eq!(provider.calls(), 0);
    assert!(harness.alert.trips() >= 1);
}

#[test]
fn concurrent_admission_overshoot_is_bounded_by_cap_minus_one_times_the_operation_ceiling() {
    let config = LanguageLayerAdmissionConfig::conservative_defaults();
    let bound = config.concurrent_overshoot_bound_micros();
    assert_eq!(bound, 3 * OPERATION_CEILING_MICROS);

    let ledger = MemoryLanguageLayerLedger::new();
    block_on(ledger.settle(billed_seed(
        player(),
        as_of(),
        PLAYER_CEILING_MICROS - OPERATION_WORST_CASE_MICROS,
    )))
    .unwrap();
    let session = ReviewSessionSpend::new();
    let concurrency = ProviderConcurrency::new(config.max_concurrent_provider_calls);
    let player = player();
    let request = AdmissionRequest {
        player_id: &player,
        session: &session,
        remaining_deadline: Duration::from_secs(5),
        as_of: as_of(),
    };

    let (a, b, c, d) = block_on(async {
        tokio::join!(
            admit(&request, &config, &ledger, &concurrency),
            admit(&request, &config, &ledger, &concurrency),
            admit(&request, &config, &ledger, &concurrency),
            admit(&request, &config, &ledger, &concurrency),
        )
    });
    let admitted = [a, b, c, d]
        .into_iter()
        .map(|result| result.expect("in-memory ledger"))
        .filter(Admission::is_admitted)
        .count();
    let overshoot = (admitted.saturating_sub(1) as i64) * OPERATION_CEILING_MICROS;
    assert!(overshoot <= bound);
}

#[test]
fn a_review_session_that_loses_residency_re_meters_from_zero() {
    let harness = Harness::new();
    let provider = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.0075)));
    let outcome = block_on(harness.attempt(&context(HostedTask::Comment), &provider));
    match outcome {
        HostedAttemptOutcome::Settled { record, .. } => {
            assert_eq!(record.cost_micros, 7500);
        }
        HostedAttemptOutcome::Denied { .. } => panic!("$0.0075 must admit"),
    }
    assert_eq!(harness.session.spent_micros(), 7500);
    let new_session = ReviewSessionSpend::new();
    assert_eq!(new_session.spent_micros(), 0);
    assert_eq!(
        block_on(harness.ledger.player_rolling_30_day(&player(), as_of())).unwrap(),
        7500
    );
}

#[test]
fn admission_concurrency_timeout_and_retry_delay_are_not_fingerprint_axes() {
    let first = fingerprint();
    let material = canonical_axis_material(&first.axes);
    let mut other = LanguageLayerAdmissionConfig::conservative_defaults();
    other.max_concurrent_provider_calls = 16;
    other.provider_attempt_timeout_ceiling = Duration::from_secs(5);
    other.comment_authoring_deadline = Duration::from_secs(3);
    other.rate_shaped_retry_delay = Duration::from_millis(50);
    // Distinctive values and field names must not appear on the axis record.
    for leak in [
        "maxConcurrentProviderCalls",
        "providerAttemptTimeoutCeiling",
        "commentAuthoringDeadline",
        "rateShapedRetryDelay",
        "max_concurrent_provider_calls",
    ] {
        assert!(
            !material.contains(leak),
            "{leak} leaked into the fingerprint"
        );
    }
    assert_eq!(
        other.slot_wait(Duration::from_secs(30)),
        Duration::from_secs(5)
    );
    assert_eq!(other.retry_delay(), Duration::from_millis(50));
    let again = fingerprint();
    assert_eq!(first.digest, again.digest);
    assert_eq!(
        first.digest,
        fingerprint_from_pin(&compiled_pin_record(), EvaluationEnvironment::Staging).digest
    );
}

// Enforced by construction, as the name says: a `const` assertion fails the
// build rather than a test run, so the tier relationship cannot regress even
// on a build that never executes this module's tests.
const _: () = assert!(OPERATION_WORST_CASE_MICROS < OPERATION_CEILING_MICROS);
const _: () = assert!(2 * OPERATION_WORST_CASE_MICROS > OPERATION_CEILING_MICROS);

#[test]
fn review_session_spend_ignores_negative_and_saturates() {
    let session = ReviewSessionSpend::new();
    session.record(-12);
    assert_eq!(session.spent_micros(), 0);
    session.record(i64::MAX);
    session.record(50);
    assert_eq!(session.spent_micros(), i64::MAX);
}

#[test]
fn next_request_id_uses_the_ll_prefix() {
    let first = next_request_id();
    let second = next_request_id();
    assert!(first.starts_with("ll-"));
    assert!(second.starts_with("ll-"));
    assert_ne!(first, second);
}

#[test]
fn next_request_id_uses_a_uuid_suffix() {
    let request_id = next_request_id();
    let suffix = request_id
        .strip_prefix("ll-")
        .expect("request id keeps the ll- prefix");
    assert_eq!(suffix.len(), 32, "request id suffix is a UUID: {suffix}");
    assert!(
        suffix.chars().all(|ch| ch.is_ascii_hexdigit()),
        "request id suffix is hex: {suffix}"
    );
}

#[test]
fn concurrency_timeout_denies_without_a_provider_call() {
    let config = LanguageLayerAdmissionConfig::conservative_defaults();
    let ledger = MemoryLanguageLayerLedger::new();
    let session = ReviewSessionSpend::new();
    let concurrency = ProviderConcurrency::new(0);
    let player = player();
    let request = AdmissionRequest {
        player_id: &player,
        session: &session,
        remaining_deadline: Duration::from_millis(5),
        as_of: as_of(),
    };
    let admission = block_on(admit(&request, &config, &ledger, &concurrency)).unwrap();
    match admission {
        Admission::Denied(reason) => {
            assert_eq!(reason, DenialReason::ConcurrencyUnavailable);
            assert_eq!(reason.as_str(), "concurrencyUnavailable");
        }
        Admission::Admitted(_) => panic!("zero-cap semaphore must time out"),
    }
}

#[test]
fn web_first_open_wait_shares_comment_authoring_deadline() {
    assert_eq!(
        LanguageLayerAdmissionConfig::conservative_defaults().comment_authoring_deadline,
        Duration::from_secs(COMMENT_AUTHORING_DEADLINE_SECONDS)
    );
    assert_eq!(COMMENT_AUTHORING_DEADLINE_SECONDS, 10);
    assert_ne!(
        COMMENT_AUTHORING_DEADLINE_SECONDS,
        crate::operating_limits::COACH_TURN_DEADLINE_SECONDS
    );
    assert_eq!(
        crate::shared_assets::shared_limits().comment_authoring_deadline_seconds,
        COMMENT_AUTHORING_DEADLINE_SECONDS
    );
}

#[test]
fn conservative_defaults_stay_the_shipped_numbers() {
    let config = LanguageLayerAdmissionConfig::conservative_defaults();
    assert_eq!(config.max_concurrent_provider_calls, 4);
    assert_eq!(
        config.provider_attempt_timeout_ceiling,
        Duration::from_secs(20)
    );
    assert_eq!(config.comment_authoring_deadline, Duration::from_secs(10));
    assert_eq!(
        config.coach_turn_authoring_deadline,
        Duration::from_secs(crate::operating_limits::COACH_TURN_DEADLINE_SECONDS)
    );
    assert_eq!(config.rate_shaped_retry_delay, Duration::from_millis(1000));
    assert_eq!(
        config.rate_limit_cooldown(None),
        Duration::from_millis(1000)
    );
    assert_eq!(
        config.rate_limit_cooldown(Some(Duration::ZERO)),
        Duration::from_millis(1000)
    );
    assert_eq!(
        config.rate_limit_cooldown(Some(Duration::from_secs(7))),
        Duration::from_secs(7)
    );
}

#[test]
fn a_429_opens_an_engine_wide_cooldown_that_denies_without_a_provider_call() {
    let mut harness = Harness::new();
    harness.config.rate_shaped_retry_delay = Duration::from_millis(80);
    let limited = SpyProvider::new(completion(
        CompletionOutcome::RateLimited {
            retry_after: Some(Duration::from_millis(80)),
            source: RateLimitDelaySource::Header,
        },
        None,
    ));
    let first = block_on(harness.attempt(&context(HostedTask::Comment), &limited));
    match first {
        HostedAttemptOutcome::Settled { record, attempt } => {
            assert_eq!(record.error_class, Some(AttemptErrorClass::RateLimited));
            assert_eq!(
                record.error_class.map(AttemptErrorClass::as_str),
                Some("rateLimited")
            );
            assert_eq!(record.provider_cooldown, Some(Duration::from_millis(80)));
            assert_eq!(
                attempt.map(|settled| settled.outcome),
                Some(CompletionOutcome::RateLimited {
                    retry_after: Some(Duration::from_millis(80)),
                    source: RateLimitDelaySource::Header,
                })
            );
        }
        HostedAttemptOutcome::Denied { .. } => panic!("the 429 itself is a settled attempt"),
    }
    assert_eq!(limited.calls(), 1);

    let peer = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let comment = block_on(harness.attempt(&context(HostedTask::Comment), &peer));
    match comment {
        HostedAttemptOutcome::Denied {
            reason,
            fallback,
            record,
        } => {
            assert_eq!(reason, DenialReason::ProviderCooldown);
            assert_eq!(reason.as_str(), "providerCooldown");
            assert_eq!(fallback, HostedFallback::SafeRendering);
            assert_eq!(record.budget_decision, BudgetDecision::ProviderCooldown);
            assert_eq!(
                record.capture_outcome,
                Some(CaptureOutcome::ProviderCooldown)
            );
            assert_eq!(record.provider_cooldown, Some(Duration::from_millis(80)));
        }
        HostedAttemptOutcome::Settled { .. } => panic!("cooldown must deny comments"),
    }
    let turn = block_on(harness.attempt(&context(HostedTask::HostTurn), &peer));
    match turn {
        HostedAttemptOutcome::Denied {
            reason, fallback, ..
        } => {
            assert_eq!(reason, DenialReason::ProviderCooldown);
            assert_eq!(fallback, HostedFallback::Unavailable);
        }
        HostedAttemptOutcome::Settled { .. } => panic!("cooldown must deny HostTurns"),
    }
    assert_eq!(peer.calls(), 0);
}

#[test]
fn a_429_without_retry_after_uses_the_one_second_floor() {
    let harness = Harness::new();
    assert_eq!(
        harness.config.rate_shaped_retry_delay,
        Duration::from_millis(1000)
    );
    let limited = SpyProvider::new(completion(
        CompletionOutcome::RateLimited {
            retry_after: None,
            source: RateLimitDelaySource::Unspecified,
        },
        None,
    ));
    block_on(harness.attempt(&context(HostedTask::Comment), &limited));
    let remaining = harness
        .concurrency
        .cooldown_remaining()
        .expect("missing Retry-After still opens a cooldown");
    assert!(
        remaining <= Duration::from_secs(1) && remaining > Duration::ZERO,
        "floor cooldown should be at most 1 s and still open, was {remaining:?}"
    );
}

#[test]
fn admission_proceeds_after_the_cooldown_elapses() {
    let mut harness = Harness::new();
    harness.config.rate_shaped_retry_delay = Duration::from_millis(20);
    let limited = SpyProvider::new(completion(
        CompletionOutcome::RateLimited {
            retry_after: Some(Duration::from_millis(20)),
            source: RateLimitDelaySource::Header,
        },
        None,
    ));
    block_on(harness.attempt(&context(HostedTask::Comment), &limited));
    std::thread::sleep(Duration::from_millis(40));
    let recovered = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let outcome = block_on(harness.attempt(&context(HostedTask::Comment), &recovered));
    match outcome {
        HostedAttemptOutcome::Settled { record, .. } => {
            assert_eq!(record.budget_decision, BudgetDecision::Admitted);
        }
        HostedAttemptOutcome::Denied { reason, .. } => {
            panic!("elapsed cooldown must admit, denied {reason:?}")
        }
    }
    assert_eq!(recovered.calls(), 1);
}

#[test]
fn a_503_does_not_open_the_rate_limit_cooldown() {
    let harness = Harness::new();
    let outage = SpyProvider::new(completion(CompletionOutcome::HttpError, None));
    let first = block_on(harness.attempt(&context(HostedTask::Comment), &outage));
    match first {
        HostedAttemptOutcome::Settled { record, .. } => {
            assert_eq!(record.error_class, None);
        }
        HostedAttemptOutcome::Denied { .. } => panic!("503 is a settled HTTP error"),
    }
    let next = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let outcome = block_on(harness.attempt(&context(HostedTask::Comment), &next));
    match outcome {
        HostedAttemptOutcome::Settled { .. } => {}
        HostedAttemptOutcome::Denied { reason, .. } => {
            panic!("503 must not start a cooldown, denied {reason:?}")
        }
    }
    assert_eq!(next.calls(), 1);
}

#[test]
fn retry_info_delay_is_honoured_when_the_header_is_missing() {
    let mut harness = Harness::new();
    harness.config.rate_shaped_retry_delay = Duration::from_millis(20);
    let limited = SpyProvider::new(completion(
        CompletionOutcome::RateLimited {
            retry_after: Some(Duration::from_millis(80)),
            source: RateLimitDelaySource::RetryInfo,
        },
        None,
    ));
    let first = block_on(harness.attempt(&context(HostedTask::Comment), &limited));
    match first {
        HostedAttemptOutcome::Settled { record, .. } => {
            assert_eq!(record.provider_cooldown, Some(Duration::from_millis(80)));
        }
        HostedAttemptOutcome::Denied { .. } => panic!("RetryInfo 429 is a settled attempt"),
    }
    let remaining = harness
        .concurrency
        .cooldown_remaining()
        .expect("RetryInfo opens a cooldown");
    assert!(
        remaining <= Duration::from_millis(80) && remaining > Duration::ZERO,
        "RetryInfo cooldown should be at most 80 ms, was {remaining:?}"
    );
}

#[test]
fn consecutive_429s_double_the_floor_up_to_fifteen_minutes() {
    let mut harness = Harness::new();
    harness.config.rate_shaped_retry_delay = Duration::from_millis(20);
    let limited = SpyProvider::new(completion(
        CompletionOutcome::RateLimited {
            retry_after: None,
            source: RateLimitDelaySource::Unspecified,
        },
        None,
    ));
    block_on(harness.attempt(&context(HostedTask::Comment), &limited));
    assert_eq!(
        harness.concurrency.honoured_cooldown(),
        Some(Duration::from_millis(20))
    );
    std::thread::sleep(Duration::from_millis(30));
    block_on(harness.attempt(&context(HostedTask::Comment), &limited));
    assert_eq!(
        harness.concurrency.honoured_cooldown(),
        Some(Duration::from_millis(40))
    );
    std::thread::sleep(Duration::from_millis(50));
    block_on(harness.attempt(&context(HostedTask::Comment), &limited));
    assert_eq!(
        harness.concurrency.honoured_cooldown(),
        Some(Duration::from_millis(80))
    );

    for _ in 0..20 {
        harness.concurrency.honor_rate_limit(
            None,
            RateLimitDelaySource::Unspecified,
            &harness.config,
        );
    }
    assert_eq!(
        harness.concurrency.honoured_cooldown(),
        Some(MAX_HONORED_RETRY_AFTER)
    );
}

#[test]
fn a_seeded_global_ceiling_wins_recording_over_an_open_cooldown() {
    let harness = Harness::new();
    let other = PlayerId::try_from("player-ceiling-peer".to_string()).expect("fixture player id");
    block_on(
        harness
            .ledger
            .settle(billed_seed(other, as_of(), GLOBAL_CEILING_MICROS)),
    )
    .unwrap();
    harness.concurrency.honor_rate_limit(
        Some(Duration::from_secs(60)),
        RateLimitDelaySource::Header,
        &harness.config,
    );
    let provider = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let outcome = block_on(harness.attempt(&context(HostedTask::Comment), &provider));
    match outcome {
        HostedAttemptOutcome::Denied { reason, record, .. } => {
            assert_eq!(reason, DenialReason::GlobalCeiling);
            assert_eq!(record.budget_decision, BudgetDecision::Denied);
            assert_eq!(record.capture_outcome, Some(CaptureOutcome::BudgetRefused));
            assert_eq!(record.provider_cooldown, None);
        }
        HostedAttemptOutcome::Settled { .. } => panic!("ceiling must win recording"),
    }
    assert_eq!(provider.calls(), 0);
}

#[test]
fn host_turn_fallback_is_unavailable() {
    assert_eq!(HostedTask::HostTurn.fallback(), HostedFallback::Unavailable);
}

#[test]
fn host_turn_envelope_denial_reserves_nothing() {
    let session = Arc::new(ReviewSessionSpend::new());
    session.record(REVIEW_SESSION_CEILING_MICROS);
    let ledger = MemoryLanguageLayerLedger::new();
    let concurrency = ProviderConcurrency::new(4);
    let player = player();
    let request = AdmissionRequest {
        player_id: &player,
        session: session.as_ref(),
        remaining_deadline: Duration::from_secs(15),
        as_of: as_of(),
    };
    let outcome = block_on(admit_host_turn_envelope(
        &request,
        session.clone(),
        &ledger,
        &concurrency,
    ))
    .unwrap();
    assert!(matches!(
        outcome,
        HostTurnEnvelopeAdmission::Denied(DenialReason::ReviewSessionCeiling)
    ));
    assert_eq!(session.committed_micros(), REVIEW_SESSION_CEILING_MICROS);
    assert_eq!(session.spent_micros(), REVIEW_SESSION_CEILING_MICROS);
}

#[test]
fn host_turn_envelope_starts_only_while_committed_fits_the_hold() {
    let ledger = MemoryLanguageLayerLedger::new();
    let concurrency = ProviderConcurrency::new(4);
    let player = player();
    let room = REVIEW_SESSION_CEILING_MICROS - HOST_TURN_ENVELOPE_MICROS;
    let session = Arc::new(ReviewSessionSpend::new());
    session.record(room);
    let request = AdmissionRequest {
        player_id: &player,
        session: session.as_ref(),
        remaining_deadline: Duration::from_secs(15),
        as_of: as_of(),
    };
    assert!(matches!(
        block_on(admit_host_turn_envelope(
            &request,
            session.clone(),
            &ledger,
            &concurrency,
        ))
        .unwrap(),
        HostTurnEnvelopeAdmission::Admitted(_)
    ));
    session.record(1);
    assert!(matches!(
        block_on(admit_host_turn_envelope(
            &request,
            session.clone(),
            &ledger,
            &concurrency,
        ))
        .unwrap(),
        HostTurnEnvelopeAdmission::Denied(DenialReason::ReviewSessionCeiling)
    ));
}

#[test]
fn host_turn_envelope_releases_unspent_reservation() {
    let session = Arc::new(ReviewSessionSpend::new());
    let ledger = MemoryLanguageLayerLedger::new();
    let concurrency = ProviderConcurrency::new(4);
    let player = player();
    let request = AdmissionRequest {
        player_id: &player,
        session: session.as_ref(),
        remaining_deadline: Duration::from_secs(15),
        as_of: as_of(),
    };
    let HostTurnEnvelopeAdmission::Admitted(envelope) = block_on(admit_host_turn_envelope(
        &request,
        session.clone(),
        &ledger,
        &concurrency,
    ))
    .unwrap() else {
        panic!("envelope fits an empty session");
    };
    assert_eq!(session.committed_micros(), 0);
    assert_eq!(session.spent_micros(), HOST_TURN_ENVELOPE_MICROS);
    session.record(1_000);
    envelope.release();
    assert_eq!(session.committed_micros(), 1_000);
    assert_eq!(session.spent_micros(), 1_000);
}

#[test]
fn host_turn_envelope_drop_releases_only_its_reservation() {
    let session = Arc::new(ReviewSessionSpend::new());
    assert!(session.try_reserve(5_000));
    let ledger = MemoryLanguageLayerLedger::new();
    let concurrency = ProviderConcurrency::new(4);
    let player = player();
    let request = AdmissionRequest {
        player_id: &player,
        session: session.as_ref(),
        remaining_deadline: Duration::from_secs(15),
        as_of: as_of(),
    };
    let HostTurnEnvelopeAdmission::Admitted(envelope) = block_on(admit_host_turn_envelope(
        &request,
        session.clone(),
        &ledger,
        &concurrency,
    ))
    .unwrap() else {
        panic!("envelope fits beside a 5_000 leftover");
    };
    assert_eq!(session.spent_micros(), 5_000 + HOST_TURN_ENVELOPE_MICROS);
    drop(envelope);
    assert_eq!(session.committed_micros(), 0);
    assert_eq!(session.spent_micros(), 5_000);
}

#[test]
fn comment_admission_still_uses_worst_case_not_the_host_turn_envelope() {
    let harness = Harness::new();
    harness
        .session
        .record(REVIEW_SESSION_CEILING_MICROS - OPERATION_WORST_CASE_MICROS);
    let provider = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let outcome = block_on(harness.attempt(&context(HostedTask::Comment), &provider));
    match outcome {
        HostedAttemptOutcome::Settled { .. } => {}
        HostedAttemptOutcome::Denied { reason, .. } => {
            panic!("Comment still admits at worst-case room: {reason:?}")
        }
    }
    let player = player();
    let session = Arc::new(ReviewSessionSpend::new());
    session.record(harness.session.spent_micros());
    let request = AdmissionRequest {
        player_id: &player,
        session: session.as_ref(),
        remaining_deadline: Duration::from_secs(15),
        as_of: as_of(),
    };
    let denied = block_on(admit_host_turn_envelope(
        &request,
        session.clone(),
        &harness.ledger,
        &harness.concurrency,
    ))
    .unwrap();
    assert!(matches!(
        denied,
        HostTurnEnvelopeAdmission::Denied(DenialReason::ReviewSessionCeiling)
    ));
}

#[test]
fn comment_admission_denies_while_a_host_turn_envelope_is_held() {
    let harness = Harness::new();
    harness.session.record(1_000);
    assert!(harness.session.try_reserve(HOST_TURN_ENVELOPE_MICROS));
    let provider = SpyProvider::new(completion(CompletionOutcome::Completed, Some(0.001)));
    let outcome = block_on(harness.attempt(&context(HostedTask::Comment), &provider));
    match outcome {
        HostedAttemptOutcome::Denied { reason, .. } => {
            assert_eq!(reason, DenialReason::ReviewSessionCeiling);
        }
        HostedAttemptOutcome::Settled { .. } => {
            panic!("a held HostTurn envelope must deny Comment")
        }
    }
    assert_eq!(provider.calls(), 0);
}

#[test]
fn host_turn_step_denies_when_envelope_reservation_cannot_cover_worst_case() {
    let session = Arc::new(ReviewSessionSpend::new());
    assert!(session.try_reserve(HOST_TURN_ENVELOPE_MICROS));
    session.record(HOST_TURN_ENVELOPE_MICROS - OPERATION_WORST_CASE_MICROS + 1);
    assert!(session.reserved_micros() < OPERATION_WORST_CASE_MICROS);
    let ledger = MemoryLanguageLayerLedger::new();
    let concurrency = ProviderConcurrency::new(4);
    let player = player();
    let request = AdmissionRequest {
        player_id: &player,
        session: session.as_ref(),
        remaining_deadline: Duration::from_secs(15),
        as_of: as_of(),
    };
    let admission = block_on(admit_host_turn_step(
        &request,
        &LanguageLayerAdmissionConfig::conservative_defaults(),
        &ledger,
        &concurrency,
    ))
    .unwrap();
    assert!(matches!(
        admission,
        Admission::Denied(DenialReason::ReviewSessionCeiling)
    ));
}
