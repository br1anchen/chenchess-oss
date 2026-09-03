use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};
use tokio::{
    io::{AsyncWriteExt, BufReader, Lines},
    process::{ChildStdin, ChildStdout},
};

use crate::{
    operating_limits::PROVIDER_POSITION_TIMEOUT_SECONDS,
    provider_concurrency::{collect_ordered_provider_positions, IndexedProviderError},
};

mod cache;
mod multi_pv;
mod stockfish_session;
mod worker_limit;

pub use cache::ExactEngineCache;
pub use worker_limit::EngineWorkerLimit;

#[derive(Debug, Clone, Copy)]
pub struct EngineAnalysisInput<'a> {
    pub position: &'a str,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineAnalysis {
    pub best_move: String,
    pub evaluation: PositionEvaluation,
    pub principal_variation: Vec<String>,
    pub depth: u8,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum PositionEvaluation {
    Centipawns(i32),
    MateIn(i32),
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineProvenance {
    pub version: String,
    pub binary_sha256: String,
    pub depth: u8,
    pub threads: u8,
    pub hash_mib: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineAnalysisOutput {
    pub analysis: EngineAnalysis,
    pub provenance: Option<EngineProvenance>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankedEngineAnalysis {
    pub rank: u8,
    pub analysis: EngineAnalysis,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineMultiPvOutput {
    pub requested_variations: u8,
    pub variations: Vec<RankedEngineAnalysis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<EngineProvenance>,
}

#[derive(Debug, Clone)]
pub struct TimedEngineAnalysis {
    pub analysis: EngineAnalysis,
    pub provenance: Option<EngineProvenance>,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCacheIdentity {
    provider: String,
    binary_sha256: String,
    depth: u8,
    threads: u8,
    hash_mib: u16,
}

impl EngineCacheIdentity {
    /// The identity a provider with this provenance files its analyses under.
    pub fn from_provenance(provider: &str, provenance: &EngineProvenance) -> Self {
        Self {
            provider: provider.to_owned(),
            binary_sha256: provenance.binary_sha256.clone(),
            depth: provenance.depth,
            threads: provenance.threads,
            hash_mib: provenance.hash_mib,
        }
    }

    fn matches(&self, provenance: &EngineProvenance) -> bool {
        !provenance.version.is_empty()
            && self.binary_sha256 == provenance.binary_sha256
            && self.depth == provenance.depth
            && self.threads == provenance.threads
            && self.hash_mib == provenance.hash_mib
    }
}

#[derive(Debug)]
pub struct IndexedEngineAnalysisError {
    pub index: usize,
    pub error: EngineAnalysisError,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineAnalysisError {
    #[error("Stockfish process failed: {0}")]
    Process(#[from] std::io::Error),
    #[error("Stockfish did not respond within the analysis timeout")]
    Timeout,
    #[error("Stockfish UCI protocol error: {0}")]
    Protocol(String),
    #[error("Engine Analysis input is invalid: {0}")]
    InvalidInput(String),
}

pub trait EngineAnalyzer: Send + Sync + 'static {
    fn provider_name(&self) -> &'static str {
        "Engine Analysis adapter"
    }

    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>;

    fn analyze_with_provenance<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysisOutput, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            Ok(EngineAnalysisOutput {
                analysis: self.analyze(input).await?,
                provenance: self.provenance(),
            })
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        None
    }

    fn cache_identity(&self) -> Option<EngineCacheIdentity> {
        None
    }

    fn supports_multi_pv(&self) -> bool {
        false
    }

    fn analyze_multi_pv<'a>(
        &'a self,
        _input: EngineAnalysisInput<'a>,
        _variation_count: u8,
    ) -> Pin<Box<dyn Future<Output = Result<EngineMultiPvOutput, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async {
            Err(EngineAnalysisError::InvalidInput(
                "this Engine Analysis adapter does not support MultiPV".to_string(),
            ))
        })
    }

    fn analyze_positions(
        self: Arc<Self>,
        positions: Vec<String>,
        concurrency: usize,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<TimedEngineAnalysis>, IndexedEngineAnalysisError>>
                + Send,
        >,
    > {
        Box::pin(async move {
            let analyzer = self.clone();
            collect_ordered_provider_positions(positions, concurrency, move |position| {
                let analyzer = analyzer.clone();
                async move {
                    let started = Instant::now();
                    let output = analyzer
                        .analyze_with_provenance(EngineAnalysisInput {
                            position: &position,
                        })
                        .await?;
                    Ok(TimedEngineAnalysis {
                        analysis: output.analysis,
                        provenance: output.provenance,
                        duration: started.elapsed(),
                    })
                }
            })
            .await
            .map_err(
                |IndexedProviderError { index, error }| IndexedEngineAnalysisError { index, error },
            )
        })
    }
}

#[derive(Clone)]
pub struct StockfishAdapter {
    program: PathBuf,
    binary_sha256: Option<String>,
    #[cfg(test)]
    args: Vec<String>,
    depth: u8,
    threads: u8,
    hash_mib: u16,
    timeout: Duration,
    #[cfg(test)]
    child_environment: Vec<(String, String)>,
    /// Shared across clones: one retained engine process per adapter, not per
    /// caller. Cloning an adapter must not multiply resident engines.
    sessions: stockfish_session::SessionCache,
}

impl StockfishAdapter {
    pub fn new(program: PathBuf, depth: u8) -> Self {
        let binary_sha256 = stockfish_binary_sha256(&program);
        Self {
            program,
            binary_sha256,
            #[cfg(test)]
            args: Vec::new(),
            depth,
            threads: 1,
            hash_mib: 16,
            timeout: Duration::from_secs(PROVIDER_POSITION_TIMEOUT_SECONDS),
            #[cfg(test)]
            child_environment: Vec::new(),
            sessions: stockfish_session::SessionCache::default(),
        }
    }

    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let Some(program) = std::env::var_os("STOCKFISH_PATH").filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        let depth = std::env::var("STOCKFISH_DEPTH")
            .unwrap_or_else(|_| "16".to_string())
            .parse::<u8>()
            .context("STOCKFISH_DEPTH must be a whole number between 1 and 255")?;
        anyhow::ensure!(depth > 0, "STOCKFISH_DEPTH must be between 1 and 255");
        Ok(Some(Self::new(PathBuf::from(program), depth)))
    }

    #[cfg(test)]
    pub fn with_command(program: PathBuf, args: Vec<String>, depth: u8, timeout: Duration) -> Self {
        let binary_sha256 = stockfish_binary_sha256(&program);
        Self {
            program,
            binary_sha256,
            args,
            depth,
            threads: 1,
            hash_mib: 16,
            timeout,
            child_environment: vec![("FAKE_STOCKFISH_PROCESS".to_string(), "1".to_string())],
            sessions: stockfish_session::SessionCache::default(),
        }
    }

    #[cfg(test)]
    fn with_hanging_command(
        program: PathBuf,
        args: Vec<String>,
        depth: u8,
        timeout: Duration,
    ) -> Self {
        let mut adapter = Self::with_command(program, args, depth, timeout);
        adapter
            .child_environment
            .push(("FAKE_STOCKFISH_HANG".to_string(), "1".to_string()));
        adapter
    }

    async fn run(&self, position: &str) -> Result<EngineRunOutput, EngineAnalysisError> {
        stockfish_session::run(self, position).await
    }

    async fn run_multi_pv(
        &self,
        position: &str,
        variation_count: u8,
    ) -> Result<EngineMultiPvRunOutput, EngineAnalysisError> {
        stockfish_session::run_multi_pv(self, position, variation_count).await
    }

    fn measured_provenance(&self, version: Option<String>) -> Option<EngineProvenance> {
        Some(EngineProvenance {
            version: version?,
            binary_sha256: self.binary_sha256.clone()?,
            depth: self.depth,
            threads: self.threads,
            hash_mib: self.hash_mib,
        })
    }
}

fn stockfish_binary_sha256(program: &Path) -> Option<String> {
    std::fs::read(program)
        .ok()
        .map(|binary| format!("{:x}", Sha256::digest(binary)))
}

struct EngineRunOutput {
    analysis: EngineAnalysis,
    stockfish_version: Option<String>,
}

struct EngineMultiPvRunOutput {
    variations: Vec<RankedEngineAnalysis>,
    stockfish_version: Option<String>,
}

async fn run_position_protocol(
    stdin: &mut ChildStdin,
    lines: &mut Lines<BufReader<ChildStdout>>,
    position: &str,
    depth: u8,
) -> Result<EngineAnalysis, EngineAnalysisError> {
    send_command(stdin, &format!("position fen {position}")).await?;
    send_command(stdin, &format!("go depth {depth}")).await?;

    let mut latest = None;
    let best_move = loop {
        let line = next_line(lines).await?;
        if let Some(info) = parse_info(&line)?.filter(|info| info.rank == 1) {
            latest = Some(info);
        }
        if let Some(best_move) = line.strip_prefix("bestmove ") {
            let best_move = best_move
                .split_whitespace()
                .next()
                .ok_or_else(|| EngineAnalysisError::Protocol("missing best move".to_string()))?;
            break normalize_best_move(best_move)?;
        }
    };
    let info = if best_move == "0000" {
        terminal_info(position, depth)?
    } else {
        latest.ok_or_else(|| {
            EngineAnalysisError::Protocol(
                "bestmove arrived without evaluation and principal variation".to_string(),
            )
        })?
    };

    Ok(EngineAnalysis {
        best_move,
        evaluation: info.evaluation,
        principal_variation: info.principal_variation,
        depth: info.depth,
    })
}

fn normalize_best_move(candidate: &str) -> Result<String, EngineAnalysisError> {
    if matches!(candidate, "(none)" | "0000") {
        Ok("0000".to_string())
    } else {
        require_normal_uci(candidate)?;
        Ok(candidate.to_string())
    }
}

fn terminal_info(position: &str, depth: u8) -> Result<AnalysisInfo, EngineAnalysisError> {
    let position: Chess = Fen::from_ascii(position.as_bytes())
        .map_err(|_| EngineAnalysisError::Protocol("terminal FEN is malformed".to_string()))?
        .into_position(CastlingMode::Standard)
        .map_err(|_| EngineAnalysisError::Protocol("terminal FEN is illegal".to_string()))?;
    let evaluation = if position.is_checkmate() {
        PositionEvaluation::MateIn(0)
    } else if position.outcome().is_some() {
        PositionEvaluation::Centipawns(0)
    } else {
        return Err(EngineAnalysisError::Protocol(
            "Stockfish returned no move for a nonterminal Position".to_string(),
        ));
    };
    Ok(AnalysisInfo {
        rank: 1,
        depth,
        evaluation,
        principal_variation: Vec::new(),
    })
}

impl EngineAnalyzer for StockfishAdapter {
    fn provider_name(&self) -> &'static str {
        "Stockfish"
    }

    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            validate_input(input.position, self.depth)?;
            Ok(self.run(input.position).await?.analysis)
        })
    }

    fn analyze_with_provenance<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysisOutput, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            validate_input(input.position, self.depth)?;
            let output = self.run(input.position).await?;
            Ok(EngineAnalysisOutput {
                provenance: self.measured_provenance(output.stockfish_version),
                analysis: output.analysis,
            })
        })
    }

    fn cache_identity(&self) -> Option<EngineCacheIdentity> {
        Some(EngineCacheIdentity {
            provider: self.provider_name().to_owned(),
            binary_sha256: self.binary_sha256.clone()?,
            depth: self.depth,
            threads: self.threads,
            hash_mib: self.hash_mib,
        })
    }

    fn supports_multi_pv(&self) -> bool {
        true
    }

    fn analyze_multi_pv<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
        variation_count: u8,
    ) -> Pin<Box<dyn Future<Output = Result<EngineMultiPvOutput, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            validate_input(input.position, self.depth)?;
            multi_pv::validate_count(variation_count)?;
            let output = self.run_multi_pv(input.position, variation_count).await?;
            Ok(EngineMultiPvOutput {
                provenance: self.measured_provenance(output.stockfish_version),
                requested_variations: variation_count,
                variations: output.variations,
            })
        })
    }

    fn analyze_positions(
        self: Arc<Self>,
        positions: Vec<String>,
        concurrency: usize,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<TimedEngineAnalysis>, IndexedEngineAnalysisError>>
                + Send,
        >,
    > {
        Box::pin(stockfish_session::run_positions(
            self,
            positions,
            concurrency,
        ))
    }
}

fn validate_input(position: &str, depth: u8) -> Result<(), EngineAnalysisError> {
    if position.trim().is_empty() || position.contains(['\r', '\n']) {
        return Err(EngineAnalysisError::InvalidInput(
            "position must be a single non-empty FEN line".to_string(),
        ));
    }
    if depth == 0 {
        return Err(EngineAnalysisError::InvalidInput(
            "analysis depth must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

struct AnalysisInfo {
    rank: u8,
    depth: u8,
    evaluation: PositionEvaluation,
    principal_variation: Vec<String>,
}

fn parse_info(line: &str) -> Result<Option<AnalysisInfo>, EngineAnalysisError> {
    if !line.starts_with("info ") {
        return Ok(None);
    }
    let fields = line.split_whitespace().collect::<Vec<_>>();
    let Some(depth_index) = fields.iter().position(|field| *field == "depth") else {
        return Ok(None);
    };
    let Some(score_index) = fields.iter().position(|field| *field == "score") else {
        return Ok(None);
    };
    let Some(pv_index) = fields.iter().position(|field| *field == "pv") else {
        return Ok(None);
    };

    let depth = parse_field::<u8>(&fields, depth_index + 1, "depth")?;
    let rank = fields
        .iter()
        .position(|field| *field == "multipv")
        .map(|index| parse_field::<u8>(&fields, index + 1, "multipv rank"))
        .transpose()?
        .unwrap_or(1);
    if rank == 0 {
        return Err(EngineAnalysisError::Protocol(
            "MultiPV rank must be greater than zero".to_string(),
        ));
    }
    let score_kind = field(&fields, score_index + 1, "score kind")?;
    let score = parse_field::<i32>(&fields, score_index + 2, "score")?;
    let evaluation = match score_kind {
        "cp" => PositionEvaluation::Centipawns(score),
        "mate" => PositionEvaluation::MateIn(score),
        other => {
            return Err(EngineAnalysisError::Protocol(format!(
                "unsupported score kind {other}"
            )))
        }
    };
    let principal_variation = fields[pv_index + 1..]
        .iter()
        .map(|candidate| {
            require_normal_uci(candidate)?;
            Ok((*candidate).to_string())
        })
        .collect::<Result<Vec<_>, EngineAnalysisError>>()?;
    if principal_variation.is_empty() {
        return Err(EngineAnalysisError::Protocol(
            "principal variation is empty".to_string(),
        ));
    }

    Ok(Some(AnalysisInfo {
        rank,
        depth,
        evaluation,
        principal_variation,
    }))
}

fn field<'a>(fields: &'a [&str], index: usize, name: &str) -> Result<&'a str, EngineAnalysisError> {
    fields
        .get(index)
        .copied()
        .ok_or_else(|| EngineAnalysisError::Protocol(format!("info line is missing {name}")))
}

fn parse_field<T: std::str::FromStr>(
    fields: &[&str],
    index: usize,
    name: &str,
) -> Result<T, EngineAnalysisError> {
    field(fields, index, name)?
        .parse()
        .map_err(|_| EngineAnalysisError::Protocol(format!("info line has invalid {name}")))
}

fn require_normal_uci(candidate: &str) -> Result<(), EngineAnalysisError> {
    if matches!(candidate.parse(), Ok(UciMove::Normal { .. })) {
        Ok(())
    } else {
        Err(EngineAnalysisError::Protocol(format!(
            "invalid UCI move {candidate}"
        )))
    }
}

async fn send_command(
    stdin: &mut tokio::process::ChildStdin,
    command: &str,
) -> Result<(), EngineAnalysisError> {
    stdin.write_all(command.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

async fn read_until(
    lines: &mut Lines<BufReader<ChildStdout>>,
    expected: &str,
) -> Result<(), EngineAnalysisError> {
    loop {
        if next_line(lines).await? == expected {
            return Ok(());
        }
    }
}

async fn read_stockfish_version(
    lines: &mut Lines<BufReader<ChildStdout>>,
) -> Result<Option<String>, EngineAnalysisError> {
    let mut version = None;
    loop {
        let line = next_line(lines).await?;
        if line == "uciok" {
            return Ok(version);
        }
        if let Some(name) = line.strip_prefix("id name Stockfish ") {
            version = name.split_whitespace().next().map(str::to_string);
        }
    }
}

async fn next_line(
    lines: &mut Lines<BufReader<ChildStdout>>,
) -> Result<String, EngineAnalysisError> {
    lines.next_line().await?.ok_or_else(|| {
        EngineAnalysisError::Protocol("Stockfish exited before completing UCI output".to_string())
    })
}

#[cfg(test)]
mod tests;
