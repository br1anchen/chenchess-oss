use serde::{Deserialize, Serialize};

use crate::{review_durability::path::hashed_path_segment, review_session_contract::PlayerId};

use super::DailyCoachingStoreError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub(crate) struct DailyCoachingOwnerKey(String);

impl DailyCoachingOwnerKey {
    pub(crate) fn for_player(player_id: &PlayerId) -> Self {
        Self(hashed_path_segment(player_id.as_str()))
    }

    pub(crate) fn parse(value: String) -> Result<Self, DailyCoachingStoreError> {
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value))
        } else {
            Err(DailyCoachingStoreError::InvalidRecord)
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for DailyCoachingOwnerKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?)
            .map_err(|_| serde::de::Error::custom("invalid Daily Coaching owner key"))
    }
}
