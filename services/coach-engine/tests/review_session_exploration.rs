use std::{
    collections::BTreeMap,
    fs,
    future::{pending, Future},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use chen_chess_coach_engine::{
    engine_analysis::{
        EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer,
        PositionEvaluation,
    },
    evaluation_recording::{ReviewSessionProviderRecording, PINNED_STOCKFISH_DEPTH},
    pipeline_evaluation::recorded_comment_case,
    review_session_contract::*,
    review_session_exploration::{
        AlternativeMoveCancellation, AlternativeMoveExploration, ExploreAlternativeMoveError,
        ExploreAlternativeMoveRequest,
    },
    review_session_host::{
        dispatch, EvaluateLineArgs, HostCapabilityCall, HostCapabilityEvidence,
        HostCapabilityStore, OpponentReplies, StoredHostMoment,
    },
    review_session_start::start_review_session,
};
use serde::de::DeserializeOwned;
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};
use tokio::sync::watch;

#[tokio::test]
async fn verified_stockfish_recording_builds_revisitable_multi_ply_branches() {
    let recording = provider_recording();
    let recorded_alternative_reply = recording
        .content
        .entries
        .iter()
        .find_map(|entry| match entry {
            EvidenceEntry::Branch { branch, .. }
                if branch.branch_ref.as_str() == "branch:capture:e7e6:g5f3" =>
            {
                Some(branch.resulting_position_ref.clone())
            }
            _ => None,
        })
        .expect("recording should contain the alternative reply");
    let core = core_at_ply(24);
    let engine = Arc::new(TestEngine::recording(&recording));
    let exploration = exploration(core, root_engine_entry(&recording, 24), engine.clone());

    let best = explore_root(&exploration, "d7b6", "best").await.unwrap();
    assert_eq!(best.alternative_move.evaluation.best_move_uci, "d7b6");
    assert_eq!(
        best.alternative_move.evaluation.comparison,
        EvaluationLoss::Centipawns { value: 0 }
    );
    assert_eq!(
        best.alternative_move.strongest_reply,
        StrongestReply::Offered {
            uci: "c4e2".to_string()
        }
    );

    let reply = explore_child(&exploration, &best.alternative_move, "c4e2", "reply")
        .await
        .unwrap();
    assert_eq!(
        reply.alternative_move.parent,
        BranchParent::Move {
            branch_ref: best.alternative_move.branch_ref.clone()
        }
    );

    let alternative = explore_root(&exploration, "e7e6", "alternative")
        .await
        .unwrap();
    assert_eq!(
        alternative.alternative_move.evaluation.comparison,
        EvaluationLoss::Centipawns { value: 44 }
    );
    let alternative_reply = explore_child(
        &exploration,
        &alternative.alternative_move,
        "g5f3",
        "alternative-reply",
    )
    .await
    .unwrap();

    let snapshot = exploration.current_state().await;
    assert_eq!(snapshot.imported_move_uci, "a8a7");
    assert_eq!(snapshot.committed_moves.len(), 4);
    assert_eq!(engine.calls(), 4);
    assert_eq!(
        alternative_reply
            .alternative_move
            .resulting_position
            .position_ref
            .as_str(),
        recorded_alternative_reply.as_str()
    );
    assert!(!snapshot
        .evidence_packet
        .entries
        .iter()
        .any(|entry| matches!(entry, EvidenceEntry::HumanMoveModel { .. })));
    assert_eq!(
        exploration.remaining_allowance().await,
        ReviewSessionLimits::V1.max_committed_alternative_moves - 4
    );
}

#[tokio::test]
async fn restored_durable_evidence_starts_with_full_exploration_allowance() {
    let recording = provider_recording();
    let engine = Arc::new(TestEngine::recording(&recording));
    let mut restored_core = core_at_ply(24);
    let first = exploration(
        restored_core.clone(),
        root_engine_entry(&recording, 24),
        engine.clone(),
    );
    explore_root(&first, "d7b6", "durable-evidence")
        .await
        .unwrap();
    restored_core.evidence_packet = first.current_state().await.evidence_packet;
    assert!(restored_core
        .evidence_packet
        .entries
        .iter()
        .any(|entry| matches!(entry, EvidenceEntry::Branch { .. })));
    let root_engine = restored_core
        .evidence_packet
        .entries
        .iter()
        .find(|entry| {
            matches!(
                entry,
                EvidenceEntry::EngineAnalysis { position_ref, .. }
                    if position_ref == &restored_core.position_snapshot.position_ref
            )
        })
        .unwrap()
        .clone();
    let restored = exploration(restored_core, root_engine, engine);

    assert_eq!(
        restored.remaining_allowance().await,
        ReviewSessionLimits::V1.max_committed_alternative_moves
    );
}

/// Re-exploring a move whose Branch evidence the restored packet already
/// carries must not append that evidence a second time.
///
/// Evidence is content-addressed, so the repeat is the same entry rather than a
/// second fact. A duplicate id fails cache assembly for the whole Game Import
/// from then on — permanently, and for every later move, not only the repeated
/// one — and reaches the Player as an unexplained `persistence` failure.
///
/// A restored session is where this bites: the durable packet carries the
/// Branch entry, while the committed-move index that would short-circuit the
/// repeat is in-memory and does not survive.
#[tokio::test]
async fn re_exploring_a_restored_branch_does_not_duplicate_its_evidence() {
    let recording = provider_recording();
    let engine = Arc::new(TestEngine::recording(&recording));
    let mut restored_core = core_at_ply(24);
    let first = exploration(
        restored_core.clone(),
        root_engine_entry(&recording, 24),
        engine.clone(),
    );
    explore_root(&first, "d7b6", "durable-evidence")
        .await
        .unwrap();
    restored_core.evidence_packet = first.current_state().await.evidence_packet;
    let root_engine = restored_core
        .evidence_packet
        .entries
        .iter()
        .find(|entry| {
            matches!(
                entry,
                EvidenceEntry::EngineAnalysis { position_ref, .. }
                    if position_ref == &restored_core.position_snapshot.position_ref
            )
        })
        .unwrap()
        .clone();
    let restored = exploration(restored_core, root_engine, engine);

    explore_root(&restored, "d7b6", "re-explored-after-restore")
        .await
        .unwrap();

    let entries = restored.current_state().await.evidence_packet.entries;
    let mut ids = entries
        .iter()
        .map(|entry| entry.metadata().evidence_id.clone())
        .collect::<Vec<_>>();
    let appended = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        appended,
        "every evidence id in the packet has to be unique",
    );
}

#[tokio::test]
async fn illegal_and_ambiguous_moves_are_rejected_before_provider_work() {
    let recording = provider_recording();
    let core = core_at_ply(24);
    let engine = Arc::new(TestEngine::recording(&recording));
    let exploration = exploration(core, root_engine_entry(&recording, 24), engine.clone());
    let initial = exploration.current_state().await;

    let illegal = explore_root(&exploration, "a1a8", "illegal")
        .await
        .unwrap_err();
    assert!(matches!(
        illegal,
        ExploreAlternativeMoveError::Rejected {
            reason: CommandRejectionReason::IllegalMove,
            ..
        }
    ));
    assert_eq!(engine.calls(), 0);
    assert_eq!(
        exploration.current_state().await.evidence_packet,
        initial.evidence_packet
    );

    let false_check = exploration
        .explore(
            request(
                &exploration,
                BranchParent::Root {
                    position_ref: initial.root_position.position_ref.clone(),
                },
                initial.root_position.position_ref.clone(),
                MoveInput::San {
                    san: "Nb6+".to_string(),
                },
                "false-check",
            )
            .await,
            AlternativeMoveCancellation::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        false_check,
        ExploreAlternativeMoveError::Rejected {
            reason: CommandRejectionReason::IllegalMove,
            ..
        }
    ));
    assert_eq!(engine.calls(), 0);
    assert_eq!(exploration.current_state().await, initial);

    let _branch = explore_root(&exploration, "e7e6", "branch").await.unwrap();
    let before_ambiguous = exploration.current_state().await;
    // Two Knights reach c5 from the reviewed Position, so the SAN alone does
    // not name a move and the rejection has to hand back both.
    let ambiguous = exploration
        .explore(
            request(
                &exploration,
                BranchParent::Root {
                    position_ref: initial.root_position.position_ref.clone(),
                },
                initial.root_position.position_ref.clone(),
                MoveInput::San {
                    san: "Nf6".to_string(),
                },
                "ambiguous",
            )
            .await,
            AlternativeMoveCancellation::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        ambiguous,
        ExploreAlternativeMoveError::Rejected {
            reason: CommandRejectionReason::AmbiguousMove,
            recovery: RejectionRecovery::ChooseLegalMove {
                matching_moves: vec!["d7f6".to_string(), "h5f6".to_string()],
            },
        }
    );
    assert_eq!(engine.calls(), 1);
    assert_eq!(exploration.current_state().await, before_ambiguous);
}

#[tokio::test]
async fn provider_failure_and_cancellation_commit_nothing() {
    let recording = provider_recording();

    let failing_engine = Arc::new(TestEngine::failing());
    let failing = exploration(
        core_at_ply(24),
        root_engine_entry(&recording, 24),
        failing_engine.clone(),
    );
    let before_failure = failing.current_state().await;
    assert_eq!(
        explore_root(&failing, "d7b6", "failure").await.unwrap_err(),
        ExploreAlternativeMoveError::Unavailable(ProviderUnavailableReason::StockfishProcess)
    );
    assert_eq!(failing.current_state().await, before_failure);

    let blocking_engine = Arc::new(TestEngine::blocking());
    let blocking = Arc::new(exploration(
        core_at_ply(24),
        root_engine_entry(&recording, 24),
        blocking_engine.clone(),
    ));
    let cancellation = AlternativeMoveCancellation::default();
    let started = blocking_engine.started();
    let task = {
        let blocking = blocking.clone();
        let cancellation = cancellation.clone();
        tokio::spawn(async move {
            explore_root_with_cancellation(&blocking, "d7b6", "cancel", cancellation).await
        })
    };
    started.await;
    let overlap = explore_root(&blocking, "e7e6", "overlap")
        .await
        .unwrap_err();
    assert_eq!(
        overlap,
        ExploreAlternativeMoveError::Conflict(
            OperationConflictReason::AlternativeMoveEvaluationAlreadyActive
        )
    );
    cancellation.cancel();
    assert_eq!(
        task.await.unwrap().unwrap_err(),
        ExploreAlternativeMoveError::Cancelled
    );
    let cancelled = blocking.current_state().await;
    assert!(cancelled.committed_moves.is_empty());
    assert!(cancelled.active_evaluation.is_none());
}

#[tokio::test(start_paused = true)]
async fn provider_timeout_commits_nothing() {
    let recording = provider_recording();
    let timeout_engine = Arc::new(TestEngine::blocking());
    let timed = exploration(
        core_at_ply(24),
        root_engine_entry(&recording, 24),
        timeout_engine,
    );
    assert_eq!(
        explore_root(&timed, "d7b6", "timeout").await.unwrap_err(),
        ExploreAlternativeMoveError::Unavailable(ProviderUnavailableReason::Timeout {
            provider: ProviderKind::Stockfish,
        })
    );
    assert!(timed.current_state().await.committed_moves.is_empty());
}

#[tokio::test]
async fn mate_comparisons_are_structured_and_terminal_nodes_cannot_extend() {
    let recording = provider_recording();
    let mixed_engine = Arc::new(TestEngine::fixed(EngineAnalysis {
        best_move: "g5f3".to_string(),
        evaluation: PositionEvaluation::MateIn(-3),
        principal_variation: vec!["g5f3".to_string()],
        depth: PINNED_STOCKFISH_DEPTH,
    }));
    let mixed = exploration(
        core_at_ply(24),
        root_engine_entry(&recording, 24),
        mixed_engine,
    );
    let mixed_result = explore_root(&mixed, "e7e6", "mixed").await.unwrap();
    assert_eq!(
        mixed_result.alternative_move.evaluation.comparison,
        EvaluationLoss::Mate {
            best: MateComparison::NotForced,
            selected: MateComparison::Forced {
                outcome: MateOutcome::Win,
                distance_plies: 4,
            },
        }
    );

    let terminal_core = core_at_ply(90);
    let terminal_seed = engine_seed(
        &terminal_core,
        EngineAnalysisEvidence {
            evaluation: EngineEvaluation::Mate {
                outcome: MateOutcome::Win,
                distance_plies: 1,
                perspective: Color::Black,
            },
            best_move_uci: "g4d1".to_string(),
            principal_variation: vec!["g4d1".to_string()],
        },
        pinned_provenance(&recording),
    );
    let terminal_engine = Arc::new(TestEngine::fixed(EngineAnalysis {
        best_move: "0000".to_string(),
        evaluation: PositionEvaluation::MateIn(0),
        principal_variation: Vec::new(),
        depth: PINNED_STOCKFISH_DEPTH,
    }));
    let terminal = exploration(terminal_core, terminal_seed, terminal_engine.clone());
    let mate = explore_root(&terminal, "g4d1", "mate").await.unwrap();
    assert_eq!(
        mate.alternative_move.strongest_reply,
        StrongestReply::Terminal
    );
    assert!(matches!(
        mate.alternative_move.evaluation.selected_move,
        EngineEvaluation::Mate {
            outcome: MateOutcome::Win,
            distance_plies: 1,
            perspective: Color::Black,
        }
    ));
    let provider_calls = terminal_engine.calls();
    let extension = explore_child(&terminal, &mate.alternative_move, "f3f2", "past-mate")
        .await
        .unwrap_err();
    assert!(matches!(
        extension,
        ExploreAlternativeMoveError::Rejected {
            reason: CommandRejectionReason::TerminalPosition,
            ..
        }
    ));
    assert_eq!(terminal_engine.calls(), provider_calls);
}

#[tokio::test]
async fn host_evaluate_line_reuses_exploration_allowance_on_an_in_memory_store() {
    let recording = provider_recording();
    let engine = Arc::new(TestEngine::recording(&recording));
    let exploration = exploration(
        core_at_ply(24),
        root_engine_entry(&recording, 24),
        engine.clone(),
    );
    let case = recorded_comment_case(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evaluation/corpus"),
        "tactical-white-human-likely",
    )
    .unwrap();
    let mut facts = case.moments[0].facts.clone();
    match &mut facts {
        ReviewMomentCommentFacts::Positive { moment }
        | ReviewMomentCommentFacts::Improvement { moment }
        | ReviewMomentCommentFacts::Neutral { moment } => moment.ply = 26,
    }
    let store = HostCapabilityStore::new(vec![StoredHostMoment::from_facts(
        facts,
        ReviewSessionEvidencePacket {
            entries: Vec::new(),
        },
        Some(ReviewMomentLearningMaterial::empty()),
    )
    .with_exploration(exploration)]);

    let dispatched = dispatch(
        &store,
        26,
        &HostCapabilityCall::EvaluateLine(EvaluateLineArgs {
            moves: vec!["Nb6".to_string()],
            opponent_replies: OpponentReplies::Supplied,
        }),
    )
    .await
    .unwrap();
    assert!(dispatched.call_id.starts_with("call:evaluateLine:"));
    let HostCapabilityEvidence::EvaluatedLine {
        requested_moves,
        commits,
    } = dispatched.evidence
    else {
        panic!("evaluate_line returns an evaluated line");
    };
    assert_eq!(requested_moves, vec!["Nb6".to_string()]);
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].evaluation.best_move_uci, "d7b6");
    assert_eq!(dispatched.projection["requestedMoves"][0], "Nb6");
    assert_eq!(dispatched.projection["evaluations"][0]["source"], "player");
    assert_eq!(
        dispatched.projection["evaluations"][0]["requestedMove"],
        "Nb6"
    );
    assert!(dispatched
        .allowed_chess_literals
        .iter()
        .any(|literal| literal == "Nb6"));
    assert!(!dispatched
        .allowed_chess_literals
        .iter()
        .any(|literal| literal == "d7b6"));
    assert_eq!(engine.calls(), 1);

    let second_line = dispatch(
        &store,
        26,
        &HostCapabilityCall::EvaluateLine(EvaluateLineArgs {
            moves: vec!["e6".to_string()],
            opponent_replies: OpponentReplies::Supplied,
        }),
    )
    .await
    .unwrap();
    assert_ne!(second_line.call_id, dispatched.call_id);
    assert_eq!(
        second_line.projection["evaluations"][0]["requestedMove"],
        "e6"
    );
    assert_eq!(engine.calls(), 2);
}

#[tokio::test]
async fn exact_retries_reuse_evidence_and_session_limits_reject_before_analysis() {
    let recording = provider_recording();
    let engine = Arc::new(TestEngine::recording(&recording));
    let reusable = exploration(
        core_at_ply(24),
        root_engine_entry(&recording, 24),
        engine.clone(),
    );
    let first = explore_root(&reusable, "d7b6", "first").await.unwrap();
    let repeated = explore_root(&reusable, "d7b6", "repeat").await.unwrap();
    assert_eq!(repeated, first);
    assert_eq!(engine.calls(), 1);
    assert_eq!(reusable.current_state().await.committed_moves.len(), 1);

    let dynamic_engine = Arc::new(TestEngine::dynamic());
    let limited = exploration(
        core_at_ply(24),
        root_engine_entry(&recording, 24),
        dynamic_engine.clone(),
    );
    let root = limited.current_state().await.root_position;
    let root_legal_moves = legal_moves(&root);
    assert!(
        root_legal_moves.len()
            > usize::from(ReviewSessionLimits::V1.max_committed_alternative_moves)
    );
    for (index, uci) in root_legal_moves
        .iter()
        .take(usize::from(
            ReviewSessionLimits::V1.max_committed_alternative_moves,
        ))
        .enumerate()
    {
        explore_root(&limited, uci, &format!("limit-{index}"))
            .await
            .unwrap();
    }
    let calls_at_limit = dynamic_engine.calls();
    let error = explore_root(
        &limited,
        &root_legal_moves[usize::from(ReviewSessionLimits::V1.max_committed_alternative_moves)],
        "over-limit",
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        ExploreAlternativeMoveError::Rejected {
            reason: CommandRejectionReason::AlternativeMoveLimit,
            ..
        }
    ));
    assert_eq!(dynamic_engine.calls(), calls_at_limit);

    let depth_engine = Arc::new(TestEngine::dynamic());
    let deep = exploration(
        core_at_ply(24),
        root_engine_entry(&recording, 24),
        depth_engine.clone(),
    );
    let mut parent = None;
    let mut source = deep.current_state().await.root_position;
    for depth in 1..=ReviewSessionLimits::V1.max_branch_depth_plies {
        let move_uci = legal_moves(&source)[0].clone();
        let commit = if let Some(parent) = &parent {
            explore_child(&deep, parent, &move_uci, &format!("depth-{depth}"))
                .await
                .unwrap()
        } else {
            explore_root(&deep, &move_uci, &format!("depth-{depth}"))
                .await
                .unwrap()
        };
        source = commit.alternative_move.resulting_position.clone();
        parent = Some(commit.alternative_move);
    }
    let parent = parent.unwrap();
    let calls_at_depth = depth_engine.calls();
    let error = explore_child(&deep, &parent, &legal_moves(&source)[0], "over-depth")
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ExploreAlternativeMoveError::Rejected {
            reason: CommandRejectionReason::BranchDepthLimit,
            ..
        }
    ));
    assert_eq!(depth_engine.calls(), calls_at_depth);
}

async fn explore_root(
    exploration: &AlternativeMoveExploration,
    uci: &str,
    suffix: &str,
) -> Result<
    chen_chess_coach_engine::review_session_exploration::AlternativeMoveCommit,
    ExploreAlternativeMoveError,
> {
    explore_root_with_cancellation(
        exploration,
        uci,
        suffix,
        AlternativeMoveCancellation::default(),
    )
    .await
}

async fn explore_root_with_cancellation(
    exploration: &AlternativeMoveExploration,
    uci: &str,
    suffix: &str,
    cancellation: AlternativeMoveCancellation,
) -> Result<
    chen_chess_coach_engine::review_session_exploration::AlternativeMoveCommit,
    ExploreAlternativeMoveError,
> {
    let root = exploration.current_state().await.root_position;
    exploration
        .explore(
            request(
                exploration,
                BranchParent::Root {
                    position_ref: root.position_ref.clone(),
                },
                root.position_ref,
                MoveInput::Uci {
                    uci: uci.to_string(),
                },
                suffix,
            )
            .await,
            cancellation,
        )
        .await
}

async fn explore_child(
    exploration: &AlternativeMoveExploration,
    parent: &AlternativeMoveResult,
    uci: &str,
    suffix: &str,
) -> Result<
    chen_chess_coach_engine::review_session_exploration::AlternativeMoveCommit,
    ExploreAlternativeMoveError,
> {
    exploration
        .explore(
            request(
                exploration,
                BranchParent::Move {
                    branch_ref: parent.branch_ref.clone(),
                },
                parent.resulting_position.position_ref.clone(),
                MoveInput::Uci {
                    uci: uci.to_string(),
                },
                suffix,
            )
            .await,
            AlternativeMoveCancellation::default(),
        )
        .await
}

async fn request(
    _exploration: &AlternativeMoveExploration,
    parent: BranchParent,
    source_position_ref: PositionRef,
    move_input: MoveInput,
    suffix: &str,
) -> ExploreAlternativeMoveRequest {
    ExploreAlternativeMoveRequest {
        parent,
        source_position_ref,
        move_input,
        idempotency_key: IdempotencyKey::try_from(format!("key:test:{suffix}")).unwrap(),
    }
}

fn exploration(
    core: ReviewSessionCoreContract,
    root_engine: EvidenceEntry,
    engine: Arc<TestEngine>,
) -> AlternativeMoveExploration {
    AlternativeMoveExploration::new(core, &root_engine, engine).unwrap()
}

fn core_at_ply(ply: u16) -> ReviewSessionCoreContract {
    start_review_session(
        RequestId::try_from(format!("request:test:{ply}")).unwrap(),
        CoachTurnId::try_from(format!("coach-turn:test:{ply}")).unwrap(),
        fixture(contract_fixture_root().join("imported-game.json")),
        ReviewMomentSelection::PlayerSelectedMoment { ply },
    )
    .unwrap()
}

fn root_engine_entry(recording: &ReviewSessionProviderRecording, ply: u16) -> EvidenceEntry {
    let core = core_at_ply(ply);
    recording
        .content
        .entries
        .iter()
        .find(|entry| {
            matches!(
                entry,
                EvidenceEntry::EngineAnalysis { position_ref, .. }
                    if position_ref == &core.position_snapshot.position_ref
            )
        })
        .unwrap()
        .clone()
}

fn engine_seed(
    core: &ReviewSessionCoreContract,
    analysis: EngineAnalysisEvidence,
    provenance: EvidenceProvenance,
) -> EvidenceEntry {
    EvidenceEntry::EngineAnalysis {
        metadata: EvidenceMetadata {
            evidence_id: EvidenceId::try_from(zero_digest()).unwrap(),
            dependencies: Vec::new(),
            content_digest: ArtifactDigest::try_from(zero_digest()).unwrap(),
            provenance,
        },
        position_ref: core.position_snapshot.position_ref.clone(),
        analysis,
    }
    .with_computed_identity()
}

fn pinned_provenance(recording: &ReviewSessionProviderRecording) -> EvidenceProvenance {
    recording
        .content
        .entries
        .iter()
        .find_map(|entry| match entry {
            EvidenceEntry::EngineAnalysis { metadata, .. } => Some(metadata.provenance.clone()),
            _ => None,
        })
        .unwrap()
}

fn provider_recording() -> ReviewSessionProviderRecording {
    fixture(canonical_fixture_root().join("review-session-provider-recording.json"))
}

fn legal_moves(position: &PositionSnapshot) -> Vec<String> {
    let chess: Chess = Fen::from_ascii(position.fen.as_bytes())
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap();
    chess
        .legal_moves()
        .into_iter()
        .map(|chess_move| UciMove::from_move(&chess_move, CastlingMode::Standard).to_string())
        .collect()
}

enum TestBehavior {
    Recording(BTreeMap<String, EngineAnalysis>),
    Dynamic,
    Failing,
    Blocking,
    Fixed(EngineAnalysis),
}

struct TestEngine {
    behavior: TestBehavior,
    calls: AtomicUsize,
    started: watch::Sender<bool>,
}

impl TestEngine {
    fn recording(recording: &ReviewSessionProviderRecording) -> Self {
        let positions = recording
            .content
            .entries
            .iter()
            .filter_map(|entry| match entry {
                EvidenceEntry::Position { position, .. } => {
                    Some((position.position_ref.clone(), position.fen.clone()))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let responses = recording
            .content
            .entries
            .iter()
            .filter_map(|entry| match entry {
                EvidenceEntry::EngineAnalysis {
                    position_ref,
                    analysis,
                    ..
                } => Some((
                    positions.get(position_ref).unwrap().clone(),
                    raw_analysis(analysis),
                )),
                _ => None,
            })
            .collect();
        Self::new(TestBehavior::Recording(responses))
    }

    fn dynamic() -> Self {
        Self::new(TestBehavior::Dynamic)
    }

    fn failing() -> Self {
        Self::new(TestBehavior::Failing)
    }

    fn blocking() -> Self {
        Self::new(TestBehavior::Blocking)
    }

    fn fixed(analysis: EngineAnalysis) -> Self {
        Self::new(TestBehavior::Fixed(analysis))
    }

    fn new(behavior: TestBehavior) -> Self {
        let (started, _) = watch::channel(false);
        Self {
            behavior,
            calls: AtomicUsize::new(0),
            started,
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Acquire)
    }

    async fn started(&self) {
        let mut started = self.started.subscribe();
        while !*started.borrow_and_update() {
            started.changed().await.unwrap();
        }
    }
}

impl EngineAnalyzer for TestEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        self.calls.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move {
            match &self.behavior {
                TestBehavior::Recording(responses) => responses
                    .get(input.position)
                    .cloned()
                    .ok_or_else(|| EngineAnalysisError::Protocol("missing recording".to_string())),
                TestBehavior::Dynamic => dynamic_analysis(input.position),
                TestBehavior::Failing => Err(EngineAnalysisError::Protocol(
                    "controlled provider failure".to_string(),
                )),
                TestBehavior::Blocking => {
                    self.started.send_replace(true);
                    pending().await
                }
                TestBehavior::Fixed(analysis) => Ok(analysis.clone()),
            }
        })
    }
}

fn dynamic_analysis(fen: &str) -> Result<EngineAnalysis, EngineAnalysisError> {
    let position: Chess = Fen::from_ascii(fen.as_bytes())
        .map_err(|_| EngineAnalysisError::InvalidInput("bad FEN".to_string()))?
        .into_position(CastlingMode::Standard)
        .map_err(|_| EngineAnalysisError::InvalidInput("illegal FEN".to_string()))?;
    let best_move = position
        .legal_moves()
        .first()
        .map(|chess_move| UciMove::from_move(chess_move, CastlingMode::Standard).to_string())
        .unwrap_or_else(|| "0000".to_string());
    Ok(EngineAnalysis {
        principal_variation: if best_move == "0000" {
            Vec::new()
        } else {
            vec![best_move.clone()]
        },
        best_move,
        evaluation: if position.is_checkmate() {
            PositionEvaluation::MateIn(0)
        } else {
            PositionEvaluation::Centipawns(0)
        },
        depth: PINNED_STOCKFISH_DEPTH,
    })
}

fn raw_analysis(analysis: &EngineAnalysisEvidence) -> EngineAnalysis {
    EngineAnalysis {
        best_move: analysis.best_move_uci.clone(),
        evaluation: match analysis.evaluation {
            EngineEvaluation::Centipawns { value, .. } => PositionEvaluation::Centipawns(value),
            EngineEvaluation::Mate {
                outcome,
                distance_plies,
                ..
            } => PositionEvaluation::MateIn(match outcome {
                MateOutcome::Win => i32::from(distance_plies),
                MateOutcome::Loss => -i32::from(distance_plies),
            }),
        },
        principal_variation: analysis.principal_variation.clone(),
        depth: PINNED_STOCKFISH_DEPTH,
    }
}

fn fixture<T: DeserializeOwned>(path: impl AsRef<Path>) -> T {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn contract_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/coach-engine-sdk/fixtures")
}

fn canonical_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/shared-assets/fixtures/Synthet1")
}

fn zero_digest() -> String {
    format!("sha256:{}", "0".repeat(64))
}
