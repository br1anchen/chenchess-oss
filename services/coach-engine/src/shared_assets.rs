use std::path::PathBuf;

use serde::Deserialize;

pub const GROUNDING_SENTENCES_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/shared-assets/grounding/sentences.json"
));

pub const LIMITS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/shared-assets/limits.json"
));

const CANONICAL_GAME_RELATIVE: &str = "../../packages/shared-assets/fixtures/Synthet1";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedLimits {
    pub comment_authoring_deadline_seconds: u64,
    pub host_turn_max_prior_turns: u8,
}

pub fn shared_assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/shared-assets")
}

pub fn canonical_game_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CANONICAL_GAME_RELATIVE)
}

pub fn shared_limits() -> SharedLimits {
    serde_json::from_str(LIMITS_JSON).expect("packages/shared-assets/limits.json is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_json_matches_the_named_constants() {
        let limits = shared_limits();
        assert_eq!(
            limits.comment_authoring_deadline_seconds,
            crate::language_layer_ledger::COMMENT_AUTHORING_DEADLINE_SECONDS
        );
        assert_eq!(
            limits.host_turn_max_prior_turns,
            crate::review_session_contract::ReviewSessionLimits::V1.max_host_turn_prior_turns
        );
    }
}
