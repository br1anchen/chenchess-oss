pub const MAX_GAME_PLIES: usize = 400;
/// Bounds cached Position, Engine Analysis, and Human Move Model evidence retained by one
/// Review Moment. Branch and Provenance entries have their own domain cardinalities.
pub const MAX_REVIEW_MOMENT_CACHED_EVIDENCE_ENTRIES: usize = 256;
/// Bounds one local JSONL command independently of the host terminal's input queue.
pub const MAX_REVIEW_SESSION_COMMAND_BYTES: usize = 16 * 1024 * 1024;
/// Eight single-threaded Stockfish searches use the certified 10-CPU runtime before the
/// Maia phase, leaving two logical CPUs for the host.
pub const REVIEW_FACTS_ENGINE_CONCURRENCY: usize = 8;
/// Four Maia requests run after Stockfish completes; the service pins PyTorch's CPU
/// intra-operation limit to two threads and its inter-operation limit to one.
pub const REVIEW_FACTS_HUMAN_CONCURRENCY: usize = 4;
pub const PROVIDER_POSITION_TIMEOUT_SECONDS: u64 = 30;
pub const RUNTIME_STARTUP_TIMEOUT_SECONDS: u64 = 600;
pub const LIVE_COMMAND_TIMEOUT_SECONDS: u64 = 14_400;
pub const CANCELLATION_BUDGET_MILLISECONDS: u64 = 5_000;
/// Includes one fresh Stockfish process initialization plus the pinned depth-16 search.
/// Staging acceptance showed that the previous three-second bound expired even for the
/// starting position despite healthy CPU headroom, while the adapter's independent
/// per-position timeout remains the final 30-second provider ceiling.
pub const ALTERNATIVE_MOVE_DEADLINE_MILLISECONDS: u64 = 10_000;
pub const MAIA_DEADLINE_MILLISECONDS: u64 = 2_000;
pub const COACH_TURN_DEADLINE_SECONDS: u64 = 30;
/// One HostTurn, shared across every step including the corrective retry.
pub const HOST_TURN_DEADLINE_SECONDS: u64 = 15;
pub const REVIEW_MOMENT_PREPARATION_DEADLINE_SECONDS: u64 = 15;
pub const PROGRESS_EVERY_CASES: usize = 1;
pub const EVALUATION_CENTIPAWN_TOLERANCE: u32 = 15;
pub const PROBABILITY_TOLERANCE: f64 = 0.02;
pub const PROJECTED_PLAN_BEAM_WIDTH: usize = 3;
pub const PROJECTED_PLAN_ENGINE_CONCURRENCY: usize = 8;
pub const PROJECTED_PLAN_REQUIRED_HALF_MOVES: usize = 4;
/// Keeps recent owner-scoped imports available for the ID-only handoff and in-game navigation
/// without turning the in-memory processor into unbounded review history.
pub const IMPORTED_REVIEW_FACTS_CAPACITY: usize = 256;
