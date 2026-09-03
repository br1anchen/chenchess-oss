//! Measure the pinned Stockfish and Maia primitives used by Review Sessions.
//!
//! This binary deliberately measures provider calls, not unimplemented Review Session workflows.
//! Composite workflow budgets can be derived from these distributions and their explicit
//! provider-call counts.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{bail, ensure, Context, Result};
use chen_chess_coach_engine::{
    engine_analysis::{
        EngineAnalysis, EngineAnalysisInput, EngineAnalyzer, PositionEvaluation, StockfishAdapter,
    },
    human_move_model::{HumanMoveModel, HumanMovePrediction, MaiaHttpAdapter},
    types::EloProfile,
};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tokio::{io::AsyncWriteExt, process::Command, task::JoinSet, time::sleep};

const POSITIONS: [(&str, &str); 6] = [
    (
        "initial",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    ),
    (
        "opening-tactic",
        "r1bqkbnr/pppp1ppp/8/4p3/2BnP3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
    ),
    (
        "quiet-middlegame",
        "r2q1rk1/ppp2ppp/2npbn2/8/2BPP3/2P2N2/PP3PPP/R1BQR1K1 w - - 4 10",
    ),
    (
        "complex-middlegame",
        "r3r1k1/pp1q1ppp/2pb1nn1/8/2BP4/2N1PN2/PPQ2PPP/R1B2RK1 w - - 4 12",
    ),
    ("pawn-endgame", "8/5pk1/6p1/3p4/3P4/5KP1/5P2/8 w - - 0 40"),
    ("forced-mate", "7k/6pp/8/7Q/8/8/8/4K3 w - - 0 1"),
];
const CANDIDATE_WIDTHS: [usize; 8] = [1, 2, 3, 4, 5, 8, 12, 20];
const MAIA_LIMIT: u8 = 20;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    stockfish: Option<PathBuf>,
    #[arg(long)]
    skip_stockfish: bool,
    #[arg(long, default_value = "12,14,16,18")]
    depths: String,
    #[arg(long, default_value_t = 3)]
    stockfish_repeats: usize,
    #[arg(long, default_value_t = 10)]
    cancellation_repeats: usize,
    #[arg(long)]
    maia_base_url: Option<String>,
    #[arg(long, default_value = "1200,1900")]
    maia_elos: String,
    #[arg(long, default_value_t = 3)]
    maia_repeats: usize,
    #[arg(long)]
    maia_concurrency: Option<usize>,
    #[arg(long)]
    review_command: Option<PathBuf>,
    #[arg(long)]
    review_pgn: Option<PathBuf>,
    #[arg(long)]
    review_case: Option<PathBuf>,
    #[arg(long, default_value_t = 1200)]
    review_elo: u16,
    #[arg(long, value_parser = ["white", "black", "both"], default_value = "both")]
    review_side: String,
    #[arg(long, default_value_t = 1)]
    review_repeats: usize,
}

#[derive(Debug)]
struct MeasuredEngineAnalysis {
    elapsed: Duration,
    analysis: EngineAnalysis,
}

type MaiaConcurrencyResult = (String, f64, HumanMovePrediction);
type MaiaConcurrencyTaskResult = Result<MaiaConcurrencyResult>;
type MaiaConcurrencyJoinResult =
    std::result::Result<MaiaConcurrencyTaskResult, tokio::task::JoinError>;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut result = Map::new();
    result.insert(
        "measuredAt".to_string(),
        Value::String(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)),
    );
    result.insert(
        "logicalCpuCount".to_string(),
        std::thread::available_parallelism()
            .ok()
            .map(|count| json!(count.get()))
            .unwrap_or(Value::Null),
    );

    if !args.skip_stockfish {
        let stockfish = args
            .stockfish
            .as_deref()
            .context("--stockfish is required unless --skip-stockfish is used")?;
        let depths = comma_separated_ints::<u8>(&args.depths, "--depths")?;
        let mut stockfish_measurement =
            measure_stockfish(stockfish, &depths, args.stockfish_repeats).await?;
        stockfish_measurement.insert(
            "terminationAfterCancel".to_string(),
            measure_stockfish_cancellation(stockfish, args.cancellation_repeats).await?,
        );
        result.insert(
            "stockfish".to_string(),
            Value::Object(stockfish_measurement),
        );
    }

    if let Some(base_url) = args.maia_base_url.as_deref() {
        let concurrency = args
            .maia_concurrency
            .context("--maia-concurrency is required with --maia-base-url")?;
        ensure!(concurrency > 0, "--maia-concurrency must be positive");
        let elos = comma_separated_ints::<u16>(&args.maia_elos, "--maia-elos")?;
        result.insert(
            "maia".to_string(),
            measure_maia(base_url, &elos, args.maia_repeats, concurrency).await?,
        );
    }

    let review_sources =
        usize::from(args.review_pgn.is_some()) + usize::from(args.review_case.is_some());
    if args.review_command.is_some() && review_sources != 1 {
        bail!("--review-command requires one of --review-pgn or --review-case");
    }
    if args.review_command.is_none() && review_sources > 0 {
        bail!("--review-pgn and --review-case require --review-command");
    }
    if let Some(command) = args.review_command.as_deref() {
        result.insert(
            "reviewSessionGameImport".to_string(),
            measure_review_command(
                command,
                args.review_pgn.as_deref(),
                args.review_case.as_deref(),
                args.review_elo,
                &args.review_side,
                args.review_repeats,
            )
            .await?,
        );
    }

    println!("{}", serde_json::to_string_pretty(&Value::Object(result))?);
    Ok(())
}

async fn measure_stockfish(
    program: &Path,
    depths: &[u8],
    repeats: usize,
) -> Result<Map<String, Value>> {
    ensure!(!depths.is_empty(), "--depths must not be empty");
    ensure!(repeats > 0, "--stockfish-repeats must be positive");
    let mut raw = Vec::new();

    for &depth in depths {
        let adapter = StockfishAdapter::new(program.to_path_buf(), depth);
        let mut positions = Vec::new();
        for (_, fen) in POSITIONS {
            let mut samples = Vec::new();
            for _ in 0..repeats {
                let started = Instant::now();
                let analysis = adapter
                    .analyze(EngineAnalysisInput { position: fen })
                    .await
                    .with_context(|| format!("Stockfish failed at depth {depth}"))?;
                samples.push(MeasuredEngineAnalysis {
                    elapsed: started.elapsed(),
                    analysis,
                });
            }
            positions.push(samples);
        }
        raw.push((depth, positions));
    }

    let reference_depth = *depths
        .iter()
        .max()
        .expect("a non-empty depth list has a maximum");
    let reference_index = raw
        .iter()
        .position(|(depth, _)| *depth == reference_depth)
        .expect("the reference depth belongs to the measured data");
    let mut summaries = Vec::new();
    for (depth, positions) in &raw {
        let latency = positions
            .iter()
            .flatten()
            .map(|sample| milliseconds(sample.elapsed))
            .collect::<Vec<_>>();
        let comparisons = positions
            .iter()
            .enumerate()
            .map(|(position_index, samples)| {
                let result = samples.last().expect("a positive repeat count produces a sample");
                let reference = raw[reference_index].1[position_index]
                    .last()
                    .expect("a positive repeat count produces a reference sample");
                let (score_kind, score_value) = evaluation(&result.analysis.evaluation);
                let (reference_kind, reference_value) = evaluation(&reference.analysis.evaluation);
                json!({
                    "position": POSITIONS[position_index].0,
                    "bestMoveMatchesReference": result.analysis.best_move == reference.analysis.best_move,
                    "centipawnDeltaToReference":
                        (score_kind == "cp" && reference_kind == "cp").then_some((score_value - reference_value).abs()),
                    "score": { "kind": score_kind, "value": score_value },
                    "referenceScore": { "kind": reference_kind, "value": reference_value },
                })
            })
            .collect::<Vec<_>>();
        summaries.push(json!({
            "depth": depth,
            "latency": distribution(latency),
            "referenceDepth": reference_depth,
            "comparisons": comparisons,
        }));
    }

    Ok(Map::from_iter([
        (
            "program".to_string(),
            Value::String(
                std::fs::canonicalize(program)
                    .unwrap_or_else(|_| program.to_path_buf())
                    .display()
                    .to_string(),
            ),
        ),
        ("positions".to_string(), json!(POSITIONS.len())),
        ("repeatsPerPosition".to_string(), json!(repeats)),
        ("depths".to_string(), Value::Array(summaries)),
    ]))
}

async fn measure_stockfish_cancellation(program: &Path, repeats: usize) -> Result<Value> {
    ensure!(repeats > 0, "--cancellation-repeats must be positive");
    let mut elapsed = Vec::new();
    for _ in 0..repeats {
        let adapter = StockfishAdapter::new(program.to_path_buf(), 99);
        let task = tokio::spawn(async move {
            adapter
                .analyze(EngineAnalysisInput {
                    position: POSITIONS[3].1,
                })
                .await
        });
        sleep(Duration::from_millis(50)).await;
        let started = Instant::now();
        task.abort();
        let _ = task.await;
        elapsed.push(milliseconds(started.elapsed()));
    }
    Ok(distribution(elapsed))
}

async fn measure_maia(
    base_url: &str,
    elos: &[u16],
    repeats: usize,
    concurrency: usize,
) -> Result<Value> {
    ensure!(!elos.is_empty(), "--maia-elos must not be empty");
    ensure!(repeats > 0, "--maia-repeats must be positive");
    let adapter: Arc<dyn HumanMoveModel> = Arc::new(MaiaHttpAdapter::new(base_url));
    let mut summaries = Vec::new();

    for &elo in elos {
        predict_maia(adapter.as_ref(), POSITIONS[0].1, elo).await?;
        let mut times = Vec::new();
        let mut serial_payloads = HashMap::new();
        let mut captured_mass: BTreeMap<usize, Vec<f64>> =
            BTreeMap::from_iter(CANDIDATE_WIDTHS.map(|width| (width, Vec::new())));
        for (_, fen) in POSITIONS {
            for _ in 0..repeats {
                let (elapsed, payload) = predict_maia(adapter.as_ref(), fen, elo).await?;
                times.push(elapsed);
                if let Some(previous) = serial_payloads.get(fen) {
                    ensure!(
                        previous == &payload,
                        "serial Maia response changed between calls"
                    );
                }
                serial_payloads.insert(fen.to_string(), payload.clone());
                for (&width, values) in &mut captured_mass {
                    values.push(
                        payload
                            .candidates
                            .iter()
                            .take(width)
                            .map(|candidate| candidate.probability)
                            .sum(),
                    );
                }
            }
        }
        let candidate_mass_by_width = captured_mass
            .into_iter()
            .map(|(width, values)| {
                (
                    width.to_string(),
                    json!({
                        "min": round(*values.iter().min_by(|a, b| a.total_cmp(b)).expect("measurement has samples"), 4),
                        "p50": round(percentile(&values, 0.5), 4),
                        "max": round(*values.iter().max_by(|a, b| a.total_cmp(b)).expect("measurement has samples"), 4),
                    }),
                )
            })
            .collect::<Map<_, _>>();
        summaries.push(json!({
            "elo": elo,
            "latency": distribution(times),
            "candidateMassByWidth": candidate_mass_by_width,
            "concurrent": measure_maia_concurrency(adapter.clone(), elo, repeats, concurrency, &serial_payloads).await?,
        }));
    }
    Ok(json!({
        "baseUrl": base_url,
        "positions": POSITIONS.len(),
        "repeatsPerPosition": repeats,
        "elos": summaries,
    }))
}

async fn measure_maia_concurrency(
    adapter: Arc<dyn HumanMoveModel>,
    elo: u16,
    repeats: usize,
    width: usize,
    serial_payloads: &HashMap<String, HumanMovePrediction>,
) -> Result<Value> {
    ensure!(width > 0, "Maia concurrency width must be positive");
    let positions = POSITIONS
        .iter()
        .flat_map(|(_, fen)| std::iter::repeat_n((*fen).to_string(), repeats))
        .collect::<Vec<_>>();
    let request_count = positions.len();
    let started = Instant::now();
    let mut tasks = JoinSet::new();
    let mut elapsed = Vec::with_capacity(request_count);
    for fen in positions {
        let adapter = adapter.clone();
        tasks.spawn(async move {
            let (duration, payload) = predict_maia(adapter.as_ref(), &fen, elo).await?;
            Ok::<_, anyhow::Error>((fen, duration, payload))
        });
        if tasks.len() >= width {
            let (_, duration, _) =
                validate_maia_concurrent_result(tasks.join_next().await, serial_payloads)?;
            elapsed.push(duration);
        }
    }
    while !tasks.is_empty() {
        let (_, duration, _) =
            validate_maia_concurrent_result(tasks.join_next().await, serial_payloads)?;
        elapsed.push(duration);
    }
    let wall_ms = milliseconds(started.elapsed());
    Ok(json!({
        "width": width,
        "requests": request_count,
        "requestLatency": distribution(elapsed),
        "wallMs": round(wall_ms, 2),
        "requestsPerSecond": round(request_count as f64 / (wall_ms / 1_000.0), 2),
        "responsesMatchSerial": true,
    }))
}

fn validate_maia_concurrent_result(
    result: Option<MaiaConcurrencyJoinResult>,
    serial_payloads: &HashMap<String, HumanMovePrediction>,
) -> Result<MaiaConcurrencyResult> {
    let (fen, elapsed, payload) =
        result.context("Maia concurrency worker ended unexpectedly")???;
    let serial = serial_payloads
        .get(&fen)
        .context("concurrent Maia request used an unknown position")?;
    ensure!(
        serial == &payload,
        "concurrent Maia response differed from serial response"
    );
    Ok((fen, elapsed, payload))
}

async fn predict_maia(
    adapter: &dyn HumanMoveModel,
    fen: &str,
    elo: u16,
) -> Result<(f64, HumanMovePrediction)> {
    let started = Instant::now();
    let prediction = adapter
        .predict(chen_chess_coach_engine::human_move_model::HumanMoveInput {
            position: fen,
            elo: EloProfile::try_from(elo).map_err(anyhow::Error::msg)?,
            limit: MAIA_LIMIT,
        })
        .await?;
    Ok((milliseconds(started.elapsed()), prediction))
}

async fn measure_review_command(
    command: &Path,
    pgn_path: Option<&Path>,
    case_path: Option<&Path>,
    elo: u16,
    side: &str,
    repeats: usize,
) -> Result<Value> {
    ensure!(repeats > 0, "--review-repeats must be positive");
    let (source, source_name) = match (pgn_path, case_path) {
        (Some(path), None) => (
            json!({ "kind": "localPgnFile", "path": path }),
            path.display().to_string(),
        ),
        (None, Some(path)) => {
            let case = serde_json::from_str::<Value>(&tokio::fs::read_to_string(path).await?)?;
            let pgn = case
                .pointer("/input/pgn")
                .and_then(Value::as_str)
                .context("review case has no input.pgn")?;
            (
                json!({ "kind": "pastedPgn", "pgn": pgn }),
                path.display().to_string(),
            )
        }
        _ => bail!("review command requires exactly one PGN source"),
    };
    let mut elapsed = Vec::new();
    let mut reported_timing = Vec::new();
    let mut review_sha256 = None;

    for _ in 0..repeats {
        let request = json!({
            "requestId": "request:measure:import",
            "operationId": "operation:measure:import",
            "surface": "coachSkill",
            "command": {
                "kind": "importGame",
                "source": source,
                "reviewSide": { "kind": "selected", "reviewSide": side },
                "eloProfile": { "kind": "playerProvided", "rating": elo },
            },
        });
        let started = Instant::now();
        let mut child = Command::new(command)
            .arg("review-session")
            .arg("--jsonl")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to start review command {}", command.display()))?;
        let mut stdin = child.stdin.take().context("review command has no stdin")?;
        stdin
            .write_all(format!("{}\n", serde_json::to_string(&request)?).as_bytes())
            .await?;
        drop(stdin);
        let output = child.wait_with_output().await?;
        ensure!(
            output.status.success(),
            "Review Session measurement command failed: exitStatus={}; stdout={}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim(),
        );
        elapsed.push(milliseconds(started.elapsed()));
        let events = String::from_utf8(output.stdout)?
            .lines()
            .map(serde_json::from_str::<Value>)
            .collect::<serde_json::Result<Vec<_>>>()?;
        let terminal = events
            .last()
            .context("Review Session measurement returned no events")?;
        let event = terminal
            .get("event")
            .context("Review Session measurement event is malformed")?;
        ensure!(
            event.get("kind") == Some(&Value::String("completed".to_string()))
                && event.pointer("/result/kind")
                    == Some(&Value::String("gameImported".to_string())),
            "Review Session measurement import did not complete: terminal={terminal}; stderr={}",
            String::from_utf8_lossy(&output.stderr).trim(),
        );
        let result = event
            .get("result")
            .expect("a completed import has a result");
        let timing = result.get("timing").cloned().unwrap_or(Value::Null);
        ensure!(
            timing.is_null() || timing.is_object(),
            "Review Session measurement import reported invalid pipeline timing"
        );
        reported_timing.push(timing);
        let review = result
            .get("review")
            .context("Review Session measurement import has no review")?;
        let digest = format!(
            "{:x}",
            Sha256::digest(serde_json_canonicalizer::to_vec(review)?)
        );
        if let Some(previous) = &review_sha256 {
            ensure!(
                previous == &digest,
                "review command returned different Game Review facts between runs"
            );
        }
        review_sha256 = Some(digest);
    }
    Ok(json!({
        "command": std::fs::canonicalize(command).unwrap_or_else(|_| command.to_path_buf()).display().to_string(),
        "source": source_name,
        "repeats": repeats,
        "latency": distribution(elapsed),
        "reportedTimingByRun": reported_timing,
        "gameReviewSha256": review_sha256,
    }))
}

fn comma_separated_ints<T>(value: &str, flag: &str) -> Result<Vec<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let values = value
        .split(',')
        .map(|part| {
            part.parse().map_err(|error| {
                anyhow::anyhow!("{flag} contains an invalid number {part:?}: {error}")
            })
        })
        .collect::<Result<Vec<T>>>()?;
    ensure!(!values.is_empty(), "{flag} must not be empty");
    Ok(values)
}

fn evaluation(value: &PositionEvaluation) -> (&'static str, i32) {
    match value {
        PositionEvaluation::Centipawns(value) => ("cp", *value),
        PositionEvaluation::MateIn(value) => ("mate", *value),
    }
}

fn distribution(mut values: Vec<f64>) -> Value {
    values.sort_by(|left, right| left.total_cmp(right));
    json!({
        "minMs": round(values[0], 2),
        "p50Ms": round(percentile(&values, 0.5), 2),
        "p95Ms": round(percentile(&values, 0.95), 2),
        "maxMs": round(*values.last().expect("a distribution has values"), 2),
    })
}

fn percentile(values: &[f64], fraction: f64) -> f64 {
    let index = ((fraction * values.len() as f64).ceil() as usize).saturating_sub(1);
    values[index]
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn round(value: f64, decimals: u32) -> f64 {
    let multiplier = 10_f64.powi(decimals as i32);
    (value * multiplier).round() / multiplier
}

#[cfg(test)]
mod tests {
    use std::{future::Future, pin::Pin};

    use super::*;
    use chen_chess_coach_engine::{
        human_move_model::{HumanMoveInput, HumanMoveModelError},
        types::HumanMoveCandidate,
    };

    #[derive(Clone)]
    struct FakeMaia {
        drift: bool,
    }

    impl HumanMoveModel for FakeMaia {
        fn predict<'a>(
            &'a self,
            input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async move {
                Ok(HumanMovePrediction {
                    candidates: vec![HumanMoveCandidate {
                        uci: if self.drift {
                            format!("{}-drifted", input.position)
                        } else {
                            input.position.to_string()
                        },
                        probability: 1.0,
                        rank: 1,
                    }],
                    win_probability: Some(0.5),
                })
            })
        }
    }

    fn serial_payloads() -> HashMap<String, HumanMovePrediction> {
        POSITIONS
            .iter()
            .map(|(_, fen)| {
                (
                    (*fen).to_string(),
                    HumanMovePrediction {
                        candidates: vec![HumanMoveCandidate {
                            uci: (*fen).to_string(),
                            probability: 1.0,
                            rank: 1,
                        }],
                        win_probability: Some(0.5),
                    },
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn concurrent_maia_measurement_checks_responses_and_reports_throughput() {
        let result = measure_maia_concurrency(
            Arc::new(FakeMaia { drift: false }),
            1200,
            2,
            4,
            &serial_payloads(),
        )
        .await
        .expect("matching fake responses should measure successfully");

        assert_eq!(result["width"], 4);
        assert_eq!(result["requests"], 12);
        assert!(result["requestLatency"]["p50Ms"].as_f64().is_some());
        assert!(result["requestsPerSecond"]
            .as_f64()
            .is_some_and(|value| value > 0.0));
        assert_eq!(result["responsesMatchSerial"], true);
    }

    #[tokio::test]
    async fn concurrent_maia_measurement_rejects_response_drift() {
        let error = measure_maia_concurrency(
            Arc::new(FakeMaia { drift: true }),
            1200,
            1,
            4,
            &serial_payloads(),
        )
        .await
        .expect_err("drifted fake response must be rejected");

        assert!(error.to_string().contains("differed from serial"));
    }
}
