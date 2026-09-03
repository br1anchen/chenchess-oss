//! The pinned generation contract as a reviewed in-repo asset, plus the
//! bind-time posture assertion that refuses a hosted Language Layer when the
//! pinned endpoint stops being admissible.
//!
//! #370, #380,
//! and ADR 0051: changing the pin is a reviewed edit, not an env var. Env
//! supplies only the OpenRouter secret. [`configured_language_layer_runtime`]
//! runs the live posture assertion before serve.
//!
//! The pin is a generation contract. It does not record, fetch, or digest a
//! vendor documentation page. Boot asserts OpenRouter account posture and the
//! pinned endpoint's ZDR listing — facts an API answers about the route
//! served — and asserts nothing about a counterparty's documentation.
//! Revisit of ChenChess ToS and #340 admissibility is post-v1.

use crate::deployment::DeploymentEnvironment;
use crate::evaluation_fingerprint::{
    evaluation_fingerprint, CaptureOrigin, EvaluationEnvironment, EvaluationFingerprint,
    EvaluationFingerprintAxes, EvaluationGenerationSettings, LanguageLayerAttestation,
    StructuredOutputMode, EVALUATION_CONTRACT_VERSION,
};
use crate::language_layer_prompt::{
    coaching_profile_projection_schema_digest, comment_evidence_schema_digest,
    comment_prompt_digest, comment_schema_digest,
};
use crate::language_layer_provider::{LanguageLayerProvider, PostureError};
use crate::review_session_contract::{ArtifactDigest, DeliverySurface};

const PIN_RECORD_JSON: &str = include_str!("../pin-record.json");

pub const OPENROUTER_API_KEY: &str = "OPENROUTER_API_KEY";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinRecord {
    pub model: String,
    pub catalogue_slug: String,
    pub endpoint_tag: String,
    pub allow_fallbacks: bool,
    pub require_parameters: bool,
    pub structured_output_mode: StructuredOutputMode,
    pub determinism: PinDeterminism,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinDeterminism {
    pub temperature: bool,
    pub seed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootRefusal {
    KeyUnavailable,
    /// `DEPLOYMENT_ENVIRONMENT` did not parse. Live bind stays off so the
    /// process does not claim a Staging fingerprint.
    EnvironmentUnparsed,
    PostureOutage(String),
    PostureDivergence(String),
}

impl BootRefusal {
    pub fn as_alert(&self) -> String {
        match self {
            Self::KeyUnavailable => {
                "hosted Language Layer refused: OpenRouter key unavailable".into()
            }
            Self::EnvironmentUnparsed => {
                "hosted Language Layer unused: DEPLOYMENT_ENVIRONMENT unparsed; live bind stays off"
                    .into()
            }
            Self::PostureOutage(detail) => {
                format!("hosted Language Layer refused: OpenRouter posture outage: {detail}")
            }
            Self::PostureDivergence(detail) => {
                format!("hosted Language Layer refused: account posture diverged: {detail}")
            }
        }
    }
}

pub enum HostedLanguageLayerBinding {
    Bound {
        provider: LanguageLayerProvider,
        fingerprint: EvaluationFingerprint,
        pin: PinRecord,
    },
    Refused {
        reason: BootRefusal,
        fingerprint: Option<EvaluationFingerprint>,
        pin: PinRecord,
    },
}

impl HostedLanguageLayerBinding {
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::Bound { .. })
    }

    pub fn fingerprint(&self) -> Option<&EvaluationFingerprint> {
        match self {
            Self::Bound { fingerprint, .. } => Some(fingerprint),
            Self::Refused { fingerprint, .. } => fingerprint.as_ref(),
        }
    }

    pub fn pin(&self) -> &PinRecord {
        match self {
            Self::Bound { pin, .. } | Self::Refused { pin, .. } => pin,
        }
    }
}

/// The reviewed pin record compiled into this binary.
pub fn compiled_pin_record() -> PinRecord {
    serde_json::from_str(PIN_RECORD_JSON).expect("pin-record.json is a reviewed, well-formed asset")
}

/// Evaluation Fingerprint axes from the pin record and compiled prompt/schema
/// digests. Computable before the process serves.
pub fn fingerprint_from_pin(
    pin: &PinRecord,
    environment: EvaluationEnvironment,
) -> EvaluationFingerprint {
    evaluation_fingerprint(EvaluationFingerprintAxes {
        evaluation_contract_version: EVALUATION_CONTRACT_VERSION.to_string(),
        environment,
        capture_origin: CaptureOrigin::QualityCapture,
        delivery_surface: DeliverySurface::Web,
        code_revision: format!("chen-chess-coach-engine/{}", env!("CARGO_PKG_VERSION")),
        pipeline_revision: EVALUATION_CONTRACT_VERSION.to_string(),
        language_layer_attestation: LanguageLayerAttestation::Attested {
            pin: format!("{}/{}", pin.model, pin.endpoint_tag),
            provider_allowlist: vec![pin.endpoint_tag.clone()],
            generation_settings: EvaluationGenerationSettings {
                max_output_tokens: pin.max_tokens,
                temperature: pin.determinism.temperature,
                seed: pin.determinism.seed,
            },
            structured_output_mode: pin.structured_output_mode,
            prompt_digest: artifact_digest(comment_prompt_digest()),
            response_schema_digest: artifact_digest(comment_schema_digest()),
            evidence_schema_digest: artifact_digest(comment_evidence_schema_digest()),
            coaching_profile_projection_schema_digest: artifact_digest(
                coaching_profile_projection_schema_digest(),
            ),
        },
    })
}

fn artifact_digest(value: String) -> ArtifactDigest {
    ArtifactDigest::try_from(value).expect("compiled digest is a valid ArtifactDigest")
}

/// Reuse [`DeploymentEnvironment::parse`]. Unknown or missing does not fail-open
/// to Staging; the caller must not claim a Staging fingerprint.
fn evaluation_environment_from_deployment(value: Option<&str>) -> Option<EvaluationEnvironment> {
    value
        .and_then(|value| DeploymentEnvironment::parse(value).ok())
        .map(|environment| match environment {
            DeploymentEnvironment::Staging => EvaluationEnvironment::Staging,
            DeploymentEnvironment::Production => EvaluationEnvironment::Production,
        })
}

/// One trimmed secret for presence and [`LanguageLayerProvider::new`].
fn hosted_api_key(api_key: Option<&str>) -> Option<&str> {
    api_key.map(str::trim).filter(|key| !key.is_empty())
}

pub fn classify_posture_error(error: PostureError) -> BootRefusal {
    match error {
        PostureError::PinnedEndpointNotOnZdr { tag } => {
            BootRefusal::PostureDivergence(format!("pinned endpoint {tag} missing from ZDR"))
        }
        PostureError::KeyUnreadable(detail) => BootRefusal::PostureDivergence(detail),
        PostureError::Transport(detail) => BootRefusal::PostureOutage(detail),
        PostureError::Client(detail) => BootRefusal::PostureOutage(detail),
    }
}

/// Compose the hosted Language Layer, including the live #370 posture assertion.
///
/// Refusal never errors the process — the web surface stays on deterministic
/// safe rendering.
pub async fn compose_hosted_language_layer(
    api_key: Option<&str>,
    environment: EvaluationEnvironment,
) -> HostedLanguageLayerBinding {
    let pin = compiled_pin_record();
    let fingerprint = fingerprint_from_pin(&pin, environment);
    let Some(hosted_api_key) = hosted_api_key(api_key) else {
        return HostedLanguageLayerBinding::Refused {
            reason: BootRefusal::KeyUnavailable,
            fingerprint: Some(fingerprint),
            pin,
        };
    };
    let provider = match LanguageLayerProvider::new(hosted_api_key) {
        Ok(provider) => provider,
        Err(error) => {
            return HostedLanguageLayerBinding::Refused {
                reason: classify_posture_error(error),
                fingerprint: Some(fingerprint),
                pin,
            };
        }
    };
    if let Err(error) = provider
        .assert_posture(&[(pin.catalogue_slug.clone(), pin.endpoint_tag.clone())])
        .await
    {
        return HostedLanguageLayerBinding::Refused {
            reason: classify_posture_error(error),
            fingerprint: Some(fingerprint),
            pin,
        };
    }
    HostedLanguageLayerBinding::Bound {
        provider,
        fingerprint,
        pin,
    }
}

/// Unparsed-environment fallback: report the compiled pin, do not await the
/// posture assertion, and do not claim a Staging fingerprint.
fn language_layer_without_parsed_environment(api_key: Option<&str>) -> HostedLanguageLayerBinding {
    let pin = compiled_pin_record();
    if hosted_api_key(api_key).is_none() {
        return HostedLanguageLayerBinding::Refused {
            reason: BootRefusal::KeyUnavailable,
            fingerprint: None,
            pin,
        };
    }
    HostedLanguageLayerBinding::Refused {
        reason: BootRefusal::EnvironmentUnparsed,
        fingerprint: None,
        pin,
    }
}

fn report_language_layer_binding(binding: &HostedLanguageLayerBinding) {
    match binding {
        HostedLanguageLayerBinding::Bound {
            fingerprint, pin, ..
        } => {
            tracing::info!(
                digest = fingerprint.digest.as_str(),
                model = pin.model.as_str(),
                endpoint = pin.endpoint_tag.as_str(),
                "hosted Language Layer bound; evaluation fingerprint ready before serve"
            );
        }
        HostedLanguageLayerBinding::Refused {
            reason: BootRefusal::EnvironmentUnparsed,
            fingerprint,
            pin,
        } => {
            tracing::info!(
                alert = BootRefusal::EnvironmentUnparsed.as_alert(),
                digest = fingerprint.as_ref().map(|fp| fp.digest.as_str()),
                model = pin.model.as_str(),
                endpoint = pin.endpoint_tag.as_str(),
                "hosted Language Layer unused: DEPLOYMENT_ENVIRONMENT unparsed; compiled pin reported without a live bind"
            );
        }
        HostedLanguageLayerBinding::Refused {
            reason,
            fingerprint,
            ..
        } => {
            tracing::error!(
                alert = reason.as_alert(),
                digest = fingerprint.as_ref().map(|fp| fp.digest.as_str()),
                "hosted Language Layer unbound; web stays on safe rendering"
            );
        }
    }
}

/// What `main` awaits. Runs the live ADR 0051 posture assertion when
/// `DEPLOYMENT_ENVIRONMENT` parses. Does not fail-open to Staging when it
/// does not.
pub async fn configured_language_layer_runtime() -> HostedLanguageLayerBinding {
    let api_key = std::env::var(OPENROUTER_API_KEY).ok();
    let deployment = std::env::var("DEPLOYMENT_ENVIRONMENT").ok();
    let binding = match evaluation_environment_from_deployment(deployment.as_deref()) {
        Some(environment) => compose_hosted_language_layer(api_key.as_deref(), environment).await,
        None => language_layer_without_parsed_environment(api_key.as_deref()),
    };
    report_language_layer_binding(&binding);
    binding
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    /// The digest the staging deployment prints on its bind line.
    ///
    /// The rollback runbook has the operator compare this value across an
    /// unbind and a restore, on the premise that the credential is not a
    /// fingerprint axis. That premise is only worth acting on if an
    /// accidental identity change fails here first, rather than showing up as a
    /// digest that moved during a rollback for reasons nobody can name.
    ///
    /// Observed live on staging 2026-08-22T07:07:05Z as `sha256:7a350e29…`.
    /// Moved 2026-08-29 by the comment-prompt additions — line description,
    /// the unearned-outcome ban — and again the same day when the first of
    /// those collided with the intent-presentation check (then
    /// `validate_intent_presentation`, now `present_intent`): telling the model
    /// what a line "aims at" spent the aim vocabulary on description, so the
    /// hedged hypothesis sentence stopped matching and staging fell back on
    /// 5 of 5 comments with `MissingUncertainty`. The line rule now says
    /// "threatens or defends" and the hypothesis shape is stated outright.
    /// That failure mode is gone as of 2026-09-01 in any case: a comment that
    /// writes no hedged sentence is given one rather than refused, so nothing
    /// emits `MissingUncertainty` any more.
    /// Moved once more on 2026-08-30: the own-sentence marker rule now shows
    /// the three frames models actually wrap `{achievement}` in, after one
    /// comment in five rejected as `MisplacedMarker`. Moved again the same
    /// day for piece naming: a move is now read out with the piece the
    /// notation already names, so commentary stops reading as bare SAN.
    /// Moved once more the same day to carve the markers out of that rule:
    /// piece naming applies to the model's own words, never to a marker's
    /// rendering. `{playedMove}` renders as bare notation by design, and
    /// 22 of 24 staging rejections that day were `MissingRequiredMarker` —
    /// the model dropping the marker rather than write what the rule banned.
    /// Moved on 2026-09-02 for the endpoint tag rather than the prompt.
    /// Google serves Gemini 3.x under the `flex` service tier, and the ZDR
    /// listing moved with it: `google/gemini-3.5-flash-lite` is on that list
    /// only as `google-vertex/global/flex`, while `google-vertex/global` --
    /// the tag this record used to pin, and where Gemini 2.5 still lives --
    /// carries no ZDR entry for it. With `provider.zdr` set, that pin matched
    /// no endpoint and every generation came back 404 "No endpoints available
    /// matching your guardrail restrictions and data policy". Same model, same
    /// zero-retention guarantee, the tier Google now serves it on.
    /// The next staging deploy prints this value on its bind line.
    /// A deliberate pin change updates it; anything else is a bug.
    #[test]
    fn the_staging_evaluation_fingerprint_is_pinned() {
        let fingerprint =
            fingerprint_from_pin(&compiled_pin_record(), EvaluationEnvironment::Staging);
        assert_eq!(
            fingerprint.digest.as_str(),
            "sha256:acf43b93adcef8ff618b76f442f6a750264c4fb046aff3d3616634f9496a787f"
        );
    }

    #[test]
    fn the_compiled_pin_record_is_the_adr_0050_contract() {
        let pin = compiled_pin_record();
        assert_eq!(pin.model, "google/gemini-3.5-flash-lite-20260721");
        assert_eq!(pin.endpoint_tag, "google-vertex/global/flex");
        assert!(pin.endpoint_tag.contains('/'));
        assert!(!pin.determinism.temperature);
        assert!(pin.determinism.seed);
        assert_eq!(pin.max_tokens, 512);
        assert_eq!(
            pin.structured_output_mode,
            StructuredOutputMode::NativeSchema
        );
    }

    #[test]
    fn changing_the_pin_is_an_asset_edit_not_a_code_constant() {
        assert!(PIN_RECORD_JSON.contains("google/gemini-3.5-flash-lite-20260721"));
        assert!(PIN_RECORD_JSON.contains("google-vertex/global/flex"));
    }

    #[test]
    fn fingerprint_is_reportable_before_serve() {
        let pin = compiled_pin_record();
        let fingerprint = fingerprint_from_pin(&pin, EvaluationEnvironment::Staging);
        assert!(fingerprint.digest.as_str().starts_with("sha256:"));
        assert_eq!(
            fingerprint.axes.evaluation_contract_version,
            EVALUATION_CONTRACT_VERSION
        );
        match &fingerprint.axes.language_layer_attestation {
            LanguageLayerAttestation::Attested {
                pin: attested_pin, ..
            } => {
                assert!(attested_pin.contains(&pin.model));
                assert!(attested_pin.contains(&pin.endpoint_tag));
            }
            LanguageLayerAttestation::Unattested { .. } => {
                panic!("web pin record is attested")
            }
        }
    }

    #[test]
    fn unknown_or_missing_deployment_does_not_fail_open_to_staging() {
        assert_eq!(evaluation_environment_from_deployment(None), None);
        assert_eq!(evaluation_environment_from_deployment(Some("")), None);
        assert_eq!(evaluation_environment_from_deployment(Some("prod")), None);
        assert_eq!(
            evaluation_environment_from_deployment(Some("STAGING")),
            None
        );
        assert_eq!(
            evaluation_environment_from_deployment(Some("staging")),
            Some(EvaluationEnvironment::Staging)
        );
        assert_eq!(
            evaluation_environment_from_deployment(Some("production")),
            Some(EvaluationEnvironment::Production)
        );
    }

    #[test]
    fn unparsed_environment_refuses_without_claiming_staging() {
        match language_layer_without_parsed_environment(Some("sk-or-test")) {
            HostedLanguageLayerBinding::Refused {
                reason,
                fingerprint,
                pin,
            } => {
                assert_eq!(reason, BootRefusal::EnvironmentUnparsed);
                assert!(
                    fingerprint.is_none(),
                    "unparsed environment must not claim Staging"
                );
                assert_eq!(pin.model, compiled_pin_record().model);
            }
            HostedLanguageLayerBinding::Bound { .. } => {
                panic!("unparsed environment must not bind")
            }
        }
    }

    #[test]
    fn hosted_api_key_is_the_same_trimmed_value_for_presence_and_provider() {
        assert_eq!(hosted_api_key(Some("  sk-or-test\n")), Some("sk-or-test"));
        assert_eq!(hosted_api_key(Some("   ")), None);
        assert_eq!(hosted_api_key(Some("")), None);
        assert_eq!(hosted_api_key(None), None);
    }

    #[test]
    fn posture_errors_distinguish_outage_from_divergence() {
        match classify_posture_error(PostureError::PinnedEndpointNotOnZdr {
            tag: "google-vertex/global".into(),
        }) {
            BootRefusal::PostureDivergence(detail) => {
                assert!(detail.contains("google-vertex/global"));
            }
            other => panic!("expected divergence, got {other:?}"),
        }
        match classify_posture_error(PostureError::Transport("503".into())) {
            BootRefusal::PostureOutage(detail) => assert_eq!(detail, "503"),
            other => panic!("expected outage, got {other:?}"),
        }
        match classify_posture_error(PostureError::KeyUnreadable("bad key".into())) {
            BootRefusal::PostureDivergence(detail) => assert_eq!(detail, "bad key"),
            other => panic!("expected key divergence, got {other:?}"),
        }
    }

    #[test]
    fn compose_refuses_without_a_key_and_still_reports_a_fingerprint() {
        let binding = block_on(compose_hosted_language_layer(
            None,
            EvaluationEnvironment::Staging,
        ));
        match binding {
            HostedLanguageLayerBinding::Refused {
                reason,
                fingerprint,
                ..
            } => {
                assert_eq!(reason, BootRefusal::KeyUnavailable);
                assert!(fingerprint
                    .expect("compose fingerprints from a supplied environment")
                    .digest
                    .as_str()
                    .starts_with("sha256:"));
            }
            HostedLanguageLayerBinding::Bound { .. } => panic!("empty key must not bind"),
        }
    }

    #[test]
    fn compose_refuses_a_whitespace_only_key() {
        let binding = block_on(compose_hosted_language_layer(
            Some("   "),
            EvaluationEnvironment::Staging,
        ));
        match binding {
            HostedLanguageLayerBinding::Refused { reason, .. } => {
                assert_eq!(reason, BootRefusal::KeyUnavailable);
            }
            HostedLanguageLayerBinding::Bound { .. } => panic!("whitespace key must not bind"),
        }
    }

    #[test]
    fn unparsed_environment_refuses_without_a_key() {
        match language_layer_without_parsed_environment(None) {
            HostedLanguageLayerBinding::Refused { reason, .. } => {
                assert_eq!(reason, BootRefusal::KeyUnavailable);
            }
            HostedLanguageLayerBinding::Bound { .. } => panic!("empty key must not bind"),
        }
    }
}
