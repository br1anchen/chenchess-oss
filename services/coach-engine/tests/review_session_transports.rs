use std::{
    fs,
    io::Write,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[cfg(unix)]
use std::{os::fd::FromRawFd, os::unix::fs::FileTypeExt};

use chen_chess_coach_engine::{
    review_session_contract::*,
    review_session_processor::{ProcessorCommandAdmission, ProcessorPrincipal},
    review_session_transport::{ReviewSessionCommandExecutor, ReviewSessionWebBinding},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

const WEB_SUBJECT: &str = "transport-player";

#[tokio::test]
async fn web_binding_forwards_opaque_import_ids() {
    let executor = Arc::new(CountingExecutor::default());
    let binding = web_binding(executor.clone());

    let _receiver = binding.submit(
        WEB_SUBJECT,
        &serde_json::to_vec(&envelope(
            "start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: GameImportId::try_from("game-import:transport".to_string())
                    .unwrap(),
            },
        ))
        .unwrap(),
    );
    tokio::task::yield_now().await;
    assert_eq!(executor.submissions.load(Ordering::SeqCst), 1);

    let _receiver = binding.submit(
        WEB_SUBJECT,
        &serde_json::to_vec(&envelope("publish", publish_command())).unwrap(),
    );
    tokio::task::yield_now().await;
    assert_eq!(executor.submissions.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn web_binding_forwards_only_privacy_safe_trace_handles() {
    let executor = Arc::new(CountingExecutor::default());
    let binding = web_binding(executor.clone());
    let command = serde_json::to_vec(&envelope(
        "trace",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: GameImportId::try_from("game-import:transport".to_string()).unwrap(),
        },
    ))
    .unwrap();

    let _receiver = binding.submit_with_trace(
        WEB_SUBJECT,
        &command,
        Some("trace:review-session:123e4567-e89b-42d3-a456-426614174000"),
    );
    assert_eq!(
        executor.trace_id.lock().unwrap().as_deref(),
        Some("trace:review-session:123e4567-e89b-42d3-a456-426614174000"),
    );

    let _receiver = binding.submit_with_trace(WEB_SUBJECT, &command, Some("player@example.com"));
    assert_eq!(executor.trace_id.lock().unwrap().as_deref(), None);
}

#[tokio::test]
async fn cli_process_streams_valid_events_and_stable_rejections_as_jsonl() {
    let cli_dir =
        std::env::temp_dir().join(format!("chenchess-transport-cli-{}", std::process::id()));
    fs::create_dir_all(&cli_dir).expect("CLI fixture directory should be created");
    let mut child = Command::new(env!("CARGO_BIN_EXE_chenchess"))
        .args(["review-session", "--jsonl"])
        .env("STOCKFISH_PATH", "/not-used-by-admission-rejections")
        .env("MAIA_BASE_URL", "http://127.0.0.1:1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("chenchess should start");
    let mut stdin = child.stdin.take().expect("stdin should be piped");
    let mut start = envelope(
        "cli-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: GameImportId::try_from("game-import:missing".to_string()).unwrap(),
        },
    );
    start.surface = DeliverySurface::CoachSkill;
    let mut input = b"not-json\n{\"unexpected\":true}\n".to_vec();
    input.extend(serde_json::to_vec(&start).unwrap());
    input.push(b'\n');
    stdin
        .write_all(&input)
        .await
        .expect("command should be written");
    stdin.shutdown().await.expect("stdin should close");
    drop(stdin);

    let output = child
        .wait_with_output()
        .await
        .expect("chenchess should exit");
    fs::remove_dir_all(cli_dir).expect("CLI fixture directory should be removed");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<ReviewSessionEventEnvelope>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(events.iter().any(|event| matches!(
        event.event,
        ReviewSessionEvent::Rejected {
            reason: CommandRejectionReason::MalformedInput,
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event.event,
        ReviewSessionEvent::Rejected {
            reason: CommandRejectionReason::UnknownCommand,
            ..
        }
    )));
    assert!(
        events.iter().any(|event| matches!(
            event.event,
            ReviewSessionEvent::Rejected {
                reason: CommandRejectionReason::UnknownGameImport,
                ..
            }
        )),
        "{events:#?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cli_process_accepts_long_jsonl_commands_while_host_stdin_is_a_canonical_pty() {
    let cli_dir = std::env::temp_dir().join(format!("chenchess-pty-cli-{}", std::process::id()));
    fs::create_dir_all(&cli_dir).expect("CLI fixture directory should be created");
    let fifo_path = cli_dir.join("commands.fifo");
    let (_master, slave) = open_pty();
    let mut child = Command::new(env!("CARGO_BIN_EXE_chenchess"))
        .args(["review-session", "--jsonl", "--command-fifo"])
        .arg(&fifo_path)
        .env("STOCKFISH_PATH", "/not-used-by-invalid-pgn")
        .env("MAIA_BASE_URL", "http://127.0.0.1:1")
        .stdin(std::process::Stdio::from(slave))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("chenchess should start with PTY stdin");
    let stdout = child.stdout.take().expect("stdout should be piped");
    let mut lines = BufReader::new(stdout).lines();
    let mut command = serde_json::to_vec(&serde_json::json!({
        "requestId": "request:transport:long-pty-command",
        "operationId": "operation:transport:long-pty-command",
        "surface": "coachSkill",
        "command": {
            "kind": "importGame",
            "source": {
                "kind": "pastedPgn",
                "pgn": "x".repeat(2_048)
            },
            "reviewSide": { "kind": "selected", "reviewSide": "white" },
            "eloProfile": { "kind": "playerProvided", "rating": 1200 }
        }
    }))
    .expect("command should serialize");
    command.push(b'\n');
    assert!(command.len() > 1_024);

    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if fifo_path
                .metadata()
                .is_ok_and(|metadata| metadata.file_type().is_fifo())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("ChenChess should create its command FIFO");
    let mut command_fifo = fs::OpenOptions::new()
        .write(true)
        .open(&fifo_path)
        .expect("command FIFO should open for writing");
    command_fifo
        .write_all(&command)
        .expect("FIFO command should be written");

    let event = tokio::time::timeout(Duration::from_secs(5), lines.next_line())
        .await
        .expect("ChenChess should read the complete FIFO command without waiting forever")
        .expect("CLI stdout should remain readable")
        .expect("the command should emit an event");
    let event: ReviewSessionEventEnvelope =
        serde_json::from_str(&event).expect("stdout should contain a JSONL event");
    assert_eq!(
        event.request_id,
        RequestId::try_from("request:transport:long-pty-command".to_string()).unwrap()
    );

    let child_id = child.id().expect("test process should have an ID");
    assert_eq!(
        // SAFETY: `child_id` is this test's live process id; SIGINT is a
        // valid signal and the waiter below joins that same child.
        unsafe { libc::kill(child_id as libc::pid_t, libc::SIGINT) },
        0
    );
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("test process should stop after SIGINT")
        .expect("test process should exit");
    assert_eq!(status.code(), Some(130));
    assert!(
        !fifo_path.exists(),
        "command FIFO should be removed on graceful shutdown"
    );
    fs::remove_dir_all(cli_dir).expect("CLI fixture directory should be removed");
}

#[cfg(unix)]
fn open_pty() -> (fs::File, fs::File) {
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: `openpty` writes the two local fd slots and the remaining
    // arguments are optional null name/termios/winsize pointers.
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(
        opened,
        0,
        "PTY should open: {}",
        std::io::Error::last_os_error()
    );

    // SAFETY: `termios` is a C struct filled by `tcgetattr` before any field
    // is read; zeroing it is the libc-required uninitialized start state.
    let mut settings = unsafe { std::mem::zeroed::<libc::termios>() };
    assert_eq!(
        // SAFETY: `slave` is a live PTY fd from `openpty`; `settings` is a
        // valid `termios` out-pointer.
        unsafe { libc::tcgetattr(slave, &mut settings) },
        0,
        "PTY settings should be readable: {}",
        std::io::Error::last_os_error()
    );
    settings.c_lflag |= libc::ICANON | libc::ECHO;
    settings.c_iflag |= libc::IMAXBEL;
    assert_eq!(
        // SAFETY: `slave` remains the same live PTY fd; `settings` now holds
        // the values just read and the two flag bits this test needs.
        unsafe { libc::tcsetattr(slave, libc::TCSANOW, &settings) },
        0,
        "PTY settings should be writable: {}",
        std::io::Error::last_os_error()
    );

    // SAFETY: both fds came from a successful `openpty` and are not used
    // again except through these owning `File` values.
    unsafe { (fs::File::from_raw_fd(master), fs::File::from_raw_fd(slave)) }
}

fn web_binding(executor: Arc<CountingExecutor>) -> ReviewSessionWebBinding {
    ReviewSessionWebBinding::new(executor)
}

fn envelope(label: &str, command: ReviewSessionCommand) -> ReviewSessionCommandEnvelope {
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from(format!("request:transport:{label}")).unwrap(),
        operation_id: OperationId::try_from(format!("operation:transport:{label}")).unwrap(),
        surface: DeliverySurface::Web,
        command,
    }
}

fn publish_command() -> ReviewSessionCommand {
    let coach_turn_id = CoachTurnId::try_from("coach-turn:transport:publish".to_string()).unwrap();
    let dimension = AssessmentDimension {
        explanation: "The server validates prepared evidence before accepting this assessment."
            .to_string(),
        evidence_refs: Vec::new(),
    };
    ReviewSessionCommand::PublishCoachTurn {
        game_import_id: GameImportId::try_from("review-session:transport:publish".to_string())
            .unwrap(),
        review_moment_id: CriticalMomentId::try_from("review-moment:transport:1".to_string())
            .unwrap(),
        coach_turn_id: coach_turn_id.clone(),
        assessment: Box::new(AlternativeMoveAssessment {
            coach_turn_id,
            alternative_move_id: AlternativeMoveId::try_from(
                "alternative-move:transport:publish".to_string(),
            )
            .unwrap(),
            objective_quality: dimension.clone(),
            findability: dimension.clone(),
            resilience: dimension,
        }),
        idempotency_key: IdempotencyKey::try_from("idempotency-key:transport:publish".to_string())
            .unwrap(),
    }
}

#[derive(Default)]
struct CountingExecutor {
    submissions: AtomicUsize,
    trace_id: Mutex<Option<String>>,
}

impl ReviewSessionCommandExecutor for CountingExecutor {
    fn submit(
        self: Arc<Self>,
        _principal: ProcessorPrincipal,
        _admission: ProcessorCommandAdmission,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        self.submissions.fetch_add(1, Ordering::SeqCst);
        let (_sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        receiver
    }

    fn submit_with_trace(
        self: Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
        trace_id: Option<String>,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        *self.trace_id.lock().unwrap() = trace_id;
        self.submit(principal, admission)
    }
}
