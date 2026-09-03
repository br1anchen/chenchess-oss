use std::{fs, path::Path, process::Stdio, sync::Arc, time::Instant};

use chen_chess_coach_engine::{
    engine_analysis::{EngineAnalyzer, StockfishAdapter},
    human_move_model::{HumanMoveModel, MaiaHttpAdapter},
    local_runtime::{Platform, RuntimeManager, RuntimePaths, SystemProcessRunner},
    operating_limits::{
        CANCELLATION_BUDGET_MILLISECONDS, EVALUATION_CENTIPAWN_TOLERANCE,
        LIVE_COMMAND_TIMEOUT_SECONDS, MAX_GAME_PLIES, PROBABILITY_TOLERANCE, PROGRESS_EVERY_CASES,
        PROVIDER_POSITION_TIMEOUT_SECONDS, RUNTIME_STARTUP_TIMEOUT_SECONDS,
    },
    pipeline_evaluation::{run_live_evaluation, LiveProviderProvenance},
    review_facts::ReviewFactsService,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::{
    process::Command,
    time::{sleep, timeout, Duration},
};

#[path = "certification/review_session.rs"]
mod review_session;

const STOCKFISH_DEPTH: u8 = 16;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CertificationReport {
    certified: bool,
    certified_at: DateTime<Utc>,
    code_revision: String,
    platform: CertifiedPlatform,
    limits: CertifiedLimits,
    provenance: CertifiedProvenance,
    measurements: CertificationMeasurements,
    review_session: review_session::ReviewSessionCertification,
}

#[derive(Debug, Serialize)]
struct CertifiedPlatform {
    os: &'static str,
    arch: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CertifiedLimits {
    max_game_plies: usize,
    provider_position_timeout_seconds: u64,
    runtime_startup_timeout_seconds: u64,
    live_command_timeout_seconds: u64,
    cancellation_budget_milliseconds: u64,
    progress_every_cases: usize,
    centipawn_tolerance: u32,
    probability_tolerance: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CertifiedProvenance {
    unit_version: String,
    stockfish: String,
    maia_image: String,
    maia_package: String,
    maia_model: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CertificationMeasurements {
    cold_command_milliseconds: u64,
    warm_command_milliseconds: u64,
    runtime_startup_milliseconds: u64,
    evaluation_milliseconds: u64,
    warm_evaluation_milliseconds: u64,
    cancellation_milliseconds: u64,
    maia_resources: String,
}

pub(super) async fn certify(
    corpus_dir: &Path,
    output: &Path,
) -> Result<CertificationReport, CertificationError> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err(CertificationError::UnsupportedPlatform);
    }

    let paths = RuntimePaths::from_environment()?;
    let config_path = paths.config_file();
    let config_before = fs::read(&config_path)?;
    let executable = std::env::current_exe()?;
    let cancellation_milliseconds = measure_live_command_cancellation(&executable).await?;

    let (runner, process_cancellation) = SystemProcessRunner::cancellable();
    let startup_started = Instant::now();
    let startup_runner = runner.clone();
    let startup_paths = paths.clone();
    let mut startup = tokio::task::spawn_blocking(move || {
        RuntimeManager::new(startup_runner)
            .certification_config(&startup_paths, Platform::current())
    });
    let lease = tokio::select! {
        result = &mut startup => result
            .map_err(|error| CertificationError::StartupTask(error.to_string()))??,
        _ = sleep(Duration::from_secs(RUNTIME_STARTUP_TIMEOUT_SECONDS)) => {
            process_cancellation.cancel();
            let _ = timeout(
                Duration::from_millis(CANCELLATION_BUDGET_MILLISECONDS),
                &mut startup,
            ).await;
            return Err(CertificationError::StartupTimeout);
        }
    };
    if fs::read(config_path)? != config_before {
        return Err(CertificationError::RuntimeChangedDuringProbe);
    }
    let runtime_startup_milliseconds = elapsed_milliseconds(startup_started);

    let engine = Arc::new(StockfishAdapter::new(
        lease.config.stockfish_path.clone(),
        STOCKFISH_DEPTH,
    )) as Arc<dyn EngineAnalyzer>;
    let human =
        Arc::new(MaiaHttpAdapter::new(&lease.config.maia_base_url)) as Arc<dyn HumanMoveModel>;
    let service = ReviewFactsService::new(engine.clone(), human.clone());
    let provenance = LiveProviderProvenance {
        stockfish: format!(
            "Stockfish {} depth {}",
            lease.config.stockfish_version, STOCKFISH_DEPTH
        ),
        maia_image: lease.config.maia_image.clone(),
        maia_package: lease.config.maia_package.clone(),
        maia_model: lease.config.maia_model.clone(),
    };

    let evaluation_started = Instant::now();
    let cold_budget = Duration::from_secs(LIVE_COMMAND_TIMEOUT_SECONDS)
        .checked_sub(startup_started.elapsed())
        .ok_or(CertificationError::CommandTimeout)?;
    require_clean_evaluation(
        timeout(
            cold_budget,
            run_live_evaluation(corpus_dir, &service, provenance.clone()),
        )
        .await
        .map_err(|_| CertificationError::CommandTimeout)??,
    )?;
    let evaluation_milliseconds = elapsed_milliseconds(evaluation_started);
    let cold_command_milliseconds =
        runtime_startup_milliseconds.saturating_add(evaluation_milliseconds);

    let warm_started = Instant::now();
    require_clean_evaluation(
        timeout(
            Duration::from_secs(LIVE_COMMAND_TIMEOUT_SECONDS),
            run_live_evaluation(corpus_dir, &service, provenance.clone()),
        )
        .await
        .map_err(|_| CertificationError::CommandTimeout)??,
    )?;
    let warm_evaluation_milliseconds = elapsed_milliseconds(warm_started);

    let second_warm_started = Instant::now();
    require_clean_evaluation(
        timeout(
            Duration::from_secs(LIVE_COMMAND_TIMEOUT_SECONDS),
            run_live_evaluation(corpus_dir, &service, provenance.clone()),
        )
        .await
        .map_err(|_| CertificationError::CommandTimeout)??,
    )?;
    let second_warm_evaluation_milliseconds = elapsed_milliseconds(second_warm_started);
    let review_session = review_session::certify(
        engine,
        human,
        [
            evaluation_milliseconds,
            warm_evaluation_milliseconds,
            second_warm_evaluation_milliseconds,
        ],
    )
    .await?;
    let maia_resources = RuntimeManager::new(runner).maia_resources_for_lease(&lease)?;

    let report = CertificationReport {
        certified: true,
        certified_at: Utc::now(),
        code_revision: required_code_revision()?,
        platform: CertifiedPlatform {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        },
        limits: CertifiedLimits {
            max_game_plies: MAX_GAME_PLIES,
            provider_position_timeout_seconds: PROVIDER_POSITION_TIMEOUT_SECONDS,
            runtime_startup_timeout_seconds: RUNTIME_STARTUP_TIMEOUT_SECONDS,
            live_command_timeout_seconds: LIVE_COMMAND_TIMEOUT_SECONDS,
            cancellation_budget_milliseconds: CANCELLATION_BUDGET_MILLISECONDS,
            progress_every_cases: PROGRESS_EVERY_CASES,
            centipawn_tolerance: EVALUATION_CENTIPAWN_TOLERANCE,
            probability_tolerance: PROBABILITY_TOLERANCE,
        },
        provenance: CertifiedProvenance {
            unit_version: lease.config.unit_version.clone(),
            stockfish: provenance.stockfish,
            maia_image: provenance.maia_image,
            maia_package: provenance.maia_package,
            maia_model: provenance.maia_model,
        },
        measurements: CertificationMeasurements {
            cold_command_milliseconds,
            warm_command_milliseconds: warm_evaluation_milliseconds,
            runtime_startup_milliseconds,
            evaluation_milliseconds,
            warm_evaluation_milliseconds,
            cancellation_milliseconds,
            maia_resources,
        },
        review_session,
    };
    validate_measurements(&report.measurements)?;
    write_report(output, &report)?;
    Ok(report)
}

fn require_clean_evaluation(
    report: chen_chess_coach_engine::pipeline_evaluation::EvaluationReport,
) -> Result<(), CertificationError> {
    report
        .require_clean()
        .map_err(|drift| CertificationError::PipelineDrift(drift.differing_cases))
}

async fn measure_live_command_cancellation(executable: &Path) -> Result<u64, CertificationError> {
    let probe_corpus = TemporaryProbeCorpus::new()?;
    let marker = TemporaryMarker(std::env::temp_dir().join(format!(
        "chenchess-live-start-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    )));
    let mut child = Command::new(executable)
        .args(["certification-probe", "--corpus-dir"])
        .arg(&probe_corpus.0)
        .arg("--start-file")
        .arg(&marker.0)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    timeout(
        Duration::from_secs(
            RUNTIME_STARTUP_TIMEOUT_SECONDS.saturating_add(PROVIDER_POSITION_TIMEOUT_SECONDS),
        ),
        async {
            loop {
                if marker.0.is_file() {
                    return Ok::<(), std::io::Error>(());
                }
                if child.try_wait()?.is_some() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "live command exited before Stockfish started",
                    ));
                }
                sleep(Duration::from_millis(10)).await;
            }
        },
    )
    .await
    .map_err(|_| CertificationError::CancellationProbeTimeout)??;

    let started = Instant::now();
    let pid = child.id().ok_or(CertificationError::MissingPid)?;
    let signal = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output()
        .await?;
    if !signal.status.success() {
        return Err(CertificationError::ChildFailure(
            String::from_utf8_lossy(&signal.stderr).trim().to_string(),
        ));
    }
    let status = timeout(
        Duration::from_millis(CANCELLATION_BUDGET_MILLISECONDS),
        child.wait(),
    )
    .await
    .map_err(|_| CertificationError::CancellationBudgetExceeded)??;
    if status.code() != Some(130) {
        return Err(CertificationError::UnexpectedCancellation(status.code()));
    }
    Ok(elapsed_milliseconds(started))
}

struct TemporaryProbeCorpus(std::path::PathBuf);

impl TemporaryProbeCorpus {
    fn new() -> Result<Self, CertificationError> {
        let root = std::env::temp_dir().join(format!(
            "chenchess-certification-corpus-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root)?;
        fs::write(
            root.join("cancellation-probe.case.json"),
            br#"{
  "id": "cancellation-probe",
  "description": "Private one-move corpus used only to measure live command cancellation.",
  "input": {
    "pgn": "1. e4 *",
    "elo": 1500,
    "operation": { "kind": "selectedMoment", "selectedPly": 1 }
  },
  "provenance": {
    "stockfish": "probe",
    "maiaImage": "probe",
    "maiaPackage": "maia2==0.11.0",
    "maiaModel": "probe"
  },
  "evidence": [{
    "ply": 1,
    "engineBefore": {
      "bestMove": "e2e4",
      "evaluation": { "kind": "centipawns", "value": 0 },
      "principalVariation": ["e2e4"],
      "depth": 16
    },
    "afterMove": {
      "kind": "analyzed",
      "evaluation": { "kind": "centipawns", "value": 0 }
    },
    "humanBefore": {
      "candidates": [{ "uci": "e2e4", "probability": 0.5, "rank": 1 }],
      "winProbability": 0.5
    }
  }]
}
"#,
        )?;
        Ok(Self(root))
    }
}

impl Drop for TemporaryProbeCorpus {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct TemporaryMarker(std::path::PathBuf);

impl Drop for TemporaryMarker {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        let _ = fs::remove_file(self.0.with_extension("stockfish-probe"));
    }
}

fn validate_measurements(
    measurements: &CertificationMeasurements,
) -> Result<(), CertificationError> {
    let command_budget = LIVE_COMMAND_TIMEOUT_SECONDS.saturating_mul(1_000);
    if measurements.cold_command_milliseconds > command_budget
        || measurements.warm_command_milliseconds > command_budget
    {
        return Err(CertificationError::CommandTimeout);
    }
    let startup_budget = RUNTIME_STARTUP_TIMEOUT_SECONDS.saturating_mul(1_000);
    if measurements.runtime_startup_milliseconds > startup_budget {
        return Err(CertificationError::StartupTimeout);
    }
    if measurements.cancellation_milliseconds > CANCELLATION_BUDGET_MILLISECONDS {
        return Err(CertificationError::CancellationBudgetExceeded);
    }
    Ok(())
}

fn write_report(path: &Path, report: &CertificationReport) -> Result<(), CertificationError> {
    let parent = path
        .parent()
        .ok_or_else(|| CertificationError::InvalidOutput(path.to_path_buf()))?;
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent)?;
    }
    let staged = path.with_extension("json.next");
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    fs::write(&staged, bytes)?;
    fs::rename(staged, path)?;
    Ok(())
}

fn elapsed_milliseconds(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn required_code_revision() -> Result<String, CertificationError> {
    let revision = std::env::var("CHENCHESS_CODE_REVISION")
        .map_err(|_| CertificationError::MissingCodeRevision)?;
    let revision = revision.trim();
    if revision.is_empty() {
        return Err(CertificationError::MissingCodeRevision);
    }
    Ok(revision.to_string())
}

#[derive(Debug, thiserror::Error)]
pub(super) enum CertificationError {
    #[error("live certification is supported only on macOS Apple Silicon")]
    UnsupportedPlatform,
    #[error("live certification command exceeded its time budget")]
    CommandTimeout,
    #[error("live certification runtime startup exceeded its time budget")]
    StartupTimeout,
    #[error("live certification startup task failed: {0}")]
    StartupTask(String),
    #[error("live certification found material drift in {0} case(s)")]
    PipelineDrift(usize),
    #[error("live certification cancellation probe did not reach Stockfish")]
    CancellationProbeTimeout,
    #[error("live certification cancellation exceeded its time budget")]
    CancellationBudgetExceeded,
    #[error("live certification child failed: {0}")]
    ChildFailure(String),
    #[error("live certification child did not expose a process ID")]
    MissingPid,
    #[error("live certification cancellation exited with unexpected code {0:?}")]
    UnexpectedCancellation(Option<i32>),
    #[error("the installed runtime changed during the cancellation probe")]
    RuntimeChangedDuringProbe,
    #[error("invalid certification output path {0}")]
    InvalidOutput(std::path::PathBuf),
    #[error("CHENCHESS_CODE_REVISION must identify the certified source revision")]
    MissingCodeRevision,
    #[error(transparent)]
    ReviewSession(#[from] anyhow::Error),
    #[error(transparent)]
    Runtime(#[from] chen_chess_coach_engine::local_runtime::RuntimeError),
    #[error(transparent)]
    Evaluation(#[from] chen_chess_coach_engine::pipeline_evaluation::EvaluationError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::review_session::LatencyDistribution;

    #[test]
    fn certification_latency_reports_the_required_percentiles() {
        let measured = LatencyDistribution::from_milliseconds([10, 50, 20, 40, 30]);

        assert_eq!(measured.samples, 5);
        assert_eq!(measured.p50_milliseconds, 30);
        assert_eq!(measured.p95_milliseconds, 50);
        assert_eq!(measured.maximum_milliseconds, 50);
    }
}
