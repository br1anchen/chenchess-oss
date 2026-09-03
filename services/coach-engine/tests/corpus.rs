//! The pinned evaluation corpus, replayed.
//!
//! Its own test target rather than part of `domain` because it is the one test
//! whose cost grows with the corpus: it replays every recorded case through the
//! Rule Extractor, and the corpus grows whenever a Fact Shape needs an
//! Exemplar. Its cost is proportional to the corpus, which was most of what a
//! `domain` run cost and all of what made that run too slow for an edit loop.
//!
//! It is not optional. `chenchess-rust#test` names this target beside `domain`,
//! `session`, `boundary` and `runtime`, so the release gate runs it; what
//! changes is that `cargo test --test domain` no longer waits for it.

use std::{fs, path::Path};

use chen_chess_coach_engine::pipeline_evaluation::run_fast_evaluation;
use serde_json::json;

#[test]
fn repository_corpus_matches_all_pinned_baselines() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("evaluation/corpus");

    let report = run_fast_evaluation(&corpus).expect("repository corpus should evaluate");

    assert_eq!(report.evaluated_cases, 10);
    assert!(report.differences.is_empty());
}

#[test]
fn repository_corpus_preserves_its_certified_scenario_coverage() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("evaluation/corpus");
    let read = |case_id: &str, kind: &str| -> serde_json::Value {
        let path = corpus.join(format!("{case_id}.{kind}.json"));
        serde_json::from_slice(&fs::read(&path).expect("corpus artifact should exist"))
            .expect("corpus artifact should be JSON")
    };
    let case_ids = [
        "advanced-both-threshold",
        "beginner-below-threshold",
        "positional-black-intermediate",
        "selected-nonautomatic",
        "selected-terminal-mate",
        "tactical-white-human-likely",
    ];
    let first_case = read(case_ids[0], "case");
    let published_image = first_case["provenance"]["maiaImage"]
        .as_str()
        .expect("Maia provenance should be a string");
    let image_digest = published_image
        .strip_prefix("maia-runtime@sha256:")
        .expect("Maia provenance should pin the recorded runtime image");
    assert_eq!(image_digest.len(), 64);
    assert!(image_digest.bytes().all(|byte| byte.is_ascii_hexdigit()));

    for case_id in case_ids {
        let case = read(case_id, "case");
        let baseline = read(case_id, "baseline");
        assert_eq!(case["provenance"]["maiaImage"], published_image);
        assert_eq!(baseline["provenance"]["maiaImage"], published_image);
    }

    let advanced = read("advanced-both-threshold", "baseline");
    assert_eq!(
        advanced["facts"]["criticalMoments"][0]["category"],
        "positional"
    );
    assert!(
        advanced["facts"]["criticalMoments"][0]["objective"]["centipawnLoss"]
            .as_u64()
            .is_some_and(|loss| loss >= 70)
    );

    let beginner_case = read("beginner-below-threshold", "case");
    let beginner = read("beginner-below-threshold", "baseline");
    assert_eq!(beginner["facts"]["criticalMoments"], json!([]));

    let black_case = read("positional-black-intermediate", "case");
    let black = read("positional-black-intermediate", "baseline");
    assert_eq!(beginner_case["input"]["pgn"], black_case["input"]["pgn"]);
    for index in 0..2 {
        assert_eq!(
            beginner_case["evidence"][index]["engineBefore"],
            black_case["evidence"][index]["engineBefore"]
        );
        assert_eq!(
            beginner_case["evidence"][index]["afterMove"],
            black_case["evidence"][index]["afterMove"]
        );
    }
    assert_eq!(black["facts"]["criticalMoments"][0]["side"], "black");
    assert_eq!(
        black["facts"]["criticalMoments"][0]["category"],
        "positional"
    );
    assert_eq!(
        black["facts"]["criticalMoments"][0]["objective"]["centipawnLoss"],
        125
    );

    let selected_case = read("selected-nonautomatic", "case");
    let selected = read("selected-nonautomatic", "baseline");
    assert_eq!(
        selected_case["input"]["operation"]["kind"],
        "selectedMoment"
    );
    assert!(
        selected["facts"]["selectedMoment"]["objective"]["centipawnLoss"]
            .as_u64()
            .is_some_and(|loss| loss < 100)
    );

    let terminal = read("selected-terminal-mate", "baseline");
    assert!(terminal["facts"]["selectedMoment"]["objective"]["playedEvaluation"].is_null());

    let tactical = read("tactical-white-human-likely", "baseline");
    assert_eq!(
        tactical["facts"]["criticalMoments"][0]["category"],
        "tactical"
    );
    assert_eq!(
        tactical["facts"]["criticalMoments"][0]["human"]["playedMoveIsHumanLikely"],
        true
    );
    assert_eq!(
        tactical["facts"]["criticalMoments"][0]["human"]["playedMoveRank"],
        1
    );
}
