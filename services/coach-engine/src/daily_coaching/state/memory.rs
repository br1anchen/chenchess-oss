use std::{collections::BTreeMap, sync::Mutex, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};

#[cfg(test)]
use crate::profile_game_feed::ProfileGameWindowEntry;
use crate::review_session_contract::PlayerId;

use super::{
    DailyCoachingDocument, DailyCoachingOwnerKey, DailyCoachingProvider, DailyCoachingStore,
    DailyCoachingStoreError, NudgeAdmission, ProfileHealthObservation, StoreFuture,
    StoredPlayingProfileConnection,
};

#[derive(Default)]
pub(crate) struct InMemoryDailyCoachingStore {
    documents: Mutex<BTreeMap<DailyCoachingOwnerKey, DailyCoachingDocument>>,
}

impl DailyCoachingStore for InMemoryDailyCoachingStore {
    #[cfg(test)]
    fn read<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            let documents = self
                .documents
                .lock()
                .expect("in-memory Daily Coaching state is not poisoned");
            Ok(documents
                .get(owner_key)
                .cloned()
                .unwrap_or_else(|| DailyCoachingDocument::empty(owner_key.clone())))
        })
    }

    fn list(&self) -> StoreFuture<'_, Vec<DailyCoachingDocument>> {
        Box::pin(async move {
            Ok(self
                .documents
                .lock()
                .expect("in-memory Daily Coaching state is not poisoned")
                .values()
                .cloned()
                .collect())
        })
    }

    fn bind_player<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            let mut documents = self
                .documents
                .lock()
                .expect("in-memory Daily Coaching state is not poisoned");
            let Some(document) = documents.get_mut(owner_key) else {
                return Ok(DailyCoachingDocument::empty(owner_key.clone()));
            };
            document.bind_player(player_id)?;
            document.validate()?;
            Ok(document.clone())
        })
    }

    fn connect_profile<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
        connection: StoredPlayingProfileConnection,
        timezone: String,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, StoredPlayingProfileConnection> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document| {
                document.connect(player_id, connection, timezone, now)
            })
        })
    }

    fn replace_profile<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        connection: StoredPlayingProfileConnection,
        expected_identity_username: String,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document| {
                document.replace(connection, &expected_identity_username)
            })
        })
    }

    fn remove_profile<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        provider: DailyCoachingProvider,
        expected_identity_username: String,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document| {
                document.remove(provider, &expected_identity_username)
            })
        })
    }

    fn set_enabled<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document| document.set_enabled(enabled, now))
        })
    }

    fn advance_daily_window<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        expected: NaiveDate,
        next: NaiveDate,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document| {
                document.advance_daily_window(expected, next)
            })
        })
    }

    #[cfg(test)]
    fn resolve_initial_backfill<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        expected_run_fence: u64,
        provider: DailyCoachingProvider,
        expected_identity_username: String,
        games: Vec<ProfileGameWindowEntry>,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document| {
                document.resolve_initial_backfill(
                    expected_run_fence,
                    provider,
                    &expected_identity_username,
                    games,
                )
            })
        })
    }

    fn accept_nudge<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        now: DateTime<Utc>,
        minimum_interval: Duration,
    ) -> StoreFuture<'a, NudgeAdmission> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document| {
                document.accept_nudge(now, minimum_interval)
            })
        })
    }

    fn observe_profile_health<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        provider: DailyCoachingProvider,
        expected_identity_username: &'a str,
        observation: ProfileHealthObservation,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, Option<DailyCoachingDocument>> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document| {
                document.observe_profile_health(
                    provider,
                    expected_identity_username,
                    observation,
                    now,
                )
            })
        })
    }
}

impl InMemoryDailyCoachingStore {
    fn mutate_document<T>(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        mutation: impl FnOnce(&mut DailyCoachingDocument) -> Result<T, DailyCoachingStoreError>,
    ) -> Result<T, DailyCoachingStoreError> {
        let mut documents = self
            .documents
            .lock()
            .expect("in-memory Daily Coaching state is not poisoned");
        let document = documents
            .entry(owner_key.clone())
            .or_insert_with(|| DailyCoachingDocument::empty(owner_key.clone()));
        let result = mutation(document)?;
        document.validate()?;
        Ok(result)
    }

    pub(in crate::daily_coaching) fn at_run_fence<T>(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        expected_fence: u64,
        operation: impl FnOnce(bool, Option<PlayerId>) -> T,
    ) -> T {
        let documents = self
            .documents
            .lock()
            .expect("in-memory Daily Coaching state is not poisoned");
        let fenced = documents.get(owner_key).is_none_or(|document| {
            !document.is_enabled() || document.run_fence() != expected_fence
        });
        let player_id = documents
            .get(owner_key)
            .and_then(DailyCoachingDocument::player_id)
            .cloned();
        operation(fenced, player_id)
    }

    pub(in crate::daily_coaching) fn with_document<T>(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        operation: impl FnOnce(Option<&mut DailyCoachingDocument>) -> T,
    ) -> T {
        let mut documents = self
            .documents
            .lock()
            .expect("in-memory Daily Coaching state is not poisoned");
        operation(documents.get_mut(owner_key))
    }

    #[cfg(test)]
    pub(in crate::daily_coaching) fn insert_for_test(&self, document: DailyCoachingDocument) {
        self.documents
            .lock()
            .expect("in-memory Daily Coaching state is not poisoned")
            .insert(document.owner_key().clone(), document);
    }
}
