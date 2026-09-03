use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    engine_analysis::EngineProvenance,
    imported_games::ImportedGameCard,
    operating_limits::IMPORTED_REVIEW_FACTS_CAPACITY,
    review_session_contract::{
        DecisionExplanation, GameImportId, GameReview, GameReviewCriticalMoment, ImportedGame,
    },
    review_session_processor::ProcessorPrincipal,
};

mod firestore;

pub(crate) use firestore::game_import_store;

pub(crate) const GAME_IMPORT_SCHEMA_VERSION: u8 = 1;

pub type GameImportStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, GameImportStoreError>> + Send + 'a>>;

pub trait GameImportStore: Send + Sync {
    fn create<'a>(&'a self, record: GameImportRecord) -> GameImportStoreFuture<'a, ()>;

    fn create_with_imported_game_card<'a>(
        &'a self,
        _record: GameImportRecord,
        _card: ImportedGameCard,
    ) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async {
            Err(GameImportStoreError::Configuration(
                "Game Import store does not support Imported Game cards".to_string(),
            ))
        })
    }

    fn upsert_imported_game_card<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
        _card: ImportedGameCard,
    ) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async {
            Err(GameImportStoreError::Configuration(
                "Game Import store does not support Imported Game cards".to_string(),
            ))
        })
    }

    /// Removes one Game from this Player's records, card and all.
    ///
    /// Deleting what is already gone succeeds: a retried delete is not an
    /// error, and the Player has the outcome they asked for either way.
    fn delete_imported_game<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
        _deleted: DeletedImportedGame,
    ) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async {
            Err(GameImportStoreError::Configuration(
                "Game Import store does not support deletion".to_string(),
            ))
        })
    }

    fn list_imported_game_cards<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<ImportedGameCard>> {
        Box::pin(async {
            Err(GameImportStoreError::Configuration(
                "Game Import store does not support Imported Game cards".to_string(),
            ))
        })
    }

    fn list_game_import_records<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<GameImportRecord>>;

    fn find<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        game_import_id: &'a GameImportId,
    ) -> GameImportStoreFuture<'a, GameImportLookup>;

    fn retain_for_review_session<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup>;

    fn resolve_review_session_reference<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameImportRecord {
    pub schema_version: u8,
    pub game_import_id: GameImportId,
    pub owner: ProcessorPrincipal,
    pub created_at: DateTime<Utc>,
    pub imported_game: ImportedGame,
    pub frozen_review: GameReview,
    pub player_selected_moments: BTreeMap<u16, GameReviewCriticalMoment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_provenance: Option<EngineProvenance>,
}

/// One Game's removal, resolved before anything is written.
///
/// A Game is one Imported Game card and every Game Import behind it: the same
/// Game reviewed from the same side at more than one Elo Profile is more than
/// one Game Import, and a delete that took only the one the card points at
/// would orphan the rest. The set is resolved by the caller, which is the only
/// place that can see the Player's whole shelf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedImportedGame {
    pub game_import_ids: Vec<GameImportId>,
    pub imported_game_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameImportReference {
    pub game_import_id: GameImportId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionGame {
    pub(crate) source_game_import_id: GameImportId,
    pub(crate) source_reference: GameImportReference,
    pub(crate) imported_game: ImportedGame,
    pub(crate) frozen_review: GameReview,
    pub(crate) player_selected_moments: BTreeMap<u16, GameReviewCriticalMoment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) engine_provenance: Option<EngineProvenance>,
}

impl From<&GameImportRecord> for ReviewSessionGame {
    fn from(record: &GameImportRecord) -> Self {
        Self {
            source_game_import_id: record.game_import_id.clone(),
            source_reference: record.reference(),
            imported_game: record.imported_game.clone(),
            frozen_review: record.frozen_review.clone(),
            player_selected_moments: record.player_selected_moments.clone(),
            engine_provenance: record.engine_provenance.clone(),
        }
    }
}

impl ReviewSessionGame {
    pub(crate) fn review(&self) -> &GameReview {
        &self.frozen_review
    }

    pub(crate) fn player_selected_moment(&self, ply: u16) -> Option<ImportedCriticalMoment> {
        self.player_selected_moments
            .get(&ply)
            .cloned()
            .map(|moment| ImportedCriticalMoment {
                moment,
                engine_provenance: self.engine_provenance.clone(),
                decision_explanation: None,
            })
    }

    pub(crate) fn automatic_critical_moments(&self) -> Vec<ImportedCriticalMoment> {
        automatic_critical_moments(&self.frozen_review, self.engine_provenance.as_ref())
    }

    /// Resolves a Critical Moment by identity rather than by how it was chosen.
    ///
    /// A Critical Moment ID is a function of the Game and the ply, so the same
    /// moment carries the same ID whether the pipeline surfaced it or the
    /// Player asked for it. Restoring a stored Review Moment needs the frozen
    /// facts at that ID and nothing more; asking instead whether the moment is
    /// still in the automatic set makes every read depend on the analysis
    /// agreeing with the one that wrote the checkpoint.
    pub(crate) fn critical_moment(
        &self,
        critical_moment_id: &crate::review_session_contract::CriticalMomentId,
    ) -> Option<GameReviewCriticalMoment> {
        self.frozen_review
            .critical_moments
            .iter()
            .find(|moment| moment.critical_moment_id == *critical_moment_id)
            .cloned()
    }
}

impl GameImportRecord {
    pub fn new(
        game_import_id: GameImportId,
        owner: ProcessorPrincipal,
        imported_game: ImportedGame,
        frozen_review: GameReview,
        player_selected_moments: Vec<GameReviewCriticalMoment>,
        engine_provenance: Option<EngineProvenance>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: GAME_IMPORT_SCHEMA_VERSION,
            game_import_id,
            owner,
            created_at,
            imported_game,
            frozen_review,
            player_selected_moments: player_selected_moments
                .into_iter()
                .map(|moment| (moment.ply, moment))
                .collect(),
            engine_provenance,
        }
    }

    pub fn review(&self) -> &GameReview {
        &self.frozen_review
    }

    pub fn automatic_critical_moments(&self) -> Vec<ImportedCriticalMoment> {
        automatic_critical_moments(&self.frozen_review, self.engine_provenance.as_ref())
    }

    pub fn player_selected_moment(&self, ply: u16) -> Option<ImportedCriticalMoment> {
        self.player_selected_moments
            .get(&ply)
            .cloned()
            .map(|moment| ImportedCriticalMoment {
                moment,
                engine_provenance: self.engine_provenance.clone(),
                decision_explanation: None,
            })
    }

    pub fn reference(&self) -> GameImportReference {
        GameImportReference {
            game_import_id: self.game_import_id.clone(),
        }
    }

    fn has_valid_shape(&self) -> bool {
        self.schema_version == GAME_IMPORT_SCHEMA_VERSION
    }

    fn matches_reference(&self, reference: &GameImportReference) -> bool {
        self.has_valid_shape() && self.game_import_id == reference.game_import_id
    }
}

fn automatic_critical_moments(
    review: &GameReview,
    engine_provenance: Option<&EngineProvenance>,
) -> Vec<ImportedCriticalMoment> {
    let mut moments = review
        .critical_moments
        .iter()
        .filter(|moment| {
            matches!(
                moment.provenance,
                crate::review_session_contract::GameReviewMomentProvenance::Automatic
            )
        })
        .cloned()
        .map(|moment| ImportedCriticalMoment {
            decision_explanation: moment.decision_explanation.clone(),
            moment,
            engine_provenance: engine_provenance.cloned(),
        })
        .collect::<Vec<_>>();
    moments.sort_by_key(|moment| moment.moment.ply);
    moments
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedCriticalMoment {
    pub moment: GameReviewCriticalMoment,
    pub engine_provenance: Option<EngineProvenance>,
    pub decision_explanation: Option<DecisionExplanation>,
}

pub enum GameImportLookup {
    Found(Box<GameImportRecord>),
    NotFound,
    OwnerMismatch,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GameImportReferenceLookup {
    Found(Box<GameImportRecord>),
    NotFound,
    OwnerMismatch,
}

#[derive(Default)]
struct InMemoryGameImports {
    records: BTreeMap<GameImportId, GameImportRecord>,
    insertion_order: VecDeque<GameImportId>,
    imported_game_cards: BTreeMap<String, ImportedGameCard>,
}

impl InMemoryGameImports {
    fn insert(
        &mut self,
        record: GameImportRecord,
        card: Option<ImportedGameCard>,
    ) -> Result<(), GameImportStoreError> {
        if self.records.contains_key(&record.game_import_id) {
            return Err(GameImportStoreError::Conflict);
        }
        let inserted_id = record.game_import_id.clone();
        self.insertion_order.push_back(inserted_id.clone());
        self.records.insert(inserted_id.clone(), record);
        if let Some(card) = card {
            self.imported_game_cards
                .insert(card.imported_game_key.clone(), card);
        }
        while self.records.len() > IMPORTED_REVIEW_FACTS_CAPACITY {
            let Some(index) = self
                .insertion_order
                .iter()
                .position(|id| id != &inserted_id && self.records.contains_key(id))
            else {
                break;
            };
            let evicted = self
                .insertion_order
                .remove(index)
                .expect("the selected insertion-order entry exists");
            self.records.remove(&evicted);
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryGameImportStore {
    state: Mutex<InMemoryGameImports>,
}

impl GameImportStore for InMemoryGameImportStore {
    fn create<'a>(&'a self, record: GameImportRecord) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async move { self.state.lock().await.insert(record, None) })
    }

    fn create_with_imported_game_card<'a>(
        &'a self,
        record: GameImportRecord,
        card: ImportedGameCard,
    ) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async move {
            if !card.is_valid() || record.game_import_id != card.game_import_id {
                return Err(GameImportStoreError::InvalidRecord);
            }
            self.state.lock().await.insert(record, Some(card))
        })
    }

    fn upsert_imported_game_card<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
        card: ImportedGameCard,
    ) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async move {
            if !card.is_valid() {
                return Err(GameImportStoreError::InvalidRecord);
            }
            self.state
                .lock()
                .await
                .imported_game_cards
                .insert(card.imported_game_key.clone(), card);
            Ok(())
        })
    }

    fn delete_imported_game<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        deleted: DeletedImportedGame,
    ) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let mut removed_an_owned_record = false;
            for game_import_id in &deleted.game_import_ids {
                if state
                    .records
                    .get(game_import_id)
                    .is_some_and(|record| &record.owner == owner)
                {
                    state.records.remove(game_import_id);
                    state.insertion_order.retain(|id| id != game_import_id);
                    removed_an_owned_record = true;
                }
            }
            /* The card key is the Game and the reviewed side, so two Players
            who imported the same Game share it. Firestore keys cards under the
            owner's subtree; this map does not, so the owner's own record is
            what says the card is theirs to remove. */
            if removed_an_owned_record {
                state.imported_game_cards.remove(&deleted.imported_game_key);
            }
            Ok(())
        })
    }

    fn list_imported_game_cards<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<ImportedGameCard>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            Ok(state
                .imported_game_cards
                .values()
                .filter(|card| {
                    card.is_valid()
                        && state
                            .records
                            .get(&card.game_import_id)
                            .is_some_and(|record| &record.owner == owner)
                })
                .cloned()
                .collect())
        })
    }

    fn list_game_import_records<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<GameImportRecord>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            Ok(state
                .records
                .values()
                .filter(|record| &record.owner == owner && record.has_valid_shape())
                .cloned()
                .collect())
        })
    }

    fn find<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        game_import_id: &'a GameImportId,
    ) -> GameImportStoreFuture<'a, GameImportLookup> {
        Box::pin(async move {
            let record = self.state.lock().await.records.get(game_import_id).cloned();
            Ok(match record {
                None => GameImportLookup::NotFound,
                Some(record) if &record.owner != owner => GameImportLookup::OwnerMismatch,
                Some(record) if !record.has_valid_shape() => {
                    return Err(GameImportStoreError::InvalidRecord);
                }
                Some(record) => GameImportLookup::Found(Box::new(record)),
            })
        })
    }

    fn retain_for_review_session<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let Some(record) = state.records.get(&reference.game_import_id) else {
                return Ok(GameImportReferenceLookup::NotFound);
            };
            if &record.owner != owner {
                return Ok(GameImportReferenceLookup::OwnerMismatch);
            }
            if !record.matches_reference(reference) {
                return Err(GameImportStoreError::InvalidRecord);
            }
            Ok(GameImportReferenceLookup::Found(Box::new(record.clone())))
        })
    }

    fn resolve_review_session_reference<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let Some(record) = state.records.get(&reference.game_import_id) else {
                return Ok(GameImportReferenceLookup::NotFound);
            };
            if &record.owner != owner {
                return Ok(GameImportReferenceLookup::OwnerMismatch);
            }
            if !record.matches_reference(reference) {
                return Err(GameImportStoreError::InvalidRecord);
            }
            Ok(GameImportReferenceLookup::Found(Box::new(record.clone())))
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GameImportStoreError {
    #[error("Game Import persistence is misconfigured: {0}")]
    Configuration(String),
    #[error("Game Import persistence transport failed")]
    Transport,
    #[error("Game Import persistence is unavailable")]
    Unavailable,
    #[error("Game Import identity already exists")]
    Conflict,
    #[error("Game Import persistence returned an invalid record")]
    InvalidRecord,
}

impl GameImportStoreError {
    pub(crate) fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::Configuration(_) => "configuration",
            Self::Transport => "transport",
            Self::Unavailable => "unavailable",
            Self::Conflict => "conflict",
            Self::InvalidRecord => "invalid-record",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_session_contract::{
        OperationCompletion, PlayerId, ReviewSessionEvent, ReviewSessionEventEnvelope,
    };

    #[tokio::test]
    async fn in_memory_records_are_player_owned_durable_and_stay_bounded() {
        let store = InMemoryGameImportStore::default();
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let record = fixture_record(0, created_at);
        let id = record.game_import_id.clone();
        let owner = record.owner.clone();
        store.create(record).await.unwrap();

        assert!(matches!(
            store.find(&owner, &id).await.unwrap(),
            GameImportLookup::Found(_)
        ));
        assert!(matches!(
            store
                .find(
                    &ProcessorPrincipal::Player(
                        PlayerId::try_from("firebase-player-b".to_string()).unwrap()
                    ),
                    &id,
                )
                .await
                .unwrap(),
            GameImportLookup::OwnerMismatch
        ));
        for index in 1..=IMPORTED_REVIEW_FACTS_CAPACITY {
            store
                .create(fixture_record(index, created_at))
                .await
                .unwrap();
        }
        assert!(matches!(
            store.find(&owner, &id).await.unwrap(),
            GameImportLookup::NotFound
        ));
    }

    #[tokio::test]
    async fn in_memory_lists_player_owned_game_import_records() {
        let store = InMemoryGameImportStore::default();
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let record = fixture_record(0, created_at);
        let owner = record.owner.clone();
        store.create(record.clone()).await.unwrap();

        assert_eq!(
            store.list_game_import_records(&owner).await.unwrap(),
            vec![record]
        );
        let other = ProcessorPrincipal::Player(
            PlayerId::try_from("firebase-player-b".to_string()).unwrap(),
        );
        assert!(store
            .list_game_import_records(&other)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn unsupported_generation_imports_are_unavailable_instead_of_translated() {
        let store = InMemoryGameImportStore::default();
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let mut stale = fixture_record(0, created_at);
        stale.schema_version = 0;
        let id = stale.game_import_id.clone();
        let owner = stale.owner.clone();
        store.state.lock().await.records.insert(id.clone(), stale);

        assert!(matches!(
            store.find(&owner, &id).await,
            Err(GameImportStoreError::InvalidRecord)
        ));
    }

    #[tokio::test]
    async fn review_session_reference_remains_player_owned_and_fails_closed() {
        let store = InMemoryGameImportStore::default();
        let created_at = "2026-07-26T10:00:00Z".parse().unwrap();
        let record = fixture_record(0, created_at);
        let reference = record.reference();
        let owner = record.owner.clone();
        store.create(record).await.unwrap();

        let retained = store
            .retain_for_review_session(&owner, &reference)
            .await
            .unwrap();
        assert!(matches!(retained, GameImportReferenceLookup::Found(_)));
        assert!(matches!(
            store.find(&owner, &reference.game_import_id).await.unwrap(),
            GameImportLookup::Found(_)
        ));
        assert!(matches!(
            store
                .resolve_review_session_reference(&owner, &reference)
                .await
                .unwrap(),
            GameImportReferenceLookup::Found(_)
        ));

        let other = ProcessorPrincipal::Player(
            PlayerId::try_from("firebase-player-b".to_string()).unwrap(),
        );
        assert!(matches!(
            store
                .resolve_review_session_reference(&other, &reference)
                .await
                .unwrap(),
            GameImportReferenceLookup::OwnerMismatch
        ));

        store
            .state
            .lock()
            .await
            .records
            .remove(&reference.game_import_id);
        assert!(matches!(
            store
                .resolve_review_session_reference(&owner, &reference)
                .await
                .unwrap(),
            GameImportReferenceLookup::NotFound
        ));
    }

    fn fixture_record(index: usize, created_at: DateTime<Utc>) -> GameImportRecord {
        let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/events.json"
        )))
        .unwrap();
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
        let snapshot = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
        )))
        .unwrap();
        GameImportRecord::new(
            GameImportId::try_from(format!("game-import:fixture:{index}")).unwrap(),
            ProcessorPrincipal::Player(
                PlayerId::try_from("firebase-player-a".to_string()).unwrap(),
            ),
            snapshot,
            review,
            Vec::new(),
            None,
            created_at,
        )
    }
}
