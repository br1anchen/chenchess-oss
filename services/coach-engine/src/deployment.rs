use std::fmt;

use reqwest::Url;

/// Where this deployment answers. An operator names their own origin; nothing
/// here carries one, so a fork is never asked to run under someone else's.
const PUBLIC_ORIGIN_ENV: &str = "PUBLIC_URL";
const DEFAULT_PUBLIC_ORIGIN: &str = "http://127.0.0.1:4173";
const STAGING_APPLICATION_DATABASE_ID: &str = "coach-app-staging";
const PRODUCTION_APPLICATION_DATABASE_ID: &str = "coach-app-production";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeploymentEnvironment {
    Staging,
    Production,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct DeploymentConfigurationError(&'static str);

impl DeploymentEnvironment {
    pub(crate) fn parse(value: &str) -> Result<Self, DeploymentConfigurationError> {
        match value {
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            _ => Err(DeploymentConfigurationError(
                "DEPLOYMENT_ENVIRONMENT must be staging or production",
            )),
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }

    pub(crate) const fn application_database_id(self) -> &'static str {
        match self {
            Self::Staging => STAGING_APPLICATION_DATABASE_ID,
            Self::Production => PRODUCTION_APPLICATION_DATABASE_ID,
        }
    }

    pub(crate) fn public_origin(self) -> String {
        std::env::var(PUBLIC_ORIGIN_ENV)
            .ok()
            .map(|origin| origin.trim_end_matches('/').to_string())
            .filter(|origin| !origin.is_empty())
            .unwrap_or_else(|| DEFAULT_PUBLIC_ORIGIN.to_string())
    }

    pub(crate) fn validate_coach_oauth(
        self,
        issuer: &str,
        resource: &str,
    ) -> Result<(), DeploymentConfigurationError> {
        validate_coach_oauth_against(&self.public_origin(), issuer, resource)
    }
}

/// The rule, with the origin passed in: `public_origin` reads the environment,
/// and a test that had to set a process-wide variable would race its siblings.
fn validate_coach_oauth_against(
    public_origin: &str,
    issuer: &str,
    resource: &str,
) -> Result<(), DeploymentConfigurationError> {
    {
        let issuer = parse_issuer(issuer)?;
        let loopback = matches!(issuer.host_str(), Some("127.0.0.1" | "localhost"));
        if issuer.origin().ascii_serialization() != public_origin && !loopback {
            return Err(DeploymentConfigurationError(
                "OAUTH_ISSUER must match this deployment's PUBLIC_URL, or be loopback",
            ));
        }
        let expected_resource = issuer
            .join("/mcp")
            .map_err(|_| DeploymentConfigurationError("OAUTH_ISSUER must be a valid origin"))?;
        if resource != expected_resource.as_str() {
            return Err(DeploymentConfigurationError(
                "COACH_MCP_RESOURCE must be the configured OAUTH_ISSUER with the /mcp path",
            ));
        }
        Ok(())
    }
}

impl fmt::Display for DeploymentConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for DeploymentConfigurationError {}

fn parse_issuer(value: &str) -> Result<Url, DeploymentConfigurationError> {
    let issuer = Url::parse(value)
        .map_err(|_| DeploymentConfigurationError("OAUTH_ISSUER must be a valid origin"))?;
    if !matches!(issuer.scheme(), "http" | "https")
        || issuer.path() != "/"
        || issuer.query().is_some()
        || issuer.fragment().is_some()
        || !issuer.username().is_empty()
        || issuer.password().is_some()
    {
        return Err(DeploymentConfigurationError(
            "OAUTH_ISSUER must be an HTTP or HTTPS origin without credentials or suffix",
        ));
    }
    if issuer.scheme() != "https" && !matches!(issuer.host_str(), Some("127.0.0.1" | "localhost")) {
        return Err(DeploymentConfigurationError(
            "OAUTH_ISSUER must use HTTPS outside loopback",
        ));
    }
    Ok(issuer)
}

#[cfg(test)]
mod tests {
    use super::validate_coach_oauth_against;

    const ORIGIN: &str = "https://coach.example";

    #[test]
    fn the_issuer_must_match_the_configured_public_origin() {
        assert!(validate_coach_oauth_against(ORIGIN, ORIGIN, "https://coach.example/mcp").is_ok());

        let error = validate_coach_oauth_against(
            ORIGIN,
            "https://other.example",
            "https://other.example/mcp",
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "OAUTH_ISSUER must match this deployment's PUBLIC_URL, or be loopback"
        );
    }

    #[test]
    fn loopback_is_accepted_whatever_the_public_origin_is() {
        assert!(validate_coach_oauth_against(
            ORIGIN,
            "http://127.0.0.1:8787",
            "http://127.0.0.1:8787/mcp"
        )
        .is_ok());
    }

    #[test]
    fn the_resource_must_be_the_issuer_with_the_mcp_path() {
        let error = validate_coach_oauth_against(ORIGIN, ORIGIN, "https://elsewhere.example/mcp")
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "COACH_MCP_RESOURCE must be the configured OAUTH_ISSUER with the /mcp path"
        );
    }

    #[test]
    fn a_non_loopback_issuer_must_use_https() {
        let error = validate_coach_oauth_against(
            "http://coach.example",
            "http://coach.example",
            "http://coach.example/mcp",
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "OAUTH_ISSUER must use HTTPS outside loopback"
        );
    }
}
