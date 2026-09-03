use uuid::{Uuid, Variant, Version};

use crate::{deployment::DeploymentEnvironment, review_session_contract::PlayerId};

use super::{AuthError, AuthenticationPurpose};

pub(crate) const MCP_CONFORMANCE_PLAYER_PREFIX: &str = "benchmark-issue-335-mcp-conformance:";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum McpConformancePolicy {
    Disabled,
    Staging,
}

impl McpConformancePolicy {
    pub(super) fn for_environment(environment: DeploymentEnvironment) -> Self {
        match environment {
            DeploymentEnvironment::Staging => Self::Staging,
            DeploymentEnvironment::Production => Self::Disabled,
        }
    }

    pub(super) fn firebase_purpose(
        self,
        player_id: &PlayerId,
        sign_in_provider: Option<&str>,
        claim: Option<bool>,
    ) -> Result<AuthenticationPurpose, AuthError> {
        self.classify(player_id, claim, sign_in_provider == Some("custom"))
    }

    pub(super) fn coach_purpose(
        self,
        player_id: &PlayerId,
        claim: Option<bool>,
    ) -> Result<AuthenticationPurpose, AuthError> {
        self.classify(player_id, claim, true)
    }

    fn classify(
        self,
        player_id: &PlayerId,
        claim: Option<bool>,
        provider_allowed: bool,
    ) -> Result<AuthenticationPurpose, AuthError> {
        let reserved_namespace = player_id
            .as_str()
            .starts_with(MCP_CONFORMANCE_PLAYER_PREFIX);
        let claimed = claim == Some(true);
        if !reserved_namespace && !claimed {
            return Ok(AuthenticationPurpose::Player);
        }
        if self == Self::Staging
            && is_mcp_conformance_player_id(player_id.as_str())
            && claimed
            && provider_allowed
        {
            Ok(AuthenticationPurpose::McpConformance)
        } else {
            Err(AuthError::InvalidToken)
        }
    }
}

pub(crate) fn is_mcp_conformance_player_id(player_id: &str) -> bool {
    let Some(suffix) = player_id.strip_prefix(MCP_CONFORMANCE_PLAYER_PREFIX) else {
        return false;
    };
    let Ok(identifier) = Uuid::parse_str(suffix) else {
        return false;
    };
    identifier.get_version() == Some(Version::Random)
        && identifier.get_variant() == Variant::RFC4122
        && identifier.hyphenated().to_string() == suffix
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFORMANCE_PLAYER_ID: &str =
        "benchmark-issue-335-mcp-conformance:019c1510-391c-4d67-8ff1-35775e85c504";

    #[test]
    fn player_id_grammar_requires_the_reserved_prefix_and_canonical_uuid_v4() {
        assert!(is_mcp_conformance_player_id(CONFORMANCE_PLAYER_ID));
        for player_id in [
            "firebase-player-a",
            "benchmark-issue-335-mcp-conformance:019c1510-391c-1d67-8ff1-35775e85c504",
            "benchmark-issue-335-mcp-conformance:019C1510-391C-4D67-8FF1-35775E85C504",
            "benchmark-issue-335-mcp-conformance:019c1510391c4d678ff135775e85c504",
        ] {
            assert!(!is_mcp_conformance_player_id(player_id), "{player_id}");
        }
    }

    #[test]
    fn staging_policy_requires_the_entire_conformance_tuple() {
        let player_id = PlayerId::try_from(CONFORMANCE_PLAYER_ID.to_string()).unwrap();
        assert_eq!(
            McpConformancePolicy::Staging
                .firebase_purpose(&player_id, Some("custom"), Some(true))
                .unwrap(),
            AuthenticationPurpose::McpConformance
        );
        for (policy, provider, claim) in [
            (McpConformancePolicy::Disabled, Some("custom"), Some(true)),
            (McpConformancePolicy::Staging, Some("password"), Some(true)),
            (McpConformancePolicy::Staging, Some("custom"), Some(false)),
            (McpConformancePolicy::Staging, Some("custom"), None),
        ] {
            assert!(matches!(
                policy.firebase_purpose(&player_id, provider, claim),
                Err(AuthError::InvalidToken)
            ));
        }
    }

    #[test]
    fn reserved_prefix_and_purpose_claim_are_never_independently_authoritative() {
        let malformed_reserved =
            PlayerId::try_from(format!("{MCP_CONFORMANCE_PLAYER_PREFIX}not-a-uuid")).unwrap();
        let ordinary = PlayerId::try_from("firebase-player-a".to_string()).unwrap();

        for player_id in [&malformed_reserved, &ordinary] {
            assert!(matches!(
                McpConformancePolicy::Staging.firebase_purpose(
                    player_id,
                    Some("custom"),
                    Some(true),
                ),
                Err(AuthError::InvalidToken)
            ));
        }
    }
}
