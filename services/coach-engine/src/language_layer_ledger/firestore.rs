//! Production [`LanguageLayerLedger`]: operational record and spend counters
//! in one Firestore transaction.
//!
//! Player-owned documents live under the account-deletion subtree. The global
//! calendar-month counter is not player data.

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::account_deletion::application_data_document_path;
use crate::evaluation_fingerprint::{CaptureOutcome, EvaluationStepObservation};
use crate::firestore::{FirestoreDatabase, FirestoreError};
use crate::review_durability::path::hashed_path_segment;
use crate::review_session_contract::PlayerId;

use super::{
    day_key, month_key, AttemptErrorClass, BudgetDecision, DenialReason, LanguageLayerLedger,
    LanguageLayerOperationalRecord, LedgerError, LedgerFuture,
};

const SPEND_COLLECTION: &str = "languageLayerSpend";
const RECORD_COLLECTION: &str = "languageLayerRecords";
const GLOBAL_SPEND_COLLECTION: &str = "languageLayerGlobalSpend";
const MAX_TRANSACTION_ATTEMPTS: usize = 4;

pub(crate) struct FirestoreLanguageLayerLedger {
    database: FirestoreDatabase,
}

impl FirestoreLanguageLayerLedger {
    pub(crate) fn new(database: FirestoreDatabase) -> Self {
        Self { database }
    }

    fn player_day_path(player_id: &PlayerId, as_of: DateTime<Utc>) -> [String; 4] {
        let owner = application_data_document_path(player_id);
        [
            owner[0].clone(),
            owner[1].clone(),
            SPEND_COLLECTION.to_string(),
            day_key(as_of),
        ]
    }

    fn player_day_path_on(player_id: &PlayerId, day: &str) -> [String; 4] {
        let owner = application_data_document_path(player_id);
        [
            owner[0].clone(),
            owner[1].clone(),
            SPEND_COLLECTION.to_string(),
            day.to_string(),
        ]
    }

    fn player_record_path(player_id: &PlayerId, request_id: &str) -> [String; 4] {
        let owner = application_data_document_path(player_id);
        [
            owner[0].clone(),
            owner[1].clone(),
            RECORD_COLLECTION.to_string(),
            hashed_path_segment(request_id),
        ]
    }

    fn global_month_path(as_of: DateTime<Utc>) -> [String; 2] {
        [GLOBAL_SPEND_COLLECTION.to_string(), month_key(as_of)]
    }

    async fn mutate_settle(
        &self,
        record: LanguageLayerOperationalRecord,
    ) -> Result<(), LedgerError> {
        let player_day_owned = Self::player_day_path(&record.player_id, record.settled_at);
        let player_record_owned = Self::player_record_path(&record.player_id, &record.request_id);
        let global_owned = Self::global_month_path(record.settled_at);
        let bill = record.budget_decision == BudgetDecision::Admitted && record.cost_micros > 0;
        let document = OperationalRecordDocument::from_record(&record);
        let cost = record.cost_micros;

        for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
            let transaction = self.database.begin_transaction().await?;
            let player_path = player_day_owned
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let record_path = player_record_owned
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let global_path = global_owned.iter().map(String::as_str).collect::<Vec<_>>();

            let existing_record = self
                .database
                .get_document_in_transaction::<OperationalRecordDocument>(
                    &record_path,
                    &transaction,
                )
                .await?;
            if existing_record.is_some() {
                self.database.rollback_transaction(transaction).await?;
                return Ok(());
            }

            let mut writes = Vec::new();
            writes.push(self.database.create_write(&record_path, &document, &[])?);

            if bill {
                let fetched_player = self
                    .database
                    .get_document_in_transaction::<DaySpendDocument>(&player_path, &transaction)
                    .await?;
                let player_existed = fetched_player.is_some();
                let mut player = fetched_player.unwrap_or(DaySpendDocument { cost_micros: 0 });
                player.cost_micros = player.cost_micros.saturating_add(cost);
                writes.push(if player_existed {
                    self.database.update_write(&player_path, &player, &[])?
                } else {
                    self.database.create_write(&player_path, &player, &[])?
                });

                let fetched_global = self
                    .database
                    .get_document_in_transaction::<GlobalMonthDocument>(&global_path, &transaction)
                    .await?;
                let global_existed = fetched_global.is_some();
                let mut global = fetched_global.unwrap_or_else(|| GlobalMonthDocument {
                    month: month_key(record.settled_at),
                    cost_micros: 0,
                });
                global.cost_micros = global.cost_micros.saturating_add(cost);
                writes.push(if global_existed {
                    self.database.update_write(&global_path, &global, &[])?
                } else {
                    self.database.create_write(&global_path, &global, &[])?
                });
            }

            match self.database.commit_transaction(transaction, writes).await {
                Ok(()) => return Ok(()),
                Err(FirestoreError::Conflict) if attempt + 1 < MAX_TRANSACTION_ATTEMPTS => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(LedgerError::Unavailable)
    }
}

impl LanguageLayerLedger for FirestoreLanguageLayerLedger {
    fn player_rolling_30_day(
        &self,
        player_id: &PlayerId,
        as_of: DateTime<Utc>,
    ) -> LedgerFuture<'_, i64> {
        let player_id = player_id.clone();
        Box::pin(async move {
            let end = as_of.date_naive();
            let mut total = 0i64;
            for offset in 0..30 {
                let Some(day) = end.checked_sub_signed(TimeDelta::days(offset)) else {
                    break;
                };
                let key = day.format("%Y-%m-%d").to_string();
                let owned = Self::player_day_path_on(&player_id, &key);
                let path = owned.iter().map(String::as_str).collect::<Vec<_>>();
                if let Some(document) = self
                    .database
                    .get_document::<DaySpendDocument>(&path)
                    .await?
                {
                    total = total.saturating_add(document.cost_micros);
                }
            }
            Ok(total)
        })
    }

    fn global_calendar_month(&self, as_of: DateTime<Utc>) -> LedgerFuture<'_, i64> {
        Box::pin(async move {
            let owned = Self::global_month_path(as_of);
            let path = owned.iter().map(String::as_str).collect::<Vec<_>>();
            Ok(self
                .database
                .get_document::<GlobalMonthDocument>(&path)
                .await?
                .map(|document| document.cost_micros)
                .unwrap_or(0))
        })
    }

    fn settle(&self, record: LanguageLayerOperationalRecord) -> LedgerFuture<'_, ()> {
        Box::pin(async move { self.mutate_settle(record).await })
    }

    fn records(&self) -> LedgerFuture<'_, Vec<LanguageLayerOperationalRecord>> {
        Box::pin(async { Err(LedgerError::Unavailable) })
    }
}

impl From<FirestoreError> for LedgerError {
    fn from(_: FirestoreError) -> Self {
        Self::Unavailable
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DaySpendDocument {
    cost_micros: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GlobalMonthDocument {
    month: String,
    cost_micros: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationalRecordDocument {
    request_id: String,
    player_id: String,
    settled_at: DateTime<Utc>,
    latency_ms: u64,
    cost_micros: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prompt_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    completion_tokens: Option<u64>,
    budget_decision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    denial_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error_class: Option<String>,
    #[serde(default = "unverified_pin_verification")]
    pin_verification: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pin_cause: Option<String>,
    fingerprint_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_cooldown_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    steps: Vec<EvaluationStepObservation>,
}

impl OperationalRecordDocument {
    fn from_record(record: &LanguageLayerOperationalRecord) -> Self {
        Self {
            request_id: record.request_id.clone(),
            player_id: record.player_id.as_str().to_string(),
            settled_at: record.settled_at,
            latency_ms: u64::try_from(record.latency.as_millis()).unwrap_or(u64::MAX),
            cost_micros: record.cost_micros,
            prompt_tokens: record.prompt_tokens,
            completion_tokens: record.completion_tokens,
            budget_decision: record.budget_decision.as_str().to_string(),
            denial_reason: record
                .denial_reason
                .map(DenialReason::as_str)
                .map(str::to_string),
            error_class: record
                .error_class
                .map(AttemptErrorClass::as_str)
                .map(str::to_string),
            pin_verification: record.pin_verification.as_str().to_string(),
            pin_cause: record.pin_cause.map(|cause| cause.as_str().to_string()),
            fingerprint_digest: record.fingerprint_digest.clone(),
            capture_outcome: record.capture_outcome.map(|outcome| {
                match outcome {
                    CaptureOutcome::Published => "published",
                    CaptureOutcome::Rejected => "rejected",
                    CaptureOutcome::Failed => "failed",
                    CaptureOutcome::BudgetRefused => "budgetRefused",
                    CaptureOutcome::ProviderCooldown => "providerCooldown",
                }
                .to_string()
            }),
            provider_cooldown_ms: record
                .provider_cooldown
                .map(|wait| u64::try_from(wait.as_millis()).unwrap_or(u64::MAX)),
            steps: record.steps.clone(),
        }
    }
}

fn unverified_pin_verification() -> String {
    "unverified".to_string()
}
