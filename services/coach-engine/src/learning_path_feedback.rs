use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::game_import_store::ReviewSessionGame;
use crate::review_session_contract::{
    CriticalMomentId, DeliverySurface, GameRef, LearningPathFeedbackState, LearningPathRef,
    LearningPathVote, LearningPlanSelectionPolicyVersion, LearningResourceCatalogVersion,
    LearningResourceId, LearningTrackKey, LearningTrackPurpose, LearningTrackSupport,
    LearningTrackSupportBasis, PlayerId,
};
use crate::{
    firestore::{FirestoreDatabase, FirestoreError},
    review_durability::path::hashed_path_segment,
};

pub type LearningPathFeedbackStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, LearningPathFeedbackError>> + Send + 'a>>;

pub trait LearningPathFeedbackStore: Send + Sync {
    fn record_exposure<'a>(
        &'a self,
        player_id: &'a PlayerId,
        sample: LearningPathSample,
        surface: DeliverySurface,
    ) -> LearningPathFeedbackStoreFuture<'a, LearningPathFeedbackState>;

    fn update_vote<'a>(
        &'a self,
        player_id: &'a PlayerId,
        sample: LearningPathSample,
        surface: DeliverySurface,
        vote: Option<LearningPathVote>,
    ) -> LearningPathFeedbackStoreFuture<'a, LearningPathFeedbackState>;

    fn analytics(&self) -> LearningPathFeedbackStoreFuture<'_, LearningPathFeedbackAnalytics>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningPathSample {
    pub learning_path_ref: LearningPathRef,
    pub game_ref: GameRef,
    pub critical_moment_id: CriticalMomentId,
    pub key: LearningTrackKey,
    pub purpose: LearningTrackPurpose,
    pub proof_family: LearningProofFamily,
    pub selection_policy_version: LearningPlanSelectionPolicyVersion,
    pub resource_catalog_version: LearningResourceCatalogVersion,
    pub resource_ids: Vec<LearningResourceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LearningProofFamily {
    DecisionExplanation,
    Opening,
}

pub(crate) fn resolve_sample(
    game: &ReviewSessionGame,
    learning_path_ref: &LearningPathRef,
) -> Option<LearningPathSample> {
    game.frozen_review
        .critical_moments
        .iter()
        .chain(game.player_selected_moments.values())
        .find_map(|moment| {
            moment.learning_material.tracks.iter().find_map(|track| {
                track.support.iter().find_map(|support| {
                    let (candidate_ref, critical_moment_id, purpose, basis) = match support {
                        LearningTrackSupport::Improvement {
                            learning_path_ref,
                            critical_moment_id,
                            basis,
                            ..
                        } => (
                            learning_path_ref,
                            critical_moment_id,
                            LearningTrackPurpose::Improvement,
                            basis,
                        ),
                        LearningTrackSupport::Reinforcement {
                            learning_path_ref,
                            critical_moment_id,
                            basis,
                            ..
                        } => (
                            learning_path_ref,
                            critical_moment_id,
                            LearningTrackPurpose::Reinforcement,
                            basis,
                        ),
                    };
                    if candidate_ref != learning_path_ref
                        || critical_moment_id != &moment.critical_moment_id
                    {
                        return None;
                    }
                    let proof_family = proof_family(basis);
                    Some(LearningPathSample {
                        learning_path_ref: candidate_ref.clone(),
                        game_ref: game.imported_game.game.game_ref.clone(),
                        critical_moment_id: critical_moment_id.clone(),
                        key: track.key.clone(),
                        purpose,
                        proof_family,
                        selection_policy_version: moment.learning_material.selection_policy_version,
                        resource_catalog_version: moment.learning_material.resource_catalog_version,
                        resource_ids: track
                            .resources
                            .iter()
                            .map(|resource| resource.resource_id.clone())
                            .collect(),
                    })
                })
            })
        })
}

fn proof_family(basis: &LearningTrackSupportBasis) -> LearningProofFamily {
    match basis {
        LearningTrackSupportBasis::DecisionExplanation { .. } => {
            LearningProofFamily::DecisionExplanation
        }
        LearningTrackSupportBasis::Opening { .. } => LearningProofFamily::Opening,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FeedbackRecord {
    sample: LearningPathSample,
    exposed_surfaces: BTreeSet<DeliverySurface>,
    vote: Option<LearningPathVote>,
    vote_surface: Option<DeliverySurface>,
}

const USERS_COLLECTION: &str = "users";
const FEEDBACK_COLLECTION: &str = "learningPathFeedback";
const ANALYTICS_COLLECTION: &str = "learningPathFeedbackAnalytics";
const MAX_TRANSACTION_ATTEMPTS: usize = 5;

pub(crate) fn learning_path_feedback_store(
    database: FirestoreDatabase,
) -> Arc<dyn LearningPathFeedbackStore> {
    Arc::new(FirestoreLearningPathFeedbackStore { database })
}

impl FeedbackRecord {
    fn state(&self) -> LearningPathFeedbackState {
        LearningPathFeedbackState {
            learning_path_ref: self.sample.learning_path_ref.clone(),
            current_vote: self.vote,
            exposed_surfaces: self.exposed_surfaces.iter().copied().collect(),
        }
    }
}

#[derive(Default)]
pub struct InMemoryLearningPathFeedbackStore {
    records: Mutex<BTreeMap<(PlayerId, LearningPathRef), FeedbackRecord>>,
}

impl LearningPathFeedbackStore for InMemoryLearningPathFeedbackStore {
    fn record_exposure<'a>(
        &'a self,
        player_id: &'a PlayerId,
        sample: LearningPathSample,
        surface: DeliverySurface,
    ) -> LearningPathFeedbackStoreFuture<'a, LearningPathFeedbackState> {
        Box::pin(async move {
            let key = (player_id.clone(), sample.learning_path_ref.clone());
            let mut records = self.records.lock().await;
            let record = records.entry(key).or_insert_with(|| FeedbackRecord {
                sample: sample.clone(),
                exposed_surfaces: BTreeSet::new(),
                vote: None,
                vote_surface: None,
            });
            ensure_same_sample(&record.sample, &sample)?;
            record.exposed_surfaces.insert(surface);
            Ok(record.state())
        })
    }

    fn update_vote<'a>(
        &'a self,
        player_id: &'a PlayerId,
        sample: LearningPathSample,
        surface: DeliverySurface,
        vote: Option<LearningPathVote>,
    ) -> LearningPathFeedbackStoreFuture<'a, LearningPathFeedbackState> {
        Box::pin(async move {
            let key = (player_id.clone(), sample.learning_path_ref.clone());
            let mut records = self.records.lock().await;
            let record = records
                .get_mut(&key)
                .ok_or(LearningPathFeedbackError::ExposureRequired)?;
            ensure_same_sample(&record.sample, &sample)?;
            if !record.exposed_surfaces.contains(&surface) {
                return Err(LearningPathFeedbackError::ExposureRequired);
            }
            record.vote = vote;
            record.vote_surface = vote.map(|_| surface);
            Ok(record.state())
        })
    }

    fn analytics(&self) -> LearningPathFeedbackStoreFuture<'_, LearningPathFeedbackAnalytics> {
        Box::pin(async move {
            let records = self.records.lock().await;
            Ok(LearningPathFeedbackAnalytics::from_records(
                records.values(),
            ))
        })
    }
}

struct FirestoreLearningPathFeedbackStore {
    database: FirestoreDatabase,
}

impl FirestoreLearningPathFeedbackStore {
    async fn expose(
        &self,
        player_id: &PlayerId,
        sample: LearningPathSample,
        surface: DeliverySurface,
    ) -> Result<LearningPathFeedbackState, LearningPathFeedbackError> {
        for _ in 0..MAX_TRANSACTION_ATTEMPTS {
            let transaction = self.database.begin_transaction().await?;
            let path = feedback_path(player_id, &sample.learning_path_ref);
            let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
            let existing = self
                .database
                .get_document_in_transaction::<FeedbackRecord>(&path_refs, &transaction)
                .await?;
            let mut record = existing.unwrap_or_else(|| FeedbackRecord {
                sample: sample.clone(),
                exposed_surfaces: BTreeSet::new(),
                vote: None,
                vote_surface: None,
            });
            if let Err(error) = ensure_same_sample(&record.sample, &sample) {
                let _ = self.database.rollback_transaction(transaction).await;
                return Err(error);
            }
            let inserted = record.exposed_surfaces.insert(surface);
            let mut writes = vec![self.database.upsert_write(&path_refs, &record, &[])?];
            if inserted {
                let key = slice_key(&sample, surface);
                let aggregate_path = analytics_path(&key);
                let aggregate_refs = aggregate_path
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                let mut aggregate = self
                    .database
                    .get_document_in_transaction::<StoredAnalyticsSlice>(
                        &aggregate_refs,
                        &transaction,
                    )
                    .await?
                    .unwrap_or_else(|| StoredAnalyticsSlice::empty(key));
                if let Err(error) = increment(&mut aggregate.counts.exposure_count)
                    .and_then(|()| aggregate.counts.validate())
                {
                    let _ = self.database.rollback_transaction(transaction).await;
                    return Err(error);
                }
                writes.push(
                    self.database
                        .upsert_write(&aggregate_refs, &aggregate, &[])?,
                );
            }
            match self.database.commit_transaction(transaction, writes).await {
                Ok(()) => return Ok(record.state()),
                Err(FirestoreError::Conflict) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(LearningPathFeedbackError::Unavailable)
    }

    async fn vote(
        &self,
        player_id: &PlayerId,
        sample: LearningPathSample,
        surface: DeliverySurface,
        vote: Option<LearningPathVote>,
    ) -> Result<LearningPathFeedbackState, LearningPathFeedbackError> {
        for _ in 0..MAX_TRANSACTION_ATTEMPTS {
            let transaction = self.database.begin_transaction().await?;
            let path = feedback_path(player_id, &sample.learning_path_ref);
            let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
            let Some(mut record) = self
                .database
                .get_document_in_transaction::<FeedbackRecord>(&path_refs, &transaction)
                .await?
            else {
                let _ = self.database.rollback_transaction(transaction).await;
                return Err(LearningPathFeedbackError::ExposureRequired);
            };
            if let Err(error) = ensure_same_sample(&record.sample, &sample) {
                let _ = self.database.rollback_transaction(transaction).await;
                return Err(error);
            }
            if !record.exposed_surfaces.contains(&surface) {
                let _ = self.database.rollback_transaction(transaction).await;
                return Err(LearningPathFeedbackError::ExposureRequired);
            }
            let previous = record.vote.zip(record.vote_surface);
            let next = vote.map(|vote| (vote, surface));
            let mut affected_surfaces = BTreeSet::new();
            if let Some((_, old_surface)) = previous {
                affected_surfaces.insert(old_surface);
            }
            if let Some((_, new_surface)) = next {
                affected_surfaces.insert(new_surface);
            }
            let mut aggregates = Vec::new();
            for affected_surface in affected_surfaces {
                let key = slice_key(&sample, affected_surface);
                let aggregate_path = analytics_path(&key);
                let aggregate_refs = aggregate_path
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                let aggregate = self
                    .database
                    .get_document_in_transaction::<StoredAnalyticsSlice>(
                        &aggregate_refs,
                        &transaction,
                    )
                    .await?
                    .unwrap_or_else(|| StoredAnalyticsSlice::empty(key));
                aggregates.push((aggregate_path, aggregate));
            }
            if let Err(error) = apply_vote_delta(&mut aggregates, previous, next) {
                let _ = self.database.rollback_transaction(transaction).await;
                return Err(error);
            }
            record.vote = vote;
            record.vote_surface = vote.map(|_| surface);
            let mut writes = vec![self.database.upsert_write(&path_refs, &record, &[])?];
            for (aggregate_path, aggregate) in aggregates {
                let aggregate_refs = aggregate_path
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                writes.push(
                    self.database
                        .upsert_write(&aggregate_refs, &aggregate, &[])?,
                );
            }
            match self.database.commit_transaction(transaction, writes).await {
                Ok(()) => return Ok(record.state()),
                Err(FirestoreError::Conflict) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(LearningPathFeedbackError::Unavailable)
    }
}

impl LearningPathFeedbackStore for FirestoreLearningPathFeedbackStore {
    fn record_exposure<'a>(
        &'a self,
        player_id: &'a PlayerId,
        sample: LearningPathSample,
        surface: DeliverySurface,
    ) -> LearningPathFeedbackStoreFuture<'a, LearningPathFeedbackState> {
        Box::pin(self.expose(player_id, sample, surface))
    }

    fn update_vote<'a>(
        &'a self,
        player_id: &'a PlayerId,
        sample: LearningPathSample,
        surface: DeliverySurface,
        vote: Option<LearningPathVote>,
    ) -> LearningPathFeedbackStoreFuture<'a, LearningPathFeedbackState> {
        Box::pin(self.vote(player_id, sample, surface, vote))
    }

    fn analytics(&self) -> LearningPathFeedbackStoreFuture<'_, LearningPathFeedbackAnalytics> {
        Box::pin(async move {
            let slices = self
                .database
                .list_documents::<StoredAnalyticsSlice>(&[ANALYTICS_COLLECTION])
                .await?
                .into_iter()
                .map(|(_, stored)| LearningPathFeedbackSlice::new(stored.key, stored.counts))
                .collect();
            Ok(LearningPathFeedbackAnalytics { slices })
        })
    }
}

fn feedback_path(player_id: &PlayerId, learning_path_ref: &LearningPathRef) -> Vec<String> {
    vec![
        USERS_COLLECTION.to_string(),
        hashed_path_segment(player_id.as_str()),
        FEEDBACK_COLLECTION.to_string(),
        hashed_path_segment(learning_path_ref.as_str()),
    ]
}

fn analytics_path(key: &LearningPathFeedbackSliceKey) -> Vec<String> {
    let material = serde_json_canonicalizer::to_vec(key)
        .expect("analytics slice keys have an infallible canonical representation");
    vec![
        ANALYTICS_COLLECTION.to_string(),
        hashed_path_segment(material),
    ]
}

fn slice_key(
    sample: &LearningPathSample,
    surface: DeliverySurface,
) -> LearningPathFeedbackSliceKey {
    LearningPathFeedbackSliceKey {
        idea: sample.key.clone(),
        purpose: sample.purpose,
        selection_policy_version: sample.selection_policy_version,
        resource_catalog_version: sample.resource_catalog_version,
        surface,
    }
}

fn apply_vote_delta(
    aggregates: &mut [(Vec<String>, StoredAnalyticsSlice)],
    previous: Option<(LearningPathVote, DeliverySurface)>,
    next: Option<(LearningPathVote, DeliverySurface)>,
) -> Result<(), LearningPathFeedbackError> {
    if let Some((vote, surface)) = previous {
        let counts = counts_for_surface(aggregates, surface)?;
        match vote {
            LearningPathVote::ThumbsUp => {
                counts.thumbs_up_count = counts
                    .thumbs_up_count
                    .checked_sub(1)
                    .ok_or(LearningPathFeedbackError::InvalidSample)?;
            }
            LearningPathVote::ThumbsDown => {
                counts.thumbs_down_count = counts
                    .thumbs_down_count
                    .checked_sub(1)
                    .ok_or(LearningPathFeedbackError::InvalidSample)?;
            }
        }
    }
    if let Some((vote, surface)) = next {
        let counts = counts_for_surface(aggregates, surface)?;
        match vote {
            LearningPathVote::ThumbsUp => increment(&mut counts.thumbs_up_count)?,
            LearningPathVote::ThumbsDown => increment(&mut counts.thumbs_down_count)?,
        }
    }
    for (_, aggregate) in aggregates {
        aggregate.counts.validate()?;
    }
    Ok(())
}

fn increment(value: &mut u64) -> Result<(), LearningPathFeedbackError> {
    *value = value
        .checked_add(1)
        .ok_or(LearningPathFeedbackError::InvalidSample)?;
    Ok(())
}

fn counts_for_surface(
    aggregates: &mut [(Vec<String>, StoredAnalyticsSlice)],
    surface: DeliverySurface,
) -> Result<&mut LearningPathFeedbackCounts, LearningPathFeedbackError> {
    aggregates
        .iter_mut()
        .find(|(_, aggregate)| aggregate.key.surface == surface)
        .map(|(_, aggregate)| &mut aggregate.counts)
        .ok_or(LearningPathFeedbackError::InvalidSample)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredAnalyticsSlice {
    key: LearningPathFeedbackSliceKey,
    counts: LearningPathFeedbackCounts,
}

impl StoredAnalyticsSlice {
    fn empty(key: LearningPathFeedbackSliceKey) -> Self {
        Self {
            key,
            counts: LearningPathFeedbackCounts::default(),
        }
    }
}

fn ensure_same_sample(
    existing: &LearningPathSample,
    submitted: &LearningPathSample,
) -> Result<(), LearningPathFeedbackError> {
    if existing == submitted {
        Ok(())
    } else {
        Err(LearningPathFeedbackError::InvalidSample)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LearningPathFeedbackAnalytics {
    pub slices: Vec<LearningPathFeedbackSlice>,
}

impl LearningPathFeedbackAnalytics {
    fn from_records<'a>(records: impl Iterator<Item = &'a FeedbackRecord>) -> Self {
        let mut slices =
            BTreeMap::<LearningPathFeedbackSliceKey, LearningPathFeedbackCounts>::new();
        for record in records {
            for surface in &record.exposed_surfaces {
                let key = LearningPathFeedbackSliceKey {
                    idea: record.sample.key.clone(),
                    purpose: record.sample.purpose,
                    selection_policy_version: record.sample.selection_policy_version,
                    resource_catalog_version: record.sample.resource_catalog_version,
                    surface: *surface,
                };
                let counts = slices.entry(key).or_default();
                counts.exposure_count += 1;
                if record.vote_surface == Some(*surface) {
                    match record.vote {
                        Some(LearningPathVote::ThumbsUp) => counts.thumbs_up_count += 1,
                        Some(LearningPathVote::ThumbsDown) => counts.thumbs_down_count += 1,
                        None => {}
                    }
                }
            }
        }
        Self {
            slices: slices
                .into_iter()
                .map(|(key, counts)| LearningPathFeedbackSlice::new(key, counts))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningPathFeedbackSliceKey {
    pub idea: LearningTrackKey,
    pub purpose: LearningTrackPurpose,
    pub selection_policy_version: LearningPlanSelectionPolicyVersion,
    pub resource_catalog_version: LearningResourceCatalogVersion,
    pub surface: DeliverySurface,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningPathFeedbackCounts {
    pub exposure_count: u64,
    pub thumbs_up_count: u64,
    pub thumbs_down_count: u64,
}

impl LearningPathFeedbackCounts {
    fn validate(self) -> Result<(), LearningPathFeedbackError> {
        let votes = self
            .thumbs_up_count
            .checked_add(self.thumbs_down_count)
            .ok_or(LearningPathFeedbackError::InvalidSample)?;
        if votes <= self.exposure_count {
            Ok(())
        } else {
            Err(LearningPathFeedbackError::InvalidSample)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LearningPathFeedbackSlice {
    pub key: LearningPathFeedbackSliceKey,
    pub exposure_count: u64,
    pub vote_count: u64,
    pub thumbs_up_count: u64,
    pub thumbs_down_count: u64,
    pub relevance_rate: Option<f64>,
    pub response_rate: Option<f64>,
}

impl LearningPathFeedbackSlice {
    fn new(key: LearningPathFeedbackSliceKey, counts: LearningPathFeedbackCounts) -> Self {
        let vote_count = counts.thumbs_up_count + counts.thumbs_down_count;
        Self {
            key,
            exposure_count: counts.exposure_count,
            vote_count,
            thumbs_up_count: counts.thumbs_up_count,
            thumbs_down_count: counts.thumbs_down_count,
            relevance_rate: (vote_count > 0)
                .then_some(counts.thumbs_up_count as f64 / vote_count as f64),
            response_rate: (counts.exposure_count > 0)
                .then_some(vote_count as f64 / counts.exposure_count as f64),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LearningPathFeedbackError {
    #[error("the selected Learning Path sample does not match its stored identity")]
    InvalidSample,
    #[error("the Learning Path must be exposed on this surface before voting")]
    ExposureRequired,
    #[error("Learning Path feedback storage is unavailable")]
    Unavailable,
}

impl From<FirestoreError> for LearningPathFeedbackError {
    fn from(error: FirestoreError) -> Self {
        match error {
            FirestoreError::InvalidDocument => Self::InvalidSample,
            FirestoreError::Configuration(_)
            | FirestoreError::Transport
            | FirestoreError::Unavailable
            | FirestoreError::Conflict => Self::Unavailable,
        }
    }
}

#[cfg(test)]
#[path = "learning_path_feedback/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "learning_path_feedback/firestore_tests.rs"]
mod firestore_tests;
