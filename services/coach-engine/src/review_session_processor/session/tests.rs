use std::{fs, future::Future, path::Path, pin::Pin, sync::Arc};

use serde::de::DeserializeOwned;

use super::*;
use crate::{
    engine_analysis::{EngineAnalysis, EngineAnalysisError, EngineAnalysisInput},
    game_import_store::{GameImportRecord, ImportedCriticalMoment},
    human_move_model::{HumanMoveInput, HumanMoveModelError, HumanMovePrediction},
    pipeline_evaluation::recorded_comment_case,
    review_annotation_store::{
        InMemoryReviewAnnotationStore, ReviewAnnotationAddress, ReviewAnnotationLog,
    },
    review_session_coaching::{
        AlternativeMoveAssessmentAuthor, CoachTurnActivity, CoachTurnAuthorInput,
    },
    review_session_host::{HostCapabilityEvidence, HostMomentClassification},
    review_session_start::start_review_session,
};

async fn empty_annotations(game_import_id: &GameImportId) -> Arc<ReviewAnnotationLog> {
    Arc::new(
        ReviewAnnotationLog::load(
            Arc::new(InMemoryReviewAnnotationStore::default()),
            ReviewAnnotationAddress {
                owner: ProcessorPrincipal::LocalCoach,
                game_import_id: game_import_id.clone(),
            },
        )
        .await
        .unwrap(),
    )
}

struct UnusedEngine;

impl EngineAnalyzer for UnusedEngine {
    fn analyze<'a>(
        &'a self,
        _input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async {
            Err(EngineAnalysisError::Protocol(
                "the captured recording should provide every root analysis".to_string(),
            ))
        })
    }
}

struct UnusedHuman;

impl HumanMoveModel for UnusedHuman {
    fn predict<'a>(
        &'a self,
        _input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        Box::pin(async {
            Err(HumanMoveModelError::InvalidInput(
                "the captured recording should provide every root prediction".to_string(),
            ))
        })
    }
}

struct UnusedCoachAuthor;

impl AlternativeMoveAssessmentAuthor for UnusedCoachAuthor {
    fn assess<'a>(
        &'a self,
        _input: CoachTurnAuthorInput,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<AlternativeMoveAssessment, ProviderUnavailableReason>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async { Err(ProviderUnavailableReason::LanguageLayer) })
    }
}

#[tokio::test]
async fn session_accepts_an_empty_automatic_moment_set() {
    let coaching: Arc<AlternativeMoveCoaching> = Arc::new(AlternativeMoveCoaching::new(
        Arc::new(UnusedHuman),
        Arc::new(UnusedCoachAuthor),
        Arc::new(CoachTurnActivity::default()),
    ));
    let game_import_id =
        GameImportId::try_from("game-import:empty-automatic-set".to_string()).unwrap();
    let annotations = empty_annotations(&game_import_id).await;
    let game_import = fixture_import(
        game_import_id,
        fixture("packages/coach-engine-sdk/fixtures/imported-game.json"),
    );
    let session = ProcessorSession::from_review_moments(AssembledSession {
        owner: ProcessorPrincipal::LocalCoach,
        game_import,
        lifetime: ReviewSessionLifetime::new(chrono::Utc::now()),
        checkpoint_revision: 1,
        review_moments: Vec::new(),
        coaching,
        annotations,
        spend: Arc::new(crate::language_layer_ledger::ReviewSessionSpend::new()),
        captures: Arc::new(crate::quality_capture::HostedCaptureBuffer::new()),
    })
    .await
    .unwrap();

    assert!(session.prepared_review_moments().await.is_empty());
    assert!(session
        .review_moment(&CriticalMomentId::try_from("review-moment:empty:1".to_string()).unwrap())
        .await
        .is_none());
}

#[tokio::test]
async fn review_moments_keep_state_isolated_within_one_session() {
    let game: ImportedGame = fixture("packages/coach-engine-sdk/fixtures/imported-game.json");
    let recording: ReviewSessionProviderRecording =
        fixture("packages/shared-assets/fixtures/Synthet1/review-session-provider-recording.json");
    let engine: Arc<dyn EngineAnalyzer> = Arc::new(UnusedEngine);
    let human: Arc<dyn HumanMoveModel> = Arc::new(UnusedHuman);
    let author: Arc<dyn AlternativeMoveAssessmentAuthor> = Arc::new(UnusedCoachAuthor);

    let first_core = core_for(game.clone(), 24, "first");
    let first_id = first_core.review_moment.moment_id.clone();
    let first_facts = host_facts_at_ply(24, "tactical-white-human-likely");
    let game_import_id =
        GameImportId::try_from("game-import:session-state-isolation".to_string()).unwrap();
    let annotations = empty_annotations(&game_import_id).await;
    let game_import = fixture_import(game_import_id, game.clone());
    let session = ProcessorSession::new(
        ProcessorPrincipal::LocalCoach,
        game_import,
        ReviewSessionLifetime::new(chrono::Utc::now()),
        first_core,
        annotations.clone(),
        ProcessorSessionBuildInput {
            recording: Some(&recording),
            engine: engine.clone(),
            human: human.clone(),
            author,
            activity: Arc::new(CoachTurnActivity::default()),
            factual_moment: Some(&first_facts),
        },
    )
    .await
    .unwrap();

    let second_core = core_for(game, 25, "second");
    let second_id = second_core.review_moment.moment_id.clone();
    let second_facts = host_facts_at_ply(25, "positional-black-intermediate");
    let second = Arc::new(
        ProcessorReviewMoment::new(
            second_core.clone(),
            Some(&recording),
            engine,
            human,
            Some(&second_facts),
            annotations,
        )
        .await
        .unwrap(),
    );
    session
        .insert_review_moment(Arc::new(ProcessorReviewMomentEntry::from_prepared(
            second_core,
            None,
            second,
        )))
        .await
        .unwrap();

    let first = session.review_moment(&first_id).await.unwrap();
    let second = session.review_moment(&second_id).await.unwrap();
    assert!(!Arc::ptr_eq(&first.exploration, &second.exploration));
    assert_eq!(first.core_snapshot().await.review_moment.ply, 24);
    assert_eq!(second.core_snapshot().await.review_moment.ply, 25);
    let shared_key = IdempotencyKey::try_from("key:shared-value".to_string()).unwrap();
    assert!(first.claim_idempotency_key(shared_key.clone()).await);
    assert!(second.claim_idempotency_key(shared_key).await);
    assert!(
        !first
            .claim_idempotency_key(
                IdempotencyKey::try_from("key:shared-value".to_string()).unwrap(),
            )
            .await
    );

    let listed = session
        .dispatch_host_capability(
            26,
            &crate::review_session_host::HostCapabilityCall::ListMoments,
        )
        .await
        .unwrap();
    assert_eq!(listed.call_id, "call:listMoments");
    let HostCapabilityEvidence::MomentList { moments } = &listed.evidence else {
        panic!("listMoments returns a moment list");
    };
    assert_eq!(moments.len(), 2);
    assert_eq!(moments[0].ply, 24);
    assert_eq!(moments[1].ply, 25);
    assert_eq!(
        moments[0].classification,
        HostMomentClassification::from(
            &ReviewMomentCommentFacts::try_from_presented_moment(first_facts.moment.clone())
                .unwrap()
        )
    );
    assert_eq!(
        moments[1].classification,
        HostMomentClassification::from(
            &ReviewMomentCommentFacts::try_from_presented_moment(second_facts.moment.clone())
                .unwrap()
        )
    );
    assert_eq!(moments[0].played_san, first_facts.moment.played_san);
    assert_eq!(moments[1].played_san, second_facts.moment.played_san);
}

fn host_facts_at_ply(ply: u16, case_id: &str) -> ImportedCriticalMoment {
    let case = recorded_comment_case(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("evaluation/corpus"),
        case_id,
    )
    .unwrap();
    let mut moment = case.moments[0].facts.moment().clone();
    moment.ply = ply;
    ImportedCriticalMoment {
        moment,
        engine_provenance: None,
        decision_explanation: None,
    }
}

fn core_for(game: ImportedGame, ply: u16, suffix: &str) -> ReviewSessionCoreContract {
    start_review_session(
        RequestId::try_from(format!("request:session-state-isolation:{suffix}")).unwrap(),
        CoachTurnId::try_from(format!("coach-turn:session-state-isolation:{suffix}")).unwrap(),
        game,
        ReviewMomentSelection::PlayerSelectedMoment { ply },
    )
    .unwrap()
}

fn fixture_import(game_import_id: GameImportId, snapshot: ImportedGame) -> Arc<ReviewSessionGame> {
    let events: Vec<ReviewSessionEventEnvelope> =
        fixture("packages/coach-engine-sdk/fixtures/events.json");
    let review = events
        .into_iter()
        .find_map(|event| match event.event {
            ReviewSessionEvent::Completed { result } => match *result {
                OperationCompletion::GameImported { review, .. } => Some(*review),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    let record = GameImportRecord::new(
        game_import_id,
        ProcessorPrincipal::LocalCoach,
        snapshot,
        review,
        Vec::new(),
        None,
        chrono::Utc::now(),
    );
    Arc::new(ReviewSessionGame::from(&record))
}

fn fixture<T: DeserializeOwned>(relative_path: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative_path);
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
