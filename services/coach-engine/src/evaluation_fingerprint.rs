//! The Evaluation Fingerprint — one canonicalization function the repository shares.
//!
//! #369 and ADR 0049: a
//! SHA-256 digest over a canonical, ordered axis set, resolvable to one
//! immutable axis record. Axes are declared configuration, so a deployment can
//! compute its fingerprint at process start, before it serves. Everything
//! observed per call is a sibling of the digest and is never an argument to
//! [`evaluation_fingerprint`].

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::review_session_contract::{ArtifactDigest, DeliverySurface};

/// Current Evaluation Contract Version. Changing the axis set bumps this and
/// yields new digests; historical records keep theirs and are never recomputed.
pub const EVALUATION_CONTRACT_VERSION: &str = "evaluation-fingerprint/v1";

/// Declared configuration that produces one fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationFingerprintAxes {
    pub evaluation_contract_version: String,
    pub environment: EvaluationEnvironment,
    pub capture_origin: CaptureOrigin,
    pub delivery_surface: DeliverySurface,
    pub code_revision: String,
    pub pipeline_revision: String,
    pub language_layer_attestation: LanguageLayerAttestation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvaluationEnvironment {
    Staging,
    Production,
    BakeOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureOrigin {
    QualityCapture,
    Harness,
}

/// Attested axes carry the exact pin. Unattested axes carry the host identity
/// and no pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LanguageLayerAttestation {
    #[serde(rename_all = "camelCase")]
    Attested {
        pin: String,
        provider_allowlist: Vec<String>,
        generation_settings: EvaluationGenerationSettings,
        structured_output_mode: StructuredOutputMode,
        prompt_digest: ArtifactDigest,
        response_schema_digest: ArtifactDigest,
        evidence_schema_digest: ArtifactDigest,
        coaching_profile_projection_schema_digest: ArtifactDigest,
    },
    #[serde(rename_all = "camelCase")]
    Unattested {
        coach_app_host: String,
        coach_app_host_version: String,
        instruction_bundle_digest: ArtifactDigest,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationGenerationSettings {
    pub max_output_tokens: u32,
    pub temperature: bool,
    pub seed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StructuredOutputMode {
    NativeSchema,
}

/// The digest plus the immutable axis record it resolves to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationFingerprint {
    pub digest: ArtifactDigest,
    pub axes: EvaluationFingerprintAxes,
}

/// Observed per call. Recorded beside the digest; never passed to
/// [`evaluation_fingerprint`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationFingerprintObservations {
    pub served_provider: Option<String>,
    pub pin_verification: PinVerificationVerdict,
    pub capture_trigger: CaptureTrigger,
    pub capture_outcome: CaptureOutcome,
    #[serde(default)]
    pub served_endpoint: Option<String>,
    #[serde(default)]
    pub served_region: Option<String>,
    /// None means the declared default was served.
    #[serde(default)]
    pub routed_service_tier: Option<String>,
    /// HostTurn per-step observations. Empty for Comment and Coach Turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<EvaluationStepObservation>,
}

/// Closed HostTurn capability recorded on a step observation (D9 / D11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostTurnStepCapability {
    ReadMoment,
    ListMoments,
    EvaluateLine,
    LearningMaterial,
}

/// One HostTurn model call, recorded beside the fingerprint digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationStepObservation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub served_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub served_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u64>,
    pub cost_micros: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<HostTurnStepCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PinVerificationVerdict {
    Passed,
    Failed,
    Unverified,
    NotApplicable,
}

impl PinVerificationVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Unverified => "unverified",
            Self::NotApplicable => "notApplicable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureTrigger {
    Preference,
    FeedbackInduced,
    Harness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureOutcome {
    Published,
    Rejected,
    Failed,
    BudgetRefused,
    ProviderCooldown,
}

/// One function owns digest computation. Every producer of a fingerprint calls
/// this.
pub fn evaluation_fingerprint(axes: EvaluationFingerprintAxes) -> EvaluationFingerprint {
    let digest = digest_over(&canonical_axis_material(&axes));
    EvaluationFingerprint { digest, axes }
}

/// Ordered axis material. An accidental axis reorder or serialization change
/// fails the golden test rather than silently reissuing every identity.
pub fn canonical_axis_material(axes: &EvaluationFingerprintAxes) -> String {
    serde_json_canonicalizer::to_string(&axis_pairs(axes))
        .expect("evaluation fingerprint axes have an RFC 8785 canonical form")
}

fn axis_pairs(axes: &EvaluationFingerprintAxes) -> Vec<(String, Value)> {
    let mut pairs = vec![
        axis(
            "evaluationContractVersion",
            Value::String(axes.evaluation_contract_version.clone()),
        ),
        axis("environment", json_value(&axes.environment)),
        axis("captureOrigin", json_value(&axes.capture_origin)),
        axis("deliverySurface", json_value(&axes.delivery_surface)),
        axis(
            "languageLayerAttestation",
            Value::String(match &axes.language_layer_attestation {
                LanguageLayerAttestation::Attested { .. } => "attested".to_string(),
                LanguageLayerAttestation::Unattested { .. } => "unattested".to_string(),
            }),
        ),
        axis("codeRevision", Value::String(axes.code_revision.clone())),
        axis(
            "pipelineRevision",
            Value::String(axes.pipeline_revision.clone()),
        ),
    ];
    match &axes.language_layer_attestation {
        LanguageLayerAttestation::Attested {
            pin,
            provider_allowlist,
            generation_settings,
            structured_output_mode,
            prompt_digest,
            response_schema_digest,
            evidence_schema_digest,
            coaching_profile_projection_schema_digest,
        } => {
            let mut allowlist = provider_allowlist.clone();
            allowlist.sort();
            pairs.push(axis("pin", Value::String(pin.clone())));
            pairs.push(axis("providerAllowlist", json_value(&allowlist)));
            pairs.push(axis("generationSettings", json_value(generation_settings)));
            pairs.push(axis(
                "structuredOutputMode",
                json_value(structured_output_mode),
            ));
            pairs.push(axis(
                "promptDigest",
                Value::String(prompt_digest.as_str().to_string()),
            ));
            pairs.push(axis(
                "responseSchemaDigest",
                Value::String(response_schema_digest.as_str().to_string()),
            ));
            pairs.push(axis(
                "evidenceSchemaDigest",
                Value::String(evidence_schema_digest.as_str().to_string()),
            ));
            pairs.push(axis(
                "coachingProfileProjectionSchemaDigest",
                Value::String(
                    coaching_profile_projection_schema_digest
                        .as_str()
                        .to_string(),
                ),
            ));
        }
        LanguageLayerAttestation::Unattested {
            coach_app_host,
            coach_app_host_version,
            instruction_bundle_digest,
        } => {
            pairs.push(axis("coachAppHost", Value::String(coach_app_host.clone())));
            pairs.push(axis(
                "coachAppHostVersion",
                Value::String(coach_app_host_version.clone()),
            ));
            pairs.push(axis(
                "instructionBundleDigest",
                Value::String(instruction_bundle_digest.as_str().to_string()),
            ));
        }
    }
    pairs
}

fn axis(name: &str, value: Value) -> (String, Value) {
    (name.to_string(), value)
}

fn json_value(value: &impl Serialize) -> Value {
    serde_json::to_value(value).expect("evaluation fingerprint axes are serializable")
}

fn digest_over(material: &str) -> ArtifactDigest {
    ArtifactDigest::try_from(format!("sha256:{:x}", Sha256::digest(material.as_bytes())))
        .expect("a SHA-256 evaluation fingerprint is a valid ArtifactDigest")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN_ATTESTED_MATERIAL: &str = r#"[["evaluationContractVersion","evaluation-fingerprint/v1"],["environment","staging"],["captureOrigin","qualityCapture"],["deliverySurface","web"],["languageLayerAttestation","attested"],["codeRevision","git:test"],["pipelineRevision","pipeline:test"],["pin","openrouter/test-model:exact"],["providerAllowlist",["google-vertex/global","openrouter"]],["generationSettings",{"maxOutputTokens":512,"seed":true,"temperature":false}],["structuredOutputMode","nativeSchema"],["promptDigest","sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],["responseSchemaDigest","sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"],["evidenceSchemaDigest","sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"],["coachingProfileProjectionSchemaDigest","sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"]]"#;
    const GOLDEN_ATTESTED_DIGEST: &str =
        "sha256:22f57cb530f198835d60aa9d4167870be2486e3ab8965b9622369e71a387a8b0";
    const GOLDEN_UNATTESTED_MATERIAL: &str = r#"[["evaluationContractVersion","evaluation-fingerprint/v1"],["environment","staging"],["captureOrigin","qualityCapture"],["deliverySurface","coachApp"],["languageLayerAttestation","unattested"],["codeRevision","git:test"],["pipelineRevision","pipeline:test"],["coachAppHost","chatgpt"],["coachAppHostVersion","1.0.0"],["instructionBundleDigest","sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"]]"#;
    const GOLDEN_UNATTESTED_DIGEST: &str =
        "sha256:89051d262132dd7337e64e75e9362db014ab4bf44b99f3fdaa3184bd9044c4eb";

    fn digest_fixture(byte: char) -> ArtifactDigest {
        ArtifactDigest::try_from(format!("sha256:{}", byte.to_string().repeat(64)))
            .expect("fixed golden digest is valid")
    }

    fn attested_fixture() -> EvaluationFingerprintAxes {
        EvaluationFingerprintAxes {
            evaluation_contract_version: EVALUATION_CONTRACT_VERSION.to_string(),
            environment: EvaluationEnvironment::Staging,
            capture_origin: CaptureOrigin::QualityCapture,
            delivery_surface: DeliverySurface::Web,
            code_revision: "git:test".to_string(),
            pipeline_revision: "pipeline:test".to_string(),
            language_layer_attestation: LanguageLayerAttestation::Attested {
                pin: "openrouter/test-model:exact".to_string(),
                provider_allowlist: vec![
                    "openrouter".to_string(),
                    "google-vertex/global".to_string(),
                ],
                generation_settings: EvaluationGenerationSettings {
                    max_output_tokens: 512,
                    temperature: false,
                    seed: true,
                },
                structured_output_mode: StructuredOutputMode::NativeSchema,
                prompt_digest: digest_fixture('a'),
                response_schema_digest: digest_fixture('b'),
                evidence_schema_digest: digest_fixture('c'),
                coaching_profile_projection_schema_digest: digest_fixture('d'),
            },
        }
    }

    fn unattested_fixture() -> EvaluationFingerprintAxes {
        EvaluationFingerprintAxes {
            evaluation_contract_version: EVALUATION_CONTRACT_VERSION.to_string(),
            environment: EvaluationEnvironment::Staging,
            capture_origin: CaptureOrigin::QualityCapture,
            delivery_surface: DeliverySurface::CoachApp,
            code_revision: "git:test".to_string(),
            pipeline_revision: "pipeline:test".to_string(),
            language_layer_attestation: LanguageLayerAttestation::Unattested {
                coach_app_host: "chatgpt".to_string(),
                coach_app_host_version: "1.0.0".to_string(),
                instruction_bundle_digest: digest_fixture('e'),
            },
        }
    }

    #[test]
    fn golden_attested_canonicalization_pins_order_and_serialization() {
        let axes = attested_fixture();
        assert_eq!(canonical_axis_material(&axes), GOLDEN_ATTESTED_MATERIAL);
        assert_eq!(
            evaluation_fingerprint(axes).digest.as_str(),
            GOLDEN_ATTESTED_DIGEST
        );
    }

    #[test]
    fn golden_unattested_canonicalization_carries_host_identity_and_no_pin() {
        let axes = unattested_fixture();
        let material = canonical_axis_material(&axes);
        assert_eq!(material, GOLDEN_UNATTESTED_MATERIAL);
        assert!(material.contains(r#"["languageLayerAttestation","unattested"]"#));
        assert!(material.contains(r#"["coachAppHost","chatgpt"]"#));
        assert!(!material.contains(r#"["pin""#));
        assert_eq!(
            evaluation_fingerprint(axes).digest.as_str(),
            GOLDEN_UNATTESTED_DIGEST
        );
    }

    #[test]
    fn bumping_the_evaluation_contract_version_yields_a_new_digest() {
        let mut bumped = attested_fixture();
        bumped.evaluation_contract_version = "evaluation-fingerprint/v2".to_string();
        let digest = evaluation_fingerprint(bumped).digest;
        assert_ne!(digest.as_str(), GOLDEN_ATTESTED_DIGEST);
        assert!(digest.as_str().starts_with("sha256:"));
    }

    #[test]
    fn historical_records_keep_their_digest_when_the_live_version_changes() {
        let historical = evaluation_fingerprint(attested_fixture());
        let mut live = attested_fixture();
        live.evaluation_contract_version = "evaluation-fingerprint/v2".to_string();
        let live = evaluation_fingerprint(live);
        assert_eq!(historical.digest.as_str(), GOLDEN_ATTESTED_DIGEST);
        assert_ne!(live.digest, historical.digest);
        assert_eq!(
            historical.axes.evaluation_contract_version,
            EVALUATION_CONTRACT_VERSION
        );
    }

    #[test]
    fn per_call_observations_are_not_inside_the_digest() {
        let axes = attested_fixture();
        let digest = evaluation_fingerprint(axes.clone()).digest;
        let observations = EvaluationFingerprintObservations {
            served_provider: Some("openai".to_string()),
            pin_verification: PinVerificationVerdict::Passed,
            capture_trigger: CaptureTrigger::Preference,
            capture_outcome: CaptureOutcome::Published,
            served_endpoint: Some("ep-1".to_string()),
            served_region: Some("global".to_string()),
            routed_service_tier: None,
            steps: Vec::new(),
        };
        let material = canonical_axis_material(&axes);
        for absent in [
            "servedProvider",
            "pinVerification",
            "captureTrigger",
            "captureOutcome",
            "openai",
        ] {
            assert!(
                !material.contains(absent),
                "{absent} leaked into the digest material"
            );
        }
        let polluted = serde_json_canonicalizer::to_string(&{
            let mut pairs = axis_pairs(&axes);
            pairs.push(axis(
                "servedProvider",
                json_value(&observations.served_provider),
            ));
            pairs.push(axis(
                "pinVerification",
                json_value(&observations.pin_verification),
            ));
            pairs.push(axis(
                "captureTrigger",
                json_value(&observations.capture_trigger),
            ));
            pairs.push(axis(
                "captureOutcome",
                json_value(&observations.capture_outcome),
            ));
            pairs
        })
        .unwrap();
        assert_ne!(digest_over(&polluted), digest);
        assert_eq!(evaluation_fingerprint(axes).digest, digest);
    }

    #[test]
    fn axis_reorder_changes_the_digest() {
        let axes = attested_fixture();
        let mut reordered = axis_pairs(&axes);
        reordered.swap(0, 1);
        let reordered = serde_json_canonicalizer::to_string(&reordered).unwrap();
        assert_ne!(digest_over(&reordered), evaluation_fingerprint(axes).digest);
    }

    #[test]
    fn delivery_surface_separates_web_and_coach_app_captures() {
        let web = attested_fixture();
        let mut coach_app = web.clone();
        coach_app.delivery_surface = DeliverySurface::CoachApp;
        assert_eq!(web.delivery_surface, DeliverySurface::Web);
        assert_eq!(coach_app.delivery_surface, DeliverySurface::CoachApp);
        assert_ne!(
            evaluation_fingerprint(web).digest,
            evaluation_fingerprint(coach_app).digest,
            "web and Coach App quality captures must be separable via DeliverySurface"
        );
    }

    #[test]
    fn the_runtime_can_report_its_fingerprint_from_declared_configuration_alone() {
        let fingerprint = evaluation_fingerprint(attested_fixture());
        assert_eq!(
            fingerprint.axes.evaluation_contract_version,
            EVALUATION_CONTRACT_VERSION
        );
        assert!(matches!(
            fingerprint.axes.language_layer_attestation,
            LanguageLayerAttestation::Attested { .. }
        ));
    }
}
