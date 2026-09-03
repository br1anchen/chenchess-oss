use anyhow::Context;

#[derive(Debug)]
pub(super) struct CoachMcpEnvironment {
    pub(super) jwt_jwks: String,
    pub(super) issuer: String,
    pub(super) resource: String,
}

pub(super) fn required_env(name: &str) -> anyhow::Result<String> {
    optional_env(name)?.with_context(|| format!("{name} is required"))
}

pub(super) fn optional_env(name: &str) -> anyhow::Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => {
            anyhow::ensure!(!value.trim().is_empty(), "{name} must not be empty");
            Ok(Some(value))
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("{name} must contain valid text")
        }
    }
}

pub(super) fn required_coach_mcp_environment(
    jwt_jwks: Option<String>,
    issuer: Option<String>,
    resource: Option<String>,
) -> anyhow::Result<CoachMcpEnvironment> {
    match (jwt_jwks, issuer, resource) {
        (Some(jwt_jwks), Some(issuer), Some(resource)) => Ok(CoachMcpEnvironment {
            jwt_jwks,
            issuer,
            resource,
        }),
        _ => anyhow::bail!("JWT_JWKS, OAUTH_ISSUER, and COACH_MCP_RESOURCE are required together"),
    }
}

/// Coach MCP admits a separately-minted access token, over and above the
/// Firebase ID token every Player already carries. A deployment that mints no
/// such token names none of the three variables at all; one that does must
/// name all three. Naming only some is the one shape that is always a mistake.
pub(super) fn optional_coach_mcp_environment(
    jwt_jwks: Option<String>,
    issuer: Option<String>,
    resource: Option<String>,
) -> anyhow::Result<Option<CoachMcpEnvironment>> {
    if jwt_jwks.is_none() && issuer.is_none() && resource.is_none() {
        return Ok(None);
    }
    required_coach_mcp_environment(jwt_jwks, issuer, resource).map(Some)
}

pub(super) fn required_value(name: &str, value: String) -> anyhow::Result<String> {
    anyhow::ensure!(!value.trim().is_empty(), "{name} must not be empty");
    Ok(value)
}
