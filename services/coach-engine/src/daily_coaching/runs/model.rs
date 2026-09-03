use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer};

use super::{
    DailyCoachingOwnerKey, DailyCoachingRunConnection, DailyCoachingRunDocument,
    DailyCoachingRunGame, DailyCoachingRunLease, DailyCoachingRunOutcome, DailyCoachingRunState,
    DailyCoachingRunStatus, DailyCoachingRunStoreError, PlayerId, RUN_SCHEMA_VERSION,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoredDailyCoachingRunDocument {
    #[serde(deserialize_with = "deserialize_current_schema_version")]
    schema_version: u16,
    owner_key: DailyCoachingOwnerKey,
    player_id: Option<PlayerId>,
    run_id: String,
    coverage_date: NaiveDate,
    timezone: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    due_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
    claimed_at: DateTime<Utc>,
    status: DailyCoachingRunStatus,
    run_fence: u64,
    takeover_count: u32,
    connections: Vec<DailyCoachingRunConnection>,
    selection: Option<Vec<DailyCoachingRunGame>>,
    #[serde(default)]
    regeneration_count: u32,
    #[serde(default)]
    lease: Option<DailyCoachingRunLease>,
    #[serde(default)]
    outcome: Option<DailyCoachingRunOutcome>,
    #[serde(default)]
    finished_at: Option<DateTime<Utc>>,
    next_attempt_at: DateTime<Utc>,
    purge_at: DateTime<Utc>,
}

fn deserialize_current_schema_version<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let actual = u16::deserialize(deserializer)?;
    if actual == RUN_SCHEMA_VERSION {
        Ok(actual)
    } else {
        Err(serde::de::Error::custom(
            "unexpected Daily Coaching Run schema version",
        ))
    }
}

impl TryFrom<StoredDailyCoachingRunDocument> for DailyCoachingRunDocument {
    type Error = DailyCoachingRunStoreError;

    fn try_from(stored: StoredDailyCoachingRunDocument) -> Result<Self, Self::Error> {
        let state = match (
            stored.status,
            stored.lease,
            stored.outcome,
            stored.finished_at,
        ) {
            (DailyCoachingRunStatus::Active, Some(lease), None, None) => {
                DailyCoachingRunState::Active {
                    lease,
                    next_attempt_at: stored.next_attempt_at,
                }
            }
            (DailyCoachingRunStatus::PendingSelection, None, None, None) => {
                DailyCoachingRunState::PendingSelection {
                    next_attempt_at: stored.next_attempt_at,
                }
            }
            (DailyCoachingRunStatus::Completed, None, Some(outcome), Some(finished_at)) => {
                DailyCoachingRunState::Completed {
                    outcome,
                    finished_at,
                    next_attempt_at: stored.next_attempt_at,
                }
            }
            _ => return Err(DailyCoachingRunStoreError::InvalidRecord),
        };
        let document = Self {
            schema_version: stored.schema_version,
            owner_key: stored.owner_key,
            player_id: stored.player_id,
            run_id: stored.run_id,
            coverage_date: stored.coverage_date,
            timezone: stored.timezone,
            starts_at: stored.starts_at,
            ends_at: stored.ends_at,
            due_at: stored.due_at,
            deadline: stored.deadline,
            claimed_at: stored.claimed_at,
            run_fence: stored.run_fence,
            takeover_count: stored.takeover_count,
            connections: stored.connections,
            selection: stored.selection,
            regeneration_count: stored.regeneration_count,
            state,
            purge_at: stored.purge_at,
        };
        document.validate()?;
        Ok(document)
    }
}
