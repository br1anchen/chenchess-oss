use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use chrono::Utc;

use crate::{
    firestore::{FirestoreDatabase, FirestoreWrite},
    review_session_contract::PlayerId,
    review_session_processor::ProcessorPrincipal,
};

mod firestore;
mod hosted;
mod model;

pub use hosted::HostedCaptureBuffer;
pub(crate) use hosted::{hosted_language_layer_capture, HostedGenerationInput};
pub use model::ReviewFeedbackReason;
pub(crate) use model::{HostedLanguageLayerTask, QualityCaptureDraft, RecordedProseRejection};

pub(crate) use firestore::FirestoreQualityCaptureStore;

const CURRENT_DISCLOSURE_VERSION: u16 = 1;

type StoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, QualityCaptureStoreError>> + Send + 'a>>;

pub trait QualityCapturePreferenceStore: Send + Sync {
    fn preference<'a>(&'a self, player_id: &'a PlayerId) -> StoreFuture<'a, RetentionPreference>;

    fn set_preference<'a>(
        &'a self,
        player_id: &'a PlayerId,
        enabled: bool,
    ) -> StoreFuture<'a, RetentionPreference>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetentionPreference {
    pub available: bool,
    pub enabled: bool,
    pub disclosure_required: bool,
    pub deleted_review_snapshots: usize,
}

impl RetentionPreference {
    fn unavailable() -> Self {
        Self {
            available: false,
            enabled: false,
            disclosure_required: false,
            deleted_review_snapshots: 0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QualityCaptureStoreError {
    #[error("quality capture persistence is misconfigured: {0}")]
    Configuration(String),
    #[error("quality capture persistence transport failed")]
    Transport,
    #[error("quality capture persistence is unavailable")]
    Unavailable,
    #[error("quality capture persistence conflicted")]
    Conflict,
    #[error("quality capture persistence returned an invalid record")]
    InvalidRecord,
}

pub struct NoQualityCaptureStore;

impl QualityCapturePreferenceStore for NoQualityCaptureStore {
    fn preference<'a>(&'a self, _player_id: &'a PlayerId) -> StoreFuture<'a, RetentionPreference> {
        Box::pin(async { Ok(RetentionPreference::unavailable()) })
    }

    fn set_preference<'a>(
        &'a self,
        _player_id: &'a PlayerId,
        _enabled: bool,
    ) -> StoreFuture<'a, RetentionPreference> {
        Box::pin(async { Ok(RetentionPreference::unavailable()) })
    }
}

#[derive(Default)]
struct InMemoryQualityState {
    preferences: BTreeMap<PlayerId, bool>,
    outboxes: BTreeMap<PlayerId, Vec<QualityCaptureDraft>>,
    held: BTreeMap<PlayerId, Vec<QualityCaptureDraft>>,
}

#[derive(Default)]
pub struct InMemoryQualityCaptureStore {
    state: Mutex<InMemoryQualityState>,
}

impl InMemoryQualityCaptureStore {
    fn enqueue(&self, player_id: &PlayerId, drafts: &[QualityCaptureDraft]) {
        let mut state = self
            .state
            .lock()
            .expect("in-memory quality capture state is not poisoned");
        let preference = preference_for(state.preferences.get(player_id).copied(), 0);
        for draft in drafts {
            if hosted::writes_without_preference(draft)
                || hosted::preference_allows_outbox(&preference)
            {
                state
                    .outboxes
                    .entry(player_id.clone())
                    .or_default()
                    .push(draft.clone());
            } else if hosted::holds_when_preference_off(draft) {
                state
                    .held
                    .entry(player_id.clone())
                    .or_default()
                    .push(draft.clone());
            }
        }
    }

    fn take_held(&self, player_id: &PlayerId) -> Option<QualityCaptureDraft> {
        self.state
            .lock()
            .expect("in-memory quality capture state is not poisoned")
            .held
            .get_mut(player_id)
            .and_then(Vec::pop)
    }

    /// The Player's most recent exported generation, as the join a Review
    /// Feedback Report is written against.
    fn feedback_anchor(&self, player_id: &PlayerId) -> Option<model::FeedbackAnchor> {
        self.recorded_outbox(player_id)
            .iter()
            .rev()
            .find_map(QualityCaptureDraft::feedback_anchor)
    }

    pub(crate) fn recorded_outbox(&self, player_id: &PlayerId) -> Vec<QualityCaptureDraft> {
        self.state
            .lock()
            .expect("in-memory quality capture state is not poisoned")
            .outboxes
            .get(player_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn language_layer_fingerprint_digests(&self, player_id: &PlayerId) -> Vec<String> {
        self.recorded_outbox(player_id)
            .iter()
            .filter_map(|draft| match &draft.content {
                model::QualityCaptureContent::LanguageLayerGeneration { fingerprint, .. } => {
                    Some(fingerprint.digest.as_str().to_string())
                }
                _ => None,
            })
            .collect()
    }

    /// Capture Outcomes of the Player's hosted Language Layer generations.
    ///
    /// A comment that the Grounding Gate rejected still exports a capture, and
    /// the Player still reads a Safe Review Moment Rendering. Only the outcome
    /// separates the two, so certification reads it rather than inferring
    /// success from the presence of a capture.
    pub fn language_layer_capture_outcomes(
        &self,
        player_id: &PlayerId,
    ) -> Vec<crate::evaluation_fingerprint::CaptureOutcome> {
        self.recorded_outbox(player_id)
            .iter()
            .filter_map(|draft| match &draft.content {
                model::QualityCaptureContent::LanguageLayerGeneration { observations, .. } => {
                    Some(observations.capture_outcome)
                }
                _ => None,
            })
            .collect()
    }

    pub fn preference_generation_count(&self, player_id: &PlayerId) -> usize {
        self.language_layer_trigger_count(
            player_id,
            crate::evaluation_fingerprint::CaptureTrigger::Preference,
        )
    }

    pub fn feedback_induced_generation_count(&self, player_id: &PlayerId) -> usize {
        self.language_layer_trigger_count(
            player_id,
            crate::evaluation_fingerprint::CaptureTrigger::FeedbackInduced,
        )
    }

    fn language_layer_trigger_count(
        &self,
        player_id: &PlayerId,
        trigger: crate::evaluation_fingerprint::CaptureTrigger,
    ) -> usize {
        self.recorded_outbox(player_id)
            .iter()
            .filter(|draft| {
                matches!(
                    &draft.content,
                    model::QualityCaptureContent::LanguageLayerGeneration { observations, .. }
                        if observations.capture_trigger == trigger
                )
            })
            .count()
    }

    pub fn language_layer_surfaces_are_web(&self, player_id: &PlayerId) -> bool {
        use crate::review_session_contract::DeliverySurface;

        let mut saw_generation = false;
        for draft in self.recorded_outbox(player_id) {
            if let model::QualityCaptureContent::LanguageLayerGeneration { fingerprint, .. } =
                draft.content
            {
                saw_generation = true;
                if fingerprint.axes.delivery_surface != DeliverySurface::Web {
                    return false;
                }
            }
        }
        saw_generation
    }

    pub fn feedback_fingerprint_digests(&self, player_id: &PlayerId) -> Vec<String> {
        self.recorded_outbox(player_id)
            .iter()
            .filter_map(|draft| match &draft.content {
                model::QualityCaptureContent::FeedbackAnnotation {
                    fingerprint_digest, ..
                } => Some(fingerprint_digest.as_str().to_string()),
                _ => None,
            })
            .collect()
    }
}

impl QualityCapturePreferenceStore for InMemoryQualityCaptureStore {
    fn preference<'a>(&'a self, player_id: &'a PlayerId) -> StoreFuture<'a, RetentionPreference> {
        Box::pin(async move {
            let state = self
                .state
                .lock()
                .expect("in-memory quality capture state is not poisoned");
            Ok(preference_for(state.preferences.get(player_id).copied(), 0))
        })
    }

    fn set_preference<'a>(
        &'a self,
        player_id: &'a PlayerId,
        enabled: bool,
    ) -> StoreFuture<'a, RetentionPreference> {
        Box::pin(async move {
            let mut state = self
                .state
                .lock()
                .expect("in-memory quality capture state is not poisoned");
            state.preferences.insert(player_id.clone(), enabled);
            Ok(preference_for(Some(enabled), 0))
        })
    }
}

fn preference_for(enabled: Option<bool>, deleted_review_snapshots: usize) -> RetentionPreference {
    RetentionPreference {
        available: true,
        enabled: enabled.unwrap_or(true),
        disclosure_required: enabled.is_none(),
        deleted_review_snapshots,
    }
}

#[derive(Clone)]
pub(crate) enum QualityCaptureAppender {
    Inert,
    Live(FirestoreDatabase),
    /// In-process store used by certification and tests. Production Firestore
    /// uses [`Self::Live`] or [`Self::Inert`].
    Memory(Arc<InMemoryQualityCaptureStore>),
}

impl QualityCaptureAppender {
    pub(crate) fn for_application(database: FirestoreDatabase) -> Self {
        if database.is_application() {
            Self::Live(database)
        } else {
            Self::Inert
        }
    }

    pub(crate) fn memory(store: Arc<InMemoryQualityCaptureStore>) -> Self {
        Self::Memory(store)
    }

    /// Certification and tests persist Memory captures after the in-process
    /// cache write succeeds. Firestore already writes the outbox in the same
    /// product-DB transaction as `replace_moment`.
    pub(crate) async fn commit_in_process_after_persist(
        &self,
        owner: &ProcessorPrincipal,
        captures: &[QualityCaptureDraft],
    ) {
        let (Self::Memory(store), ProcessorPrincipal::Player(player_id)) = (self, owner) else {
            return;
        };
        store.enqueue(player_id, captures);
    }

    /// Submit induces through this persist seam. Persist errors never fail the
    /// business command.
    ///
    /// Which generation the feedback lands on follows the preference, because
    /// that is what decided where the generation went. Consented ones exported
    /// and are anchored by their outbox row; withdrawn ones are held, and the
    /// held one is induced. Taking the held generation on the consented path
    /// would drop it for an annotation that never used it.
    pub(crate) async fn record_feedback(
        &self,
        owner: &ProcessorPrincipal,
        preference: &RetentionPreference,
        reason_codes: Vec<ReviewFeedbackReason>,
    ) {
        let exports = hosted::preference_allows_outbox(preference);
        let (exported, held) = if exports {
            (self.feedback_anchor(owner).await, None)
        } else {
            (None, self.take_held(owner).await)
        };
        let drafts = match hosted::induce_feedback(
            exported,
            held,
            preference,
            reason_codes,
            Utc::now(),
        ) {
            Ok(hosted::FeedbackInduction::AnnotationOnly { annotation }) => vec![*annotation],
            Ok(hosted::FeedbackInduction::Induced {
                capture,
                annotation,
            }) => vec![*capture, *annotation],
            Err(error) => {
                tracing::error!(
                    %error,
                    "quality capture feedback induction failed closed; business command continues"
                );
                return;
            }
        };
        self.commit_best_effort(owner, &drafts).await;
    }

    async fn take_held(&self, owner: &ProcessorPrincipal) -> Option<QualityCaptureDraft> {
        let ProcessorPrincipal::Player(player_id) = owner else {
            return None;
        };
        match self {
            Self::Memory(store) => store.take_held(player_id),
            Self::Live(database) => firestore::take_held(database, player_id).await,
            Self::Inert => None,
        }
    }

    async fn feedback_anchor(&self, owner: &ProcessorPrincipal) -> Option<model::FeedbackAnchor> {
        let ProcessorPrincipal::Player(player_id) = owner else {
            return None;
        };
        match self {
            Self::Memory(store) => store.feedback_anchor(player_id),
            Self::Live(database) => firestore::feedback_anchor(database, player_id).await,
            Self::Inert => None,
        }
    }

    pub(crate) async fn prepare_firestore_writes(
        &self,
        owner: &ProcessorPrincipal,
        captures: &[QualityCaptureDraft],
    ) -> Vec<FirestoreWrite> {
        if let (Self::Memory(store), ProcessorPrincipal::Player(player_id)) = (self, owner) {
            store.enqueue(player_id, captures);
            return Vec::new();
        }
        let (Self::Live(database), ProcessorPrincipal::Player(player_id)) = (self, owner) else {
            return Vec::new();
        };
        let mut writes = Vec::new();
        for capture in captures {
            match firestore::prepare_outbox_writes(database, player_id, capture).await {
                Ok(prepared) => writes.extend(prepared),
                Err(error) => {
                    tracing::error!(
                        category = error.diagnostic_category(),
                        "quality capture gate failed closed; business persistence will continue"
                    );
                }
            }
        }
        writes
    }

    pub(crate) async fn prepare_game_analysis_writes(
        &self,
        owner: &ProcessorPrincipal,
        record: &crate::game_import_store::GameImportRecord,
    ) -> Vec<FirestoreWrite> {
        if !matches!(
            (self, owner),
            (Self::Live(_), ProcessorPrincipal::Player(_))
        ) {
            return Vec::new();
        }
        match QualityCaptureDraft::game_analysis(record) {
            Ok(capture) => {
                self.prepare_firestore_writes(owner, std::slice::from_ref(&capture))
                    .await
            }
            Err(error) => {
                tracing::error!(%error, "completed Game Analysis was not capturable");
                Vec::new()
            }
        }
    }

    pub(crate) async fn commit_best_effort(
        &self,
        owner: &ProcessorPrincipal,
        captures: &[QualityCaptureDraft],
    ) {
        if let (Self::Memory(store), ProcessorPrincipal::Player(player_id)) = (self, owner) {
            store.enqueue(player_id, captures);
            return;
        }
        let Self::Live(database) = self else {
            return;
        };
        let ProcessorPrincipal::Player(player_id) = owner else {
            return;
        };
        for capture in captures {
            match firestore::prepare_outbox_writes(database, player_id, capture).await {
                Ok(writes) if !writes.is_empty() => {
                    if let Err(error) = database.commit(writes).await {
                        tracing::error!(
                            category = QualityCaptureStoreError::from(error).diagnostic_category(),
                            "quality capture best-effort persist failed; business command continues"
                        );
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(
                        category = error.diagnostic_category(),
                        "quality capture gate failed closed; business persistence will continue"
                    );
                }
            }
        }
    }
}

pub struct QualityCaptureRuntime {
    preference_store: Arc<dyn QualityCapturePreferenceStore>,
    persist: QualityCaptureAppender,
    exporter: Option<Arc<FirestoreQualityCaptureStore>>,
}

impl QualityCaptureRuntime {
    pub fn preference_store(&self) -> Arc<dyn QualityCapturePreferenceStore> {
        self.preference_store.clone()
    }

    pub fn in_memory(store: Arc<InMemoryQualityCaptureStore>) -> Self {
        Self {
            preference_store: store.clone(),
            persist: QualityCaptureAppender::Memory(store),
            exporter: None,
        }
    }

    /// Submit induces through the persist seam after product load. Persist
    /// errors never fail the business command.
    pub(crate) async fn record_feedback(
        &self,
        player_id: &PlayerId,
        reason_codes: Vec<ReviewFeedbackReason>,
    ) -> Result<(), QualityCaptureStoreError> {
        let preference = self.preference_store.preference(player_id).await?;
        self.persist
            .record_feedback(
                &ProcessorPrincipal::Player(player_id.clone()),
                &preference,
                reason_codes,
            )
            .await;
        Ok(())
    }

    pub fn spawn_exporter(&self) {
        let Some(exporter) = self.exporter.clone() else {
            return;
        };
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if let Err(error) = exporter.process_due().await {
                    tracing::error!(
                        category = error.diagnostic_category(),
                        "quality capture exporter pass failed"
                    );
                }
            }
        });
    }
}

pub async fn configured_quality_capture_runtime() -> anyhow::Result<QualityCaptureRuntime> {
    if std::env::var_os("FIREBASE_PROJECT_ID").is_none() {
        return Ok(QualityCaptureRuntime {
            preference_store: Arc::new(NoQualityCaptureStore),
            persist: QualityCaptureAppender::Inert,
            exporter: None,
        });
    }
    let application = FirestoreDatabase::from_env()?;
    let persist = QualityCaptureAppender::for_application(application.clone());
    match FirestoreDatabase::quality_from_env_optional()? {
        Some(quality) => {
            let store = Arc::new(FirestoreQualityCaptureStore::new(application, quality));
            Ok(QualityCaptureRuntime {
                preference_store: store.clone(),
                persist,
                exporter: Some(store),
            })
        }
        None => {
            let store = Arc::new(FirestoreQualityCaptureStore::preference_only(application));
            Ok(QualityCaptureRuntime {
                preference_store: store,
                persist,
                exporter: None,
            })
        }
    }
}

impl QualityCaptureStoreError {
    fn diagnostic_category(&self) -> &'static str {
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

    fn player() -> PlayerId {
        PlayerId::try_from("firebase-player".to_string()).unwrap()
    }

    #[tokio::test]
    async fn disclosure_acknowledgement_gates_in_memory_captures() {
        let store = InMemoryQualityCaptureStore::default();

        assert_eq!(
            store.preference(&player()).await.unwrap(),
            RetentionPreference {
                available: true,
                enabled: true,
                disclosure_required: true,
                deleted_review_snapshots: 0,
            }
        );
        assert_eq!(
            store.set_preference(&player(), true).await.unwrap(),
            RetentionPreference {
                available: true,
                enabled: true,
                disclosure_required: false,
                deleted_review_snapshots: 0,
            }
        );
    }
}
