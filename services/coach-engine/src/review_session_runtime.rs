use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use crate::{
    daily_coaching::{
        configured_daily_coaching_runtime, DailyCoachingRuntime, DailyGameReviewExecutor,
    },
    engine_analysis::{EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer},
    firestore::FirestoreDatabase,
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction},
    imported_games::ImportedGamesRuntime,
    lichess::ReqwestLichessExportClient,
    pin_record::HostedLanguageLayerBinding,
    review_durability::ReviewDurability,
    review_session_coaching::{AlternativeMoveAssessmentAuthor, CoachTurnAuthorInput},
    review_session_contract::{AlternativeMoveAssessment, ProviderUnavailableReason},
    review_session_processor::ReviewSessionProcessor,
    review_session_transport::ReviewSessionCommandExecutor,
};

#[cfg(test)]
use crate::{lichess::LichessExportClient, review_session_processor::PlayerTrafficPolicy};

pub struct ReviewSessionExecutors {
    command: Arc<dyn ReviewSessionCommandExecutor>,
    daily_coaching: Arc<dyn DailyGameReviewExecutor>,
    imported_games: ImportedGamesRuntime,
}

impl ReviewSessionExecutors {
    pub fn command(&self) -> Arc<dyn ReviewSessionCommandExecutor> {
        self.command.clone()
    }

    pub fn daily_coaching_runtime(&self) -> anyhow::Result<DailyCoachingRuntime> {
        configured_daily_coaching_runtime(self.daily_coaching.clone())
    }

    pub fn imported_games_runtime(&self) -> ImportedGamesRuntime {
        self.imported_games.clone()
    }
}

pub fn build_review_session_executors(
    engine: Option<Arc<dyn EngineAnalyzer>>,
    human: Option<Arc<dyn HumanMoveModel>>,
    hosted: HostedLanguageLayerBinding,
) -> anyhow::Result<ReviewSessionExecutors> {
    let database = FirestoreDatabase::from_env()?;
    build_review_session_executors_with_startup(
        engine,
        human,
        ReviewDurability::firestore(database),
        None,
        Some(hosted),
    )
}

pub fn build_review_session_executor(
    engine: Option<Arc<dyn EngineAnalyzer>>,
    human: Option<Arc<dyn HumanMoveModel>>,
) -> anyhow::Result<Arc<dyn ReviewSessionCommandExecutor>> {
    let database = FirestoreDatabase::from_env()?;
    Ok(build_review_session_executors_with_startup(
        engine,
        human,
        ReviewDurability::firestore(database),
        None,
        None,
    )?
    .command())
}

pub fn build_review_session_executor_with_runtime_startup(
    engine: Option<Arc<dyn EngineAnalyzer>>,
    human: Option<Arc<dyn HumanMoveModel>>,
    runtime_startup: Duration,
) -> anyhow::Result<Arc<dyn ReviewSessionCommandExecutor>> {
    Ok(build_review_session_executors_with_startup(
        engine,
        human,
        ReviewDurability::in_memory(),
        Some(runtime_startup),
        None,
    )?
    .command())
}

#[cfg(test)]
pub(crate) fn build_review_session_executor_with_providers(
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
) -> anyhow::Result<Arc<dyn ReviewSessionCommandExecutor>> {
    Ok(build_review_session_executors_with_startup(
        Some(engine),
        Some(human),
        ReviewDurability::in_memory(),
        None,
        None,
    )?
    .command())
}

#[cfg(test)]
pub(crate) fn build_review_session_executor_with_traffic(
    lichess: impl LichessExportClient + 'static,
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
    traffic: Arc<PlayerTrafficPolicy>,
) -> Arc<dyn ReviewSessionCommandExecutor> {
    Arc::new(
        ReviewSessionProcessor::new_live_with_authors(
            lichess,
            engine,
            human,
            Arc::new(NoHostedLanguageLayer),
        )
        .with_player_traffic(traffic),
    )
}

fn build_review_session_executors_with_startup(
    engine: Option<Arc<dyn EngineAnalyzer>>,
    human: Option<Arc<dyn HumanMoveModel>>,
    durability: ReviewDurability,
    runtime_startup: Option<Duration>,
    hosted: Option<HostedLanguageLayerBinding>,
) -> anyhow::Result<ReviewSessionExecutors> {
    let lichess = ReqwestLichessExportClient::new()?;
    let mut processor = ReviewSessionProcessor::new_live_with_authors(
        lichess,
        engine.unwrap_or_else(|| Arc::new(UnavailableEngine)),
        human.unwrap_or_else(|| Arc::new(UnavailableHuman)),
        Arc::new(NoHostedLanguageLayer),
    );
    let imported_games = durability.imported_games_runtime();
    processor = durability.attach(processor);
    if let Some(duration) = runtime_startup {
        processor = processor.with_runtime_startup(duration);
    }
    if let Some(binding) = hosted {
        processor = processor
            .with_hosted_language_layer(binding)
            .with_eager_web_artifacts();
    }
    let processor = Arc::new(processor);
    Ok(ReviewSessionExecutors {
        command: processor.clone(),
        daily_coaching: processor,
        imported_games,
    })
}

struct UnavailableEngine;

impl EngineAnalyzer for UnavailableEngine {
    fn analyze<'a>(
        &'a self,
        _input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async {
            Err(EngineAnalysisError::Protocol(
                "Stockfish is not configured".to_string(),
            ))
        })
    }
}

struct UnavailableHuman;

impl HumanMoveModel for UnavailableHuman {
    fn predict<'a>(
        &'a self,
        _input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        Box::pin(async {
            Err(HumanMoveModelError::InvalidInput(
                "Maia is not configured".to_string(),
            ))
        })
    }
}

struct NoHostedLanguageLayer;

impl AlternativeMoveAssessmentAuthor for NoHostedLanguageLayer {
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
