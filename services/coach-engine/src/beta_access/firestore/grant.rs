use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{review_durability::path::hashed_path_segment, review_session_contract::PlayerId};

use super::{opaque_identifier, BetaAccessStoreError, FirestoreBetaAccessStore, SCHEMA_VERSION};

pub(super) const USERS_COLLECTION: &str = "users";
pub(super) const BETA_ACCESS_COLLECTION: &str = "betaAccess";
pub(super) const BETA_ACCESS_GRANT_ID: &str = "grant";

pub(super) struct BetaAccessGrantPath {
    player_path_id: String,
}

impl BetaAccessGrantPath {
    pub(super) fn new(player_id: &PlayerId) -> Self {
        Self {
            player_path_id: hashed_path_segment(player_id.as_str()),
        }
    }

    pub(super) fn segments(&self) -> [&str; 4] {
        [
            USERS_COLLECTION,
            self.player_path_id.as_str(),
            BETA_ACCESS_COLLECTION,
            BETA_ACCESS_GRANT_ID,
        ]
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BetaAccessGrantDocument {
    schema_version: u8,
    invitation_id: String,
    pub(super) granted_at: DateTime<Utc>,
}

impl BetaAccessGrantDocument {
    pub(super) fn new(invitation_id: String, granted_at: DateTime<Utc>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            invitation_id,
            granted_at,
        }
    }

    pub(super) fn valid_shape(&self) -> bool {
        self.schema_version == SCHEMA_VERSION && opaque_identifier(&self.invitation_id, 32)
    }

    pub(super) fn grants_invitation(&self, invitation_id: &str) -> bool {
        self.valid_shape() && self.invitation_id == invitation_id
    }
}

impl FirestoreBetaAccessStore {
    pub(super) async fn player_has_access(
        &self,
        player_id: &PlayerId,
    ) -> Result<bool, BetaAccessStoreError> {
        let grant_path = BetaAccessGrantPath::new(player_id);
        let grant_path = grant_path.segments();
        let grant = self
            .database
            .get_document::<BetaAccessGrantDocument>(&grant_path)
            .await?;
        match grant {
            Some(grant) if grant.valid_shape() => Ok(true),
            Some(_) => Err(BetaAccessStoreError::InvalidRecord),
            None => Ok(false),
        }
    }
}
