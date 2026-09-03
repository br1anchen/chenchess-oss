use std::{
    fs,
    path::{Path, PathBuf},
};

use chen_chess_coach_engine::review_session_contract::{
    EvidenceEntry, ReviewSessionCoreContract, ReviewSessionEvidencePacket,
};
use serde::de::DeserializeOwned;

fn fixture<T: DeserializeOwned>(name: &str) -> T {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("API has a repository parent")
        .join("packages/coach-engine-sdk/fixtures");
    serde_json::from_slice(&fs::read(root.join(name)).expect("fixture is readable"))
        .expect("fixture is valid JSON")
}

#[test]
fn canonical_contract_keeps_evidence_server_owned_and_referenceable() {
    let core: ReviewSessionCoreContract = fixture("core-contract.json");
    let packet: ReviewSessionEvidencePacket = fixture("evidence-packet.json");

    assert_eq!(core.evidence_packet, packet);
    assert!(core
        .coach_turn_context
        .required_evidence_refs
        .iter()
        .all(|reference| {
            packet
                .entries
                .iter()
                .any(|entry| &entry.metadata().evidence_id == reference)
        }));
}

#[test]
fn evidence_entry_rejects_non_normalized_dependencies() {
    let mut entry: serde_json::Value = fixture("evidence-packet.json");
    let position_id = entry["entries"][0]["metadata"]["evidenceId"].clone();
    entry["entries"][0]["metadata"]["dependencies"] =
        serde_json::Value::Array(vec![position_id.clone(), position_id]);
    assert!(serde_json::from_value::<EvidenceEntry>(entry["entries"][0].clone()).is_err());
}
