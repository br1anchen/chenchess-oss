use chrono::{DateTime, TimeDelta, Utc};

use crate::{
    firestore::{FirestoreDatabase, FirestoreError, FirestoreVersionedDocument},
    quality_capture::FirestoreQualityCaptureStore,
    review_session_contract::PlayerId,
};

use super::{
    marker_document_id, AccountDeletionError, AccountDeletionMarker, AccountDeletionPhase,
};

const DELETED_USERS_COLLECTION: &str = "deletedUsers";
pub(super) const USERS_COLLECTION: &str = "users";

pub(super) struct AccountDeletionPersistence {
    staging: FirestoreDatabase,
    production: FirestoreDatabase,
    quality_capture: FirestoreQualityCaptureStore,
}

impl AccountDeletionPersistence {
    pub(super) fn new(
        staging: FirestoreDatabase,
        production: FirestoreDatabase,
        quality: FirestoreDatabase,
    ) -> Self {
        Self {
            staging,
            quality_capture: FirestoreQualityCaptureStore::new(production.clone(), quality),
            production,
        }
    }

    pub(super) async fn ensure_markers(
        &self,
        player_id: &PlayerId,
    ) -> Result<AccountDeletionMarker, AccountDeletionError> {
        let marker = self.ensure_production_marker(player_id).await?;
        self.ensure_staging_marker(&marker).await?;
        Ok(marker)
    }

    pub(super) async fn advance(
        &self,
        player_id: &PlayerId,
        phase: AccountDeletionPhase,
    ) -> Result<AccountDeletionMarker, AccountDeletionError> {
        loop {
            let versioned = self.load_production_marker(player_id).await?;
            if versioned.value.phase >= phase {
                return Ok(versioned.value);
            }
            let mut next = versioned.value;
            next.phase = phase;
            match self
                .update_production_marker(next.clone(), versioned.update_time)
                .await
            {
                Ok(()) => return Ok(next),
                Err(AccountDeletionError::Conflict) => {}
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) async fn withdraw_captures(
        &self,
        player_id: &PlayerId,
    ) -> Result<(), AccountDeletionError> {
        self.quality_capture
            .withdraw_all_for_account_deletion(player_id)
            .await
            .map_err(|error| match error {
                crate::quality_capture::QualityCaptureStoreError::Configuration(message) => {
                    AccountDeletionError::Configuration(message)
                }
                crate::quality_capture::QualityCaptureStoreError::Transport => {
                    AccountDeletionError::Transport
                }
                crate::quality_capture::QualityCaptureStoreError::Unavailable => {
                    AccountDeletionError::Unavailable
                }
                crate::quality_capture::QualityCaptureStoreError::Conflict => {
                    AccountDeletionError::Conflict
                }
                crate::quality_capture::QualityCaptureStoreError::InvalidRecord => {
                    AccountDeletionError::InvalidRecord
                }
            })
    }

    pub(super) async fn delete_application_data(
        &self,
        player_id: &PlayerId,
    ) -> Result<(), AccountDeletionError> {
        let path = super::application_data_document_path(player_id);
        let path = [path[0].as_str(), path[1].as_str()];
        tokio::try_join!(
            self.staging.recursive_delete_document(&path),
            self.production.recursive_delete_document(&path),
        )?;
        Ok(())
    }

    pub(super) async fn complete(&self, player_id: &PlayerId) -> Result<(), AccountDeletionError> {
        let completed_at = Utc::now();
        let purge_at = completed_at + TimeDelta::hours(2);
        self.complete_staging_marker(player_id, completed_at, purge_at)
            .await?;
        loop {
            let versioned = self.load_production_marker(player_id).await?;
            if versioned.value.phase == AccountDeletionPhase::FirebaseIdentityDeleted {
                return Ok(());
            }
            let mut completed = versioned.value;
            completed.phase = AccountDeletionPhase::FirebaseIdentityDeleted;
            completed.completed_at = Some(completed_at);
            completed.purge_at = Some(purge_at);
            match self
                .update_production_marker(completed.clone(), versioned.update_time)
                .await
            {
                Ok(()) => return Ok(()),
                Err(AccountDeletionError::Conflict) => {}
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) async fn incomplete_players(&self) -> Result<Vec<PlayerId>, AccountDeletionError> {
        let markers = self
            .production
            .list_documents::<AccountDeletionMarker>(&[DELETED_USERS_COLLECTION])
            .await?;
        let mut players = Vec::new();
        for (document_id, marker) in markers {
            marker.validate_for(&marker.player_id)?;
            if document_id != marker_document_id(&marker.player_id) {
                return Err(AccountDeletionError::InvalidRecord);
            }
            if marker.phase != AccountDeletionPhase::FirebaseIdentityDeleted {
                players.push(marker.player_id);
            }
        }
        Ok(players)
    }

    async fn ensure_production_marker(
        &self,
        player_id: &PlayerId,
    ) -> Result<AccountDeletionMarker, AccountDeletionError> {
        let document_id = marker_document_id(player_id);
        let path = [DELETED_USERS_COLLECTION, document_id.as_str()];
        if let Some(existing) = self
            .production
            .get_document::<AccountDeletionMarker>(&path)
            .await?
        {
            existing.validate_for(player_id)?;
            return Ok(existing);
        }
        let marker = AccountDeletionMarker::started(player_id.clone(), Utc::now());
        let write =
            self.production
                .create_write(&path, &marker, &[("startedAt", marker.started_at)])?;
        match self.production.commit(vec![write]).await {
            Ok(()) => Ok(marker),
            Err(FirestoreError::Conflict) => {
                let existing = self
                    .production
                    .get_document::<AccountDeletionMarker>(&path)
                    .await?
                    .ok_or(AccountDeletionError::Conflict)?;
                existing.validate_for(player_id)?;
                Ok(existing)
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn ensure_staging_marker(
        &self,
        marker: &AccountDeletionMarker,
    ) -> Result<(), AccountDeletionError> {
        let document_id = marker_document_id(&marker.player_id);
        let path = [DELETED_USERS_COLLECTION, document_id.as_str()];
        if let Some(existing) = self
            .staging
            .get_document::<AccountDeletionMarker>(&path)
            .await?
        {
            existing.validate_for(&marker.player_id)?;
            return Ok(());
        }
        let write =
            self.staging
                .create_write(&path, marker, &[("startedAt", marker.started_at)])?;
        match self.staging.commit(vec![write]).await {
            Ok(()) | Err(FirestoreError::Conflict) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    async fn load_production_marker(
        &self,
        player_id: &PlayerId,
    ) -> Result<FirestoreVersionedDocument<AccountDeletionMarker>, AccountDeletionError> {
        let document_id = marker_document_id(player_id);
        let marker = self
            .production
            .get_versioned_document::<AccountDeletionMarker>(&[
                DELETED_USERS_COLLECTION,
                &document_id,
            ])
            .await?
            .ok_or(AccountDeletionError::InvalidRecord)?;
        marker.value.validate_for(player_id)?;
        Ok(marker)
    }

    async fn update_production_marker(
        &self,
        marker: AccountDeletionMarker,
        update_time: String,
    ) -> Result<(), AccountDeletionError> {
        let document_id = marker_document_id(&marker.player_id);
        let path = [DELETED_USERS_COLLECTION, document_id.as_str()];
        let timestamps = marker_timestamps(&marker);
        self.production
            .commit(vec![self.production.update_write_at(
                &path,
                &marker,
                &timestamps,
                update_time,
            )?])
            .await
            .map_err(Into::into)
    }

    async fn complete_staging_marker(
        &self,
        player_id: &PlayerId,
        completed_at: DateTime<Utc>,
        purge_at: DateTime<Utc>,
    ) -> Result<(), AccountDeletionError> {
        let document_id = marker_document_id(player_id);
        let path = [DELETED_USERS_COLLECTION, document_id.as_str()];
        let Some(versioned) = self
            .staging
            .get_versioned_document::<AccountDeletionMarker>(&path)
            .await?
        else {
            return Err(AccountDeletionError::InvalidRecord);
        };
        versioned.value.validate_for(player_id)?;
        if versioned.value.phase == AccountDeletionPhase::FirebaseIdentityDeleted {
            return Ok(());
        }
        let mut completed = versioned.value;
        completed.phase = AccountDeletionPhase::FirebaseIdentityDeleted;
        completed.completed_at = Some(completed_at);
        completed.purge_at = Some(purge_at);
        let timestamps = marker_timestamps(&completed);
        match self
            .staging
            .commit(vec![self.staging.update_write_at(
                &path,
                &completed,
                &timestamps,
                versioned.update_time,
            )?])
            .await
        {
            Ok(()) | Err(FirestoreError::Conflict) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn marker_timestamps(marker: &AccountDeletionMarker) -> Vec<(&'static str, DateTime<Utc>)> {
    let mut timestamps = vec![("startedAt", marker.started_at)];
    if let Some(completed_at) = marker.completed_at {
        timestamps.push(("completedAt", completed_at));
    }
    if let Some(purge_at) = marker.purge_at {
        timestamps.push(("purgeAt", purge_at));
    }
    timestamps
}
