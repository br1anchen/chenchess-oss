use chrono::{
    DateTime, Days, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Utc,
};
use chrono_tz::Tz;

use super::{configuration::DailyCoachingConfiguration, state::DailyCoachingOwnerKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DailyWindow {
    pub(crate) coverage_date: NaiveDate,
    pub(crate) starts_at: DateTime<Utc>,
    pub(crate) ends_at: DateTime<Utc>,
    pub(crate) due_at: DateTime<Utc>,
    pub(crate) deadline: DateTime<Utc>,
}

impl DailyWindow {
    pub(crate) fn resolve(
        owner_key: &DailyCoachingOwnerKey,
        timezone: Tz,
        coverage_date: NaiveDate,
        configuration: &DailyCoachingConfiguration,
    ) -> Result<Self, DailyWindowError> {
        let next_date = coverage_date
            .checked_add_days(Days::new(1))
            .ok_or(DailyWindowError::OutOfRange)?;
        let following_date = next_date
            .checked_add_days(Days::new(1))
            .ok_or(DailyWindowError::OutOfRange)?;
        let starts_at = first_local_instant(timezone, midnight(coverage_date))?;
        let ends_at = first_local_instant(timezone, midnight(next_date))?;
        let due_local = midnight(next_date)
            .checked_add_signed(duration(configuration.grace_offset)?)
            .and_then(|value| {
                value.checked_add_signed(TimeDelta::seconds(i64::from(player_spread_seconds(
                    owner_key,
                    configuration.spread.as_secs(),
                ))))
            })
            .ok_or(DailyWindowError::OutOfRange)?;
        let due_at = scheduled_local_instant(timezone, due_local)?;
        let local_deadline = first_local_instant(timezone, midnight(following_date))?;
        let configured_deadline = due_at
            .checked_add_signed(duration(configuration.claim_horizon)?)
            .ok_or(DailyWindowError::OutOfRange)?;
        let deadline = local_deadline.min(configured_deadline);
        if starts_at >= ends_at || ends_at > due_at || due_at >= deadline {
            return Err(DailyWindowError::Invalid);
        }
        Ok(Self {
            coverage_date,
            starts_at,
            ends_at,
            due_at,
            deadline,
        })
    }

    pub(crate) fn run_id(&self) -> String {
        format!("daily-{}", self.coverage_date)
    }
}

pub(crate) fn local_date(
    now: DateTime<Utc>,
    timezone: &str,
) -> Result<NaiveDate, DailyWindowError> {
    let timezone = timezone
        .parse::<Tz>()
        .map_err(|_| DailyWindowError::InvalidTimezone)?;
    Ok(now.with_timezone(&timezone).date_naive())
}

pub(crate) fn next_date(date: NaiveDate) -> Result<NaiveDate, DailyWindowError> {
    date.checked_add_days(Days::new(1))
        .ok_or(DailyWindowError::OutOfRange)
}

fn player_spread_seconds(owner_key: &DailyCoachingOwnerKey, spread_seconds: u64) -> u32 {
    let prefix = u64::from_str_radix(&owner_key.as_str()[..16], 16)
        .expect("a Daily Coaching owner key has an eight-byte hexadecimal prefix");
    u32::try_from(prefix % spread_seconds).expect("spread is shorter than one day")
}

fn midnight(date: NaiveDate) -> NaiveDateTime {
    date.and_time(NaiveTime::MIN)
}

fn duration(value: std::time::Duration) -> Result<TimeDelta, DailyWindowError> {
    TimeDelta::from_std(value).map_err(|_| DailyWindowError::OutOfRange)
}

fn first_local_instant(
    timezone: Tz,
    mut local: NaiveDateTime,
) -> Result<DateTime<Utc>, DailyWindowError> {
    for _ in 0..=4 * 60 {
        match timezone.from_local_datetime(&local) {
            LocalResult::Single(value) => return Ok(value.with_timezone(&Utc)),
            LocalResult::Ambiguous(left, right) => {
                return Ok(left.min(right).with_timezone(&Utc));
            }
            LocalResult::None => {
                local = local
                    .checked_add_signed(TimeDelta::minutes(1))
                    .ok_or(DailyWindowError::OutOfRange)?;
            }
        }
    }
    Err(DailyWindowError::Invalid)
}

fn scheduled_local_instant(
    timezone: Tz,
    local: NaiveDateTime,
) -> Result<DateTime<Utc>, DailyWindowError> {
    match timezone.from_local_datetime(&local) {
        LocalResult::Single(value) => Ok(value.with_timezone(&Utc)),
        LocalResult::Ambiguous(left, right) => Ok(left.min(right).with_timezone(&Utc)),
        LocalResult::None => {
            let mut gap_start = local;
            let mut gap_end = local;
            for _ in 0..=4 * 60 {
                let previous = gap_start
                    .checked_sub_signed(TimeDelta::minutes(1))
                    .ok_or(DailyWindowError::OutOfRange)?;
                if !matches!(timezone.from_local_datetime(&previous), LocalResult::None) {
                    break;
                }
                gap_start = previous;
            }
            for _ in 0..=4 * 60 {
                if !matches!(timezone.from_local_datetime(&gap_end), LocalResult::None) {
                    let shifted = local
                        .checked_add_signed(gap_end.signed_duration_since(gap_start))
                        .ok_or(DailyWindowError::OutOfRange)?;
                    return match timezone.from_local_datetime(&shifted) {
                        LocalResult::Single(value) => Ok(value.with_timezone(&Utc)),
                        LocalResult::Ambiguous(left, right) => {
                            Ok(left.min(right).with_timezone(&Utc))
                        }
                        LocalResult::None => Err(DailyWindowError::Invalid),
                    };
                }
                gap_end = gap_end
                    .checked_add_signed(TimeDelta::minutes(1))
                    .ok_or(DailyWindowError::OutOfRange)?;
            }
            Err(DailyWindowError::Invalid)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
/// Failure to resolve a configured Daily Window.
pub enum DailyWindowError {
    /// The persisted timezone is not an IANA timezone.
    #[error("Daily Coaching timezone is invalid")]
    InvalidTimezone,
    /// Date arithmetic exceeded the supported range.
    #[error("Daily Coaching window is outside the supported date range")]
    OutOfRange,
    /// The configured timing values do not produce an ordered window.
    #[error("Daily Coaching window settings do not produce an ordered window")]
    Invalid,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_session_contract::PlayerId;

    fn owner() -> DailyCoachingOwnerKey {
        DailyCoachingOwnerKey::for_player(&PlayerId::try_from("player-a".to_string()).unwrap())
    }

    #[test]
    fn spread_is_stable_and_stays_inside_the_configured_hour() {
        let player = owner();
        let configuration = DailyCoachingConfiguration::standard();
        let window = DailyWindow::resolve(
            &player,
            chrono_tz::Europe::Oslo,
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
            &configuration,
        )
        .unwrap();

        let grace = DateTime::parse_from_rfc3339("2026-08-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(window.due_at >= grace);
        assert!(window.due_at < grace + TimeDelta::hours(1));
        assert_eq!(
            window,
            DailyWindow::resolve(
                &player,
                chrono_tz::Europe::Oslo,
                NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
                &configuration,
            )
            .unwrap()
        );
    }

    #[test]
    fn deadline_is_the_end_of_the_following_local_day_across_dst() {
        let player = owner();
        let window = DailyWindow::resolve(
            &player,
            chrono_tz::Europe::Oslo,
            NaiveDate::from_ymd_opt(2026, 10, 24).unwrap(),
            &DailyCoachingConfiguration::standard(),
        )
        .unwrap();

        assert_eq!(
            window.deadline,
            DateTime::parse_from_rfc3339("2026-10-25T23:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn spring_gap_preserves_the_players_spread_inside_the_shifted_hour() {
        let player = owner();
        let configuration = DailyCoachingConfiguration::standard();
        let window = DailyWindow::resolve(
            &player,
            chrono_tz::Europe::Oslo,
            NaiveDate::from_ymd_opt(2026, 3, 28).unwrap(),
            &configuration,
        )
        .unwrap();
        let expected_local = NaiveDate::from_ymd_opt(2026, 3, 29)
            .unwrap()
            .and_hms_opt(3, 0, 0)
            .unwrap()
            + TimeDelta::seconds(i64::from(player_spread_seconds(
                &player,
                configuration.spread.as_secs(),
            )));

        assert_eq!(
            window
                .due_at
                .with_timezone(&chrono_tz::Europe::Oslo)
                .naive_local(),
            expected_local
        );
    }
}
