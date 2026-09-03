use serde::Serialize;

use crate::{
    critical_moment_selector::adaptive_target,
    evaluation_recording::PINNED_STOCKFISH_DEPTH,
    operating_limits::{MAX_GAME_PLIES, MAX_REVIEW_MOMENT_CACHED_EVIDENCE_ENTRIES},
    review_durability::MAX_REVIEW_DURABILITY_WRITES_PER_COMMIT,
    review_session_contract::ReviewSessionLimits,
};

use super::entry::MAX_REVIEW_MOMENT_DOCUMENT_BYTES;

const MINIMUM_DOCUMENT_HEADROOM_BYTES: usize = 100 * 1024;
const FIRESTORE_COMMIT_WRITE_LIMIT: usize = 500;
const INITIAL_PROVENANCE_EVIDENCE_ENTRIES: usize = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredMomentPayload {
    published_comment: MeasuredPublishedComment,
    committed_alternatives: Vec<MeasuredCommittedAlternative>,
    completed_coach_turns: Vec<MeasuredCompletedCoachTurn>,
    provider_evidence: Vec<MeasuredProviderEvidence>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredPublishedComment {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredCommittedAlternative {
    alternative_move_id: String,
    branch_ref: String,
    parent: MeasuredBranchParent,
    source_position: MeasuredDerivedPosition,
    move_uci: String,
    resulting_position: MeasuredDerivedPosition,
    evaluation: MeasuredAlternativeEvaluation,
    strongest_reply_uci: String,
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum MeasuredBranchParent {
    Root { ply: u16 },
    Move { branch_ref: String },
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum MeasuredDerivedPosition {
    GamePly {
        ply: u16,
    },
    AlternativeLine {
        root_ply: u16,
        uci_path: Vec<String>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredAlternativeEvaluation {
    selected_centipawns: i32,
    best_move_uci: String,
    best_centipawns: i32,
    loss_centipawns: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredCompletedCoachTurn {
    coach_turn_id: String,
    alternative_move_id: String,
    objective_quality: MeasuredAssessmentDimension,
    findability: MeasuredAssessmentDimension,
    resilience: MeasuredAssessmentDimension,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredAssessmentDimension {
    explanation: String,
    evidence_refs: Vec<String>,
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum MeasuredProviderEvidence {
    Position {
        metadata: MeasuredEvidenceMetadata,
        position: MeasuredDerivedPosition,
    },
    EngineAnalysis {
        metadata: MeasuredEvidenceMetadata,
        position: MeasuredDerivedPosition,
        evaluation_centipawns: i32,
        best_move_uci: String,
        principal_variation: Vec<String>,
    },
    Branch {
        metadata: MeasuredEvidenceMetadata,
        branch_ref: String,
        parent: MeasuredBranchParent,
        source_position: MeasuredDerivedPosition,
        move_uci: String,
        resulting_position: MeasuredDerivedPosition,
    },
    Provenance {
        metadata: MeasuredEvidenceMetadata,
        provider: MeasuredEvidenceProvenance,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasuredEvidenceMetadata {
    evidence_id: String,
    dependencies: Vec<String>,
    content_digest: String,
    provenance: MeasuredEvidenceProvenance,
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum MeasuredEvidenceProvenance {
    Stockfish {
        version: String,
        binary_digest: String,
        depth: u8,
        threads: u8,
        hash_mib: u16,
    },
}

#[derive(Debug, Clone, Copy)]
enum EvidenceVariant {
    Position,
    EngineAnalysis,
    Branch,
    Provenance,
}

#[test]
fn maximum_derived_session_fits_inline_moment_and_commit_guards() {
    let measurement = measure_maximum_session();

    assert_eq!(measurement.game_plies, MAX_GAME_PLIES);
    assert_eq!(measurement.moment_count, 8);
    assert_eq!(
        measurement.committed_alternative_count,
        usize::from(ReviewSessionLimits::V1.max_committed_alternative_moves)
    );
    assert_eq!(
        measurement.completed_coach_turn_count,
        usize::from(ReviewSessionLimits::V1.max_started_coach_turns)
    );
    assert_eq!(
        measurement.provider_evidence_count,
        maximum_provider_evidence_entries()
    );
    assert!(
        measurement.maximum_moment_payload_bytes
            <= MAX_REVIEW_MOMENT_DOCUMENT_BYTES - MINIMUM_DOCUMENT_HEADROOM_BYTES,
        "maximum moment payload is {} bytes; the {}-byte guard requires at least {} bytes of headroom",
        measurement.maximum_moment_payload_bytes,
        MAX_REVIEW_MOMENT_DOCUMENT_BYTES,
        MINIMUM_DOCUMENT_HEADROOM_BYTES
    );
    assert!(
        measurement.creation_write_count <= MAX_REVIEW_DURABILITY_WRITES_PER_COMMIT,
        "session creation needs {} writes, above the repository's {}-mutation convention",
        measurement.creation_write_count,
        MAX_REVIEW_DURABILITY_WRITES_PER_COMMIT
    );
    assert!(
        measurement.creation_write_count <= FIRESTORE_COMMIT_WRITE_LIMIT,
        "session creation needs {} writes, above Firestore's {}-write commit limit",
        measurement.creation_write_count,
        FIRESTORE_COMMIT_WRITE_LIMIT
    );
    assert!(
        !measurement.maximum_moment_payload.contains("\"occupied\"")
            && !measurement.maximum_moment_payload.contains("\"fen\"")
            && !measurement
                .maximum_moment_payload
                .contains("\"positionRef\""),
        "derived durable positions must not retain serialized board state"
    );

    eprintln!(
        "durable-storage-measurement game_plies={} moments={} largest_evidence_variant={:?} \
         provider_evidence={} alternatives={} coach_turns={} moment_payload_bytes={} \
         document_headroom_bytes={} creation_writes={} artifact_write_headroom={} \
         firestore_write_headroom={}",
        measurement.game_plies,
        measurement.moment_count,
        measurement.largest_evidence_variant,
        measurement.provider_evidence_count,
        measurement.committed_alternative_count,
        measurement.completed_coach_turn_count,
        measurement.maximum_moment_payload_bytes,
        MAX_REVIEW_MOMENT_DOCUMENT_BYTES - measurement.maximum_moment_payload_bytes,
        measurement.creation_write_count,
        MAX_REVIEW_DURABILITY_WRITES_PER_COMMIT - measurement.creation_write_count,
        FIRESTORE_COMMIT_WRITE_LIMIT - measurement.creation_write_count,
    );
}

#[test]
fn production_moment_serializer_keeps_positions_derived() {
    let created_at = "2026-08-01T10:00:00Z".parse().unwrap();
    let imported = super::test_fixtures::fixture_import(created_at);
    let core = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/core-contract.json"
    )))
    .unwrap();
    let entries = super::ReviewAnalysisEntries::try_new(
        &imported,
        vec![super::CheckpointReviewSessionMoment::Pending {
            core: Box::new(core),
        }],
        created_at,
    )
    .unwrap();
    let payload =
        super::firestore::encoded_moment_payload(&entries.entries[0], &entries.game).unwrap();

    assert!(!payload.contains("\"occupied\""));
    assert!(!payload.contains("\"fen\""));
    assert!(!payload.contains("\"positionRef\""));
    assert!(!payload.contains("\"requestId\""));
    assert!(payload.contains("\"gamePly\""));
    assert!(!payload.contains("\"facts\""));
    assert!(!payload.contains("\"sessionId\""));
    assert!(!payload.contains("\"schemaVersion\""));
    assert!(
        super::firestore::encoded_moment_bytes(&entries.entries[0], &entries.game).unwrap()
            <= MAX_REVIEW_MOMENT_DOCUMENT_BYTES
    );
}

#[derive(Debug)]
struct MaximumSessionMeasurement {
    game_plies: usize,
    moment_count: usize,
    largest_evidence_variant: EvidenceVariant,
    provider_evidence_count: usize,
    committed_alternative_count: usize,
    completed_coach_turn_count: usize,
    maximum_moment_payload: String,
    maximum_moment_payload_bytes: usize,
    creation_write_count: usize,
}

fn measure_maximum_session() -> MaximumSessionMeasurement {
    let largest_evidence_variant = largest_evidence_variant();
    let payload = maximum_moment_payload(largest_evidence_variant);
    let maximum_moment_payload =
        serde_json::to_string(&payload).expect("the measurement payload is serializable");
    let moment_count = adaptive_target(MAX_GAME_PLIES);

    MaximumSessionMeasurement {
        game_plies: MAX_GAME_PLIES,
        moment_count,
        largest_evidence_variant,
        provider_evidence_count: payload.provider_evidence.len(),
        committed_alternative_count: payload.committed_alternatives.len(),
        completed_coach_turn_count: payload.completed_coach_turns.len(),
        maximum_moment_payload_bytes: maximum_moment_payload.len(),
        maximum_moment_payload,
        // One create per cache entry, and no session root to write alongside.
        creation_write_count: moment_count,
    }
}

fn maximum_moment_payload(largest_evidence_variant: EvidenceVariant) -> MeasuredMomentPayload {
    MeasuredMomentPayload {
        published_comment: MeasuredPublishedComment {
            text: maximum_text(),
        },
        committed_alternatives: (0..usize::from(
            ReviewSessionLimits::V1.max_committed_alternative_moves,
        ))
            .map(maximum_committed_alternative)
            .collect(),
        completed_coach_turns: (0..usize::from(ReviewSessionLimits::V1.max_started_coach_turns))
            .map(maximum_completed_coach_turn)
            .collect(),
        provider_evidence: (0..maximum_provider_evidence_entries())
            .map(|index| provider_evidence(largest_evidence_variant, index))
            .collect(),
    }
}

fn maximum_provider_evidence_entries() -> usize {
    MAX_REVIEW_MOMENT_CACHED_EVIDENCE_ENTRIES
        + usize::from(ReviewSessionLimits::V1.max_committed_alternative_moves)
        + INITIAL_PROVENANCE_EVIDENCE_ENTRIES
}

fn maximum_committed_alternative(index: usize) -> MeasuredCommittedAlternative {
    MeasuredCommittedAlternative {
        alternative_move_id: maximum_semantic_id("alternative", index),
        branch_ref: maximum_semantic_id("branch", index),
        parent: MeasuredBranchParent::Move {
            branch_ref: maximum_semantic_id("parent", index),
        },
        source_position: maximum_alternative_position(),
        move_uci: maximum_uci(),
        resulting_position: maximum_alternative_position(),
        evaluation: MeasuredAlternativeEvaluation {
            selected_centipawns: i32::MIN,
            best_move_uci: maximum_uci(),
            best_centipawns: i32::MAX,
            loss_centipawns: u32::MAX,
        },
        strongest_reply_uci: maximum_uci(),
    }
}

fn maximum_completed_coach_turn(index: usize) -> MeasuredCompletedCoachTurn {
    let dimension = |reference_count| MeasuredAssessmentDimension {
        explanation: maximum_text(),
        evidence_refs: (0..reference_count)
            .map(|offset| digest(index * 10 + offset))
            .collect(),
    };
    MeasuredCompletedCoachTurn {
        coach_turn_id: maximum_semantic_id("coach-turn", index),
        alternative_move_id: maximum_semantic_id("alternative", index),
        objective_quality: dimension(3),
        findability: dimension(2),
        resilience: dimension(3),
    }
}

fn largest_evidence_variant() -> EvidenceVariant {
    [
        EvidenceVariant::Position,
        EvidenceVariant::EngineAnalysis,
        EvidenceVariant::Branch,
        EvidenceVariant::Provenance,
    ]
    .into_iter()
    .max_by_key(|variant| {
        serde_json::to_vec(&provider_evidence(*variant, 0))
            .expect("the evidence measurement is serializable")
            .len()
    })
    .expect("the evidence shape has variants")
}

fn provider_evidence(variant: EvidenceVariant, index: usize) -> MeasuredProviderEvidence {
    let metadata = evidence_metadata(index);
    match variant {
        EvidenceVariant::Position => MeasuredProviderEvidence::Position {
            metadata,
            position: maximum_alternative_position(),
        },
        EvidenceVariant::EngineAnalysis => MeasuredProviderEvidence::EngineAnalysis {
            metadata,
            position: maximum_alternative_position(),
            evaluation_centipawns: i32::MIN,
            best_move_uci: maximum_uci(),
            principal_variation: vec![maximum_uci(); usize::from(PINNED_STOCKFISH_DEPTH)],
        },
        EvidenceVariant::Branch => MeasuredProviderEvidence::Branch {
            metadata,
            branch_ref: maximum_semantic_id("branch", index),
            parent: MeasuredBranchParent::Move {
                branch_ref: maximum_semantic_id("parent", index),
            },
            source_position: maximum_alternative_position(),
            move_uci: maximum_uci(),
            resulting_position: maximum_alternative_position(),
        },
        EvidenceVariant::Provenance => MeasuredProviderEvidence::Provenance {
            metadata,
            provider: maximum_provenance(),
        },
    }
}

fn evidence_metadata(index: usize) -> MeasuredEvidenceMetadata {
    MeasuredEvidenceMetadata {
        evidence_id: digest(index * 10),
        dependencies: vec![digest(index * 10 + 1), digest(index * 10 + 2)],
        content_digest: digest(index * 10 + 3),
        provenance: maximum_provenance(),
    }
}

fn maximum_provenance() -> MeasuredEvidenceProvenance {
    MeasuredEvidenceProvenance::Stockfish {
        version: "Stockfish 18 NNUE".to_string(),
        binary_digest: digest(1),
        depth: u8::MAX,
        threads: u8::MAX,
        hash_mib: u16::MAX,
    }
}

fn maximum_alternative_position() -> MeasuredDerivedPosition {
    MeasuredDerivedPosition::AlternativeLine {
        root_ply: u16::try_from(MAX_GAME_PLIES).expect("the game limit fits a ply"),
        uci_path: vec![maximum_uci(); usize::from(ReviewSessionLimits::V1.max_branch_depth_plies)],
    }
}

fn maximum_text() -> String {
    "x".repeat(usize::from(
        ReviewSessionLimits::V1.max_player_message_bytes,
    ))
}

fn maximum_uci() -> String {
    "a7a8q".to_string()
}

fn maximum_semantic_id(prefix: &str, index: usize) -> String {
    let stem = format!("{prefix}:{index:04}:");
    format!("{stem}{}", "x".repeat(128 - stem.len()))
}

fn digest(index: usize) -> String {
    format!("sha256:{index:064x}")
}

#[test]
fn measurement_exercises_both_derived_position_encodings() {
    let root = MeasuredDerivedPosition::GamePly {
        ply: u16::try_from(MAX_GAME_PLIES).expect("the game limit fits a ply"),
    };
    let root_parent = MeasuredBranchParent::Root {
        ply: u16::try_from(MAX_GAME_PLIES).expect("the game limit fits a ply"),
    };
    let alternative = maximum_alternative_position();
    let root_json = serde_json::to_string(&root).expect("the root position is serializable");
    let root_parent_json =
        serde_json::to_string(&root_parent).expect("the root parent is serializable");
    let alternative_json =
        serde_json::to_string(&alternative).expect("the alternative position is serializable");

    assert!(root_json.contains("\"ply\":400"));
    assert!(root_parent_json.contains("\"kind\":\"root\""));
    assert!(alternative_json.contains("\"uciPath\""));
    assert!(!root_json.contains("occupied") && !alternative_json.contains("occupied"));
}
