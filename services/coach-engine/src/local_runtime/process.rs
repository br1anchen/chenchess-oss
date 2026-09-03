use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use wait_timeout::ChildExt;

use super::{ProcessOutput, ProcessRunner};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Default)]
pub struct ProcessCancellation {
    cancelled: Arc<AtomicBool>,
}

impl ProcessCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Default)]
pub struct SystemProcessRunner {
    cancellation: Option<ProcessCancellation>,
}

impl SystemProcessRunner {
    pub fn cancellable() -> (Self, ProcessCancellation) {
        let cancellation = ProcessCancellation::default();
        (
            Self {
                cancellation: Some(cancellation.clone()),
            },
            cancellation,
        )
    }

    fn ensure_active(&self, program: &Path) -> Result<(), String> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(ProcessCancellation::is_cancelled)
        {
            Err(format!("{} cancelled", program.display()))
        } else {
            Ok(())
        }
    }
}

impl ProcessRunner for SystemProcessRunner {
    fn run(&self, program: &Path, args: &[String]) -> Result<ProcessOutput, String> {
        self.ensure_active(program)?;
        let child = std::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        wait_for_process_output(child, program, self.cancellation.as_ref())
    }

    fn run_with_input(
        &self,
        program: &Path,
        args: &[String],
        input: &str,
    ) -> Result<ProcessOutput, String> {
        self.ensure_active(program)?;
        let mut child = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        child
            .stdin
            .take()
            .ok_or_else(|| "process stdin was not piped".to_string())?
            .write_all(input.as_bytes())
            .map_err(|error| error.to_string())?;
        wait_for_process_output(child, program, self.cancellation.as_ref())
    }
}

fn wait_for_process_output(
    mut child: std::process::Child,
    program: &Path,
    cancellation: Option<&ProcessCancellation>,
) -> Result<ProcessOutput, String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "process stdout was not piped".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "process stderr was not piped".to_string())?;
    let stdout_reader = std::thread::spawn(move || read_process_pipe(stdout));
    let stderr_reader = std::thread::spawn(move || read_process_pipe(stderr));
    let started = Instant::now();

    let status = loop {
        if cancellation.is_some_and(ProcessCancellation::is_cancelled) {
            terminate_and_reap(&mut child, program)?;
            let _ = join_process_pipe(stdout_reader);
            let _ = join_process_pipe(stderr_reader);
            return Err(format!("{} cancelled", program.display()));
        }
        let remaining = PROCESS_TIMEOUT.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            terminate_and_reap(&mut child, program)?;
            let _ = join_process_pipe(stdout_reader);
            let _ = join_process_pipe(stderr_reader);
            return Err(format!("{} timed out after 15 minutes", program.display()));
        }
        if let Some(status) = child
            .wait_timeout(remaining.min(CANCELLATION_POLL_INTERVAL))
            .map_err(|error| error.to_string())?
        {
            break status;
        }
    };

    let stdout = join_process_pipe(stdout_reader)?;
    let stderr = join_process_pipe(stderr_reader)?;
    Ok(ProcessOutput {
        success: status.success(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

fn terminate_and_reap(child: &mut std::process::Child, program: &Path) -> Result<(), String> {
    if child
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_none()
    {
        child.kill().map_err(|error| {
            format!(
                "{} could not be terminated after cancellation or timeout: {error}",
                program.display()
            )
        })?;
    }
    let _ = child.wait();
    Ok(())
}

fn read_process_pipe(mut pipe: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn join_process_pipe(
    reader: std::thread::JoinHandle<Result<Vec<u8>, String>>,
) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| "process output reader panicked".to_string())?
}
