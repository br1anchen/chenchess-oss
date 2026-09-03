use super::digest::digest_canonical_json;
use super::{
    host_capability_schema, host_turn_step_schema, preloaded_evidence_schema_digest,
    web_host_prompt_digest,
};
use crate::evaluation_fingerprint::{
    evaluation_fingerprint, CaptureOrigin, EvaluationEnvironment, EvaluationFingerprint,
    EvaluationFingerprintAxes, EvaluationGenerationSettings, LanguageLayerAttestation,
    StructuredOutputMode, EVALUATION_CONTRACT_VERSION,
};
use crate::language_layer_prompt::coaching_profile_projection_schema_digest;
use crate::pin_record::PinRecord;
use crate::review_session_contract::{ArtifactDigest, DeliverySurface};
use serde_json::json;

/// HostTurn response-schema axis: step schema plus capability schemas.
///
/// ADR 0053 item 4: both fold into `responseSchemaDigest` so the attested
/// axis set stays on `evaluation-fingerprint/v1`. The step schema is the
/// model's output contract; the capability schemas are the call-argument
/// contract the same step carries.
pub fn host_turn_response_schema_digest() -> String {
    digest_canonical_json(&json!({
        "step": host_turn_step_schema(),
        "capabilities": host_capability_schema(),
    }))
}

pub fn host_turn_fingerprint(
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
            structured_output_mode: StructuredOutputMode::NativeSchema,
            prompt_digest: artifact_digest(web_host_prompt_digest()),
            response_schema_digest: artifact_digest(host_turn_response_schema_digest()),
            evidence_schema_digest: artifact_digest(preloaded_evidence_schema_digest()),
            coaching_profile_projection_schema_digest: artifact_digest(
                coaching_profile_projection_schema_digest(),
            ),
        },
    })
}

fn artifact_digest(value: String) -> ArtifactDigest {
    ArtifactDigest::try_from(value).expect("compiled HostTurn digest is a valid ArtifactDigest")
}
