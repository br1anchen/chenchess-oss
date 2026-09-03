use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    future::{pending, Future},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex as StdMutex,
    },
};

use chen_chess_coach_engine::{
    engine_analysis::{
        EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer,
        PositionEvaluation,
    },
    evaluation_recording::{
        ReviewSessionProviderRecording, PINNED_MAIA_CANDIDATE_LIMIT, PINNED_STOCKFISH_DEPTH,
    },
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction},
    review_session_coaching::{
        AlternativeMoveAssessmentAuthor, AlternativeMoveCoachTurnError, AlternativeMoveCoaching,
        CoachTurnActivity, CoachTurnAuthorInput, CoachTurnTargetSelection, PreparedCoachTurnTarget,
        StartAlternativeMoveCoachTurn,
    },
    review_session_contract::*,
    review_session_exploration::{
        AlternativeMoveCancellation, AlternativeMoveExploration, ExploreAlternativeMoveRequest,
    },
    review_session_start::start_review_session,
    types::HumanMoveCandidate,
};
use serde::de::DeserializeOwned;
use tokio::sync::watch;

#[tokio::test]
async fn explicit_turn_captures_one_target_and_commits_verified_maia_evidence() {
    let fixture = coaching_fixture().await;
    let author = Arc::new(TestAuthor::default());
    let coaching = unscoped_coaching(fixture.human.clone(), author.clone());

    let commit = coaching
        .coach(request(
            "one",
            PriorCoachTurn::None,
            CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
        ))
        .await
        .unwrap();

    assert_eq!(
        commit.assessment.coach_turn_id.as_str(),
        "coach-turn:test:one"
    );
    assert_eq!(
        commit.assessment.alternative_move_id,
        fixture.target.target().alternative_move_id
    );
    assert_eq!(commit.evidence_entries.len(), 2);
    assert!(commit.evidence_entries.iter().all(
        |entry| matches!(entry, EvidenceEntry::HumanMoveModel { analysis, .. }
            if analysis.player_elo.value() == 1246 && analysis.opponent_elo.value() == 1246)
    ));

    let calls = fixture.human.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls
        .iter()
        .all(|(_, elo, limit)| *elo == 1246 && *limit == PINNED_MAIA_CANDIDATE_LIMIT));
    let inputs = author.inputs();
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].target.ancestor_branch().len(), 1);
    assert_eq!(inputs[0].target.target(), fixture.target.target());
    assert!(!serde_json::to_string(inputs[0].target.context())
        .unwrap()
        .contains("intent"));
    assert!(coaching.current_state().await.active_turn.is_none());
    assert_eq!(coaching.current_state().await.assessments, vec![commit]);
}

#[tokio::test]
async fn cancellation_is_idempotent_and_commits_no_partial_state() {
    let fixture = coaching_fixture().await;
    let human = Arc::new(RecordingHuman::blocking(fixture.recording.clone()));
    let coaching = Arc::new(unscoped_coaching(
        human.clone(),
        Arc::new(TestAuthor::default()),
    ));
    let turn_id = coach_turn_id("cancel");
    let key = idempotency_key("cancel");
    let task = {
        let coaching = coaching.clone();
        let target = fixture.target.clone();
        tokio::spawn(async move {
            coaching
                .coach(StartAlternativeMoveCoachTurn {
                    coach_turn_id: turn_id,
                    message: "Please assess this move.".to_string(),
                    idempotency_key: key,
                    prior_turn: PriorCoachTurn::None,
                    target: CoachTurnTargetSelection::Explicit(Box::new(target)),
                })
                .await
        })
    };
    human.started().await;

    let turn_id = coach_turn_id("cancel");
    let key = idempotency_key("cancel");
    coaching.cancel(&turn_id, &key).await.unwrap();
    assert_eq!(
        task.await.unwrap().unwrap_err(),
        AlternativeMoveCoachTurnError::Cancelled
    );
    coaching.cancel(&turn_id, &key).await.unwrap();
    let snapshot = coaching.current_state().await;
    assert!(snapshot.active_turn.is_none());
    assert!(snapshot.assessments.is_empty());
    assert_eq!(snapshot.started_turns, 1);
}

#[tokio::test]
async fn steering_stops_the_prior_turn_then_preserves_its_target() {
    let fixture = coaching_fixture().await;
    let author = Arc::new(TestAuthor::block_first());
    let coaching = Arc::new(unscoped_coaching(fixture.human.clone(), author.clone()));
    let first = {
        let coaching = coaching.clone();
        let target = fixture.target.clone();
        tokio::spawn(async move {
            coaching
                .coach(request(
                    "steered",
                    PriorCoachTurn::None,
                    CoachTurnTargetSelection::Explicit(Box::new(target)),
                ))
                .await
        })
    };
    author.started().await;

    let replacement = coaching
        .coach(request(
            "replacement",
            PriorCoachTurn::Steers {
                coach_turn_id: coach_turn_id("steered"),
            },
            CoachTurnTargetSelection::Preserve,
        ))
        .await
        .unwrap();

    assert_eq!(
        first.await.unwrap().unwrap_err(),
        AlternativeMoveCoachTurnError::Cancelled
    );
    assert_eq!(
        replacement.assessment.alternative_move_id,
        fixture.target.target().alternative_move_id
    );
    assert_eq!(author.max_active(), 1);
    let snapshot = coaching.current_state().await;
    assert_eq!(snapshot.started_turns, 2);
    assert_eq!(snapshot.assessments, vec![replacement]);
}

#[tokio::test]
async fn one_activity_scope_admits_one_turn_at_a_time_and_frees_it_when_a_turn_ends() {
    let fixture = coaching_fixture().await;
    let holding_author = Arc::new(TestAuthor::block_first());
    let activity = Arc::new(CoachTurnActivity::default());
    let holding = Arc::new(AlternativeMoveCoaching::new(
        fixture.human.clone(),
        holding_author.clone(),
        activity.clone(),
    ));
    let joining = AlternativeMoveCoaching::new(
        fixture.human.clone(),
        Arc::new(TestAuthor::default()),
        activity,
    );
    let held = {
        let holding = holding.clone();
        let target = fixture.target.clone();
        tokio::spawn(async move {
            holding
                .coach(request(
                    "scope-held",
                    PriorCoachTurn::None,
                    CoachTurnTargetSelection::Explicit(Box::new(target)),
                ))
                .await
        })
    };
    holding_author.started().await;

    assert_eq!(
        joining
            .coach(request(
                "scope-refused",
                PriorCoachTurn::None,
                CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
            ))
            .await
            .unwrap_err(),
        AlternativeMoveCoachTurnError::Conflict(OperationConflictReason::CoachTurnAlreadyActive)
    );
    assert!(joining.current_state().await.active_turn.is_none());

    holding
        .cancel(&coach_turn_id("scope-held"), &idempotency_key("scope-held"))
        .await
        .unwrap();
    assert_eq!(
        held.await.unwrap().unwrap_err(),
        AlternativeMoveCoachTurnError::Cancelled
    );

    let admitted = joining
        .coach(request(
            "scope-freed",
            PriorCoachTurn::None,
            CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
        ))
        .await
        .unwrap();
    assert_eq!(
        admitted.assessment.coach_turn_id,
        coach_turn_id("scope-freed")
    );

    // That turn completed normally, so the scope is free again for the other
    // conversation rather than only being released by cancellation.
    let after_completion = holding
        .coach(request(
            "scope-freed-again",
            PriorCoachTurn::None,
            CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
        ))
        .await
        .unwrap();
    assert_eq!(
        after_completion.assessment.coach_turn_id,
        coach_turn_id("scope-freed-again")
    );
}

#[tokio::test]
async fn separate_activity_scopes_run_turns_independently() {
    let fixture = coaching_fixture().await;
    let holding_author = Arc::new(TestAuthor::block_first());
    let holding = Arc::new(AlternativeMoveCoaching::new(
        fixture.human.clone(),
        holding_author.clone(),
        Arc::new(CoachTurnActivity::default()),
    ));
    let other_scope = AlternativeMoveCoaching::new(
        fixture.human.clone(),
        Arc::new(TestAuthor::default()),
        Arc::new(CoachTurnActivity::default()),
    );
    let held = {
        let holding = holding.clone();
        let target = fixture.target.clone();
        tokio::spawn(async move {
            holding
                .coach(request(
                    "other-scope-held",
                    PriorCoachTurn::None,
                    CoachTurnTargetSelection::Explicit(Box::new(target)),
                ))
                .await
        })
    };
    holding_author.started().await;

    let concurrent = other_scope
        .coach(request(
            "other-scope-turn",
            PriorCoachTurn::None,
            CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
        ))
        .await
        .unwrap();

    assert_eq!(
        concurrent.assessment.coach_turn_id,
        coach_turn_id("other-scope-turn")
    );
    assert!(!held.is_finished());
    holding
        .cancel(
            &coach_turn_id("other-scope-held"),
            &idempotency_key("other-scope-held"),
        )
        .await
        .unwrap();
    assert_eq!(
        held.await.unwrap().unwrap_err(),
        AlternativeMoveCoachTurnError::Cancelled
    );
}

#[tokio::test]
async fn invalid_or_historical_steering_cannot_stop_or_replace_a_turn() {
    let fixture = coaching_fixture().await;
    let author = Arc::new(TestAuthor::block_first());
    let coaching = Arc::new(AlternativeMoveCoaching::new(
        fixture.human.clone(),
        author.clone(),
        Arc::new(CoachTurnActivity::default()),
    ));
    let active = {
        let coaching = coaching.clone();
        let target = fixture.target.clone();
        tokio::spawn(async move {
            coaching
                .coach(request(
                    "active",
                    PriorCoachTurn::None,
                    CoachTurnTargetSelection::Explicit(Box::new(target)),
                ))
                .await
        })
    };
    author.started().await;

    let duplicate = coaching
        .coach(request(
            "active",
            PriorCoachTurn::Steers {
                coach_turn_id: coach_turn_id("active"),
            },
            CoachTurnTargetSelection::Preserve,
        ))
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate,
        AlternativeMoveCoachTurnError::Rejected {
            reason: CommandRejectionReason::InvalidCommand,
            ..
        }
    ));
    assert_eq!(
        coaching
            .current_state()
            .await
            .active_turn
            .unwrap()
            .coach_turn_id,
        coach_turn_id("active")
    );

    coaching
        .cancel(&coach_turn_id("active"), &idempotency_key("active"))
        .await
        .unwrap();
    assert_eq!(
        active.await.unwrap().unwrap_err(),
        AlternativeMoveCoachTurnError::Cancelled
    );
    assert_eq!(
        coaching
            .coach(request(
                "historical",
                PriorCoachTurn::Steers {
                    coach_turn_id: coach_turn_id("active"),
                },
                CoachTurnTargetSelection::Preserve,
            ))
            .await
            .unwrap_err(),
        AlternativeMoveCoachTurnError::Conflict(OperationConflictReason::IdempotencyKeyMismatch)
    );
}

#[tokio::test]
async fn unavailable_retry_uses_a_new_identity_and_keeps_prior_success_immutable() {
    let fixture = coaching_fixture().await;
    let author = Arc::new(TestAuthor::unavailable_on([1]));
    let coaching = unscoped_coaching(fixture.human.clone(), author);
    let first = coaching
        .coach(request(
            "success",
            PriorCoachTurn::None,
            CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
        ))
        .await
        .unwrap();
    let failure = coaching
        .coach(request(
            "unavailable",
            PriorCoachTurn::None,
            CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
        ))
        .await
        .unwrap_err();
    assert_eq!(
        failure,
        AlternativeMoveCoachTurnError::Unavailable(ProviderUnavailableReason::LanguageLayer)
    );
    assert_eq!(
        coaching.current_state().await.assessments,
        vec![first.clone()]
    );

    let retry = coaching
        .coach(request(
            "retry",
            PriorCoachTurn::RetriesUnavailable {
                coach_turn_id: coach_turn_id("unavailable"),
            },
            CoachTurnTargetSelection::Preserve,
        ))
        .await
        .unwrap();
    let snapshot = coaching.current_state().await;
    assert_eq!(snapshot.started_turns, 3);
    assert_eq!(snapshot.assessments[0], first);
    assert_eq!(snapshot.assessments[1], retry);
}

/// Task B had no prose gate at all: an explanation only had to be non-empty.
#[tokio::test]
async fn coach_turn_prose_is_grounded_by_markers_and_substituted_before_it_is_committed() {
    let fixture = coaching_fixture().await;
    let coaching = unscoped_coaching(fixture.human.clone(), Arc::new(TestAuthor::default()));

    let commit = coaching
        .coach(request(
            "grounded-prose",
            PriorCoachTurn::None,
            CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
        ))
        .await
        .unwrap();

    let objective_quality = &commit.assessment.objective_quality.explanation;
    assert!(
        !objective_quality.contains('{') && !objective_quality.contains('}'),
        "markers must be substituted before the turn is committed: {objective_quality}"
    );
    assert!(objective_quality.contains("By the engine's reckoning"));
    // The evaluations the Player reads were rendered from the packet, not
    // written by the author.
    assert!(
        objective_quality.contains("0.0")
            || objective_quality.contains('+')
            || objective_quality.contains('-')
    );
    assert!(commit
        .assessment
        .resilience
        .explanation
        .contains("the reply that decides it is"));
}

#[tokio::test]
async fn ungrounded_coach_turn_prose_takes_the_whole_turn_down() {
    // One rejection per reason the prose gate exists: a figure the author
    // wrote itself, a marker no dimension defines, a square the evidence does
    // not ground, a URL, and the internal name of the human move model.
    for prose in [
        "{alternativeMove} lands at {alternativeEval} against {bestMove} at {bestEval}, roughly +0.4 better.",
        "{alternativeMove} at {alternativeEval} against {bestMove} at {bestEval}, and {inventedMarker}.",
        "{alternativeMove} at {alternativeEval} against {bestMove} at {bestEval}; the follow-up Qh5 decides it.",
        "{alternativeMove} at {alternativeEval} against {bestMove} at {bestEval}. See https://lichess.org/training/fork.",
        "{alternativeMove} at {alternativeEval} against {bestMove} at {bestEval}, and Maia agrees.",
    ] {
        let fixture = coaching_fixture().await;
        let coaching = unscoped_coaching(
            fixture.human.clone(),
            Arc::new(TestAuthor::writing(prose)),
        );

        assert_eq!(
            coaching
                .coach(request(
                    "ungrounded-prose",
                    PriorCoachTurn::None,
                    CoachTurnTargetSelection::Explicit(Box::new(fixture.target)),
                ))
                .await
                .unwrap_err(),
            AlternativeMoveCoachTurnError::Unavailable(ProviderUnavailableReason::LanguageLayer),
            "prose should have been rejected: {prose}"
        );
        assert!(
            coaching.current_state().await.assessments.is_empty(),
            "a rejected dimension takes the whole turn, never two thirds of one"
        );
    }
}

#[tokio::test]
async fn assessment_cannot_cite_evidence_outside_its_immutable_target() {
    let fixture = coaching_fixture().await;
    let coaching = unscoped_coaching(
        fixture.human.clone(),
        Arc::new(TestAuthor::with_unrelated_citation()),
    );

    assert_eq!(
        coaching
            .coach(request(
                "unrelated-citation",
                PriorCoachTurn::None,
                CoachTurnTargetSelection::Explicit(Box::new(fixture.target)),
            ))
            .await
            .unwrap_err(),
        AlternativeMoveCoachTurnError::Unavailable(ProviderUnavailableReason::LanguageLayer)
    );
    assert!(coaching.current_state().await.assessments.is_empty());
}

#[tokio::test]
async fn active_coaching_does_not_consume_the_board_exploration_slot() {
    let fixture = coaching_fixture().await;
    let author = Arc::new(TestAuthor::block_first());
    let coaching = Arc::new(unscoped_coaching(fixture.human.clone(), author.clone()));
    let task = {
        let coaching = coaching.clone();
        let target = fixture.target.clone();
        tokio::spawn(async move {
            coaching
                .coach(request(
                    "concurrent",
                    PriorCoachTurn::None,
                    CoachTurnTargetSelection::Explicit(Box::new(target)),
                ))
                .await
        })
    };
    author.started().await;

    let second = explore_root(&fixture.exploration, "e7e6", "while-coaching")
        .await
        .unwrap();
    assert_eq!(second.alternative_move.move_uci, "e7e6");
    coaching
        .cancel(&coach_turn_id("concurrent"), &idempotency_key("concurrent"))
        .await
        .unwrap();
    assert_eq!(
        task.await.unwrap().unwrap_err(),
        AlternativeMoveCoachTurnError::Cancelled
    );
    assert_eq!(
        fixture
            .exploration
            .current_state()
            .await
            .committed_moves
            .len(),
        2
    );
}

#[tokio::test]
async fn message_and_started_turn_limits_reject_before_more_provider_work() {
    let fixture = coaching_fixture().await;
    let coaching = unscoped_coaching(fixture.human.clone(), Arc::new(TestAuthor::default()));
    for (suffix, message) in [("empty", " ".to_string()), ("large", "é".repeat(2049))] {
        let mut invalid = request(
            suffix,
            PriorCoachTurn::None,
            CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
        );
        invalid.message = message;
        assert!(matches!(
            coaching.coach(invalid).await.unwrap_err(),
            AlternativeMoveCoachTurnError::Rejected {
                reason: CommandRejectionReason::MessageTooLong,
                ..
            }
        ));
    }
    assert!(fixture.human.calls().is_empty());

    for index in 0..ReviewSessionLimits::V1.max_started_coach_turns {
        coaching
            .coach(request(
                &format!("limit-{index}"),
                PriorCoachTurn::None,
                CoachTurnTargetSelection::Explicit(Box::new(fixture.target.clone())),
            ))
            .await
            .unwrap();
    }
    let calls_at_limit = fixture.human.calls().len();
    assert!(matches!(
        coaching
            .coach(request(
                "over-limit",
                PriorCoachTurn::None,
                CoachTurnTargetSelection::Explicit(Box::new(fixture.target)),
            ))
            .await
            .unwrap_err(),
        AlternativeMoveCoachTurnError::Rejected {
            reason: CommandRejectionReason::CoachTurnLimit,
            recovery: RejectionRecovery::StartNewReviewSession,
        }
    ));
    assert_eq!(fixture.human.calls().len(), calls_at_limit);
}

struct CoachingFixture {
    recording: Arc<ReviewSessionProviderRecording>,
    exploration: Arc<AlternativeMoveExploration>,
    target: PreparedCoachTurnTarget,
    human: Arc<RecordingHuman>,
}

async fn coaching_fixture() -> CoachingFixture {
    let recording = Arc::new(provider_recording());
    let core = core_at_ply(24);
    let exploration = Arc::new(
        AlternativeMoveExploration::new(
            core.clone(),
            &root_engine_entry(&recording, &core.position_snapshot.position_ref),
            Arc::new(RecordingEngine::new(&recording)),
        )
        .unwrap(),
    );
    let committed = explore_root(&exploration, "d7b6", "target").await.unwrap();
    let target = PreparedCoachTurnTarget::capture(
        &core,
        &exploration.current_state().await,
        &committed.alternative_move.alternative_move_id,
    )
    .unwrap();
    let human = Arc::new(RecordingHuman::new(recording.clone()));
    CoachingFixture {
        recording,
        exploration,
        target,
        human,
    }
}

async fn explore_root(
    exploration: &AlternativeMoveExploration,
    uci: &str,
    suffix: &str,
) -> Result<
    chen_chess_coach_engine::review_session_exploration::AlternativeMoveCommit,
    chen_chess_coach_engine::review_session_exploration::ExploreAlternativeMoveError,
> {
    let snapshot = exploration.current_state().await;
    exploration
        .explore(
            ExploreAlternativeMoveRequest {
                parent: BranchParent::Root {
                    position_ref: snapshot.root_position.position_ref.clone(),
                },
                source_position_ref: snapshot.root_position.position_ref,
                move_input: MoveInput::Uci {
                    uci: uci.to_string(),
                },
                idempotency_key: IdempotencyKey::try_from(format!("key:test:{suffix}")).unwrap(),
            },
            AlternativeMoveCancellation::default(),
        )
        .await
}

/// A Coach Turn scope of its own, for tests whose subject is not the scope.
fn unscoped_coaching(
    human: Arc<dyn HumanMoveModel>,
    author: Arc<dyn AlternativeMoveAssessmentAuthor>,
) -> AlternativeMoveCoaching {
    AlternativeMoveCoaching::new(human, author, Arc::new(CoachTurnActivity::default()))
}

fn request(
    suffix: &str,
    prior_turn: PriorCoachTurn,
    target: CoachTurnTargetSelection,
) -> StartAlternativeMoveCoachTurn {
    StartAlternativeMoveCoachTurn {
        coach_turn_id: coach_turn_id(suffix),
        message: format!("Please assess this move: {suffix}"),
        idempotency_key: idempotency_key(suffix),
        prior_turn,
        target,
    }
}

fn coach_turn_id(suffix: &str) -> CoachTurnId {
    CoachTurnId::try_from(format!("coach-turn:test:{suffix}")).unwrap()
}

fn idempotency_key(suffix: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(format!("key:coach:test:{suffix}")).unwrap()
}

#[derive(Default)]
struct TestAuthor {
    calls: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
    inputs: StdMutex<Vec<CoachTurnAuthorInput>>,
    block_first: bool,
    cite_unrelated: bool,
    /// Replaces the objective-quality prose, so a test can put exactly one
    /// ungrounded thing in front of the gate.
    objective_quality_prose: Option<String>,
    unavailable: BTreeSet<usize>,
    started: watch::Sender<bool>,
}

impl TestAuthor {
    fn block_first() -> Self {
        let (started, _) = watch::channel(false);
        Self {
            block_first: true,
            started,
            ..Self::default()
        }
    }

    fn unavailable_on(calls: impl IntoIterator<Item = usize>) -> Self {
        Self {
            unavailable: calls.into_iter().collect(),
            ..Self::default()
        }
    }

    fn with_unrelated_citation() -> Self {
        Self {
            cite_unrelated: true,
            ..Self::default()
        }
    }

    fn writing(objective_quality_prose: &str) -> Self {
        Self {
            objective_quality_prose: Some(objective_quality_prose.to_string()),
            ..Self::default()
        }
    }

    fn inputs(&self) -> Vec<CoachTurnAuthorInput> {
        self.inputs.lock().unwrap().clone()
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::Acquire)
    }

    async fn started(&self) {
        let mut started = self.started.subscribe();
        while !*started.borrow_and_update() {
            started.changed().await.unwrap();
        }
    }
}

impl AlternativeMoveAssessmentAuthor for TestAuthor {
    fn assess<'a>(
        &'a self,
        input: CoachTurnAuthorInput,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<AlternativeMoveAssessment, ProviderUnavailableReason>>
                + Send
                + 'a,
        >,
    > {
        let call = self.calls.fetch_add(1, Ordering::AcqRel);
        self.inputs.lock().unwrap().push(input.clone());
        let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
        self.max_active.fetch_max(active, Ordering::AcqRel);
        Box::pin(async move {
            let _active = ActiveAuthorCall(&self.active);
            if self.unavailable.contains(&call) {
                return Err(ProviderUnavailableReason::LanguageLayer);
            }
            if self.block_first && call == 0 {
                self.started.send_replace(true);
                pending().await
            }
            let mut assessment = assessment(&input);
            if let Some(prose) = &self.objective_quality_prose {
                assessment.objective_quality.explanation = prose.clone();
            }
            if self.cite_unrelated {
                let related = [
                    Some(&input.evidence.target_branch),
                    Some(&input.evidence.source_engine),
                    Some(&input.evidence.resulting_engine),
                    Some(&input.evidence.source_human),
                    Some(&input.evidence.resulting_human),
                ]
                .into_iter()
                .flatten()
                .collect::<BTreeSet<_>>();
                let unrelated = input
                    .evidence_packet
                    .entries
                    .iter()
                    .map(|entry| &entry.metadata().evidence_id)
                    .find(|evidence_id| !related.contains(evidence_id))
                    .expect("fixture should contain evidence outside the immutable target")
                    .clone();
                assessment.objective_quality.evidence_refs.push(unrelated);
            }
            Ok(assessment)
        })
    }
}

struct ActiveAuthorCall<'a>(&'a AtomicUsize);

impl Drop for ActiveAuthorCall<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn assessment(input: &CoachTurnAuthorInput) -> AlternativeMoveAssessment {
    let evidence = &input.evidence;
    AlternativeMoveAssessment {
        coach_turn_id: input.coach_turn_id.clone(),
        alternative_move_id: input.target.target().alternative_move_id.clone(),
        objective_quality: dimension(
            OBJECTIVE_QUALITY_PROSE,
            [
                evidence.target_branch.clone(),
                evidence.source_engine.clone(),
                evidence.resulting_engine.clone(),
            ],
        ),
        findability: dimension(
            FINDABILITY_PROSE,
            [
                evidence.target_branch.clone(),
                evidence.source_human.clone(),
            ],
        ),
        resilience: dimension(
            RESILIENCE_PROSE,
            [
                evidence.target_branch.clone(),
                evidence.resulting_engine.clone(),
                evidence.resulting_human.clone(),
            ],
        ),
    }
}

/// Marker-form assessment prose, one vocabulary per dimension.
///
/// Each dimension may only name what it cites, so the objective-quality
/// explanation is the only one that can carry an evaluation.
fn dimension(explanation: &str, ids: impl IntoIterator<Item = EvidenceId>) -> AssessmentDimension {
    AssessmentDimension {
        explanation: explanation.to_string(),
        evidence_refs: ids.into_iter().collect(),
    }
}

const OBJECTIVE_QUALITY_PROSE: &str =
    "By the engine's reckoning {alternativeMove} lands at {alternativeEval}, against {bestMove} at {bestEval}.";
const FINDABILITY_PROSE: &str =
    "Whether {alternativeMove} turns up at the board is the real question here.";
const RESILIENCE_PROSE: &str =
    "After {alternativeMove} the reply that decides it is {strongestReply}.";

struct RecordingHuman {
    responses: BTreeMap<String, HumanMoveModelEvidence>,
    calls: StdMutex<Vec<(String, u16, u8)>>,
    blocking: bool,
    started: watch::Sender<bool>,
}

impl RecordingHuman {
    fn new(recording: Arc<ReviewSessionProviderRecording>) -> Self {
        Self::with_behavior(recording, false)
    }

    fn blocking(recording: Arc<ReviewSessionProviderRecording>) -> Self {
        Self::with_behavior(recording, true)
    }

    fn with_behavior(recording: Arc<ReviewSessionProviderRecording>, blocking: bool) -> Self {
        let positions = positions_by_ref(&recording);
        let responses = recording
            .content
            .entries
            .iter()
            .filter_map(|entry| match entry {
                EvidenceEntry::HumanMoveModel {
                    position_ref,
                    analysis,
                    ..
                } => Some((
                    positions.get(position_ref).unwrap().clone(),
                    analysis.clone(),
                )),
                _ => None,
            })
            .collect();
        let (started, _) = watch::channel(false);
        Self {
            responses,
            calls: StdMutex::new(Vec::new()),
            blocking,
            started,
        }
    }

    fn calls(&self) -> Vec<(String, u16, u8)> {
        self.calls.lock().unwrap().clone()
    }

    async fn started(&self) {
        let mut started = self.started.subscribe();
        while !*started.borrow_and_update() {
            started.changed().await.unwrap();
        }
    }
}

impl HumanMoveModel for RecordingHuman {
    fn predict<'a>(
        &'a self,
        input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        self.calls.lock().unwrap().push((
            input.position.to_string(),
            input.elo.rating(),
            input.limit,
        ));
        Box::pin(async move {
            if self.blocking {
                self.started.send_replace(true);
                return pending().await;
            }
            let evidence = self.responses.get(input.position).unwrap();
            Ok(HumanMovePrediction {
                candidates: evidence
                    .candidates
                    .iter()
                    .map(|candidate| HumanMoveCandidate {
                        uci: candidate.uci.clone(),
                        probability: candidate.probability.value(),
                        rank: usize::from(candidate.rank),
                    })
                    .collect(),
                win_probability: match evidence.win_probability {
                    ProbabilityState::Available { probability } => Some(probability.value()),
                    ProbabilityState::Unavailable => None,
                },
            })
        })
    }
}

struct RecordingEngine {
    responses: BTreeMap<String, EngineAnalysis>,
}

impl RecordingEngine {
    fn new(recording: &ReviewSessionProviderRecording) -> Self {
        let positions = positions_by_ref(recording);
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
                    EngineAnalysis {
                        best_move: analysis.best_move_uci.clone(),
                        evaluation: raw_evaluation(&analysis.evaluation),
                        principal_variation: analysis.principal_variation.clone(),
                        depth: PINNED_STOCKFISH_DEPTH,
                    },
                )),
                _ => None,
            })
            .collect();
        Self { responses }
    }
}

impl EngineAnalyzer for RecordingEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.responses
                .get(input.position)
                .cloned()
                .ok_or_else(|| EngineAnalysisError::Protocol("missing recording".to_string()))
        })
    }
}

fn positions_by_ref(recording: &ReviewSessionProviderRecording) -> BTreeMap<PositionRef, String> {
    recording
        .content
        .entries
        .iter()
        .filter_map(|entry| match entry {
            EvidenceEntry::Position { position, .. } => {
                Some((position.position_ref.clone(), position.fen.clone()))
            }
            _ => None,
        })
        .collect()
}

fn raw_evaluation(evaluation: &EngineEvaluation) -> PositionEvaluation {
    match evaluation {
        EngineEvaluation::Centipawns { value, .. } => PositionEvaluation::Centipawns(*value),
        EngineEvaluation::Mate {
            outcome,
            distance_plies,
            ..
        } => PositionEvaluation::MateIn(match outcome {
            MateOutcome::Win => i32::from(*distance_plies),
            MateOutcome::Loss => -i32::from(*distance_plies),
        }),
    }
}

fn core_at_ply(ply: u16) -> ReviewSessionCoreContract {
    start_review_session(
        RequestId::try_from(format!("request:coach-test:{ply}")).unwrap(),
        CoachTurnId::try_from(format!("coach-turn:coach-test:{ply}")).unwrap(),
        fixture(contract_fixture_root().join("imported-game.json")),
        ReviewMomentSelection::PlayerSelectedMoment { ply },
    )
    .unwrap()
}

fn root_engine_entry(
    recording: &ReviewSessionProviderRecording,
    position_ref: &PositionRef,
) -> EvidenceEntry {
    recording
        .content
        .entries
        .iter()
        .find(|entry| {
            matches!(entry, EvidenceEntry::EngineAnalysis {
            position_ref: recorded,
            ..
        } if recorded == position_ref)
        })
        .unwrap_or_else(|| {
            panic!(
                "provider recording has no Engine Analysis for {}",
                position_ref.as_str()
            )
        })
        .clone()
}

fn provider_recording() -> ReviewSessionProviderRecording {
    fixture(canonical_fixture_root().join("review-session-provider-recording.json"))
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
