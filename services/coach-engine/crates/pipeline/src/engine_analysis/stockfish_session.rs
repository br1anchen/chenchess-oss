use std::{
    future::Future,
    pin::Pin,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    task::Poll,
    time::Instant,
};

use tokio::{
    io::{AsyncBufReadExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
    time::timeout,
};

use super::{
    multi_pv, read_stockfish_version, read_until, run_position_protocol, send_command,
    validate_input, EngineAnalysis, EngineAnalysisError, EngineMultiPvRunOutput, EngineRunOutput,
    IndexedEngineAnalysisError, RankedEngineAnalysis, StockfishAdapter, TimedEngineAnalysis,
};
use crate::{
    operating_limits::CANCELLATION_BUDGET_MILLISECONDS,
    provider_concurrency::{collect_ordered_results, IndexedProviderError},
};

pub(super) async fn run(
    adapter: &StockfishAdapter,
    position: &str,
) -> Result<EngineRunOutput, EngineAnalysisError> {
    let started = Instant::now();
    let mut session = match adapter.sessions.take().await {
        Some(held) => held,
        None => StockfishSession::start(adapter).await?,
    };
    let stockfish_version = session.stockfish_version.clone();
    // One rule for the whole exchange: a session that answered is retained, and
    // a session that did not is killed. Anything that fails partway leaves the
    // process mid-command, so it must never reach the next caller.
    match reset_and_analyze(&mut session, adapter, position, started).await {
        Ok(analysis) => {
            adapter.sessions.give(session).await;
            Ok(EngineRunOutput {
                analysis,
                stockfish_version,
            })
        }
        Err(error) => {
            session.terminate_and_reap().await;
            Err(error)
        }
    }
}

/// Put a session into the state a fresh process starts in, then search.
///
/// The reset runs even on a session that was just started, where it is
/// redundant — `initialize` ends with the same `ucinewgame`. One protocol round
/// trip against an idle engine is not worth a branch here, and paying it
/// unconditionally makes the invariant a reader has to hold much smaller:
/// every search in this path starts from an empty table.
async fn reset_and_analyze(
    session: &mut StockfishSession,
    adapter: &StockfishAdapter,
    position: &str,
    started: Instant,
) -> Result<EngineAnalysis, EngineAnalysisError> {
    session.reset(adapter).await?;
    session
        .analyze(adapter, position, remaining_timeout(adapter, started)?)
        .await
}

pub(super) async fn run_multi_pv(
    adapter: &StockfishAdapter,
    position: &str,
    variation_count: u8,
) -> Result<EngineMultiPvRunOutput, EngineAnalysisError> {
    let started = Instant::now();
    let mut session = StockfishSession::start(adapter).await?;
    let stockfish_version = session.stockfish_version.clone();
    let variations = session
        .analyze_multi_pv(
            adapter,
            position,
            variation_count,
            remaining_timeout(adapter, started)?,
        )
        .await?;
    session.finish().await?;
    Ok(EngineMultiPvRunOutput {
        variations,
        stockfish_version,
    })
}

pub(super) async fn run_positions(
    adapter: Arc<StockfishAdapter>,
    positions: Vec<String>,
    concurrency: usize,
) -> Result<Vec<TimedEngineAnalysis>, IndexedEngineAnalysisError> {
    let position_count = positions.len();
    if position_count == 0 {
        return Ok(Vec::new());
    }
    let worker_count = concurrency.min(position_count);
    let base_positions_per_worker = position_count / worker_count;
    let workers_with_extra_position = position_count % worker_count;
    let mut indexed_positions = positions.into_iter().enumerate();
    let earliest_error = Arc::new(AtomicUsize::new(usize::MAX));
    let workers = (0..worker_count)
        .map(|worker_index| {
            let position_count =
                base_positions_per_worker + usize::from(worker_index < workers_with_extra_position);
            Box::pin(run_worker(
                adapter.clone(),
                indexed_positions.by_ref().take(position_count).collect(),
                earliest_error.clone(),
            )) as Pin<Box<dyn Future<Output = WorkerResults> + Send>>
        })
        .collect();

    let mut results = (0..position_count).map(|_| None).collect::<Vec<_>>();
    for completed in join_workers(workers).await {
        for (index, result) in completed {
            results[index] = Some(result);
        }
    }
    collect_ordered_results(results).map_err(|IndexedProviderError { index, error }| {
        IndexedEngineAnalysisError { index, error }
    })
}

/// How long an idle engine process is kept before it is released.
///
/// An idle Stockfish holds its evaluation network resident: the pinned binary
/// measured 419 MiB with `Hash=16` and nothing to search. That is far too much
/// to keep on a cell that has gone quiet, and it is why this window exists
/// rather than the session simply living as long as the adapter.
///
/// A Player exploring a line plays several moves in a row, so the window only
/// has to outlast the gap between their own moves.
const IDLE_SESSION_RELEASE: std::time::Duration = std::time::Duration::from_secs(60);

/// One retained engine process, reused across single-position analyses.
///
/// Starting Stockfish costs a process launch and an NNUE network load. Measured
/// on the certified machine at depth 16 over twelve distinct positions, that is
/// a 341 ms median per call against 104 ms when the process is reused — most of
/// every interactive evaluation spent starting an engine rather than searching.
///
/// Exactly one session is retained. Interactive evaluations already serialize
/// on the engine lease, so a deeper pool would not shorten a queue; it would
/// only multiply the resident network. A caller that finds the slot taken
/// starts its own process and closes it, which is the behaviour every caller
/// had before.
#[derive(Clone, Default)]
pub(super) struct SessionCache {
    inner: Arc<SessionCacheInner>,
}

#[derive(Default)]
struct SessionCacheInner {
    held: Mutex<Option<StockfishSession>>,
    /// Bumped on every take and give, so a release task can tell whether the
    /// session it was scheduled for is still the one sitting in the slot.
    generation: AtomicU64,
}

impl SessionCache {
    async fn take(&self) -> Option<StockfishSession> {
        self.inner.generation.fetch_add(1, Ordering::Relaxed);
        self.inner.held.lock().await.take()
    }

    async fn give(&self, session: StockfishSession) {
        let generation = self.inner.generation.fetch_add(1, Ordering::Relaxed) + 1;
        // The slot is held only for the swap. Reaping a process is slow enough
        // that doing it under the lock would stall the next caller's take.
        let surplus = self.inner.held.lock().await.replace(session);
        if let Some(mut surplus) = surplus {
            // Another caller filled the slot first. Two retained engines buy
            // nothing, so this one is closed rather than kept.
            surplus.terminate_and_reap().await;
        }
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            tokio::time::sleep(IDLE_SESSION_RELEASE).await;
            if inner.generation.load(Ordering::Relaxed) != generation {
                return;
            }
            let idle = inner.held.lock().await.take();
            if let Some(mut idle) = idle {
                idle.terminate_and_reap().await;
            }
        });
    }
}

struct StockfishSession {
    child: Option<Child>,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    stockfish_version: Option<String>,
}

impl StockfishSession {
    async fn start(adapter: &StockfishAdapter) -> Result<Self, EngineAnalysisError> {
        let mut command = Command::new(&adapter.program);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(test)]
        command.args(&adapter.args);
        #[cfg(test)]
        command.envs(adapter.child_environment.iter().cloned());
        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| EngineAnalysisError::Protocol("stdin pipe unavailable".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| EngineAnalysisError::Protocol("stdout pipe unavailable".to_string()))?;
        let mut session = Self {
            child: Some(child),
            stdin,
            lines: BufReader::new(stdout).lines(),
            stockfish_version: None,
        };
        let initialized = timeout(adapter.timeout, session.initialize(adapter)).await;
        match initialized {
            Ok(Ok(())) => Ok(session),
            Ok(Err(error)) => {
                session.terminate_and_reap().await;
                Err(error)
            }
            Err(_) => {
                session.terminate_and_reap().await;
                Err(EngineAnalysisError::Timeout)
            }
        }
    }

    async fn initialize(&mut self, adapter: &StockfishAdapter) -> Result<(), EngineAnalysisError> {
        send_command(&mut self.stdin, "uci").await?;
        self.stockfish_version = read_stockfish_version(&mut self.lines).await?;
        send_command(
            &mut self.stdin,
            &format!("setoption name Threads value {}", adapter.threads),
        )
        .await?;
        send_command(
            &mut self.stdin,
            &format!("setoption name Hash value {}", adapter.hash_mib),
        )
        .await?;
        send_command(&mut self.stdin, "isready").await?;
        read_until(&mut self.lines, "readyok").await?;
        send_command(&mut self.stdin, "ucinewgame").await?;
        send_command(&mut self.stdin, "isready").await?;
        read_until(&mut self.lines, "readyok").await
    }

    /// Return a used session to the state a freshly started one is in.
    ///
    /// `ucinewgame` clears the transposition table and the search history, so a
    /// reused process searches from the same empty state a new one would. That
    /// is what keeps reuse a timing change: measured over twelve positions at
    /// depth 16, best move and score were identical to a fresh process in all
    /// twelve. Retaining the table instead would be faster on a repeated
    /// position and would change evaluations at a fixed depth.
    async fn reset(&mut self, adapter: &StockfishAdapter) -> Result<(), EngineAnalysisError> {
        match timeout(adapter.timeout, async {
            send_command(&mut self.stdin, "ucinewgame").await?;
            send_command(&mut self.stdin, "isready").await?;
            read_until(&mut self.lines, "readyok").await
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(EngineAnalysisError::Timeout),
        }
    }

    async fn analyze(
        &mut self,
        adapter: &StockfishAdapter,
        position: &str,
        analysis_timeout: std::time::Duration,
    ) -> Result<EngineAnalysis, EngineAnalysisError> {
        validate_input(position, adapter.depth)?;
        let protocol = timeout(
            analysis_timeout,
            run_position_protocol(&mut self.stdin, &mut self.lines, position, adapter.depth),
        )
        .await;
        match protocol {
            Ok(Ok(analysis)) => Ok(analysis),
            Ok(Err(error)) => {
                self.terminate_and_reap().await;
                Err(error)
            }
            Err(_) => {
                self.terminate_and_reap().await;
                Err(EngineAnalysisError::Timeout)
            }
        }
    }

    async fn analyze_multi_pv(
        &mut self,
        adapter: &StockfishAdapter,
        position: &str,
        variation_count: u8,
        analysis_timeout: std::time::Duration,
    ) -> Result<Vec<RankedEngineAnalysis>, EngineAnalysisError> {
        validate_input(position, adapter.depth)?;
        multi_pv::validate_count(variation_count)?;
        let protocol = timeout(
            analysis_timeout,
            multi_pv::run_protocol(
                &mut self.stdin,
                &mut self.lines,
                position,
                adapter.depth,
                variation_count,
            ),
        )
        .await;
        match protocol {
            Ok(Ok(variations)) => Ok(variations),
            Ok(Err(error)) => {
                self.terminate_and_reap().await;
                Err(error)
            }
            Err(_) => {
                self.terminate_and_reap().await;
                Err(EngineAnalysisError::Timeout)
            }
        }
    }

    async fn finish(mut self) -> Result<(), EngineAnalysisError> {
        if self.child.is_none() {
            return Ok(());
        }
        if let Err(error) = send_command(&mut self.stdin, "quit").await {
            self.terminate_and_reap().await;
            return Err(error);
        }
        let mut child = self
            .child
            .take()
            .expect("a live Stockfish session should own its child");
        ensure_success(&mut child).await
    }

    async fn terminate_and_reap(&mut self) {
        if let Some(mut child) = self.child.take() {
            terminate_and_reap(&mut child).await;
        }
    }
}

impl Drop for StockfishSession {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.start_kill();
        // Cancellation can shut down the runtime before an async reaper runs.
        let deadline =
            Instant::now() + std::time::Duration::from_millis(CANCELLATION_BUDGET_MILLISECONDS);
        loop {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Ok(None) => return,
            }
        }
    }
}

type WorkerResults = Vec<(usize, Result<TimedEngineAnalysis, EngineAnalysisError>)>;

async fn join_workers(
    workers: Vec<Pin<Box<dyn Future<Output = WorkerResults> + Send>>>,
) -> Vec<WorkerResults> {
    // Keep workers under the parent future so cancellation drops every engine session immediately.
    let mut workers = workers.into_iter().map(Some).collect::<Vec<_>>();
    let mut completed = Vec::with_capacity(workers.len());
    std::future::poll_fn(move |context| {
        let mut pending = false;
        for worker in &mut workers {
            let Some(future) = worker else {
                continue;
            };
            match future.as_mut().poll(context) {
                Poll::Ready(results) => {
                    *worker = None;
                    completed.push(results);
                }
                Poll::Pending => pending = true,
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(std::mem::take(&mut completed))
        }
    })
    .await
}

async fn run_worker(
    adapter: Arc<StockfishAdapter>,
    positions: Vec<(usize, String)>,
    earliest_error: Arc<AtomicUsize>,
) -> WorkerResults {
    let mut results = Vec::new();
    let mut positions = positions.into_iter();
    let (first_index, first_position) = positions
        .next()
        .expect("every Stockfish worker should own at least one Position");
    let first_started = Instant::now();
    let mut session = match StockfishSession::start(&adapter).await {
        Ok(session) => session,
        Err(error) => {
            earliest_error.fetch_min(first_index, Ordering::AcqRel);
            results.push((first_index, Err(error)));
            return results;
        }
    };
    let provenance = adapter.measured_provenance(session.stockfish_version.clone());
    for (index, position) in std::iter::once((first_index, first_position)).chain(positions) {
        if index > earliest_error.load(Ordering::Acquire) {
            break;
        }
        let started = if index == first_index {
            first_started
        } else {
            Instant::now()
        };
        let analysis_timeout = if index == first_index {
            match remaining_timeout(&adapter, started) {
                Ok(timeout) => timeout,
                Err(error) => {
                    session.terminate_and_reap().await;
                    earliest_error.fetch_min(index, Ordering::AcqRel);
                    results.push((index, Err(error)));
                    return results;
                }
            }
        } else {
            adapter.timeout
        };
        /* The same reset the interactive path takes, for the same reason and
        one the recordings need more. A worker searches a slice of a Game in
        order, so without this a Position's answer depends on which Positions
        preceded it in this worker and on how many workers the slice was cut
        into. Canonical ply 26 answers d6d5 from an empty table and c5d4 at 281
        once two neighbouring Positions have been searched first, which is how
        two captures of the same Position under the same binary came to record
        285 and 257. A recorded analysis has to be a function of the binary, the
        Position, and the search limits alone. */
        if let Err(error) = session.reset(&adapter).await {
            session.terminate_and_reap().await;
            earliest_error.fetch_min(index, Ordering::AcqRel);
            results.push((index, Err(error)));
            return results;
        }
        match session.analyze(&adapter, &position, analysis_timeout).await {
            Ok(analysis) => results.push((
                index,
                Ok(TimedEngineAnalysis {
                    analysis,
                    provenance: provenance.clone(),
                    duration: started.elapsed(),
                }),
            )),
            Err(error) => {
                earliest_error.fetch_min(index, Ordering::AcqRel);
                results.push((index, Err(error)));
                return results;
            }
        }
    }

    if let Err(error) = session.finish().await {
        if let Some((index, result)) = results.last_mut() {
            earliest_error.fetch_min(*index, Ordering::AcqRel);
            *result = Err(error);
        }
    }
    results
}

fn remaining_timeout(
    adapter: &StockfishAdapter,
    started: Instant,
) -> Result<std::time::Duration, EngineAnalysisError> {
    adapter
        .timeout
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(EngineAnalysisError::Timeout)
}

async fn ensure_success(child: &mut Child) -> Result<(), EngineAnalysisError> {
    let status = child.wait().await?;
    if status.success() {
        Ok(())
    } else {
        Err(EngineAnalysisError::Protocol(format!(
            "Stockfish exited with {status}"
        )))
    }
}

async fn terminate_and_reap(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}
