use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

use crate::review_session_contract::{OperationId, PlayerId};

/// Versioned Central Host Player traffic policy for the current one-engine-cell
/// Coach Engine deployment. Window state is process-local: a second replica is
/// not authorized until shared enforcement exists.
pub const PLAYER_TRAFFIC_POLICY_VERSION: &str = "v1";
pub const PLAYER_COMMAND_LIMIT: usize = 120;
pub const PLAYER_COMMAND_WINDOW_MS: u64 = 60_000;
pub const PLAYER_IMPORT_LIMIT: usize = 10;
pub const PLAYER_IMPORT_WINDOW_MS: u64 = 600_000;
pub const PLAYER_OPENING_ANALYSIS_LIMIT: usize = 10;
pub const PLAYER_OPENING_ANALYSIS_WINDOW_MS: u64 = 60_000;
pub const PLAYER_TRAFFIC_MAX_PLAYERS: usize = 10_000;

pub trait PlayerTrafficClock: Send + Sync {
    fn now_ms(&self) -> u64;
}

pub struct SystemTrafficClock;

impl PlayerTrafficClock for SystemTrafficClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

#[cfg(test)]
pub struct ControllableTrafficClock {
    now_ms: AtomicU64,
}

#[cfg(test)]
impl ControllableTrafficClock {
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms: AtomicU64::new(now_ms),
        }
    }

    pub fn advance_ms(&self, delta: u64) {
        self.now_ms.fetch_add(delta, Ordering::SeqCst);
    }
}

#[cfg(test)]
impl PlayerTrafficClock for ControllableTrafficClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Copy)]
enum TrafficClass {
    Command,
    Import,
    OpeningAnalysis,
}

impl TrafficClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Import => "import",
            Self::OpeningAnalysis => "openingAnalysis",
        }
    }
}

#[derive(Clone, Copy)]
enum TrafficDecision {
    Admitted,
    Rejected,
    Idempotent,
}

impl TrafficDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Rejected => "rejected",
            Self::Idempotent => "idempotent",
        }
    }
}

struct PlayerWindows {
    commands: VecDeque<u64>,
    imports: VecDeque<u64>,
    opening_analyses: VecDeque<u64>,
    accepted_imports: HashMap<String, u64>,
}

impl PlayerWindows {
    fn empty() -> Self {
        Self {
            commands: VecDeque::new(),
            imports: VecDeque::new(),
            opening_analyses: VecDeque::new(),
            accepted_imports: HashMap::new(),
        }
    }

    fn expire(&mut self, now_ms: u64) {
        expire_window(&mut self.commands, now_ms, PLAYER_COMMAND_WINDOW_MS);
        expire_window(&mut self.imports, now_ms, PLAYER_IMPORT_WINDOW_MS);
        expire_window(
            &mut self.opening_analyses,
            now_ms,
            PLAYER_OPENING_ANALYSIS_WINDOW_MS,
        );
        self.accepted_imports
            .retain(|_, admitted_at| now_ms.saturating_sub(*admitted_at) < PLAYER_IMPORT_WINDOW_MS);
    }

    fn is_idle(&self) -> bool {
        self.commands.is_empty()
            && self.imports.is_empty()
            && self.opening_analyses.is_empty()
            && self.accepted_imports.is_empty()
    }
}

pub struct PlayerTrafficPolicy {
    clock: Arc<dyn PlayerTrafficClock>,
    state: Mutex<HashMap<String, PlayerWindows>>,
}

impl PlayerTrafficPolicy {
    pub fn v1() -> Self {
        Self::v1_with_clock(Arc::new(SystemTrafficClock))
    }

    pub fn v1_with_clock(clock: Arc<dyn PlayerTrafficClock>) -> Self {
        Self {
            clock,
            state: Mutex::new(HashMap::new()),
        }
    }

    pub fn admit_command(&self, player_id: &PlayerId) -> Result<(), u32> {
        self.admit(player_id, TrafficClass::Command, None)
    }

    pub fn admit_import(
        &self,
        player_id: &PlayerId,
        operation_id: &OperationId,
    ) -> Result<(), u32> {
        self.admit(player_id, TrafficClass::Import, Some(operation_id))
    }

    pub fn admit_opening_analysis(&self, player_id: &PlayerId) -> Result<(), u32> {
        self.admit(player_id, TrafficClass::OpeningAnalysis, None)
    }

    fn admit(
        &self,
        player_id: &PlayerId,
        class: TrafficClass,
        operation_id: Option<&OperationId>,
    ) -> Result<(), u32> {
        let now_ms = self.clock.now_ms();
        let player_key = player_id.as_str();
        let mut players = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        expire_all(&mut players, now_ms);

        if let Some(operation_id) = operation_id {
            if let Some(windows) = players.get(player_key) {
                if windows.accepted_imports.contains_key(operation_id.as_str()) {
                    let occupancy = windows.imports.len();
                    let bounded_occupancy = players.len();
                    drop(players);
                    emit_traffic(
                        class,
                        TrafficDecision::Idempotent,
                        None,
                        occupancy,
                        bounded_occupancy,
                    );
                    return Ok(());
                }
            }
        }

        if !players.contains_key(player_key) && players.len() >= PLAYER_TRAFFIC_MAX_PLAYERS {
            let retry_after_seconds = cardinality_retry_after(&players, now_ms);
            let bounded_occupancy = players.len();
            drop(players);
            emit_traffic(
                class,
                TrafficDecision::Rejected,
                Some(retry_after_seconds),
                PLAYER_TRAFFIC_MAX_PLAYERS,
                bounded_occupancy,
            );
            return Err(retry_after_seconds);
        }

        let outcome = {
            let windows = players
                .entry(player_key.to_owned())
                .or_insert_with(PlayerWindows::empty);
            let (limit, window_ms, occupancy) = match class {
                TrafficClass::Command => (
                    PLAYER_COMMAND_LIMIT,
                    PLAYER_COMMAND_WINDOW_MS,
                    windows.commands.len(),
                ),
                TrafficClass::Import => (
                    PLAYER_IMPORT_LIMIT,
                    PLAYER_IMPORT_WINDOW_MS,
                    windows.imports.len(),
                ),
                TrafficClass::OpeningAnalysis => (
                    PLAYER_OPENING_ANALYSIS_LIMIT,
                    PLAYER_OPENING_ANALYSIS_WINDOW_MS,
                    windows.opening_analyses.len(),
                ),
            };
            if occupancy >= limit {
                let oldest_ms = match class {
                    TrafficClass::Command => windows.commands.front().copied(),
                    TrafficClass::Import => windows.imports.front().copied(),
                    TrafficClass::OpeningAnalysis => windows.opening_analyses.front().copied(),
                }
                .unwrap_or(now_ms);
                Err((retry_after_seconds(oldest_ms, window_ms, now_ms), occupancy))
            } else {
                match class {
                    TrafficClass::Command => windows.commands.push_back(now_ms),
                    TrafficClass::Import => windows.imports.push_back(now_ms),
                    TrafficClass::OpeningAnalysis => windows.opening_analyses.push_back(now_ms),
                }
                if let Some(operation_id) = operation_id {
                    windows
                        .accepted_imports
                        .insert(operation_id.as_str().to_owned(), now_ms);
                }
                Ok(occupancy + 1)
            }
        };
        let bounded_occupancy = players.len();
        drop(players);
        match outcome {
            Err((retry_after_seconds, occupancy)) => {
                emit_traffic(
                    class,
                    TrafficDecision::Rejected,
                    Some(retry_after_seconds),
                    occupancy,
                    bounded_occupancy,
                );
                Err(retry_after_seconds)
            }
            Ok(occupancy) => {
                emit_traffic(
                    class,
                    TrafficDecision::Admitted,
                    None,
                    occupancy,
                    bounded_occupancy,
                );
                Ok(())
            }
        }
    }
}

fn expire_all(players: &mut HashMap<String, PlayerWindows>, now_ms: u64) {
    players.retain(|_, windows| {
        windows.expire(now_ms);
        !windows.is_idle()
    });
}

pub(super) fn expire_window(stamps: &mut VecDeque<u64>, now_ms: u64, window_ms: u64) {
    while stamps
        .front()
        .is_some_and(|stamp| now_ms.saturating_sub(*stamp) >= window_ms)
    {
        stamps.pop_front();
    }
}

pub(super) fn retry_after_seconds(oldest_ms: u64, window_ms: u64, now_ms: u64) -> u32 {
    let ready_at_ms = oldest_ms.saturating_add(window_ms);
    let remaining_ms = ready_at_ms.saturating_sub(now_ms);
    seconds_until(remaining_ms)
}

fn cardinality_retry_after(players: &HashMap<String, PlayerWindows>, now_ms: u64) -> u32 {
    let earliest = players
        .values()
        .flat_map(|windows| {
            windows
                .commands
                .iter()
                .chain(windows.imports.iter())
                .chain(windows.opening_analyses.iter())
        })
        .copied()
        .min()
        .unwrap_or(now_ms);
    let remaining_ms = earliest
        .saturating_add(PLAYER_COMMAND_WINDOW_MS.min(PLAYER_IMPORT_WINDOW_MS))
        .saturating_sub(now_ms);
    seconds_until(remaining_ms)
}

pub(super) fn seconds_until(remaining_ms: u64) -> u32 {
    let seconds = remaining_ms.div_ceil(1_000);
    u32::try_from(seconds).unwrap_or(u32::MAX).max(1)
}

fn emit_traffic(
    class: TrafficClass,
    decision: TrafficDecision,
    retry_after_seconds: Option<u32>,
    occupancy: usize,
    bounded_occupancy: usize,
) {
    tracing::info!(
        event = "coach_engine_player_traffic",
        policy_version = PLAYER_TRAFFIC_POLICY_VERSION,
        class = class.as_str(),
        decision = decision.as_str(),
        retry_after_seconds,
        occupancy,
        bounded_occupancy,
        "player traffic policy decision"
    );
}

#[cfg(test)]
pub fn traffic_telemetry_field_names() -> [&'static str; 7] {
    [
        "event",
        "policy_version",
        "class",
        "decision",
        "retry_after_seconds",
        "occupancy",
        "bounded_occupancy",
    ]
}

#[cfg(test)]
mod tests;
