use std::sync::Arc;

use crate::{
    daily_coaching::digested_game_index,
    digested_games::{DigestedGameIndex, NoDigestedGames},
    firestore::FirestoreDatabase,
    game_analysis_store::{game_analysis_store, GameAnalysisStore, InMemoryGameAnalysisStore},
    game_import_store::{game_import_store, GameImportStore, InMemoryGameImportStore},
    imported_games::ImportedGamesRuntime,
    language_layer_ledger::{FirestoreLanguageLayerLedger, LanguageLayerLedger},
    learning_path_feedback::{
        learning_path_feedback_store, InMemoryLearningPathFeedbackStore, LearningPathFeedbackStore,
    },
    lichess::LichessExportClient,
    quality_capture::QualityCaptureAppender,
    review_analysis_cache::{
        review_analysis_cache_store, FirestoreFirstOpenPublication, InMemoryReviewAnalysisCache,
        ReviewAnalysisCacheStore,
    },
    review_annotation_store::{
        review_annotation_store, InMemoryReviewAnnotationStore, ReviewAnnotationStore,
    },
    review_session_contract::{CoachTurnId, GameImportId, RequestId, ReviewMomentSelection},
    review_session_game_identity::ReviewSessionGameIdentity,
    review_session_processor::{ProcessorPrincipal, ReviewSessionProcessor},
    review_share::{review_share_store, InMemoryReviewShareStore, ReviewShareStore},
};

pub(crate) mod path;

pub(crate) const REVIEW_DURABILITY_SCHEMA_VERSION: u8 = 1;
/// Generation 2 settled a mechanism's exchange before crediting it. Generation
/// 3 stops crediting the played move with material its line only reaches later:
/// a review stored at either earlier generation can say the Player won a piece
/// on a move that captured nothing, so the bump makes those unreachable rather
/// than trusting them to age out.
pub(crate) const REVIEW_ANALYSIS_GENERATION: u32 = 3;
#[cfg(test)]
pub(crate) const MAX_REVIEW_DURABILITY_WRITES_PER_COMMIT: usize = 200;

/// One key encoder owns the shared analysis and Player-scoped Game Import
/// addresses. The Player subtree supplies the authorization boundary; the
/// digest supplies review identity.
pub(crate) fn review_key(identity: &ReviewSessionGameIdentity) -> String {
    review_key_at(
        REVIEW_DURABILITY_SCHEMA_VERSION,
        REVIEW_ANALYSIS_GENERATION,
        identity,
    )
}

/// The generation and schema version are inside the digest, which is what makes
/// bumping either of them a cache miss rather than a stale hit. Parameterised so
/// that property is testable without shipping a second live generation.
fn review_key_at(
    schema_version: u8,
    generation: u32,
    identity: &ReviewSessionGameIdentity,
) -> String {
    let mut material = vec![schema_version];
    material.extend_from_slice(&generation.to_be_bytes());
    material.extend_from_slice(identity.as_str().as_bytes());
    path::hashed_path_segment(material)
}

pub(crate) fn game_import_id(
    owner: &ProcessorPrincipal,
    identity: &ReviewSessionGameIdentity,
) -> GameImportId {
    let key = review_key(identity);
    let owner_key = review_session_owner_key(owner);
    GameImportId::try_from(format!("game-import:{key}:{owner_key}"))
        .expect("digest-derived Game Import ID is valid")
}

/// The review key inside a Game Import ID.
///
/// Lives beside [`game_import_id`], which mints the format, so the layout is
/// known in exactly one place.
pub(crate) fn game_import_review_key(game_import_id: &GameImportId) -> Option<&str> {
    let mut parts = game_import_id.as_str().split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("game-import"), Some(key), Some(owner_key), None)
            if is_lower_hex(key, 64) && is_lower_hex(owner_key, 32) =>
        {
            Some(key)
        }
        _ => None,
    }
}

/// Whether a Game Import ID was minted for this owner.
///
/// The owner segment is a digest of the principal, so ownership is decidable
/// from the address alone. A Player minting a Review Share Grant is checked
/// against it, which keeps "you can only share your own review" a property of
/// the identifier algebra rather than a store lookup that could be forgotten.
pub(crate) fn game_import_belongs_to(
    game_import_id: &GameImportId,
    owner: &ProcessorPrincipal,
) -> bool {
    let mut parts = game_import_id.as_str().split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("game-import"), Some(key), Some(owner_key), None) if is_lower_hex(key, 64) => {
            owner_key == review_session_owner_key(owner)
        }
        _ => false,
    }
}

pub(crate) fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// The Coach Turn a Review Moment's coaching is admitted under.
///
/// Derived from the Game Import ID rather than from any session incarnation, so
/// the same Player opening the same Review Moment a week later admits under the
/// same authority instead of minting a fresh one. The owner segment inside the
/// Game Import ID is what keeps two Players on one Game apart.
pub(crate) fn review_moment_coach_turn_id(
    game_import_id: &GameImportId,
    selection: &ReviewMomentSelection,
) -> Option<CoachTurnId> {
    let digest = review_moment_identity_digest(game_import_id, selection)?;
    CoachTurnId::try_from(format!("coach-turn:{digest}")).ok()
}

pub(crate) fn review_moment_request_id(
    game_import_id: &GameImportId,
    selection: &ReviewMomentSelection,
) -> Option<RequestId> {
    let digest = review_moment_identity_digest(game_import_id, selection)?;
    RequestId::try_from(format!("request:review-moment:{digest}")).ok()
}

fn review_moment_identity_digest(
    game_import_id: &GameImportId,
    selection: &ReviewMomentSelection,
) -> Option<String> {
    game_import_review_key(game_import_id)?;
    let identity = match selection {
        ReviewMomentSelection::PipelineCriticalMoment { critical_moment_id } => (
            game_import_id.as_str().to_string(),
            critical_moment_id.as_str().to_string(),
        ),
        ReviewMomentSelection::PlayerSelectedMoment { ply } => (
            game_import_id.as_str().to_string(),
            format!("player-selected:{ply}"),
        ),
    };
    let digest = path::hashed_path_segment(
        serde_json::to_vec(&identity)
            .expect("Review Moment identity has an infallible representation"),
    );
    Some(digest)
}

fn review_session_owner_key(owner: &ProcessorPrincipal) -> String {
    let digest = match owner {
        ProcessorPrincipal::Player(player_id) => path::hashed_path_segment(player_id.as_str()),
        ProcessorPrincipal::LocalCoach => path::hashed_path_segment("local-coach"),
    };
    digest[..32].to_string()
}

/// Owns the complete persistence capability used by a Review Session runtime.
///
/// Production construction is intentionally atomic: a runtime cannot
/// accidentally combine Firestore-backed imports with in-memory analysis or
/// checkpoints. The existing individual store traits remain available as
/// domain test seams while the durable record model is reshaped.
pub(crate) struct ReviewDurability {
    game_imports: Arc<dyn GameImportStore>,
    digested_games: Arc<dyn DigestedGameIndex>,
    review_shares: Arc<dyn ReviewShareStore>,
    game_analysis: Arc<dyn GameAnalysisStore>,
    analysis_cache: Arc<dyn ReviewAnalysisCacheStore>,
    annotations: Arc<dyn ReviewAnnotationStore>,
    learning_path_feedback: Arc<dyn LearningPathFeedbackStore>,
    language_layer_ledger: Option<Arc<dyn LanguageLayerLedger>>,
    first_open_persist: Option<Arc<FirestoreFirstOpenPublication>>,
    quality_capture: QualityCaptureAppender,
}

impl ReviewDurability {
    pub(crate) fn firestore(database: FirestoreDatabase) -> Self {
        let quality_capture = QualityCaptureAppender::for_application(database.clone());
        let game_imports = game_import_store(database.clone(), quality_capture.clone());
        Self {
            game_imports: game_imports.clone(),
            digested_games: digested_game_index(database.clone()),
            review_shares: review_share_store(database.clone()),
            game_analysis: game_analysis_store(database.clone()),
            analysis_cache: review_analysis_cache_store(
                database.clone(),
                game_imports,
                quality_capture.clone(),
            ),
            annotations: review_annotation_store(database.clone()),
            learning_path_feedback: learning_path_feedback_store(database.clone()),
            language_layer_ledger: Some(Arc::new(FirestoreLanguageLayerLedger::new(
                database.clone(),
            ))),
            first_open_persist: Some(Arc::new(FirestoreFirstOpenPublication::new(
                database,
                quality_capture.clone(),
            ))),
            quality_capture,
        }
    }

    pub(crate) fn in_memory() -> Self {
        Self {
            game_imports: Arc::new(InMemoryGameImportStore::default()),
            digested_games: Arc::new(NoDigestedGames),
            review_shares: Arc::new(InMemoryReviewShareStore::default()),
            game_analysis: Arc::new(InMemoryGameAnalysisStore::default()),
            analysis_cache: Arc::new(InMemoryReviewAnalysisCache::default()),
            annotations: Arc::new(InMemoryReviewAnnotationStore::default()),
            learning_path_feedback: Arc::new(InMemoryLearningPathFeedbackStore::default()),
            language_layer_ledger: None,
            first_open_persist: None,
            quality_capture: QualityCaptureAppender::Inert,
        }
    }

    pub(crate) fn attach<C>(self, processor: ReviewSessionProcessor<C>) -> ReviewSessionProcessor<C>
    where
        C: LichessExportClient + 'static,
    {
        let mut processor = processor
            .with_game_import_store(self.game_imports)
            .with_digested_games(self.digested_games)
            .with_review_share_store(self.review_shares)
            .with_game_analysis_store(self.game_analysis)
            .with_review_analysis_cache(self.analysis_cache)
            .with_review_annotation_store(self.annotations)
            .with_learning_path_feedback_store(self.learning_path_feedback);
        if let Some(ledger) = self.language_layer_ledger {
            processor = processor.with_language_layer_ledger(ledger);
        }
        if let Some(persist) = self.first_open_persist {
            processor = processor.with_first_open_persist(persist);
        }
        processor.with_quality_capture_appender(self.quality_capture)
    }

    pub(crate) fn imported_games_runtime(&self) -> ImportedGamesRuntime {
        ImportedGamesRuntime::new(self.game_imports.clone(), self.digested_games.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_session_contract::{EloRating, GameInputSource, PlayerId, ReviewSide};

    fn identity(side: ReviewSide, elo: u16) -> ReviewSessionGameIdentity {
        ReviewSessionGameIdentity::from_request(
            &GameInputSource::LichessUrl {
                url: "https://lichess.org/Synthet1".to_string(),
            },
            side,
            EloRating::try_from(elo).unwrap(),
        )
        .unwrap()
    }

    fn player(id: &str) -> ProcessorPrincipal {
        ProcessorPrincipal::Player(PlayerId::try_from(id.to_string()).unwrap())
    }

    #[test]
    fn one_review_key_drives_analysis_and_player_scoped_import_addresses() {
        let review_identity = identity(ReviewSide::Black, 1450);
        let first = game_import_id(&player("firebase-player-a"), &review_identity);
        let second = game_import_id(&player("firebase-player-b"), &review_identity);
        let shared_key = review_key(&review_identity);
        let white_key = review_key(&identity(ReviewSide::White, 1450));
        let stronger_key = review_key(&identity(ReviewSide::Black, 1500));

        assert_ne!(first, second);
        assert!(first.as_str().contains(&shared_key));
        assert!(!first.as_str().contains(&white_key));
        assert!(!first.as_str().contains(&stronger_key));
    }

    #[test]
    fn a_generation_or_schema_bump_moves_the_analysis_cache_address() {
        let review_identity = identity(ReviewSide::Black, 1450);
        let current = review_key(&review_identity);

        assert_eq!(
            current,
            review_key_at(
                REVIEW_DURABILITY_SCHEMA_VERSION,
                REVIEW_ANALYSIS_GENERATION,
                &review_identity
            ),
            "the live key must be the parameterised encoder at the live constants"
        );
        assert_ne!(
            current,
            review_key_at(
                REVIEW_DURABILITY_SCHEMA_VERSION,
                REVIEW_ANALYSIS_GENERATION + 1,
                &review_identity
            ),
            "analysis computed at an older generation must not be reachable"
        );
        assert_ne!(
            current,
            review_key_at(
                REVIEW_DURABILITY_SCHEMA_VERSION + 1,
                REVIEW_ANALYSIS_GENERATION,
                &review_identity
            ),
        );
    }

    #[test]
    fn public_contracts_stay_pinned_across_internal_durability_cuts() {
        use crate::{
            daily_coaching::{RUN_SCHEMA_VERSION, STATE_SCHEMA_VERSION},
            local_runtime::RUNTIME_MANIFEST_SCHEMA_VERSION,
            review_session_contract::{
                ChessKnowledgeGraphVersion, DecisionExplanationGeneration,
                GameReviewTeachingVocabularyVersion, LearningPlanSelectionPolicyVersion,
                CHESS_KNOWLEDGE_GRAPH_VERSION, DECISION_EXPLANATION_GENERATION,
                LEARNING_PLAN_SELECTION_POLICY_VERSION,
            },
        };

        assert_eq!(REVIEW_DURABILITY_SCHEMA_VERSION, 1);
        assert_eq!(REVIEW_ANALYSIS_GENERATION, 3);
        assert_eq!(STATE_SCHEMA_VERSION, 1);
        assert_eq!(RUN_SCHEMA_VERSION, 1);
        assert_eq!(RUNTIME_MANIFEST_SCHEMA_VERSION, 1);
        assert_eq!(
            DECISION_EXPLANATION_GENERATION,
            DecisionExplanationGeneration::V1
        );
        assert_eq!(
            CHESS_KNOWLEDGE_GRAPH_VERSION,
            ChessKnowledgeGraphVersion::V1
        );
        assert_eq!(
            LEARNING_PLAN_SELECTION_POLICY_VERSION,
            LearningPlanSelectionPolicyVersion::V1
        );
        assert_eq!(
            serde_json::to_value(GameReviewTeachingVocabularyVersion::V1).unwrap(),
            "teaching-facts/v1"
        );
    }
}
