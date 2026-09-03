use std::{collections::BTreeMap, fs, path::Path, process::Command};

use chen_chess_coach_engine::{
    engine_analysis::{
        EngineAnalysis, EngineAnalysisInput, EngineAnalyzer, PositionEvaluation, StockfishAdapter,
    },
    evaluation_recording::*,
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMovePrediction, MaiaHttpAdapter},
    local_runtime::{
        Platform, ReviewRuntimeConfig, RuntimeManager, RuntimePaths, SystemProcessRunner,
    },
    operating_limits::{PROJECTED_PLAN_BEAM_WIDTH, PROJECTED_PLAN_REQUIRED_HALF_MOVES},
    pgn::parse_pgn,
    review_session_contract::*,
    types::EloProfile,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position};

const RECORDING_RELATIVE_PATH: &str =
    "packages/shared-assets/fixtures/Synthet1/review-session-provider-recording.json";
const ROOT_PLY: usize = 24;
const ROOT_MOVES: [&str; 2] = ["d7b6", "e7e6"];
const REVIEWED_MOVE: &str = "a8a7";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("API should have a repository parent");
    let recording_path = root.join(RECORDING_RELATIVE_PATH);
    match arguments.as_slice() {
        [] => verify_existing(&recording_path),
        [flag] if flag == "--check" => verify_existing(&recording_path),
        [flag] if flag == "--capture" => capture(root, &recording_path, false).await,
        [capture_flag, accept_flag] if capture_flag == "--capture" && accept_flag == "--accept" => {
            capture(root, &recording_path, true).await
        }
        _ => anyhow::bail!(
            "usage: capture_review_session_recording [--check | --capture [--accept]]"
        ),
    }
}

fn verify_existing(path: &Path) -> anyhow::Result<()> {
    let recording: ReviewSessionProviderRecording = serde_json::from_slice(&fs::read(path)?)?;
    recording.verify()?;
    println!(
        "verified {} ({})",
        path.display(),
        recording.content_digest.as_str()
    );
    Ok(())
}

async fn capture(root: &Path, output: &Path, accept: bool) -> anyhow::Result<()> {
    let runtime = RuntimeManager::new(SystemProcessRunner::default())
        .certification_config(&RuntimePaths::from_environment()?, Platform::current())?;
    verify_capture_runtime(&runtime.config)?;
    let actual_stockfish_digest = sha256(&fs::read(&runtime.config.stockfish_path)?);
    anyhow::ensure!(
        actual_stockfish_digest == PINNED_STOCKFISH_BINARY_DIGEST,
        "Stockfish binary digest does not match the pinned capture"
    );

    let pgn_bytes =
        fs::read(root.join("packages/shared-assets/fixtures/Synthet1/lichess-export.pgn"))?;
    let pgn = std::str::from_utf8(&pgn_bytes)?;
    anyhow::ensure!(
        sha256(&pgn_bytes) == CANONICAL_GAME_PGN_DIGEST,
        "canonical PGN digest drifted"
    );
    let game = parse_pgn(pgn)?;
    let imported_game: ImportedGame = serde_json::from_slice(&fs::read(
        root.join("packages/coach-engine-sdk/fixtures/imported-game.json"),
    )?)?;

    let stockfish = StockfishAdapter::new(
        runtime.config.stockfish_path.clone(),
        PINNED_STOCKFISH_DEPTH,
    );
    let maia = MaiaHttpAdapter::new(runtime.config.maia_base_url.clone());
    let captured_positions = capture_positions(&game, &stockfish, &maia).await?;
    let entries = capture_entries(&captured_positions, &stockfish, &maia).await?;
    let cargo_lock_digest = ArtifactDigest::try_from(sha256(&fs::read(root.join("Cargo.lock"))?))?;
    let content = ProviderRecordingContent {
        captured_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        canonical_game_id: CanonicalGameId::try_from("Synthet1".to_string())?,
        canonical_source_url: "https://lichess.org/Synthet1".to_string(),
        game_ref: GameRef::try_from(CANONICAL_GAME_PGN_DIGEST.to_string())?,
        pgn_digest: ArtifactDigest::try_from(CANONICAL_GAME_PGN_DIGEST.to_string())?,
        imported_game_digest: imported_game.digest(),
        runtime: ProviderRecordingRuntime {
            stockfish: stockfish_runtime()?,
            maia: maia_runtime()?,
            dependencies: vec![
                RecordingDependency {
                    name: "serde_json_canonicalizer".to_string(),
                    version: "0.3.2".to_string(),
                    digest: DependencyDigest::Lockfile {
                        digest: cargo_lock_digest.clone(),
                    },
                },
                RecordingDependency {
                    name: "shakmaty".to_string(),
                    version: "0.27".to_string(),
                    digest: DependencyDigest::Lockfile {
                        digest: cargo_lock_digest,
                    },
                },
            ],
        },
        entries,
    };
    let recording = ReviewSessionProviderRecording::from_content(content)?;
    let mut bytes = serde_json::to_vec_pretty(&recording)?;
    bytes.push(b'\n');
    let staged = std::env::temp_dir().join(format!(
        "chenchess-review-session-provider-recording-{}.json",
        std::process::id()
    ));
    fs::write(&staged, &bytes)?;
    show_diff(output, &staged)?;
    if accept {
        atomic_write(output, &bytes)?;
        println!("accepted {}", output.display());
    } else {
        println!(
            "capture staged at {}; rerun with --accept",
            staged.display()
        );
    }
    Ok(())
}

fn verify_capture_runtime(runtime: &ReviewRuntimeConfig) -> anyhow::Result<()> {
    anyhow::ensure!(
        runtime.stockfish_version == PINNED_STOCKFISH_VERSION,
        "installed Stockfish version does not match the pinned capture"
    );
    anyhow::ensure!(
        runtime.maia_image == PINNED_MAIA_IMAGE
            && runtime.maia_package == PINNED_MAIA_PACKAGE
            && runtime.maia_model == PINNED_MAIA_MODEL
            && format!("sha256:{}", runtime.maia_model_sha256) == PINNED_MAIA_MODEL_DIGEST
            && format!("sha256:{}", runtime.maia_config_sha256) == PINNED_MAIA_CONFIG_DIGEST,
        "installed Maia runtime does not match the pinned capture"
    );
    Ok(())
}

#[derive(Clone)]
struct CapturedPosition {
    snapshot: PositionSnapshot,
    branch: Option<CapturedBranch>,
    engine: Option<EngineAnalysis>,
}

#[derive(Clone)]
struct CapturedBranch {
    branch_ref: BranchRef,
    parent: BranchParent,
    source_position_ref: PositionRef,
    move_uci: String,
}

async fn capture_positions(
    game: &chen_chess_coach_engine::types::Game,
    stockfish: &StockfishAdapter,
    maia: &MaiaHttpAdapter,
) -> anyhow::Result<Vec<CapturedPosition>> {
    let root_index = ROOT_PLY - 1;
    let root_fen = &game
        .moves
        .get(root_index)
        .ok_or_else(|| anyhow::anyhow!("canonical Game has no ply {ROOT_PLY}"))?
        .position;
    let root_history = game.moves[..root_index]
        .iter()
        .map(|game_move| game_move.position.as_str())
        .collect::<Vec<_>>();
    let root = build_position_snapshot(root_fen, &root_history)?;
    let mut positions = vec![CapturedPosition {
        snapshot: root.clone(),
        branch: None,
        engine: None,
    }];

    for root_move in ROOT_MOVES {
        let branch_ref = BranchRef::try_from(format!("branch:capture:{root_move}"))?;
        let child_fen = play(root_fen, root_move)?;
        let mut child_history = root_history.clone();
        child_history.push(root_fen);
        let child = build_position_snapshot(&child_fen, &child_history)?;
        let analysis = stockfish
            .analyze(EngineAnalysisInput {
                position: &child.fen,
            })
            .await?;
        positions.push(CapturedPosition {
            snapshot: child.clone(),
            branch: Some(CapturedBranch {
                branch_ref: branch_ref.clone(),
                parent: BranchParent::Root {
                    position_ref: root.position_ref.clone(),
                },
                source_position_ref: root.position_ref.clone(),
                move_uci: root_move.to_string(),
            }),
            engine: Some(analysis.clone()),
        });
        let reply = analysis.best_move.clone();
        let reply_ref = BranchRef::try_from(format!("branch:capture:{root_move}:{reply}"))?;
        let leaf_fen = play(&child.fen, &reply)?;
        child_history.push(&child.fen);
        let leaf = build_position_snapshot(&leaf_fen, &child_history)?;
        positions.push(CapturedPosition {
            snapshot: leaf,
            branch: Some(CapturedBranch {
                branch_ref: reply_ref,
                parent: BranchParent::Move { branch_ref },
                source_position_ref: child.position_ref,
                move_uci: reply,
            }),
            engine: None,
        });
    }

    let reviewed_move_ref = BranchRef::try_from(format!("branch:projected-plan:{REVIEWED_MOVE}"))?;
    let played_fen = play(root_fen, REVIEWED_MOVE)?;
    let mut played_history = root_history
        .iter()
        .map(|fen| (*fen).to_string())
        .collect::<Vec<_>>();
    played_history.push(root_fen.to_string());
    let played_position = build_owned_position_snapshot(&played_fen, &played_history)?;
    positions.push(CapturedPosition {
        snapshot: played_position.clone(),
        branch: Some(CapturedBranch {
            branch_ref: reviewed_move_ref.clone(),
            parent: BranchParent::Root {
                position_ref: root.position_ref.clone(),
            },
            source_position_ref: root.position_ref,
            move_uci: REVIEWED_MOVE.to_string(),
        }),
        engine: None,
    });

    let mut beam = vec![ProjectedPlanPath {
        position: played_position,
        history: played_history,
        branch_ref: reviewed_move_ref.clone(),
        moves: Vec::new(),
        joint_probability: 1.0,
    }];
    for _ in 0..PROJECTED_PLAN_REQUIRED_HALF_MOVES {
        let mut expanded = Vec::new();
        for path in beam {
            let prediction = maia
                .predict(HumanMoveInput {
                    position: &path.position.fen,
                    elo: EloProfile::try_from(PINNED_MAIA_ELO).map_err(anyhow::Error::msg)?,
                    limit: PINNED_MAIA_CANDIDATE_LIMIT,
                })
                .await?;
            for candidate in prediction.candidates {
                if candidate.probability == 0.0 {
                    continue;
                }
                let child_fen = play(&path.position.fen, &candidate.uci).map_err(|error| {
                    anyhow::anyhow!(
                        "Maia candidate {} is invalid for {}: {error}",
                        candidate.uci,
                        path.position.position_ref.as_str()
                    )
                })?;
                let mut child_history = path.history.clone();
                child_history.push(path.position.fen.clone());
                let child = build_owned_position_snapshot(&child_fen, &child_history)?;
                let mut moves = path.moves.clone();
                moves.push(candidate.uci.clone());
                let branch_ref = BranchRef::try_from(format!(
                    "branch:projected-plan:{REVIEWED_MOVE}:{}",
                    moves.join(":")
                ))?;
                expanded.push(ProjectedPlanPath {
                    position: child,
                    history: child_history,
                    branch_ref,
                    moves,
                    joint_probability: path.joint_probability * candidate.probability,
                });
            }
        }
        expanded.sort_by(|left, right| {
            right
                .joint_probability
                .total_cmp(&left.joint_probability)
                .then_with(|| left.moves.cmp(&right.moves))
        });
        expanded.truncate(PROJECTED_PLAN_BEAM_WIDTH);
        for path in &expanded {
            let source = path
                .history
                .last()
                .ok_or_else(|| anyhow::anyhow!("Projected Plan path has no source Position"))?;
            let source_position = build_owned_position_snapshot(
                source,
                &path.history[..path.history.len().saturating_sub(1)],
            )?;
            positions.push(CapturedPosition {
                snapshot: path.position.clone(),
                branch: Some(CapturedBranch {
                    branch_ref: path.branch_ref.clone(),
                    parent: if path.moves.len() == 1 {
                        BranchParent::Move {
                            branch_ref: reviewed_move_ref.clone(),
                        }
                    } else {
                        BranchParent::Move {
                            branch_ref: BranchRef::try_from(format!(
                                "branch:projected-plan:{REVIEWED_MOVE}:{}",
                                path.moves[..path.moves.len() - 1].join(":")
                            ))?,
                        }
                    },
                    source_position_ref: source_position.position_ref,
                    move_uci: path
                        .moves
                        .last()
                        .expect("Projected Plan path has a move")
                        .clone(),
                }),
                engine: None,
            });
        }
        beam = expanded;
    }
    Ok(positions)
}

#[derive(Clone)]
struct ProjectedPlanPath {
    position: PositionSnapshot,
    history: Vec<String>,
    branch_ref: BranchRef,
    moves: Vec<String>,
    joint_probability: f64,
}

fn build_owned_position_snapshot(
    fen: &str,
    history: &[String],
) -> anyhow::Result<PositionSnapshot> {
    let history = history.iter().map(String::as_str).collect::<Vec<_>>();
    build_position_snapshot(fen, &history)
}

async fn capture_entries(
    positions: &[CapturedPosition],
    stockfish: &StockfishAdapter,
    maia: &MaiaHttpAdapter,
) -> anyhow::Result<Vec<EvidenceEntry>> {
    let mut entries = Vec::new();
    let mut position_evidence = BTreeMap::new();
    for captured in positions {
        let entry = EvidenceEntry::position(
            EvidenceMetadata::derived(REVIEW_SESSION_CAPTURE_VERSION, Vec::new()),
            captured.snapshot.clone(),
        );
        position_evidence.insert(
            captured.snapshot.position_ref.clone(),
            entry.metadata().evidence_id.clone(),
        );
        entries.push(entry);
    }

    for captured in positions {
        let position_ref = &captured.snapshot.position_ref;
        let dependency = position_evidence
            .get(position_ref)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("captured Position evidence is missing"))?;
        let engine_capture = async {
            match &captured.engine {
                Some(engine) => Ok(engine.clone()),
                None => {
                    stockfish
                        .analyze(EngineAnalysisInput {
                            position: &captured.snapshot.fen,
                        })
                        .await
                }
            }
        };
        let maia_capture = maia.predict(HumanMoveInput {
            position: &captured.snapshot.fen,
            elo: EloProfile::try_from(PINNED_MAIA_ELO).map_err(anyhow::Error::msg)?,
            limit: PINNED_MAIA_CANDIDATE_LIMIT,
        });
        let (engine, prediction) = tokio::join!(engine_capture, maia_capture);
        entries.push(engine_entry(
            &captured.snapshot,
            dependency.clone(),
            engine?,
        )?);
        entries.push(maia_entry(&captured.snapshot, dependency, prediction?)?);
    }

    for captured in positions
        .iter()
        .filter(|position| position.branch.is_some())
    {
        let branch = captured.branch.as_ref().expect("filtered branch");
        let source_evidence = position_evidence
            .get(&branch.source_position_ref)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("branch source evidence is missing"))?;
        let result_evidence = position_evidence
            .get(&captured.snapshot.position_ref)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("branch result evidence is missing"))?;
        entries.push(EvidenceEntry::branch(
            EvidenceMetadata::derived(
                REVIEW_SESSION_CAPTURE_VERSION,
                vec![source_evidence, result_evidence],
            ),
            BranchEvidence {
                branch_ref: branch.branch_ref.clone(),
                parent: branch.parent.clone(),
                source_position_ref: branch.source_position_ref.clone(),
                move_uci: branch.move_uci.clone(),
                resulting_position_ref: captured.snapshot.position_ref.clone(),
            },
        ));
    }
    Ok(entries)
}

fn engine_entry(
    position: &PositionSnapshot,
    dependency: EvidenceId,
    engine: EngineAnalysis,
) -> anyhow::Result<EvidenceEntry> {
    let evaluation = match engine.evaluation {
        PositionEvaluation::Centipawns(value) => EngineEvaluation::Centipawns {
            value,
            perspective: position.side_to_move,
        },
        PositionEvaluation::MateIn(value) => EngineEvaluation::Mate {
            outcome: if value > 0 {
                MateOutcome::Win
            } else {
                MateOutcome::Loss
            },
            distance_plies: u16::try_from(value.unsigned_abs())?,
            perspective: position.side_to_move,
        },
    };
    Ok(EvidenceEntry::engine_analysis(
        EvidenceMetadata::pending(
            vec![dependency],
            EvidenceProvenance::Stockfish {
                version: PINNED_STOCKFISH_VERSION.to_string(),
                binary_digest: ArtifactDigest::try_from(
                    PINNED_STOCKFISH_BINARY_DIGEST.to_string(),
                )?,
                depth: PINNED_STOCKFISH_DEPTH,
                threads: PINNED_STOCKFISH_THREADS,
                hash_mib: PINNED_STOCKFISH_HASH_MIB,
            },
        ),
        position.position_ref.clone(),
        EngineAnalysisEvidence {
            evaluation,
            best_move_uci: engine.best_move,
            principal_variation: engine.principal_variation,
        },
    ))
}

fn maia_entry(
    position: &PositionSnapshot,
    dependency: EvidenceId,
    prediction: HumanMovePrediction,
) -> anyhow::Result<EvidenceEntry> {
    let chess: Chess =
        Fen::from_ascii(position.fen.as_bytes())?.into_position(CastlingMode::Standard)?;
    let candidates = prediction
        .candidates
        .into_iter()
        .filter(|candidate| candidate.probability > 0.0)
        .enumerate()
        .map(|(index, candidate)| {
            let uci = UciMove::from_ascii(candidate.uci.as_bytes())?;
            uci.to_move(&chess).map_err(|_| {
                anyhow::anyhow!(
                    "Maia returned illegal candidate {} for {}",
                    candidate.uci,
                    position.position_ref.as_str()
                )
            })?;
            Ok(HumanMoveCandidateEvidence {
                uci: candidate.uci,
                probability: Probability::try_from(candidate.probability)?,
                rank: u8::try_from(index + 1)?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(!candidates.is_empty(), "Maia returned no legal candidates");
    let win_probability = prediction.win_probability.map_or(
        Ok::<_, anyhow::Error>(ProbabilityState::Unavailable),
        |probability| {
            Ok(ProbabilityState::Available {
                probability: Probability::try_from(probability)?,
            })
        },
    )?;
    Ok(EvidenceEntry::human_move_model(
        EvidenceMetadata::pending(
            vec![dependency],
            EvidenceProvenance::Maia {
                package: PINNED_MAIA_PACKAGE.to_string(),
                model: PINNED_MAIA_MODEL.to_string(),
                image: PINNED_MAIA_IMAGE.to_string(),
                model_digest: ArtifactDigest::try_from(PINNED_MAIA_MODEL_DIGEST.to_string())?,
                config_digest: ArtifactDigest::try_from(PINNED_MAIA_CONFIG_DIGEST.to_string())?,
                player_elo: EloRating::try_from(PINNED_MAIA_ELO)?,
                opponent_elo: EloRating::try_from(PINNED_MAIA_ELO)?,
                candidate_limit: PINNED_MAIA_CANDIDATE_LIMIT,
            },
        ),
        position.position_ref.clone(),
        HumanMoveModelEvidence {
            player_elo: EloRating::try_from(PINNED_MAIA_ELO)?,
            opponent_elo: EloRating::try_from(PINNED_MAIA_ELO)?,
            candidates,
            win_probability,
        },
    ))
}

fn stockfish_runtime() -> anyhow::Result<StockfishRecordingRuntime> {
    Ok(StockfishRecordingRuntime {
        version: PINNED_STOCKFISH_VERSION.to_string(),
        binary_digest: ArtifactDigest::try_from(PINNED_STOCKFISH_BINARY_DIGEST.to_string())?,
        depth: PINNED_STOCKFISH_DEPTH,
        threads: PINNED_STOCKFISH_THREADS,
        hash_mib: PINNED_STOCKFISH_HASH_MIB,
    })
}

fn maia_runtime() -> anyhow::Result<MaiaRecordingRuntime> {
    Ok(MaiaRecordingRuntime {
        package: PINNED_MAIA_PACKAGE.to_string(),
        model: PINNED_MAIA_MODEL.to_string(),
        image: PINNED_MAIA_IMAGE.to_string(),
        model_digest: ArtifactDigest::try_from(PINNED_MAIA_MODEL_DIGEST.to_string())?,
        config_digest: ArtifactDigest::try_from(PINNED_MAIA_CONFIG_DIGEST.to_string())?,
        player_elo: EloRating::try_from(PINNED_MAIA_ELO)?,
        opponent_elo: EloRating::try_from(PINNED_MAIA_ELO)?,
        candidate_limit: PINNED_MAIA_CANDIDATE_LIMIT,
    })
}

fn play(fen: &str, uci: &str) -> anyhow::Result<String> {
    let mut position: Chess =
        Fen::from_ascii(fen.as_bytes())?.into_position(CastlingMode::Standard)?;
    let chess_move = UciMove::from_ascii(uci.as_bytes())?.to_move(&position)?;
    position.play_unchecked(&chess_move);
    Ok(Fen::from_position(position, EnPassantMode::Legal).to_string())
}

fn show_diff(existing: &Path, staged: &Path) -> anyhow::Result<()> {
    if !existing.exists() {
        println!("new recording: {}", staged.display());
        return Ok(());
    }
    let output = Command::new("diff")
        .args(["-u"])
        .arg(existing)
        .arg(staged)
        .output()?;
    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    } else {
        println!("recording is unchanged");
    }
    anyhow::ensure!(
        matches!(output.status.code(), Some(0 | 1)),
        "diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn atomic_write(output: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("recording output has no parent directory"))?;
    let staged = parent.join(format!(
        ".review-session-provider-recording-{}.tmp",
        std::process::id()
    ));
    fs::write(&staged, bytes)?;
    fs::rename(&staged, output)?;
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maia_recording_rejects_illegal_provider_candidates() {
        let recording: ReviewSessionProviderRecording =
            serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/shared-assets/fixtures/Synthet1/review-session-provider-recording.json"
        )))
            .unwrap();
        let (position, dependency) = recording
            .content
            .entries
            .iter()
            .find_map(|entry| match entry {
                EvidenceEntry::Position { metadata, position } => {
                    Some((position, metadata.evidence_id.clone()))
                }
                _ => None,
            })
            .unwrap();
        let prediction = HumanMovePrediction {
            candidates: vec![chen_chess_coach_engine::types::HumanMoveCandidate {
                uci: "a1a8".to_string(),
                probability: 1.0,
                rank: 1,
            }],
            win_probability: None,
        };

        let error = maia_entry(position, dependency, prediction).unwrap_err();

        assert!(error
            .to_string()
            .contains("Maia returned illegal candidate"));
    }
}
