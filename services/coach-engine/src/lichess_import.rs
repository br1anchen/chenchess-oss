use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tokio::sync::{watch, Mutex};

use crate::{
    game_eligibility::{completed_standard_outcome, GameEligibilityError},
    lichess::{
        LichessExportClient, LichessExportError, LichessExportResponse, LichessGameUrl,
        LICHESS_EXPORT_CONTRACT_VERSION, LICHESS_MAX_RESPONSE_BYTES, LICHESS_PGN_MEDIA_TYPE,
    },
    pgn::{parse_pgn_with_metadata, ParsedPgn},
    review_session_contract::{
        ArtifactDigest, CompletedGameOutcome, ImportProgressStage, ReviewSessionLimits,
    },
};

const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const CACHE_MAX_GAMES: usize = 512;
const CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;
const RETRY_MIN_DELAY: Duration = Duration::from_millis(250);
const RETRY_MAX_DELAY: Duration = Duration::from_millis(750);
const COOLDOWN_MIN_DELAY: Duration = Duration::from_secs(60);
const COOLDOWN_MAX_DELAY: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LichessImportError {
    #[error("Lichess Game was not found")]
    GameNotFound,
    #[error("Lichess Game is private")]
    PrivateGame,
    #[error("Lichess Game is ongoing")]
    OngoingGame,
    #[error("Lichess Game was aborted")]
    AbortedGame,
    #[error("Lichess Game uses an unsupported variant")]
    UnsupportedVariant,
    #[error("Lichess export contains invalid PGN")]
    InvalidPgn,
    #[error("Lichess returned a malformed export")]
    MalformedResponse,
    #[error("Lichess export exceeded an import size limit")]
    ResponseTooLarge,
    #[error("Lichess transport is unavailable")]
    Transport,
    #[error("Lichess export timed out")]
    Timeout,
    #[error("Lichess is rate limited until {retry_at}")]
    RateLimited {
        retry_after_seconds: u32,
        retry_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedLichessGame {
    pub(crate) parsed: ParsedPgn,
    pub(crate) pgn: Vec<u8>,
    pub(crate) outcome: CompletedGameOutcome,
    pub(crate) captured_at: DateTime<Utc>,
    pub(crate) response_digest: ArtifactDigest,
    pub(crate) pgn_digest: ArtifactDigest,
}

impl PreparedLichessGame {
    fn cache_weight(&self) -> usize {
        let game = &self.parsed.game;
        let metadata = &self.parsed.metadata;
        std::mem::size_of::<Self>()
            + self.pgn.capacity()
            + game.moves.capacity() * std::mem::size_of::<crate::types::ImportedMove>()
            + optional_string_capacity(&game.white)
            + optional_string_capacity(&game.black)
            + optional_string_capacity(&game.event)
            + optional_string_capacity(&game.site)
            + optional_string_capacity(&game.result)
            + game.final_position.capacity()
            + game
                .moves
                .iter()
                .map(|game_move| {
                    game_move.san.capacity()
                        + game_move.uci.capacity()
                        + game_move.position.capacity()
                })
                .sum::<usize>()
            + optional_string_capacity(&metadata.game_id)
            + optional_string_capacity(&metadata.variant)
            + optional_string_capacity(&metadata.white_elo)
            + optional_string_capacity(&metadata.black_elo)
            + optional_string_capacity(&metadata.eco)
            + optional_string_capacity(&metadata.opening)
            + optional_string_capacity(&metadata.termination)
            + self.response_digest.as_str().len()
            + self.pgn_digest.as_str().len()
    }
}

fn optional_string_capacity(value: &Option<String>) -> usize {
    value.as_ref().map_or(0, String::capacity)
}

pub(crate) struct LichessImportGateway<C> {
    inner: Arc<GatewayInner<C>>,
}

impl<C> LichessImportGateway<C> {
    pub(crate) fn new(client: C) -> Self {
        Self::with_policy(client, GatewayPolicy::production())
    }

    fn with_policy(client: C, policy: GatewayPolicy) -> Self {
        Self {
            inner: Arc::new(GatewayInner {
                client,
                outbound: Mutex::new(()),
                state: Mutex::new(GatewayState::new(&policy)),
                policy,
            }),
        }
    }
}

impl<C: LichessExportClient + 'static> LichessImportGateway<C> {
    pub(crate) async fn import<F>(
        &self,
        source: &LichessGameUrl,
        progress: &F,
    ) -> Result<Arc<PreparedLichessGame>, LichessImportError>
    where
        F: Fn(ImportProgressStage),
    {
        let key = CacheKey::new(source.canonical_game_id());
        let join = {
            let mut state = self.inner.state.lock().await;
            if let Some(game) = state.cache.get(&key, Instant::now()) {
                ImportJoin::Ready(game)
            } else if let Some(in_flight) = state.in_flight.get(&key) {
                ImportJoin::InFlight(in_flight.subscribe())
            } else {
                let (sender, receiver) = watch::channel(GatewayUpdate::waiting());
                state.in_flight.insert(key.clone(), sender.clone());
                ImportJoin::Leader { sender, receiver }
            }
        };

        let mut receiver = match join {
            ImportJoin::Ready(game) => return Ok(game),
            ImportJoin::InFlight(receiver) => receiver,
            ImportJoin::Leader { sender, receiver } => {
                let inner = Arc::clone(&self.inner);
                let source = source.clone();
                tokio::spawn(async move {
                    inner.run_leader(key, source, sender).await;
                });
                receiver
            }
        };

        let mut seen_stages = 0;
        loop {
            let update = receiver.borrow().clone();
            for &stage in &update.stages()[seen_stages..] {
                progress(stage);
            }
            seen_stages = update.stages().len();
            if let GatewayUpdate::Finished { result, .. } = update {
                return result;
            }
            receiver
                .changed()
                .await
                .map_err(|_| LichessImportError::Transport)?;
        }
    }
}

struct GatewayInner<C> {
    client: C,
    outbound: Mutex<()>,
    state: Mutex<GatewayState>,
    policy: GatewayPolicy,
}

impl<C: LichessExportClient> GatewayInner<C> {
    async fn run_leader(
        self: Arc<Self>,
        key: CacheKey,
        source: LichessGameUrl,
        sender: watch::Sender<GatewayUpdate>,
    ) {
        let result = self.fetch_and_validate(&source, &sender).await;
        if let Ok(game) = &result {
            self.state
                .lock()
                .await
                .cache
                .insert(key.clone(), Arc::clone(game), Instant::now());
        }
        sender.send_modify(|update| update.finish(result));
        self.state.lock().await.in_flight.remove(&key);
    }

    async fn fetch_and_validate(
        &self,
        source: &LichessGameUrl,
        sender: &watch::Sender<GatewayUpdate>,
    ) -> Result<Arc<PreparedLichessGame>, LichessImportError> {
        let _outbound = self.outbound.lock().await;
        if let Some(error) = self.active_cooldown().await {
            return Err(error);
        }

        sender.send_modify(|update| update.push(ImportProgressStage::FetchingGame));
        let request = crate::lichess::export_request(source);
        let mut response = self.client.export(&request).await;
        if response.as_ref().is_err_and(should_retry) {
            tokio::time::sleep(self.policy.retry_delay()).await;
            response = self.client.export(&request).await;
        }

        match response {
            Ok(response) => {
                sender.send_modify(|update| update.push(ImportProgressStage::ValidatingGame));
                prepare_lichess_game(source, response).map(Arc::new)
            }
            Err(LichessExportError::Status {
                code: 429,
                retry_after_seconds,
            }) => Err(self.open_cooldown(retry_after_seconds).await),
            Err(error) => Err(map_export_error(error)),
        }
    }

    async fn active_cooldown(&self) -> Option<LichessImportError> {
        let state = self.state.lock().await;
        let cooldown = state.cooldown.as_ref()?;
        (cooldown.until > Instant::now()).then(|| cooldown.error())
    }

    async fn open_cooldown(&self, retry_after_seconds: Option<u32>) -> LichessImportError {
        let requested = Duration::from_secs(u64::from(retry_after_seconds.unwrap_or_default()))
            .max(self.policy.cooldown_min);
        let mut state = self.state.lock().await;
        let local_backoff = state
            .cooldown
            .as_ref()
            .map(|cooldown| (cooldown.local_backoff * 2).min(self.policy.cooldown_max))
            .unwrap_or(self.policy.cooldown_min);
        let delay = requested.max(local_backoff);
        let retry_at = Utc::now()
            + chrono::Duration::from_std(delay)
                .expect("the bounded Lichess cooldown fits chrono::Duration");
        let cooldown = Cooldown {
            until: Instant::now() + delay,
            retry_at,
            #[cfg(test)]
            delay,
            local_backoff,
        };
        let error = cooldown.error();
        state.cooldown = Some(cooldown);
        error
    }
}

#[derive(Clone)]
enum GatewayUpdate {
    Running {
        stages: Vec<ImportProgressStage>,
    },
    Finished {
        stages: Vec<ImportProgressStage>,
        result: Result<Arc<PreparedLichessGame>, LichessImportError>,
    },
}

impl GatewayUpdate {
    fn waiting() -> Self {
        Self::Running {
            stages: vec![ImportProgressStage::WaitingForLichess],
        }
    }

    fn stages(&self) -> &[ImportProgressStage] {
        match self {
            Self::Running { stages } | Self::Finished { stages, .. } => stages,
        }
    }

    fn push(&mut self, stage: ImportProgressStage) {
        let Self::Running { stages } = self else {
            unreachable!("a finished Lichess import cannot report more progress")
        };
        stages.push(stage);
    }

    fn finish(&mut self, result: Result<Arc<PreparedLichessGame>, LichessImportError>) {
        let Self::Running { stages } = self else {
            unreachable!("a Lichess import leader finishes once")
        };
        *self = Self::Finished {
            stages: std::mem::take(stages),
            result,
        };
    }
}

enum ImportJoin {
    Ready(Arc<PreparedLichessGame>),
    InFlight(watch::Receiver<GatewayUpdate>),
    Leader {
        sender: watch::Sender<GatewayUpdate>,
        receiver: watch::Receiver<GatewayUpdate>,
    },
}

struct GatewayState {
    cache: GameCache,
    cooldown: Option<Cooldown>,
    in_flight: HashMap<CacheKey, watch::Sender<GatewayUpdate>>,
}

impl GatewayState {
    fn new(policy: &GatewayPolicy) -> Self {
        Self {
            cache: GameCache::new(
                policy.cache_ttl,
                policy.cache_max_games,
                policy.cache_max_bytes,
            ),
            cooldown: None,
            in_flight: HashMap::new(),
        }
    }
}

#[derive(Clone)]
struct Cooldown {
    until: Instant,
    retry_at: DateTime<Utc>,
    #[cfg(test)]
    delay: Duration,
    local_backoff: Duration,
}

impl Cooldown {
    fn error(&self) -> LichessImportError {
        let remaining = self.until.saturating_duration_since(Instant::now());
        LichessImportError::RateLimited {
            retry_after_seconds: duration_seconds_ceil(remaining),
            retry_at: self.retry_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    canonical_game_id: String,
    export_contract_version: &'static str,
}

impl CacheKey {
    fn new(canonical_game_id: &str) -> Self {
        Self {
            canonical_game_id: canonical_game_id.to_string(),
            export_contract_version: LICHESS_EXPORT_CONTRACT_VERSION,
        }
    }
}

struct GameCache {
    entries: HashMap<CacheKey, CacheEntry>,
    ttl: Duration,
    max_games: usize,
    max_bytes: usize,
    used_bytes: usize,
    access_clock: u64,
}

struct CacheEntry {
    game: Arc<PreparedLichessGame>,
    stored_at: Instant,
    last_access: u64,
    bytes: usize,
}

impl GameCache {
    fn new(ttl: Duration, max_games: usize, max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
            max_games,
            max_bytes,
            used_bytes: 0,
            access_clock: 0,
        }
    }

    fn get(&mut self, key: &CacheKey, now: Instant) -> Option<Arc<PreparedLichessGame>> {
        self.remove_expired(now);
        self.access_clock += 1;
        let entry = self.entries.get_mut(key)?;
        entry.last_access = self.access_clock;
        Some(Arc::clone(&entry.game))
    }

    fn insert(&mut self, key: CacheKey, game: Arc<PreparedLichessGame>, now: Instant) {
        self.remove_expired(now);
        let bytes = game.cache_weight();
        if bytes > self.max_bytes || self.max_games == 0 {
            return;
        }
        if let Some(replaced) = self.entries.remove(&key) {
            self.used_bytes -= replaced.bytes;
        }
        self.access_clock += 1;
        self.used_bytes += bytes;
        self.entries.insert(
            key,
            CacheEntry {
                game,
                stored_at: now,
                last_access: self.access_clock,
                bytes,
            },
        );
        while self.entries.len() > self.max_games || self.used_bytes > self.max_bytes {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|(key, _)| key.clone())
                .expect("an over-limit cache contains an entry");
            let removed = self
                .entries
                .remove(&oldest)
                .expect("the selected LRU entry remains in the cache");
            self.used_bytes -= removed.bytes;
        }
    }

    fn remove_expired(&mut self, now: Instant) {
        let expired = self
            .entries
            .iter()
            .filter(|(_, entry)| now.saturating_duration_since(entry.stored_at) >= self.ttl)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in expired {
            let removed = self
                .entries
                .remove(&key)
                .expect("an expired entry remains in the cache until removal");
            self.used_bytes -= removed.bytes;
        }
    }
}

#[derive(Clone)]
struct GatewayPolicy {
    retry_min: Duration,
    retry_max: Duration,
    cooldown_min: Duration,
    cooldown_max: Duration,
    cache_ttl: Duration,
    cache_max_games: usize,
    cache_max_bytes: usize,
}

impl GatewayPolicy {
    fn production() -> Self {
        Self {
            retry_min: RETRY_MIN_DELAY,
            retry_max: RETRY_MAX_DELAY,
            cooldown_min: COOLDOWN_MIN_DELAY,
            cooldown_max: COOLDOWN_MAX_DELAY,
            cache_ttl: CACHE_TTL,
            cache_max_games: CACHE_MAX_GAMES,
            cache_max_bytes: CACHE_MAX_BYTES,
        }
    }

    fn retry_delay(&self) -> Duration {
        if self.retry_min == self.retry_max {
            return self.retry_min;
        }
        let width = (self.retry_max - self.retry_min).as_millis() as u64;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;
        self.retry_min + Duration::from_millis(nanos % (width + 1))
    }
}

pub(crate) fn prepare_lichess_game(
    source: &LichessGameUrl,
    response: LichessExportResponse,
) -> Result<PreparedLichessGame, LichessImportError> {
    if response.body.len() > LICHESS_MAX_RESPONSE_BYTES {
        return Err(LichessImportError::ResponseTooLarge);
    }
    if response.content_type.split(';').next().map(str::trim) != Some(LICHESS_PGN_MEDIA_TYPE) {
        return Err(LichessImportError::MalformedResponse);
    }

    let response_digest = digest(&response.body);
    let normalized = normalize_lichess_export(&response.body);
    if normalized.len() > ReviewSessionLimits::V1.max_pgn_bytes as usize {
        return Err(LichessImportError::ResponseTooLarge);
    }
    let pgn = std::str::from_utf8(normalized).map_err(|_| LichessImportError::MalformedResponse)?;
    let parsed = parse_pgn_with_metadata(pgn).map_err(|_| LichessImportError::InvalidPgn)?;
    let canonical_url = source.canonical_url();
    if parsed.metadata.game_id.as_deref() != Some(source.canonical_game_id())
        || parsed.game.site.as_deref() != Some(canonical_url.as_str())
    {
        return Err(LichessImportError::MalformedResponse);
    }
    let outcome = completed_standard_outcome(&parsed).map_err(map_eligibility_error)?;
    let pgn = normalized.to_vec();
    let pgn_digest = digest(&pgn);

    Ok(PreparedLichessGame {
        parsed,
        pgn,
        outcome,
        captured_at: response.captured_at,
        response_digest,
        pgn_digest,
    })
}

fn normalize_lichess_export(bytes: &[u8]) -> &[u8] {
    bytes
        .strip_suffix(b"\n\n")
        .map_or(bytes, |without_blank_line| {
            &bytes[..without_blank_line.len() + 1]
        })
}

fn digest(bytes: &[u8]) -> ArtifactDigest {
    ArtifactDigest::try_from(format!("sha256:{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 digests satisfy the review-session contract")
}

fn should_retry(error: &LichessExportError) -> bool {
    matches!(
        error,
        LichessExportError::Connection
            | LichessExportError::Status {
                code: 502..=504,
                ..
            }
    )
}

fn map_export_error(error: LichessExportError) -> LichessImportError {
    match error {
        LichessExportError::Status { code: 404, .. } => LichessImportError::GameNotFound,
        LichessExportError::Status {
            code: 401 | 403, ..
        } => LichessImportError::PrivateGame,
        LichessExportError::Timeout => LichessImportError::Timeout,
        LichessExportError::ResponseTooLarge { .. } => LichessImportError::ResponseTooLarge,
        LichessExportError::Client(_)
        | LichessExportError::Connection
        | LichessExportError::Transport(_)
        | LichessExportError::Status { .. } => LichessImportError::Transport,
    }
}

fn map_eligibility_error(error: GameEligibilityError) -> LichessImportError {
    match error {
        GameEligibilityError::UnsupportedVariant => LichessImportError::UnsupportedVariant,
        GameEligibilityError::Ongoing => LichessImportError::OngoingGame,
        GameEligibilityError::Aborted => LichessImportError::AbortedGame,
        GameEligibilityError::Invalid(_) => LichessImportError::InvalidPgn,
    }
}

fn duration_seconds_ceil(duration: Duration) -> u32 {
    let seconds = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
    u32::try_from(seconds).expect("bounded Lichess cooldowns fit the wire retry-seconds type")
}

#[cfg(test)]
#[path = "lichess_import_tests.rs"]
mod tests;
