use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};

#[cfg(test)]
use crate::profile_game_feed::ProfileGameWindowEntry;
use crate::{
    firestore::{FirestoreDatabase, FirestoreError},
    review_session_contract::PlayerId,
};

use super::state::{NudgeAdmission, ProfileHealthObservation};
use super::{
    DailyCoachingDocument, DailyCoachingOwnerKey, DailyCoachingProvider, DailyCoachingStore,
    DailyCoachingStoreError, StoreFuture, StoredPlayingProfileConnection,
};

const MAX_TRANSACTION_ATTEMPTS: usize = 4;

pub(super) struct FirestoreDailyCoachingStore {
    database: FirestoreDatabase,
}

impl FirestoreDailyCoachingStore {
    pub(super) fn new(database: FirestoreDatabase) -> Self {
        Self { database }
    }

    pub(super) fn document_path(owner_key: &DailyCoachingOwnerKey) -> [String; 4] {
        [
            "users".to_string(),
            owner_key.as_str().to_string(),
            "dailyCoaching".to_string(),
            "state".to_string(),
        ]
    }

    async fn mutate_document<T>(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        mutation: impl Fn(&mut DailyCoachingDocument, bool) -> Result<T, DailyCoachingStoreError>
            + Send
            + Sync,
    ) -> Result<T, DailyCoachingStoreError>
    where
        T: Send,
    {
        let owned_path = Self::document_path(owner_key);
        let path = owned_path.iter().map(String::as_str).collect::<Vec<_>>();
        for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
            let transaction = self.database.begin_transaction().await?;
            let document = self
                .database
                .get_document_in_transaction::<DailyCoachingDocument>(&path, &transaction)
                .await?;
            let existed = document.is_some();
            let mut document = match document {
                Some(document) => {
                    document.validate_for(owner_key)?;
                    document
                }
                None => DailyCoachingDocument::empty(owner_key.clone()),
            };
            let original = document.clone();
            let result = match mutation(&mut document, existed) {
                Ok(result) => result,
                Err(error) => {
                    self.database.rollback_transaction(transaction).await?;
                    return Err(error);
                }
            };
            document.validate_for(owner_key)?;
            if document == original {
                self.database.rollback_transaction(transaction).await?;
                return Ok(result);
            }
            let write = if existed {
                self.database.update_write(&path, &document, &[])?
            } else {
                self.database.create_write(&path, &document, &[])?
            };
            match self
                .database
                .commit_transaction(transaction, vec![write])
                .await
            {
                Ok(()) => return Ok(result),
                Err(FirestoreError::Conflict) if attempt + 1 < MAX_TRANSACTION_ATTEMPTS => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(DailyCoachingStoreError::Conflict)
    }
}

impl DailyCoachingStore for FirestoreDailyCoachingStore {
    #[cfg(test)]
    fn read<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            let owned_path = Self::document_path(owner_key);
            let path = owned_path.iter().map(String::as_str).collect::<Vec<_>>();
            match self
                .database
                .get_document::<DailyCoachingDocument>(&path)
                .await?
            {
                Some(document) => {
                    document.validate_for(owner_key)?;
                    Ok(document)
                }
                None => Ok(DailyCoachingDocument::empty(owner_key.clone())),
            }
        })
    }

    fn list(&self) -> StoreFuture<'_, Vec<DailyCoachingDocument>> {
        Box::pin(async move {
            let stored_documents = self
                .database
                .list_collection_group_documents::<DailyCoachingDocument>("dailyCoaching")
                .await?;
            let mut documents = Vec::with_capacity(stored_documents.len());
            for stored in stored_documents {
                let [users, owner, daily_coaching, state] = stored.path.as_slice() else {
                    return Err(DailyCoachingStoreError::InvalidRecord);
                };
                if users != "users" || daily_coaching != "dailyCoaching" || state != "state" {
                    return Err(DailyCoachingStoreError::InvalidRecord);
                }
                let owner_key = DailyCoachingOwnerKey::parse(owner.clone())?;
                if stored.path != Self::document_path(&owner_key) {
                    return Err(DailyCoachingStoreError::InvalidRecord);
                }
                let document = stored.value;
                document.validate_for(&owner_key)?;
                documents.push(document);
            }
            Ok(documents)
        })
    }

    fn bind_player<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document, existed| {
                if !existed {
                    return Ok(document.clone());
                }
                document.bind_player(player_id)?;
                Ok(document.clone())
            })
            .await
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
            self.mutate_document(owner_key, |document, _| {
                document.connect(player_id, connection.clone(), timezone.clone(), now)
            })
            .await
        })
    }

    fn replace_profile<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        connection: StoredPlayingProfileConnection,
        expected_identity_username: String,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document, _| {
                document.replace(connection.clone(), &expected_identity_username)
            })
            .await
        })
    }

    fn remove_profile<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        provider: DailyCoachingProvider,
        expected_identity_username: String,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document, _| {
                document.remove(provider, &expected_identity_username)
            })
            .await
        })
    }

    fn set_enabled<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document, _| document.set_enabled(enabled, now))
                .await
        })
    }

    fn advance_daily_window<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        expected: NaiveDate,
        next: NaiveDate,
    ) -> StoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document, _| {
                document.advance_daily_window(expected, next)
            })
            .await
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
            self.mutate_document(owner_key, |document, _| {
                document.resolve_initial_backfill(
                    expected_run_fence,
                    provider,
                    &expected_identity_username,
                    games.clone(),
                )
            })
            .await
        })
    }

    fn accept_nudge<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        now: DateTime<Utc>,
        minimum_interval: Duration,
    ) -> StoreFuture<'a, NudgeAdmission> {
        Box::pin(async move {
            self.mutate_document(owner_key, |document, _| {
                document.accept_nudge(now, minimum_interval)
            })
            .await
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
            self.mutate_document(owner_key, |document, _| {
                document.observe_profile_health(
                    provider,
                    expected_identity_username,
                    observation,
                    now,
                )
            })
            .await
        })
    }
}

impl From<FirestoreError> for DailyCoachingStoreError {
    fn from(error: FirestoreError) -> Self {
        match error {
            FirestoreError::Configuration(message) => Self::Configuration(message),
            FirestoreError::Transport => Self::Transport,
            FirestoreError::Unavailable => Self::Unavailable,
            FirestoreError::Conflict => Self::Conflict,
            FirestoreError::InvalidDocument => Self::InvalidRecord,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{review_durability::path::hashed_path_segment, review_session_contract::PlayerId};

    #[test]
    fn state_is_stored_under_the_hashed_player_subtree() {
        assert_eq!(
            FirestoreDailyCoachingStore::document_path(&DailyCoachingOwnerKey::for_player(
                &PlayerId::try_from("firebase-player".to_string()).unwrap(),
            ),)
            .join("/"),
            format!(
                "users/{}/dailyCoaching/state",
                hashed_path_segment("firebase-player")
            )
        );
    }
}
