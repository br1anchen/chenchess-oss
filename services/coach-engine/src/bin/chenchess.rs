use std::{
    future::Future,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use chen_chess_coach_engine::{
    engine_analysis::{EngineAnalyzer, StockfishAdapter},
    human_move_model::{HumanMoveModel, MaiaHttpAdapter},
    local_runtime::{
        Platform, ProcessRunner, ReqwestDownloadClient, ReviewRuntimeLease, RuntimeError,
        RuntimeInstaller, RuntimeManager, RuntimeManifest, RuntimePaths, SystemProcessRunner,
    },
    operating_limits::{
        CANCELLATION_BUDGET_MILLISECONDS, LIVE_COMMAND_TIMEOUT_SECONDS,
        RUNTIME_STARTUP_TIMEOUT_SECONDS,
    },
    pipeline_evaluation::{
        accept_fast_evaluation, accept_live_evaluation, record_multi_pv_evidence,
        run_fast_evaluation, run_live_evaluation_with_progress, LiveProviderProvenance,
    },
    review_facts::ReviewFactsService,
    review_session_runtime::build_review_session_executor_with_runtime_startup,
    review_session_transport::{
        run_review_session_jsonl_ingress, start_review_session_jsonl_ingress,
    },
};
use clap::{Parser, Subcommand};
use serde::Serialize;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[path = "chenchess/certification.rs"]
mod certification;
#[path = "chenchess/command_input.rs"]
mod command_input;
#[path = "chenchess/validation.rs"]
mod validation;

const EXIT_INVALID_INPUT: i32 = 3;
const EXIT_PROVIDER: i32 = 4;
const EXIT_VALIDATION: i32 = 5;
const EXIT_EVALUATION: i32 = 6;
const EXIT_RUNTIME: i32 = 7;
const LOCAL_STOCKFISH_DEPTH: u8 = 16;

struct ReviewSession {
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
    _lease: Option<ReviewRuntimeLease>,
}

struct ManagedReviewSession {
    service: ReviewFactsService,
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
    live_provenance: LiveProviderProvenance,
    lease: ReviewRuntimeLease,
}

impl From<ManagedReviewSession> for ReviewSession {
    fn from(session: ManagedReviewSession) -> Self {
        Self {
            engine: session.engine,
            human: session.human,
            _lease: Some(session.lease),
        }
    }
}

#[derive(Parser)]
#[command(name = "chenchess")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    ReviewSession {
        #[arg(long, required = true)]
        jsonl: bool,
        #[arg(long)]
        command_fifo: Option<PathBuf>,
    },
    ValidateReview {
        #[arg(long)]
        review_event_file: PathBuf,
        #[arg(long)]
        review_start_event_file: PathBuf,
        #[arg(long)]
        draft_file: Option<PathBuf>,
    },
    EvaluateFast {
        #[arg(long)]
        corpus_dir: PathBuf,
    },
    AcceptEvaluation {
        #[arg(long)]
        corpus_dir: PathBuf,
    },
    RecordMultiPv {
        #[arg(long)]
        corpus_dir: PathBuf,
    },
    EvaluateLive {
        #[arg(long)]
        corpus_dir: PathBuf,
    },
    AcceptLiveEvaluation {
        #[arg(long)]
        corpus_dir: PathBuf,
    },
    CertifyLive {
        #[arg(long)]
        corpus_dir: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    #[command(hide = true)]
    CertificationProbe {
        #[arg(long)]
        corpus_dir: PathBuf,
        #[arg(long)]
        start_file: PathBuf,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
}

#[derive(Subcommand)]
enum RuntimeCommand {
    Install {
        #[arg(long)]
        manifest: PathBuf,
    },
    Doctor,
    MaiaStart,
    MaiaStop,
    MaiaStatus,
    Uninstall,
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("provider failure: {0}")]
    Provider(String),
    #[error("validation failure: {0}")]
    Validation(String),
    #[error("internal failure: {0}")]
    Internal(String),
    #[error("evaluation failure: {0}")]
    Evaluation(String),
    #[error("runtime failure: {0}")]
    Runtime(String),
    #[error("damaged installation: {0}")]
    DamagedInstallation(String),
    #[error("review cancelled")]
    Cancelled,
}

impl CliError {
    fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidInput(_) => EXIT_INVALID_INPUT,
            Self::Provider(_) => EXIT_PROVIDER,
            Self::Validation(_) => EXIT_VALIDATION,
            Self::Evaluation(_) => EXIT_EVALUATION,
            Self::Runtime(_) | Self::DamagedInstallation(_) => EXIT_RUNTIME,
            Self::Cancelled => 130,
            Self::Internal(_) => 1,
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chen_chess_coach_engine=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    if let Err(error) = run(Cli::parse()).await {
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::ReviewSession { command_fifo, .. } => {
            run_review_session_command(command_fifo).await
        }
        Command::ValidateReview {
            review_event_file,
            review_start_event_file,
            draft_file,
        } => validation::review(
            &review_event_file,
            &review_start_event_file,
            draft_file.as_deref(),
        ),
        Command::EvaluateFast { corpus_dir } => {
            eprintln!("evaluating recorded pipeline evidence");
            let report = run_fast_evaluation(&corpus_dir)
                .map_err(|error| CliError::Evaluation(error.to_string()))?;
            if let Err(drift) = report.require_clean() {
                for difference in &report.differences {
                    eprintln!("## {}\n{}", difference.case_id, difference.field_diff);
                }
                write_json(&report)?;
                return Err(CliError::Evaluation(drift.to_string()));
            }
            write_json(&report)
        }
        Command::AcceptEvaluation { corpus_dir } => {
            eprintln!("accepting recorded pipeline evidence baselines");
            let report = accept_fast_evaluation(&corpus_dir)
                .map_err(|error| CliError::Evaluation(error.to_string()))?;
            write_json(&report)
        }
        Command::RecordMultiPv { corpus_dir } => run_record_multi_pv_command(&corpus_dir).await,
        Command::EvaluateLive { corpus_dir } => {
            run_live_evaluation_command(&corpus_dir, false).await
        }
        Command::AcceptLiveEvaluation { corpus_dir } => {
            run_live_evaluation_command(&corpus_dir, true).await
        }
        Command::CertifyLive { corpus_dir, output } => {
            let report = certification::certify(&corpus_dir, &output)
                .await
                .map_err(certification_error)?;
            write_json(&report)
        }
        Command::CertificationProbe {
            corpus_dir,
            start_file,
        } => run_certification_probe_command(&corpus_dir, start_file).await,
        Command::Runtime { command } => run_runtime(command).await,
    }
}

async fn run_review_session_command(command_fifo: Option<PathBuf>) -> Result<(), CliError> {
    let (input, _command_fifo) =
        command_input::CommandFifoGuard::open(command_fifo).map_err(|error| {
            CliError::InvalidInput(format!(
                "failed to open Review Session command input: {error}"
            ))
        })?;
    let ingress = start_review_session_jsonl_ingress(input);
    let mut cancellation = install_review_cancellation()?;
    let runtime_started = std::time::Instant::now();
    let session = review_service(&mut cancellation).await?;
    let executor = build_review_session_executor_with_runtime_startup(
        Some(session.engine),
        Some(session.human),
        runtime_started.elapsed(),
    )
    .map_err(|error| CliError::Runtime(error.to_string()))?;
    let run = run_review_session_jsonl_ingress(executor, ingress, tokio::io::stdout());
    tokio::pin!(run);
    tokio::select! {
        result = &mut run => result
            .map_err(|error| CliError::Internal(format!("Review Session JSONL failed: {error}"))),
        signal = cancellation.wait() => match signal {
            Ok(()) => Err(CliError::Cancelled),
            Err(error) => Err(CliError::Internal(format!(
                "failed to receive review cancellation signal: {error}"
            ))),
        },
    }
}

fn certification_error(error: certification::CertificationError) -> CliError {
    let message = error.to_string();
    match error {
        certification::CertificationError::PipelineDrift(_)
        | certification::CertificationError::Evaluation(_) => CliError::Evaluation(message),
        certification::CertificationError::InvalidOutput(_) => CliError::InvalidInput(message),
        certification::CertificationError::StartupTask(_)
        | certification::CertificationError::Json(_) => CliError::Internal(message),
        _ => CliError::Runtime(message),
    }
}

/// Records the MultiPV comparison search each recorded case needs, using only the
/// installed Stockfish: the comparison consults no Human Move Model, so this must not
/// start the Maia service or restate its provenance.
async fn run_record_multi_pv_command(corpus_dir: &Path) -> Result<(), CliError> {
    let stockfish_path = match std::env::var("STOCKFISH_PATH")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(path) => PathBuf::from(path),
        None => {
            let paths = RuntimePaths::from_environment()
                .map_err(|error| CliError::Runtime(error.to_string()))?;
            let (runner, _cancellation) = SystemProcessRunner::cancellable();
            RuntimeManager::new(runner)
                .engine_config(&paths, Platform::current())
                .map_err(review_runtime_error)?
                .stockfish_path
        }
    };
    // The depth is not overridable: a recorded comparison is only meaningful beside the
    // single-PV evidence already in the case, which the corpus states as depth 16.
    eprintln!("recording MultiPV comparison evidence at depth {LOCAL_STOCKFISH_DEPTH}");
    let engine = StockfishAdapter::new(stockfish_path, LOCAL_STOCKFISH_DEPTH);
    let report = record_multi_pv_evidence(corpus_dir, &engine)
        .await
        .map_err(|error| CliError::Evaluation(error.to_string()))?;
    write_json(&report)
}

async fn run_live_evaluation_command(corpus_dir: &Path, accept: bool) -> Result<(), CliError> {
    if live_provider_override_present() {
        return Err(CliError::Runtime(
            "live Pipeline Evaluation does not accept provider overrides".to_string(),
        ));
    }
    let mut cancellation = install_review_cancellation()?;
    let startup_started = std::time::Instant::now();
    let session = managed_review_session(&mut cancellation).await?;
    run_live_with_session(corpus_dir, accept, session, cancellation, startup_started).await
}

async fn run_certification_probe_command(
    corpus_dir: &Path,
    start_file: PathBuf,
) -> Result<(), CliError> {
    let mut cancellation = install_review_cancellation()?;
    let startup_started = std::time::Instant::now();
    let session = certification_probe_session(&mut cancellation, start_file).await?;
    run_live_with_session(corpus_dir, false, session, cancellation, startup_started).await
}

async fn run_live_with_session(
    corpus_dir: &Path,
    accept: bool,
    session: ManagedReviewSession,
    mut cancellation: ReviewCancellation,
    startup_started: std::time::Instant,
) -> Result<(), CliError> {
    let runtime_startup_milliseconds = elapsed_milliseconds(startup_started);
    let remaining_budget = std::time::Duration::from_secs(LIVE_COMMAND_TIMEOUT_SECONDS)
        .checked_sub(startup_started.elapsed())
        .ok_or_else(|| {
            CliError::Evaluation(format!(
                "live command exceeded its {LIVE_COMMAND_TIMEOUT_SECONDS}-second time budget"
            ))
        })?;
    let provenance = session.live_provenance;
    if accept {
        eprintln!("accepting live pipeline evidence and fact baselines");
        let report = cancellable_evaluation(
            accept_live_evaluation(corpus_dir, &session.service, provenance),
            &mut cancellation,
            remaining_budget,
        )
        .await?;
        return write_json(&report);
    }

    eprintln!("evaluating pinned live pipeline providers");
    let evaluation_started = std::time::Instant::now();
    let report_provenance = provenance.clone();
    let report = cancellable_evaluation(
        run_live_evaluation_with_progress(
            corpus_dir,
            &session.service,
            provenance,
            |index, total, case_id| {
                eprintln!("evaluating live case {index}/{total}: {case_id}");
            },
        ),
        &mut cancellation,
        remaining_budget,
    )
    .await?;
    let evaluation_milliseconds = elapsed_milliseconds(evaluation_started);
    let mut timed_report = serde_json::to_value(&report)
        .map_err(|error| CliError::Internal(format!("failed to serialize live report: {error}")))?;
    let fields = timed_report.as_object_mut().ok_or_else(|| {
        CliError::Internal("live evaluation report did not serialize as an object".to_string())
    })?;
    fields.insert(
        "runtimeStartupMilliseconds".to_string(),
        runtime_startup_milliseconds.into(),
    );
    fields.insert(
        "evaluationMilliseconds".to_string(),
        evaluation_milliseconds.into(),
    );
    fields.insert(
        "provenance".to_string(),
        serde_json::to_value(report_provenance).map_err(|error| {
            CliError::Internal(format!("failed to serialize live provenance: {error}"))
        })?,
    );
    if !report.differences.is_empty() {
        for difference in &report.differences {
            eprintln!("## {}\n{}", difference.case_id, difference.field_diff);
        }
        write_json(&timed_report)?;
        return Err(CliError::Evaluation(format!(
            "{} live case(s) differ from their baselines",
            report.differences.len()
        )));
    }
    write_json(&timed_report)
}

fn elapsed_milliseconds(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

async fn cancellable_evaluation<T>(
    evaluation: impl Future<
        Output = Result<T, chen_chess_coach_engine::pipeline_evaluation::EvaluationError>,
    >,
    cancellation: &mut ReviewCancellation,
    time_budget: std::time::Duration,
) -> Result<T, CliError> {
    tokio::pin!(evaluation);
    tokio::select! {
        result = &mut evaluation => result.map_err(|error| CliError::Evaluation(error.to_string())),
        _ = tokio::time::sleep(time_budget) => {
            Err(CliError::Evaluation(format!(
                "live command exceeded its {LIVE_COMMAND_TIMEOUT_SECONDS}-second time budget"
            )))
        },
        signal = cancellation.wait() => match signal {
            Ok(()) => Err(CliError::Cancelled),
            Err(error) => Err(CliError::Internal(format!(
                "failed to receive evaluation cancellation signal: {error}"
            ))),
        },
    }
}

#[cfg(unix)]
struct ReviewCancellation {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl ReviewCancellation {
    fn install() -> io::Result<Self> {
        Ok(Self {
            interrupt: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?,
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?,
        })
    }

    async fn wait(&mut self) -> io::Result<()> {
        tokio::select! {
            _ = self.interrupt.recv() => Ok(()),
            _ = self.terminate.recv() => Ok(()),
        }
    }
}

#[cfg(not(unix))]
struct ReviewCancellation;

#[cfg(not(unix))]
impl ReviewCancellation {
    fn install() -> io::Result<Self> {
        Ok(Self)
    }

    async fn wait(&mut self) -> io::Result<()> {
        tokio::signal::ctrl_c().await
    }
}

fn install_review_cancellation() -> Result<ReviewCancellation, CliError> {
    ReviewCancellation::install().map_err(|error| {
        CliError::Internal(format!(
            "failed to install review cancellation handler: {error}"
        ))
    })
}

async fn run_runtime(command: RuntimeCommand) -> Result<(), CliError> {
    let paths =
        RuntimePaths::from_environment().map_err(|error| CliError::Runtime(error.to_string()))?;
    match command {
        RuntimeCommand::Install { manifest } => {
            eprintln!("installing pinned Local Pipeline Runtime");
            let manifest: RuntimeManifest = serde_json::from_slice(
                &std::fs::read(&manifest).map_err(|error| CliError::Runtime(error.to_string()))?,
            )
            .map_err(|error| CliError::Runtime(error.to_string()))?;
            let source_cli =
                std::env::current_exe().map_err(|error| CliError::Runtime(error.to_string()))?;
            let outcome = RuntimeInstaller::new(
                ReqwestDownloadClient::default(),
                SystemProcessRunner::default(),
            )
            .install(chen_chess_coach_engine::local_runtime::InstallRequest {
                manifest,
                paths,
                source_cli,
            })
            .await
            .map_err(|error| CliError::Runtime(error.to_string()))?;
            write_json(&outcome)
        }
        RuntimeCommand::Doctor => {
            eprintln!("diagnosing Local Pipeline Runtime");
            let report = RuntimeManager::new(SystemProcessRunner::default())
                .doctor(&paths, Platform::current())
                .map_err(|error| CliError::Runtime(error.to_string()))?;
            write_json(&report)
        }
        RuntimeCommand::MaiaStart => {
            eprintln!("starting chenchess Maia service");
            let state = RuntimeManager::new(SystemProcessRunner::default())
                .start_maia(&paths)
                .map_err(|error| CliError::Runtime(error.to_string()))?;
            write_json(&state)
        }
        RuntimeCommand::MaiaStop => {
            eprintln!("stopping chenchess Maia service");
            let state = RuntimeManager::new(SystemProcessRunner::default())
                .stop_maia(&paths)
                .map_err(|error| CliError::Runtime(error.to_string()))?;
            write_json(&state)
        }
        RuntimeCommand::MaiaStatus => {
            let status = RuntimeManager::new(SystemProcessRunner::default())
                .maia_status(&paths)
                .map_err(|error| CliError::Runtime(error.to_string()))?;
            write_json(&status)
        }
        RuntimeCommand::Uninstall => {
            eprintln!("uninstalling chenchess Local Pipeline Runtime and Coach Skill");
            let outcome = RuntimeManager::new(SystemProcessRunner::default())
                .uninstall(&paths)
                .map_err(|error| CliError::Runtime(error.to_string()))?;
            write_json(&outcome)
        }
    }
}

fn development_service(
    stockfish_path: String,
    maia_base_url: String,
) -> Result<ReviewSession, CliError> {
    let stockfish_depth = std::env::var("STOCKFISH_DEPTH")
        .unwrap_or_else(|_| "16".to_string())
        .parse::<u8>()
        .map_err(|_| {
            CliError::Provider(
                "STOCKFISH_DEPTH must be a whole number between 1 and 255".to_string(),
            )
        })?;
    if stockfish_depth == 0 {
        return Err(CliError::Provider(
            "STOCKFISH_DEPTH must be between 1 and 255".to_string(),
        ));
    }
    let engine = Arc::new(StockfishAdapter::new(
        PathBuf::from(&stockfish_path),
        stockfish_depth,
    )) as Arc<dyn EngineAnalyzer>;
    let human = Arc::new(MaiaHttpAdapter::new(&maia_base_url)) as Arc<dyn HumanMoveModel>;
    Ok(ReviewSession {
        engine,
        human,
        _lease: None,
    })
}

async fn review_service(cancellation: &mut ReviewCancellation) -> Result<ReviewSession, CliError> {
    let paths =
        RuntimePaths::from_environment().map_err(|error| CliError::Runtime(error.to_string()))?;
    let executable =
        std::env::current_exe().map_err(|error| CliError::Runtime(error.to_string()))?;
    let installed = paths
        .is_installed_executable(&executable)
        .map_err(review_runtime_error)?;
    if installed && provider_override_present() {
        return Err(CliError::Runtime(
            "installed reviews do not accept provider overrides".to_string(),
        ));
    }
    if !installed {
        if let Some((stockfish_path, maia_base_url)) = development_overrides()? {
            return development_service(stockfish_path, maia_base_url);
        }
    }
    managed_review_session(cancellation)
        .await
        .map(ReviewSession::from)
}

async fn managed_review_session(
    cancellation: &mut ReviewCancellation,
) -> Result<ManagedReviewSession, CliError> {
    start_managed_session(cancellation, installed_review_service).await
}

async fn certification_probe_session(
    cancellation: &mut ReviewCancellation,
    start_file: PathBuf,
) -> Result<ManagedReviewSession, CliError> {
    start_managed_session(cancellation, move |paths, runner| {
        installed_probe_service(paths, runner, start_file)
    })
    .await
}

async fn start_managed_session(
    cancellation: &mut ReviewCancellation,
    start: impl FnOnce(RuntimePaths, SystemProcessRunner) -> Result<ManagedReviewSession, CliError>
        + Send
        + 'static,
) -> Result<ManagedReviewSession, CliError> {
    let paths =
        RuntimePaths::from_environment().map_err(|error| CliError::Runtime(error.to_string()))?;
    let (runner, process_cancellation) = SystemProcessRunner::cancellable();
    let mut startup = tokio::task::spawn_blocking(move || start(paths, runner));
    tokio::select! {
        result = &mut startup => result
            .map_err(|error| CliError::Internal(format!("runtime startup task failed: {error}")))?,
        _ = tokio::time::sleep(std::time::Duration::from_secs(RUNTIME_STARTUP_TIMEOUT_SECONDS)) => {
            process_cancellation.cancel();
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(CANCELLATION_BUDGET_MILLISECONDS),
                &mut startup,
            ).await;
            Err(CliError::Runtime(format!(
                "runtime startup exceeded its {RUNTIME_STARTUP_TIMEOUT_SECONDS}-second time budget"
            )))
        },
        signal = cancellation.wait() => {
            process_cancellation.cancel();
            let _ = startup.await;
            match signal {
                Ok(()) => Err(CliError::Cancelled),
                Err(error) => Err(CliError::Internal(format!(
                    "failed to receive review cancellation signal: {error}"
                ))),
            }
        }
    }
}

fn installed_review_service<R: ProcessRunner>(
    paths: RuntimePaths,
    runner: R,
) -> Result<ManagedReviewSession, CliError> {
    let lease = RuntimeManager::new(runner)
        .review_config(&paths, Platform::current())
        .map_err(review_runtime_error)?;
    let engine = Arc::new(StockfishAdapter::new(
        lease.config.stockfish_path.clone(),
        LOCAL_STOCKFISH_DEPTH,
    )) as Arc<dyn EngineAnalyzer>;
    managed_session_from_lease(lease, engine)
}

fn installed_probe_service<R: ProcessRunner>(
    paths: RuntimePaths,
    runner: R,
    start_file: PathBuf,
) -> Result<ManagedReviewSession, CliError> {
    let lease = RuntimeManager::new(runner)
        .review_config(&paths, Platform::current())
        .map_err(review_runtime_error)?;
    let wrapper = write_stockfish_probe(&lease.config.stockfish_path, &start_file)?;
    let engine =
        Arc::new(StockfishAdapter::new(wrapper, LOCAL_STOCKFISH_DEPTH)) as Arc<dyn EngineAnalyzer>;
    managed_session_from_lease(lease, engine)
}

#[cfg(unix)]
fn write_stockfish_probe(stockfish: &Path, marker: &Path) -> Result<PathBuf, CliError> {
    use std::os::unix::fs::PermissionsExt;

    let wrapper = marker.with_extension("stockfish-probe");
    let script = format!(
        "#!/bin/sh\nprintf 'started\\n' > {}\nexport CHENCHESS_CERTIFICATION_PROBE=1\nexec {} \"$@\"\n",
        shell_quote(marker),
        shell_quote(stockfish)
    );
    std::fs::write(&wrapper, script)
        .map_err(|error| CliError::Internal(format!("failed to write Stockfish probe: {error}")))?;
    let mut permissions = std::fs::metadata(&wrapper)
        .map_err(|error| CliError::Internal(format!("failed to inspect Stockfish probe: {error}")))?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&wrapper, permissions).map_err(|error| {
        CliError::Internal(format!(
            "failed to make Stockfish probe executable: {error}"
        ))
    })?;
    Ok(wrapper)
}

#[cfg(not(unix))]
fn write_stockfish_probe(_stockfish: &Path, _marker: &Path) -> Result<PathBuf, CliError> {
    Err(CliError::Runtime(
        "Stockfish cancellation certification is supported only on Unix".to_string(),
    ))
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}

fn managed_session_from_lease(
    lease: ReviewRuntimeLease,
    engine: Arc<dyn EngineAnalyzer>,
) -> Result<ManagedReviewSession, CliError> {
    let human =
        Arc::new(MaiaHttpAdapter::new(&lease.config.maia_base_url)) as Arc<dyn HumanMoveModel>;
    let service = ReviewFactsService::new(engine.clone(), human.clone());
    let live_provenance = LiveProviderProvenance {
        stockfish: format!(
            "Stockfish {} depth {}",
            lease.config.stockfish_version, LOCAL_STOCKFISH_DEPTH
        ),
        maia_image: lease.config.maia_image.clone(),
        maia_package: lease.config.maia_package.clone(),
        maia_model: lease.config.maia_model.clone(),
    };
    Ok(ManagedReviewSession {
        service,
        engine,
        human,
        live_provenance,
        lease,
    })
}

fn review_runtime_error(error: RuntimeError) -> CliError {
    match &error {
        RuntimeError::MutationInProgress => CliError::Runtime(
            "runtime busy: another live review or Pipeline Evaluation owns the Local Pipeline Runtime"
                .to_string(),
        ),
        RuntimeError::Checksum { .. }
        | RuntimeError::SchemaMismatch { .. }
        | RuntimeError::InvalidManifest(_)
        | RuntimeError::PathConflict(_)
        | RuntimeError::DamagedInstallation { .. }
        | RuntimeError::Io(_)
        | RuntimeError::Json(_) => CliError::DamagedInstallation(error.to_string()),
        _ => CliError::Runtime(error.to_string()),
    }
}

fn provider_override_present() -> bool {
    [
        "STOCKFISH_PATH",
        "STOCKFISH_DEPTH",
        "MAIA_BASE_URL",
        "MAIA_MODEL_TYPE",
        "MAIA_MODEL_DIR",
        "MAIA_DEVICE",
        "MAIA_PORT",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
}

fn live_provider_override_present() -> bool {
    ["STOCKFISH_PATH", "STOCKFISH_DEPTH", "MAIA_BASE_URL"]
        .iter()
        .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
}

fn development_overrides() -> Result<Option<(String, String)>, CliError> {
    let value = |key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    match (value("STOCKFISH_PATH"), value("MAIA_BASE_URL")) {
        (Some(stockfish), Some(maia)) => Ok(Some((stockfish, maia))),
        (None, None) => Ok(None),
        _ => Err(CliError::Provider(
            "STOCKFISH_PATH and MAIA_BASE_URL must be set together for a development override"
                .to_string(),
        )),
    }
}

fn read_text(path: Option<&Path>, label: &str) -> Result<String, CliError> {
    match path {
        Some(path) => std::fs::read_to_string(path).map_err(|error| {
            CliError::InvalidInput(format!(
                "failed to read {label} file {}: {error}",
                path.display()
            ))
        }),
        None => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).map_err(|error| {
                CliError::InvalidInput(format!("failed to read {label}: {error}"))
            })?;
            Ok(input)
        }
    }
}

fn write_json(value: &impl Serialize) -> Result<(), CliError> {
    serde_json::to_writer(io::stdout().lock(), value)
        .map_err(|error| CliError::Internal(format!("failed to write JSON output: {error}")))
}
