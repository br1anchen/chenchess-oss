use std::collections::{BTreeMap, BTreeSet};

use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};
use tokio::io::{BufReader, Lines};
use tokio::process::{ChildStdin, ChildStdout};

use super::{
    next_line, normalize_best_move, parse_info, read_until, send_command, AnalysisInfo,
    EngineAnalysis, EngineAnalysisError, RankedEngineAnalysis,
};

pub(super) async fn run_protocol(
    stdin: &mut ChildStdin,
    lines: &mut Lines<BufReader<ChildStdout>>,
    position: &str,
    depth: u8,
    variation_count: u8,
) -> Result<Vec<RankedEngineAnalysis>, EngineAnalysisError> {
    validate_count(variation_count)?;
    send_command(
        stdin,
        &format!("setoption name MultiPV value {variation_count}"),
    )
    .await?;
    send_command(stdin, "isready").await?;
    read_until(lines, "readyok").await?;
    send_command(stdin, &format!("position fen {position}")).await?;
    send_command(stdin, &format!("go depth {depth}")).await?;

    let mut latest = BTreeMap::<u8, AnalysisInfo>::new();
    let best_move = loop {
        let line = next_line(lines).await?;
        if let Some(info) = parse_info(&line)? {
            let replace = latest
                .get(&info.rank)
                .is_none_or(|current| current.depth <= info.depth);
            if replace {
                latest.insert(info.rank, info);
            }
        }
        if let Some(best_move) = line.strip_prefix("bestmove ") {
            let best_move = best_move
                .split_whitespace()
                .next()
                .ok_or_else(|| EngineAnalysisError::Protocol("missing best move".to_string()))?;
            break normalize_best_move(best_move)?;
        }
    };
    if best_move == "0000" {
        return Err(EngineAnalysisError::Protocol(
            "MultiPV was requested for a terminal Position".to_string(),
        ));
    }
    validate_lines(position, variation_count, depth, &best_move, &latest)?;
    Ok(latest
        .into_iter()
        .map(|(rank, info)| RankedEngineAnalysis {
            rank,
            analysis: EngineAnalysis {
                best_move: info.principal_variation[0].clone(),
                evaluation: info.evaluation,
                principal_variation: info.principal_variation,
                depth: info.depth,
            },
        })
        .collect())
}

pub(super) fn validate_count(variation_count: u8) -> Result<(), EngineAnalysisError> {
    if variation_count < 2 {
        return Err(EngineAnalysisError::InvalidInput(
            "MultiPV variation count must be at least two".to_string(),
        ));
    }
    Ok(())
}

fn validate_lines(
    position: &str,
    variation_count: u8,
    expected_depth: u8,
    best_move: &str,
    lines: &BTreeMap<u8, AnalysisInfo>,
) -> Result<(), EngineAnalysisError> {
    let (position, expected) = position_and_expected_variation_count(position, variation_count)?;
    if lines.len() != expected {
        return Err(EngineAnalysisError::Protocol(format!(
            "MultiPV returned {} ranked lines; expected {expected}",
            lines.len()
        )));
    }
    let maximum_rank = u8::try_from(expected).map_err(|_| {
        EngineAnalysisError::Protocol("MultiPV rank exceeds the supported range".to_string())
    })?;
    let expected_ranks = (1..=maximum_rank).collect::<Vec<_>>();
    if lines.keys().copied().collect::<Vec<_>>() != expected_ranks {
        return Err(EngineAnalysisError::Protocol(
            "MultiPV ranks are not contiguous from one".to_string(),
        ));
    }
    let mut roots = BTreeSet::new();
    for info in lines.values() {
        if info.depth != expected_depth {
            return Err(EngineAnalysisError::Protocol(
                "MultiPV line did not reach the requested depth".to_string(),
            ));
        }
        let root = info
            .principal_variation
            .first()
            .ok_or_else(|| EngineAnalysisError::Protocol("MultiPV line is empty".to_string()))?;
        if !roots.insert(root) {
            return Err(EngineAnalysisError::Protocol(
                "MultiPV returned duplicate root moves".to_string(),
            ));
        }
        validate_legal_line(position.clone(), &info.principal_variation)?;
    }
    if lines
        .get(&1)
        .and_then(|info| info.principal_variation.first())
        .is_none_or(|root| root != best_move)
    {
        return Err(EngineAnalysisError::Protocol(
            "MultiPV rank one disagrees with bestmove".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn position_and_expected_variation_count(
    position: &str,
    variation_count: u8,
) -> Result<(Chess, usize), EngineAnalysisError> {
    validate_count(variation_count)?;
    let position: Chess = Fen::from_ascii(position.as_bytes())
        .map_err(|_| EngineAnalysisError::Protocol("MultiPV FEN is malformed".to_string()))?
        .into_position(CastlingMode::Standard)
        .map_err(|_| EngineAnalysisError::Protocol("MultiPV FEN is illegal".to_string()))?;
    let expected = usize::from(variation_count).min(position.legal_moves().len());
    Ok((position, expected))
}

fn validate_legal_line(mut position: Chess, line: &[String]) -> Result<(), EngineAnalysisError> {
    for candidate in line {
        let uci = candidate.parse::<UciMove>().map_err(|_| {
            EngineAnalysisError::Protocol(format!("invalid MultiPV UCI move {candidate}"))
        })?;
        let chess_move = uci.to_move(&position).map_err(|_| {
            EngineAnalysisError::Protocol(format!("illegal MultiPV UCI move {candidate}"))
        })?;
        position.play_unchecked(&chess_move);
    }
    Ok(())
}
