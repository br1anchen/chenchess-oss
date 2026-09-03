use std::time::Duration;

const TICK_INTERVAL_SECONDS_ENV: &str = "DAILY_COACHING_TICK_INTERVAL_SECONDS";
const GRACE_OFFSET_SECONDS_ENV: &str = "DAILY_COACHING_GRACE_OFFSET_SECONDS";
const SPREAD_SECONDS_ENV: &str = "DAILY_COACHING_SPREAD_SECONDS";
const CLAIM_HORIZON_SECONDS_ENV: &str = "DAILY_COACHING_CLAIM_HORIZON_SECONDS";
const LEASE_TTL_SECONDS_ENV: &str = "DAILY_COACHING_LEASE_TTL_SECONDS";
const HEARTBEAT_INTERVAL_SECONDS_ENV: &str = "DAILY_COACHING_HEARTBEAT_INTERVAL_SECONDS";
const NUDGE_INTERVAL_SECONDS_ENV: &str = "DAILY_COACHING_NUDGE_INTERVAL_SECONDS";
const RUN_RETENTION_DAYS_ENV: &str = "DAILY_COACHING_RUN_RETENTION_DAYS";
const GAME_MAX_ATTEMPTS_ENV: &str = "DAILY_COACHING_GAME_MAX_ATTEMPTS";
const GAME_RETRY_INITIAL_SECONDS_ENV: &str = "DAILY_COACHING_GAME_RETRY_INITIAL_SECONDS";
const GAME_RETRY_MAX_SECONDS_ENV: &str = "DAILY_COACHING_GAME_RETRY_MAX_SECONDS";
const RUN_CLAIMS_ENABLED_ENV: &str = "DAILY_COACHING_RUN_CLAIMS_ENABLED";
const CONCURRENT_RUNS_ENV: &str = "DAILY_COACHING_CONCURRENT_RUNS";
const ENGINE_WORKERS_ENV: &str = "STOCKFISH_WORKERS";
const OPERATOR_DIGEST_UTC_HOUR_ENV: &str = "DAILY_COACHING_OPERATOR_DIGEST_UTC_HOUR";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DailyCoachingConfiguration {
    pub(crate) tick_interval: Duration,
    pub(crate) grace_offset: Duration,
    pub(crate) spread: Duration,
    pub(crate) claim_horizon: Duration,
    pub(crate) lease_ttl: Duration,
    pub(crate) heartbeat_interval: Duration,
    pub(crate) nudge_interval: Duration,
    pub(crate) run_retention_days: u32,
    pub(crate) game_max_attempts: u8,
    pub(crate) game_retry_initial: Duration,
    pub(crate) game_retry_max: Duration,
    pub(crate) operations: DailyCoachingOperationalConfiguration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DailyCoachingOperationalConfiguration {
    pub(crate) run_claims_enabled: bool,
    pub(crate) concurrent_runs: usize,
    pub(crate) engine_workers: usize,
    pub(crate) operator_digest_utc_hour: u8,
}

impl DailyCoachingConfiguration {
    pub(crate) fn from_env() -> Result<Self, DailyCoachingConfigurationError> {
        Self::new(
            duration_from_env(TICK_INTERVAL_SECONDS_ENV, 5 * 60)?,
            duration_from_env(GRACE_OFFSET_SECONDS_ENV, 2 * 60 * 60)?,
            duration_from_env(SPREAD_SECONDS_ENV, 60 * 60)?,
            duration_from_env(CLAIM_HORIZON_SECONDS_ENV, 24 * 60 * 60)?,
            duration_from_env(LEASE_TTL_SECONDS_ENV, 5 * 60)?,
            duration_from_env(HEARTBEAT_INTERVAL_SECONDS_ENV, 60)?,
            duration_from_env(NUDGE_INTERVAL_SECONDS_ENV, 5 * 60)?,
            u32_from_env(RUN_RETENTION_DAYS_ENV, 90)?,
            u8_from_env(GAME_MAX_ATTEMPTS_ENV, 5)?,
            duration_from_env(GAME_RETRY_INITIAL_SECONDS_ENV, 30)?,
            duration_from_env(GAME_RETRY_MAX_SECONDS_ENV, 5 * 60)?,
            DailyCoachingOperationalConfiguration {
                run_claims_enabled: bool_from_env(RUN_CLAIMS_ENABLED_ENV, true)?,
                concurrent_runs: usize_from_env(CONCURRENT_RUNS_ENV, 2)?,
                engine_workers: usize_from_env(ENGINE_WORKERS_ENV, 8)?,
                operator_digest_utc_hour: u8_from_env(OPERATOR_DIGEST_UTC_HOUR_ENV, 8)?,
            },
        )
    }

    pub(crate) fn standard() -> Self {
        Self::new(
            Duration::from_secs(5 * 60),
            Duration::from_secs(2 * 60 * 60),
            Duration::from_secs(60 * 60),
            Duration::from_secs(24 * 60 * 60),
            Duration::from_secs(5 * 60),
            Duration::from_secs(60),
            Duration::from_secs(5 * 60),
            90,
            5,
            Duration::from_secs(30),
            Duration::from_secs(5 * 60),
            DailyCoachingOperationalConfiguration {
                run_claims_enabled: true,
                concurrent_runs: 2,
                engine_workers: 8,
                operator_digest_utc_hour: 8,
            },
        )
        .expect("standard Daily Coaching configuration is valid")
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "each value is one independently deployable setting"
    )]
    pub(crate) fn new(
        tick_interval: Duration,
        grace_offset: Duration,
        spread: Duration,
        claim_horizon: Duration,
        lease_ttl: Duration,
        heartbeat_interval: Duration,
        nudge_interval: Duration,
        run_retention_days: u32,
        game_max_attempts: u8,
        game_retry_initial: Duration,
        game_retry_max: Duration,
        operations: DailyCoachingOperationalConfiguration,
    ) -> Result<Self, DailyCoachingConfigurationError> {
        let day = Duration::from_secs(24 * 60 * 60);
        if tick_interval.is_zero()
            || spread.is_zero()
            || claim_horizon.is_zero()
            || lease_ttl.is_zero()
            || heartbeat_interval.is_zero()
            || nudge_interval.is_zero()
            || run_retention_days == 0
            || game_max_attempts == 0
            || game_retry_initial.is_zero()
            || game_retry_initial > game_retry_max
            || grace_offset >= day
            || grace_offset.saturating_add(spread) >= day
            || heartbeat_interval >= lease_ttl
            || operations.concurrent_runs == 0
            || operations.concurrent_runs >= operations.engine_workers
            || operations.operator_digest_utc_hour > 23
        {
            return Err(DailyCoachingConfigurationError::InvalidRelationship);
        }
        Ok(Self {
            tick_interval,
            grace_offset,
            spread,
            claim_horizon,
            lease_ttl,
            heartbeat_interval,
            nudge_interval,
            run_retention_days,
            game_max_attempts,
            game_retry_initial,
            game_retry_max,
            operations,
        })
    }
}

fn duration_from_env(
    name: &'static str,
    default_seconds: u64,
) -> Result<Duration, DailyCoachingConfigurationError> {
    let seconds = match std::env::var(name) {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|_| DailyCoachingConfigurationError::InvalidValue(name))?,
        Err(std::env::VarError::NotPresent) => default_seconds,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(DailyCoachingConfigurationError::InvalidValue(name));
        }
    };
    Ok(Duration::from_secs(seconds))
}

fn u32_from_env(name: &'static str, default: u32) -> Result<u32, DailyCoachingConfigurationError> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<u32>()
            .map_err(|_| DailyCoachingConfigurationError::InvalidValue(name)),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(DailyCoachingConfigurationError::InvalidValue(name))
        }
    }
}

fn u8_from_env(name: &'static str, default: u8) -> Result<u8, DailyCoachingConfigurationError> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<u8>()
            .map_err(|_| DailyCoachingConfigurationError::InvalidValue(name)),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(DailyCoachingConfigurationError::InvalidValue(name))
        }
    }
}

fn usize_from_env(
    name: &'static str,
    default: usize,
) -> Result<usize, DailyCoachingConfigurationError> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<usize>()
            .map_err(|_| DailyCoachingConfigurationError::InvalidValue(name)),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(DailyCoachingConfigurationError::InvalidValue(name))
        }
    }
}

fn bool_from_env(
    name: &'static str,
    default: bool,
) -> Result<bool, DailyCoachingConfigurationError> {
    match std::env::var(name) {
        Ok(value) => match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(DailyCoachingConfigurationError::InvalidValue(name)),
        },
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(DailyCoachingConfigurationError::InvalidValue(name))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
/// Invalid Daily Coaching lifecycle configuration.
pub enum DailyCoachingConfigurationError {
    /// An environment value cannot be parsed as its documented type.
    #[error("{0} has an invalid value")]
    InvalidValue(&'static str),
    /// The configured timing or engine-capacity values cannot coexist safely.
    #[error("Daily Coaching settings are inconsistent")]
    InvalidRelationship,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_must_leave_time_before_the_lease_expires() {
        let result = DailyCoachingConfiguration::new(
            Duration::from_secs(300),
            Duration::from_secs(7_200),
            Duration::from_secs(3_600),
            Duration::from_secs(86_400),
            Duration::from_secs(60),
            Duration::from_secs(60),
            Duration::from_secs(300),
            90,
            5,
            Duration::from_secs(30),
            Duration::from_secs(300),
            DailyCoachingOperationalConfiguration {
                run_claims_enabled: true,
                concurrent_runs: 2,
                engine_workers: 8,
                operator_digest_utc_hour: 8,
            },
        );

        assert_eq!(
            result,
            Err(DailyCoachingConfigurationError::InvalidRelationship)
        );
    }

    #[test]
    fn batch_capacity_must_leave_an_engine_worker_for_interactive_work() {
        let mut configuration = DailyCoachingConfiguration::standard();
        configuration.operations.concurrent_runs = configuration.operations.engine_workers;

        assert_eq!(
            DailyCoachingConfiguration::new(
                configuration.tick_interval,
                configuration.grace_offset,
                configuration.spread,
                configuration.claim_horizon,
                configuration.lease_ttl,
                configuration.heartbeat_interval,
                configuration.nudge_interval,
                configuration.run_retention_days,
                configuration.game_max_attempts,
                configuration.game_retry_initial,
                configuration.game_retry_max,
                configuration.operations,
            ),
            Err(DailyCoachingConfigurationError::InvalidRelationship)
        );
    }
}
