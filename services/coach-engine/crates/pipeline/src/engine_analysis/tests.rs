use std::{
    fs::{self, OpenOptions},
    io::{BufRead, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use super::{
    EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer, PositionEvaluation, StockfishAdapter,
};

const POSITION: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";

#[tokio::test]
async fn stockfish_adapter_runs_uci_contract_for_a_selected_position() {
    let adapter = StockfishAdapter::with_command(
        fake_engine_command(),
        vec![
            "--exact".to_string(),
            "engine_analysis::tests::fake_stockfish_process".to_string(),
            "--nocapture".to_string(),
        ],
        12,
        Duration::from_secs(2),
    );

    let output = adapter
        .analyze_with_provenance(EngineAnalysisInput { position: POSITION })
        .await
        .expect("fake UCI engine should produce analysis");
    let analysis = output.analysis;

    assert_eq!(analysis.best_move, "e2e4");
    assert_eq!(analysis.evaluation, PositionEvaluation::Centipawns(34));
    assert_eq!(analysis.principal_variation, ["e2e4", "e7e5", "g1f3"]);
    assert_eq!(analysis.depth, 12);
    let provenance = output
        .provenance
        .expect("an analyzed Stockfish binary should expose its measured provenance");
    assert_eq!(provenance.version, "18");
    assert_eq!(provenance.depth, 12);
    assert_eq!(provenance.binary_sha256.len(), 64);
}

#[tokio::test]
async fn stockfish_adapter_returns_validated_ranked_multi_pv_lines() {
    let adapter = StockfishAdapter::with_command(
        fake_engine_command(),
        vec![
            "--exact".to_string(),
            "engine_analysis::tests::fake_stockfish_process".to_string(),
            "--nocapture".to_string(),
        ],
        12,
        Duration::from_secs(2),
    );

    let output = adapter
        .analyze_multi_pv(EngineAnalysisInput { position: POSITION }, 3)
        .await
        .expect("fake UCI engine should produce three ranked variations");

    assert_eq!(
        output
            .variations
            .iter()
            .map(|variation| (variation.rank, variation.analysis.best_move.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "g1f3"), (2, "b1c3"), (3, "d2d4")]
    );
    assert!(output
        .variations
        .iter()
        .all(|variation| variation.analysis.depth == 12));
    assert_eq!(output.provenance.unwrap().version, "18");
}

#[tokio::test]
async fn stockfish_adapter_reuses_one_uci_session_per_game_worker() {
    let trace_directory = fake_trace_directory();
    fs::create_dir(&trace_directory).expect("fake engine trace directory should be creatable");
    let mut adapter = StockfishAdapter::with_command(
        fake_engine_command(),
        vec![
            "--exact".to_string(),
            "engine_analysis::tests::fake_stockfish_process".to_string(),
            "--nocapture".to_string(),
        ],
        12,
        Duration::from_secs(2),
    );
    adapter.child_environment.push((
        "FAKE_STOCKFISH_TRACE_DIRECTORY".to_string(),
        trace_directory.to_string_lossy().into_owned(),
    ));

    let analyses = Arc::new(adapter)
        .analyze_positions(vec![POSITION.to_string(); 10], 8)
        .await
        .expect("eight persistent workers should analyze every Position");

    assert_eq!(analyses.len(), 10);
    assert!(analyses
        .iter()
        .all(|output| output.analysis.best_move == "e2e4"));
    let traces = fs::read_dir(&trace_directory)
        .expect("fake engine trace directory should be readable")
        .map(|entry| {
            fs::read_to_string(entry.expect("trace entry should be readable").path())
                .expect("fake engine trace should be readable")
        })
        .collect::<Vec<_>>();
    assert_eq!(traces.len(), 8);
    for trace in &traces {
        assert_eq!(trace.lines().filter(|line| *line == "spawn").count(), 1);
        assert_eq!(trace.lines().filter(|line| *line == "uci").count(), 1);
        // One at initialization, then one before every search, so a Position's
        // answer does not depend on what this worker searched before it or on
        // how many workers the slice was cut into.
        assert_eq!(
            trace.lines().filter(|line| *line == "ucinewgame").count(),
            1 + trace.lines().filter(|line| *line == "go depth 12").count()
        );
        assert_eq!(trace.lines().filter(|line| *line == "quit").count(), 1);
    }
    assert_eq!(
        traces
            .iter()
            .flat_map(|trace| trace.lines())
            .filter(|line| *line == "go depth 12")
            .count(),
        10
    );
    fs::remove_dir_all(trace_directory).expect("fake engine trace directory should be removable");
}

#[tokio::test]
async fn stockfish_game_workers_finish_positions_before_a_later_chunk_failure() {
    let adapter = StockfishAdapter::with_command(
        fake_engine_command(),
        vec![
            "--exact".to_string(),
            "engine_analysis::tests::fake_stockfish_process".to_string(),
            "--nocapture".to_string(),
        ],
        12,
        Duration::from_secs(2),
    );
    let positions = vec![
        POSITION.to_string(),
        POSITION.to_string(),
        POSITION.to_string(),
        String::new(),
        POSITION.to_string(),
    ];

    let error = Arc::new(adapter)
        .analyze_positions(positions, 2)
        .await
        .expect_err("the invalid Position in the second chunk should fail");

    assert_eq!(error.index, 3);
    assert!(matches!(error.error, EngineAnalysisError::InvalidInput(_)));
}

#[tokio::test]
async fn stockfish_adapter_reuses_one_process_across_single_position_analyses() {
    let trace_directory = fake_trace_directory();
    fs::create_dir(&trace_directory).expect("fake engine trace directory should be creatable");
    let mut adapter = StockfishAdapter::with_command(
        fake_engine_command(),
        vec![
            "--exact".to_string(),
            "engine_analysis::tests::fake_stockfish_process".to_string(),
            "--nocapture".to_string(),
        ],
        12,
        Duration::from_secs(2),
    );
    adapter.child_environment.push((
        "FAKE_STOCKFISH_TRACE_DIRECTORY".to_string(),
        trace_directory.to_string_lossy().into_owned(),
    ));

    for _ in 0..3 {
        let analysis = adapter
            .analyze(EngineAnalysisInput { position: POSITION })
            .await
            .expect("each single-position analysis should succeed");
        assert_eq!(analysis.best_move, "e2e4");
    }

    let traces = fs::read_dir(&trace_directory)
        .expect("fake engine trace directory should be readable")
        .map(|entry| {
            fs::read_to_string(entry.expect("trace entry should be readable").path())
                .expect("fake engine trace should be readable")
        })
        .collect::<Vec<_>>();
    // One process for three analyses. Starting an engine costs a process
    // launch and a network load, which is most of a short search.
    assert_eq!(traces.len(), 1);
    let trace = &traces[0];
    assert_eq!(trace.lines().filter(|line| *line == "spawn").count(), 1);
    assert_eq!(
        trace.lines().filter(|line| *line == "go depth 12").count(),
        3
    );
    // One at initialization, then one before every search: each search starts
    // from the same empty table a fresh process would have.
    assert_eq!(
        trace.lines().filter(|line| *line == "ucinewgame").count(),
        4
    );
    fs::remove_dir_all(trace_directory).expect("fake engine trace directory should be removable");
}

#[tokio::test]
async fn stockfish_adapter_does_not_retain_a_timed_out_process() {
    let trace_directory = fake_trace_directory();
    fs::create_dir(&trace_directory).expect("fake engine trace directory should be creatable");
    let mut adapter = StockfishAdapter::with_hanging_command(
        fake_engine_command(),
        vec![
            "--exact".to_string(),
            "engine_analysis::tests::fake_stockfish_process".to_string(),
            "--nocapture".to_string(),
        ],
        12,
        Duration::from_millis(50),
    );
    adapter.child_environment.push((
        "FAKE_STOCKFISH_TRACE_DIRECTORY".to_string(),
        trace_directory.to_string_lossy().into_owned(),
    ));

    for _ in 0..2 {
        let error = adapter
            .analyze(EngineAnalysisInput { position: POSITION })
            .await
            .expect_err("a hung engine should time out");
        assert!(matches!(error, EngineAnalysisError::Timeout));
    }

    // A search that timed out leaves the process mid-command, so it is
    // killed rather than handed to the next caller: two attempts, two
    // processes.
    let traces = fs::read_dir(&trace_directory)
        .expect("fake engine trace directory should be readable")
        .count();
    assert_eq!(traces, 2);
    fs::remove_dir_all(trace_directory).expect("fake engine trace directory should be removable");
}

#[tokio::test]
async fn stockfish_adapter_returns_recoverable_error_when_process_cannot_start() {
    let adapter = StockfishAdapter::new(PathBuf::from("/path/that/does/not/contain/stockfish"), 12);

    let error = adapter
        .analyze(EngineAnalysisInput { position: POSITION })
        .await
        .expect_err("missing Stockfish should be recoverable");

    assert!(matches!(error, EngineAnalysisError::Process(_)));
}

#[tokio::test]
async fn stockfish_adapter_terminates_a_timed_out_process() {
    let adapter = StockfishAdapter::with_hanging_command(
        fake_engine_command(),
        vec![
            "--exact".to_string(),
            "engine_analysis::tests::fake_stockfish_process".to_string(),
            "--nocapture".to_string(),
        ],
        12,
        Duration::from_millis(50),
    );

    let error = adapter
        .analyze(EngineAnalysisInput { position: POSITION })
        .await
        .expect_err("hung Stockfish should time out");

    assert!(matches!(error, EngineAnalysisError::Timeout));
}

#[tokio::test]
#[ignore = "requires STOCKFISH_PATH to point to a real Stockfish binary"]
async fn stockfish_adapter_analyzes_with_real_engine() {
    let program = std::env::var_os("STOCKFISH_PATH")
        .map(PathBuf::from)
        .expect("STOCKFISH_PATH is required for the ignored integration test");
    let adapter = StockfishAdapter::new(program, 8);

    let analysis = adapter
        .analyze(EngineAnalysisInput { position: POSITION })
        .await
        .expect("real Stockfish should analyze the selected position");

    assert_eq!(analysis.depth, 8);
    assert!(!analysis.best_move.is_empty());
    assert_eq!(
        analysis.principal_variation.first(),
        Some(&analysis.best_move)
    );
}

#[test]
fn fake_stockfish_process() {
    if std::env::var_os("FAKE_STOCKFISH_PROCESS").is_none() {
        return;
    }

    let mut trace = std::env::var_os("FAKE_STOCKFISH_TRACE_DIRECTORY").map(|directory| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(PathBuf::from(directory).join(std::process::id().to_string()))
            .expect("fake engine trace should open")
    });
    record_fake_stockfish_trace(&mut trace, "spawn");
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut multi_pv = false;
    for line in stdin.lock().lines() {
        let line = line.expect("fake engine input should be readable");
        record_fake_stockfish_trace(&mut trace, &line);
        match line.as_str() {
            "uci" => writeln!(stdout, "id name Stockfish 18\nuciok")
                .expect("fake engine output should be writable"),
            "isready" => {
                writeln!(stdout, "readyok").expect("fake engine output should be writable")
            }
            "ucinewgame" => {}
            "setoption name Threads value 1" | "setoption name Hash value 16" => {}
            "setoption name MultiPV value 3" => multi_pv = true,
            command if command == format!("position fen {POSITION}") => {}
            "go depth 12" => {
                if std::env::var_os("FAKE_STOCKFISH_HANG").is_some() {
                    std::thread::sleep(Duration::from_secs(60));
                } else if multi_pv {
                    writeln!(
                        stdout,
                        "info depth 12 multipv 1 score cp 34 nodes 100 pv g1f3 g8f6\ninfo depth 12 multipv 2 score cp 21 nodes 100 pv b1c3 g8f6\ninfo depth 12 multipv 3 score cp 18 nodes 100 pv d2d4 e5d4\nbestmove g1f3 ponder g8f6"
                    )
                    .expect("fake engine output should be writable");
                } else {
                    writeln!(
                        stdout,
                        "info depth 12 score cp 34 nodes 100 pv e2e4 e7e5 g1f3\nbestmove e2e4 ponder e7e5"
                    )
                    .expect("fake engine output should be writable");
                }
            }
            "quit" => return,
            command => panic!("unexpected UCI command: {command}"),
        }
        stdout.flush().expect("fake engine output should flush");
    }
}

fn fake_engine_command() -> PathBuf {
    std::env::current_exe().expect("test executable should have a path")
}

fn fake_trace_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock should follow the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "chenchess-stockfish-session-{}-{nonce}",
        std::process::id()
    ))
}

fn record_fake_stockfish_trace(trace: &mut Option<std::fs::File>, line: &str) {
    if let Some(trace) = trace {
        writeln!(trace, "{line}").expect("fake engine trace should be writable");
        trace.flush().expect("fake engine trace should flush");
    }
}
