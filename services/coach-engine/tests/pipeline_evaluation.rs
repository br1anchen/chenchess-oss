use std::{fs, future::Future, path::Path, pin::Pin, sync::Arc};

use chen_chess_coach_engine::{
    engine_analysis::{
        EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer,
        PositionEvaluation,
    },
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction},
    pipeline_evaluation::{
        accept_fast_evaluation, accept_live_evaluation, run_fast_evaluation, run_live_evaluation,
        LiveProviderProvenance,
    },
    review_facts::ReviewFactsService,
    types::HumanMoveCandidate,
};
use serde_json::json;

#[test]
fn accepted_recorded_evidence_runs_without_live_providers_and_matches_exactly() {
    let root = test_root("matches");
    write_case(&root);

    let accepted = accept_fast_evaluation(&root).expect("acceptance should write the baseline");
    assert_eq!(accepted.changed_cases, vec!["selected-quiet-move"]);
    assert!(root.join("last-accepted.diff").is_file());

    let report = run_fast_evaluation(&root).expect("recorded evidence should evaluate locally");

    assert!(report.differences.is_empty());
    fs::remove_dir_all(root).ok();
}

#[test]
fn changed_fact_fields_fail_with_json_pointer_differences() {
    let root = test_root("diff");
    write_case(&root);
    accept_fast_evaluation(&root).expect("initial baseline should be accepted");
    let baseline = root.join("selected-quiet-move.baseline.json");
    let original = fs::read(&baseline).expect("baseline should exist");
    for (pointer, replacement) in [
        ("/facts/selectedMoment/ply", json!(9)),
        ("/facts/selectedMoment/category", json!("tactical")),
        (
            "/facts/selectedMoment/objective/bestEvaluation/value",
            json!(999),
        ),
        (
            "/facts/selectedMoment/human/mostLikelyProbability",
            json!(0.01),
        ),
        ("/facts/selectedMoment/human/playedMoveRank", json!(99)),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&original).expect("baseline should be JSON");
        *value
            .pointer_mut(pointer)
            .expect("test pointer should exist in the baseline") = replacement;
        fs::write(
            &baseline,
            serde_json::to_vec_pretty(&value).expect("changed baseline should serialize"),
        )
        .expect("changed baseline should be written");

        let report = run_fast_evaluation(&root).expect("differences should be reported, not crash");
        assert_eq!(report.differences.len(), 1);
        assert!(report.differences[0].field_diff.contains(pointer));
        let drift = report
            .require_clean()
            .expect_err("a baseline difference must fail clean evaluation");
        assert_eq!(drift.differing_cases, 1);
    }
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn oversized_live_case_fails_before_provider_work() {
    let root = test_root("oversized-live");
    write_case(&root);
    let case_path = root.join("selected-quiet-move.case.json");
    let mut case: serde_json::Value =
        serde_json::from_slice(&fs::read(&case_path).unwrap()).unwrap();
    let moves = (1..=201)
        .map(|move_number| {
            if move_number % 2 == 1 {
                format!("{move_number}. Nf3 Nf6")
            } else {
                format!("{move_number}. Ng1 Ng8")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    case["input"]["pgn"] = json!(format!("{moves} *"));
    case["input"]["operation"] = json!({ "kind": "game", "reviewSide": "both" });
    fs::write(&case_path, serde_json::to_vec_pretty(&case).unwrap()).unwrap();
    let service = ReviewFactsService::new(Arc::new(NeverEngine), Arc::new(NeverHuman));

    let error = run_live_evaluation(
        &root,
        &service,
        LiveProviderProvenance {
            stockfish: "never".to_string(),
            maia_image: "never".to_string(),
            maia_package: "never".to_string(),
            maia_model: "never".to_string(),
        },
    )
    .await
    .expect_err("oversized live Games must fail before providers");

    assert!(error.to_string().contains("402 plies"));
    fs::remove_dir_all(root).ok();
}

#[test]
fn empty_corpus_and_unused_selected_evidence_are_rejected() {
    let empty = test_root("empty");
    fs::remove_dir_all(&empty).ok();
    fs::create_dir_all(&empty).expect("empty corpus should be created");
    let error = run_fast_evaluation(&empty).expect_err("empty corpus must not report success");
    assert!(error.to_string().contains("contains no cases"));
    fs::remove_dir_all(empty).ok();

    let root = test_root("extra-selected-evidence");
    write_case(&root);
    let case_path = root.join("selected-quiet-move.case.json");
    let mut case: serde_json::Value =
        serde_json::from_slice(&fs::read(&case_path).expect("case should exist"))
            .expect("case should be JSON");
    let extra = case["evidence"][0].clone();
    case["evidence"]
        .as_array_mut()
        .expect("evidence should be an array")
        .push(extra);
    fs::write(
        case_path,
        serde_json::to_vec_pretty(&case).expect("case should serialize"),
    )
    .expect("case should be written");

    let error =
        accept_fast_evaluation(&root).expect_err("unused selected evidence must be rejected");
    assert!(error.to_string().contains("exactly one evidence record"));
    fs::remove_dir_all(root).ok();

    let root = test_root("unsafe-id");
    write_case(&root);
    let case_path = root.join("selected-quiet-move.case.json");
    let mut case: serde_json::Value =
        serde_json::from_slice(&fs::read(&case_path).expect("case should exist"))
            .expect("case should be JSON");
    case["id"] = json!("../outside");
    fs::write(
        case_path,
        serde_json::to_vec_pretty(&case).expect("case should serialize"),
    )
    .expect("case should be written");

    let error = accept_fast_evaluation(&root).expect_err("unsafe ids must not form paths");
    assert!(error.to_string().contains("invalid evaluation case id"));
    assert!(!root
        .parent()
        .expect("test root should have a parent")
        .join("outside.baseline.json")
        .exists());
    fs::remove_dir_all(root).ok();

    let root = test_root("identity");
    write_case(&root);
    accept_fast_evaluation(&root).expect("initial baseline should be accepted");
    fs::write(root.join("orphan.baseline.json"), b"{}").expect("orphan baseline should be written");
    let error = run_fast_evaluation(&root).expect_err("orphan baselines must fail");
    assert!(error.to_string().contains("has no matching case"));
    fs::remove_file(root.join("orphan.baseline.json")).expect("orphan should be removed");

    let case_path = root.join("selected-quiet-move.case.json");
    let mut case: serde_json::Value =
        serde_json::from_slice(&fs::read(&case_path).expect("case should exist"))
            .expect("case should be JSON");
    case["id"] = json!("different-safe-id");
    fs::write(
        case_path,
        serde_json::to_vec_pretty(&case).expect("case should serialize"),
    )
    .expect("case should be written");
    let error = run_fast_evaluation(&root).expect_err("filename must own case identity");
    assert!(error
        .to_string()
        .contains("does not match filename identity"));
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn live_acceptance_refreshes_normalized_evidence_and_fact_baselines() {
    let root = test_root("live-acceptance");
    write_case(&root);
    let service = ReviewFactsService::new(Arc::new(LiveEngine), Arc::new(LiveHuman));
    let provenance = LiveProviderProvenance {
        stockfish: "Stockfish 18 depth 16".to_string(),
        maia_image: "maia@sha256:live".to_string(),
        maia_package: "maia2==0.11.0".to_string(),
        maia_model: "rapid".to_string(),
    };

    let accepted = accept_live_evaluation(&root, &service, provenance.clone())
        .await
        .expect("explicit live acceptance should refresh evidence and facts");
    assert_eq!(accepted.changed_cases, vec!["selected-quiet-move"]);

    let case: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("selected-quiet-move.case.json")).expect("case should be rewritten"),
    )
    .expect("case should remain JSON");
    assert_eq!(case["provenance"]["stockfish"], provenance.stockfish);
    assert_eq!(case["evidence"][0]["engineBefore"]["bestMove"], "e2e4");
    assert_eq!(case["evidence"][0]["afterMove"]["evaluation"]["value"], -30);

    let report = run_live_evaluation(&root, &service, provenance)
        .await
        .expect("accepted live providers should match their baselines");
    assert!(report.differences.is_empty());
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn live_tolerances_allow_numeric_drift_but_not_changed_moves_or_facts() {
    let root = test_root("live-tolerances");
    write_case(&root);
    let provenance = LiveProviderProvenance {
        stockfish: "Stockfish 18 depth 16".to_string(),
        maia_image: "maia@sha256:live".to_string(),
        maia_package: "maia2==0.11.0".to_string(),
        maia_model: "rapid".to_string(),
    };
    let baseline_service = ReviewFactsService::new(Arc::new(LiveEngine), Arc::new(LiveHuman));
    accept_live_evaluation(&root, &baseline_service, provenance.clone())
        .await
        .unwrap();

    let numeric_drift = ReviewFactsService::new(
        Arc::new(DriftEngine {
            best_move: "e2e4",
            centipawn_delta: 5,
        }),
        Arc::new(DriftHuman {
            probability: 0.47,
            rank: 1,
        }),
    );
    let tolerated = run_live_evaluation(&root, &numeric_drift, provenance.clone())
        .await
        .expect("numeric drift should produce a report");
    assert!(
        tolerated.differences.is_empty(),
        "unexpected tolerated drift: {:?}",
        tolerated.differences
    );

    let changed_move = ReviewFactsService::new(
        Arc::new(DriftEngine {
            best_move: "d2d4",
            centipawn_delta: 0,
        }),
        Arc::new(DriftHuman {
            probability: 0.45,
            rank: 1,
        }),
    );
    let structural = run_live_evaluation(&root, &changed_move, provenance.clone())
        .await
        .expect("structural drift should produce a report");
    assert_eq!(structural.differences.len(), 1);
    assert!(structural.differences[0]
        .field_diff
        .contains("/evidence/0/engineBefore/bestMove"));

    let case_path = root.join("selected-quiet-move.case.json");
    let mut case: serde_json::Value =
        serde_json::from_slice(&fs::read(&case_path).unwrap()).unwrap();
    case["evidence"][0]["humanBefore"]["candidates"][0]["rank"] = json!(2);
    fs::write(&case_path, serde_json::to_vec_pretty(&case).unwrap()).unwrap();
    let changed_rank = run_live_evaluation(&root, &baseline_service, provenance.clone())
        .await
        .expect("rank drift should produce a report");
    assert!(changed_rank.differences[0]
        .field_diff
        .contains("/evidence/0/humanBefore/candidates/0/rank"));
    case["evidence"][0]["humanBefore"]["candidates"][0]["rank"] = json!(1);
    fs::write(&case_path, serde_json::to_vec_pretty(&case).unwrap()).unwrap();

    let baseline_path = root.join("selected-quiet-move.baseline.json");
    let mut facts: serde_json::Value =
        serde_json::from_slice(&fs::read(&baseline_path).unwrap()).unwrap();
    facts["facts"]["selectedMoment"]["category"] = json!("tactical");
    fs::write(&baseline_path, serde_json::to_vec_pretty(&facts).unwrap()).unwrap();
    let changed_fact = run_live_evaluation(&root, &baseline_service, provenance.clone())
        .await
        .expect("fact drift should produce a report");
    assert!(changed_fact.differences[0]
        .field_diff
        .contains("/facts/selectedMoment/category"));
    facts["facts"]["selectedMoment"]["category"] = json!("positional");
    facts["facts"]["selectedMoment"]["ply"] = json!(2);
    fs::write(&baseline_path, serde_json::to_vec_pretty(&facts).unwrap()).unwrap();
    let changed_selection = run_live_evaluation(&root, &baseline_service, provenance)
        .await
        .expect("selected-ply drift should produce a report");
    assert!(changed_selection.differences[0]
        .field_diff
        .contains("/facts/selectedMoment/ply"));
    fs::remove_dir_all(root).ok();
}

struct LiveEngine;

impl EngineAnalyzer for LiveEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            let evaluation = if input.position.contains(" b ") {
                -30
            } else {
                30
            };
            Ok(EngineAnalysis {
                best_move: "e2e4".to_string(),
                evaluation: PositionEvaluation::Centipawns(evaluation),
                principal_variation: vec!["e2e4".to_string()],
                depth: 16,
            })
        })
    }
}

struct LiveHuman;

impl HumanMoveModel for LiveHuman {
    fn predict<'a>(
        &'a self,
        _input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        Box::pin(async {
            Ok(HumanMovePrediction {
                candidates: vec![HumanMoveCandidate {
                    uci: "e2e4".to_string(),
                    probability: 0.45,
                    rank: 1,
                }],
                win_probability: Some(0.5),
            })
        })
    }
}

struct DriftEngine {
    best_move: &'static str,
    centipawn_delta: i32,
}

impl EngineAnalyzer for DriftEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            let baseline = if input.position.contains(" b ") {
                -30
            } else {
                30
            };
            Ok(EngineAnalysis {
                best_move: self.best_move.to_string(),
                evaluation: PositionEvaluation::Centipawns(baseline + self.centipawn_delta),
                principal_variation: vec![self.best_move.to_string()],
                depth: 16,
            })
        })
    }
}

struct DriftHuman {
    probability: f64,
    rank: usize,
}

struct NeverEngine;

impl EngineAnalyzer for NeverEngine {
    fn analyze<'a>(
        &'a self,
        _input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        panic!("oversized Game reached Stockfish")
    }
}

struct NeverHuman;

impl HumanMoveModel for NeverHuman {
    fn predict<'a>(
        &'a self,
        _input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        panic!("oversized Game reached Maia")
    }
}

impl HumanMoveModel for DriftHuman {
    fn predict<'a>(
        &'a self,
        _input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        Box::pin(async move {
            Ok(HumanMovePrediction {
                candidates: vec![HumanMoveCandidate {
                    uci: "e2e4".to_string(),
                    probability: self.probability,
                    rank: self.rank,
                }],
                win_probability: Some(0.5),
            })
        })
    }
}

/// The corpus is the gate that never saw a MultiPV comparison, which is how a Ranked
/// Alternative shipped restating rank one's own absolute score (ADR 0041). Evaluating a
/// recorded comparison must publish the gap and nothing else.
#[test]
fn a_recorded_comparison_publishes_gaps_and_never_the_multi_pv_absolute() {
    let root = test_root("recorded-comparison");
    let case = copy_corpus_case(&root, "tactical-white-human-likely");

    accept_fast_evaluation(&root).expect("the recorded comparison should evaluate");
    let baseline: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("tactical-white-human-likely.baseline.json")).unwrap(),
    )
    .unwrap();

    let proof = &baseline["decisionExplanations"][0];
    assert_eq!(proof["ply"], 1);
    assert_eq!(proof["kind"], "durable");
    let evidence = &proof["candidateEvidence"];
    assert_eq!(evidence["kind"], "multiPv");
    assert_eq!(
        evidence["authoritativeSinglePv"]["evaluation"],
        case["evidence"][0]["engineBefore"]["evaluation"]["value"]
            .as_i64()
            .map(|value| json!({
                "kind": "centipawns",
                "perspective": "white",
                "value": value
            }))
            .unwrap()
    );

    let recorded = &case["multiPvEvidence"][0]["output"]["variations"];
    let best = recorded[0]["analysis"]["evaluation"]["value"]
        .as_i64()
        .unwrap();
    for (index, alternative) in evidence["rankedAlternatives"]
        .as_array()
        .expect("the alternatives should be an array")
        .iter()
        .enumerate()
    {
        let recorded = &recorded[index + 1]["analysis"];
        assert_eq!(alternative["rank"], index + 2);
        assert_eq!(alternative["rootMoveUci"], recorded["bestMove"]);
        assert_eq!(
            alternative["gap"],
            json!({
                "kind": "centipawns",
                "behindBest": best - recorded["evaluation"]["value"].as_i64().unwrap()
            })
        );
    }

    let published = serde_json::to_string(&baseline).unwrap();
    assert!(
        !published.contains(&format!("\"value\":{best}")),
        "the MultiPV rank-one absolute {best} reached the published review"
    );
    fs::remove_dir_all(root).ok();
}

/// A Critical Moment that reaches candidate comparison with no recorded search must fail
/// the gate: silently dropping the comparison is how the corpus stopped covering it.
#[test]
fn a_compared_moment_without_a_recorded_search_fails_the_gate() {
    let root = test_root("missing-comparison");
    let mut case = copy_corpus_case(&root, "tactical-white-human-likely");
    let case_path = root.join("tactical-white-human-likely.case.json");

    let recorded = case["multiPvEvidence"].take();
    case["multiPvEvidence"] = json!([]);
    fs::write(&case_path, serde_json::to_vec_pretty(&case).unwrap()).unwrap();
    let failure = run_fast_evaluation(&root).expect_err("a missing comparison should fail");
    assert!(
        failure
            .to_string()
            .contains("reached candidate comparison with no recorded MultiPV evidence"),
        "unexpected failure: {failure}"
    );

    case["multiPvEvidence"] = recorded;
    case["multiPvEvidence"][0]["ply"] = json!(3);
    fs::write(&case_path, serde_json::to_vec_pretty(&case).unwrap()).unwrap();
    let failure = run_fast_evaluation(&root).expect_err("an unused comparison should fail");
    assert!(
        failure
            .to_string()
            .contains("belongs to no compared Critical Moment"),
        "unexpected failure: {failure}"
    );

    // Only the first recording for a ply is read, so a duplicate would ride in unexercised.
    case["multiPvEvidence"][0]["ply"] = json!(1);
    let recorded = case["multiPvEvidence"][0].clone();
    case["multiPvEvidence"] = json!([recorded.clone(), recorded]);
    fs::write(&case_path, serde_json::to_vec_pretty(&case).unwrap()).unwrap();
    let failure = run_fast_evaluation(&root).expect_err("a duplicate comparison should fail");
    assert!(
        failure
            .to_string()
            .contains("carries more than one recorded MultiPV search"),
        "unexpected failure: {failure}"
    );
    fs::remove_dir_all(root).ok();
}

/// Dropping the authoritative provenance would leave a Game case with Critical Moments and
/// no comparison at all — the exact coverage hole, restored by omission.
#[test]
fn a_game_case_with_moments_and_no_engine_provenance_fails_the_gate() {
    let root = test_root("missing-provenance");
    let mut case = copy_corpus_case(&root, "tactical-white-human-likely");
    let case_path = root.join("tactical-white-human-likely.case.json");

    case.as_object_mut()
        .expect("the case should be a JSON object")
        .remove("engineProvenance");
    case["multiPvEvidence"] = json!([]);
    fs::write(&case_path, serde_json::to_vec_pretty(&case).unwrap()).unwrap();

    let failure = run_fast_evaluation(&root).expect_err("missing provenance should fail");
    assert!(
        failure
            .to_string()
            .contains("records no authoritative engine provenance"),
        "unexpected failure: {failure}"
    );
    fs::remove_dir_all(root).ok();
}

fn copy_corpus_case(root: &Path, case_id: &str) -> serde_json::Value {
    fs::remove_dir_all(root).ok();
    fs::create_dir_all(root).expect("test corpus should be created");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("evaluation/corpus")
        .join(format!("{case_id}.case.json"));
    let bytes = fs::read(&source).expect("the corpus case should be readable");
    fs::write(root.join(format!("{case_id}.case.json")), &bytes)
        .expect("the corpus case should be copied");
    serde_json::from_slice(&bytes).expect("the corpus case should be JSON")
}

fn write_case(root: &Path) {
    fs::remove_dir_all(root).ok();
    fs::create_dir_all(root).expect("test corpus should be created");
    let case = json!({
        "id": "selected-quiet-move",
        "description": "A player-selected sound move omitted by automatic selection.",
        "input": {
            "pgn": "1. e4 *",
            "elo": 1200,
            "operation": { "kind": "selectedMoment", "selectedPly": 1 }
        },
        "provenance": {
            "stockfish": "recorded-test-engine",
            "maiaImage": "recorded-test-image@sha256:0000",
            "maiaPackage": "maia2==0.11.0",
            "maiaModel": "rapid-test"
        },
        "evidence": [{
            "ply": 1,
            "engineBefore": {
                "bestMove": "e2e4",
                "evaluation": { "kind": "centipawns", "value": 30 },
                "principalVariation": ["e2e4"],
                "depth": 16
            },
            "afterMove": {
                "kind": "analyzed",
                "evaluation": { "kind": "centipawns", "value": -30 }
            },
            "humanBefore": {
                "candidates": [{ "uci": "e2e4", "probability": 0.45, "rank": 1 }],
                "winProbability": 0.5
            }
        }]
    });
    fs::write(
        root.join("selected-quiet-move.case.json"),
        serde_json::to_vec_pretty(&case).expect("case should serialize"),
    )
    .expect("case should be written");
}

fn test_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("chenchess-eval-{name}-{}", std::process::id()))
}
