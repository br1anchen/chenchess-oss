//! Fact Shape coverage over the pinned corpus.
//!
//! An Exemplar is the one recorded Review Moment a measurement uses for a Fact
//! Shape. The corpus is the authority: these hold that the recorded resolution
//! still names moments the corpus records, that they still digest to what was
//! recorded, and that a moment whose shape moves is reported stale rather than
//! silently re-pointed.

use std::{fs, path::PathBuf};

use chen_chess_coach_engine::pipeline_evaluation::{
    read_resolution, resolve, verify_resolution, FactShapeResolutionError, StaleReason,
};

fn evaluation(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("evaluation")
        .join(relative)
}

fn corpus() -> PathBuf {
    evaluation("corpus")
}

#[test]
fn every_recorded_exemplar_still_resolves_against_the_corpus() {
    let resolution =
        read_resolution(&evaluation("corpus/fact-shape-resolution.json")).expect("readable");
    assert!(
        !resolution.exemplars.is_empty(),
        "the recorded resolution should name an Exemplar for every Fact Shape the corpus holds"
    );

    if let Err(error) = verify_resolution(&corpus(), &resolution) {
        panic!("{error}");
    }
}

#[test]
fn re_resolving_an_unchanged_corpus_reproduces_the_recorded_resolution() {
    let path = evaluation("corpus/fact-shape-resolution.json");
    let recorded = fs::read_to_string(&path).expect("the recorded resolution should be readable");
    let resolution = read_resolution(&path).expect("the recorded resolution should parse");

    let resolved = resolve(&corpus(), Some(&resolution)).expect("the corpus should resolve");
    let mut rewritten =
        serde_json::to_vec_pretty(&resolved).expect("a resolution should serialize");
    rewritten.push(b'\n');

    assert_eq!(
        String::from_utf8(rewritten).expect("a resolution is UTF-8"),
        recorded,
        "re-resolving an unchanged corpus should rewrite the resolution byte for byte"
    );
}

/// An Exemplar addresses a moment by shape, so the one thing that must never
/// pass quietly is the moment no longer being the shape it was recorded for.
#[test]
fn an_exemplar_whose_recorded_facts_moved_is_reported_stale() {
    let mut resolution =
        read_resolution(&evaluation("corpus/fact-shape-resolution.json")).expect("readable");
    let shape = resolution
        .exemplars
        .keys()
        .next()
        .expect("the corpus holds at least one shape")
        .clone();
    resolution
        .exemplars
        .get_mut(&shape)
        .expect("the shape was just read")
        .facts_digest = format!("sha256:{}", "0".repeat(64));

    let error = verify_resolution(&corpus(), &resolution)
        .expect_err("a moved facts digest is never silently accepted");

    let FactShapeResolutionError::StaleResolution(stale) = error else {
        panic!("a moved digest should be reported as a stale resolution");
    };
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].shape, shape);
    assert!(matches!(
        stale[0].reason,
        StaleReason::FactsDigestMoved { .. }
    ));
}
