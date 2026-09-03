use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    profile_game_feed::{
        ProfileGameSourceIdentity, ProfileGameTimeControlClass, ProfileGameWindowEntry,
    },
    review_durability::path::hashed_path_segment,
    review_session_contract::{
        Color, CompletedGameOutcome, DecisiveGameTermination, DrawGameTermination, EloRating,
        GameImportId, GameReview, ImportProvenance, ImportedGame, LearningPlan,
        LearningPlanSelectionPolicyVersion, LearningResourceCatalogVersion, MetadataText,
        OpeningMetadata, RatingMetadata, RequestedReviewSide, ReviewSide,
    },
};

use super::DailyCoachingOwnerKey;

mod priorities;
pub(crate) use priorities::CoachingDigestPriority;

const DIGEST_SCHEMA_VERSION: u16 = 1;
const DIGESTED_GAME_SCHEMA_VERSION: u16 = 1;
const PRIORITY_POLICY_VERSION: CoachingDigestPriorityPolicyVersion =
    CoachingDigestPriorityPolicyVersion::V1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CoachingDigestPriorityPolicyVersion {
    #[cfg(test)]
    #[serde(rename = "coaching-digest-priority/test-only-non-current")]
    NonCurrentTestVersion,
    #[serde(rename = "coaching-digest-priority/v1")]
    V1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CoachingWindowKind {
    #[default]
    Daily,
    InitialBackfill,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrozenDigestGame {
    pub(crate) selected: ProfileGameWindowEntry,
    pub(crate) review: FrozenDailyGameReview,
    pub(crate) window_kind: CoachingWindowKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FrozenDailyGameReview {
    pub(crate) game_import_id: GameImportId,
    pub(crate) review_side: ReviewSide,
    pub(crate) player_outcome: PlayerRelativeOutcome,
    pub(crate) termination: DailyGameTermination,
    pub(crate) opening_eco: Option<String>,
    pub(crate) opening_name: Option<String>,
    pub(crate) opponent_name: Option<String>,
    pub(crate) player_rating: Option<u16>,
    pub(crate) opponent_rating: Option<u16>,
    pub(crate) played_plies: u32,
    pub(crate) learning_plan: LearningPlan,
}

impl FrozenDailyGameReview {
    pub(crate) fn capture(
        selected: &ProfileGameWindowEntry,
        game_import_id: GameImportId,
        imported: &ImportedGame,
        review: &GameReview,
    ) -> Result<Self, CoachingDigestError> {
        if source_identity(&imported.provenance).as_ref() != Some(&selected.source_identity) {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        if !matches!(
            &selected.review_request.review_side,
            RequestedReviewSide::Selected { review_side }
                if *review_side == imported.review_side
        ) {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        let (player, opponent, player_color) = match imported.review_side {
            ReviewSide::White => (&imported.game.white, &imported.game.black, Color::White),
            ReviewSide::Black => (&imported.game.black, &imported.game.white, Color::Black),
            ReviewSide::Both => return Err(CoachingDigestError::InvalidAggregate),
        };
        let played_plies = u32::try_from(imported.game.moves.len())
            .map_err(|_| CoachingDigestError::InvalidAggregate)?;
        if played_plies == 0 || played_plies != selected.played_plies {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        let (opening_eco, opening_name) = match &imported.game.opening {
            OpeningMetadata::Present { eco, name, .. } => (Some(eco.clone()), Some(name.clone())),
            OpeningMetadata::Absent => (None, None),
        };
        let (player_outcome, termination) = outcome(imported.game.outcome, player_color);
        let frozen = Self {
            game_import_id,
            review_side: imported.review_side,
            player_outcome,
            termination,
            opening_eco,
            opening_name,
            opponent_name: metadata_text(&opponent.name),
            player_rating: rating(&player.rating),
            opponent_rating: rating(&opponent.rating),
            played_plies,
            learning_plan: review.learning_plan.clone(),
        };
        frozen.validate_for_selection(selected)?;
        Ok(frozen)
    }

    pub(crate) fn validate_for_selection(
        &self,
        selected: &ProfileGameWindowEntry,
    ) -> Result<(), CoachingDigestError> {
        if !selected.is_valid()
            || self.played_plies != selected.played_plies
            || !matches!(
                selected.review_request.review_side,
                RequestedReviewSide::Selected { review_side }
                    if review_side == self.review_side
            )
        {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        validate_frozen_game_facts(FrozenGameFacts {
            source_identity: &selected.source_identity,
            source_profile: &selected.source_profile,
            time_control_raw: &selected.time_control_raw,
            time_control_class: selected.time_control_class,
            expected_clock_seconds: selected.expected_clock_seconds,
            review_side: self.review_side,
            player_outcome: self.player_outcome,
            termination: self.termination,
            opening_eco: self.opening_eco.as_deref(),
            opening_name: self.opening_name.as_deref(),
            opponent_name: self.opponent_name.as_deref(),
            player_rating: self.player_rating,
            opponent_rating: self.opponent_rating,
            played_plies: self.played_plies,
        })?;
        priorities::validate_frozen_learning_plan(&self.learning_plan, self.played_plies)
    }

    fn learning_path_count(&self) -> Result<u16, CoachingDigestError> {
        self.learning_plan
            .tracks
            .iter()
            .try_fold(0_u16, |count, track| {
                count.checked_add(u16::try_from(track.support.len()).ok()?)
            })
            .ok_or(CoachingDigestError::InvalidAggregate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PlayerRelativeOutcome {
    Win,
    Loss,
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "reason", rename_all = "camelCase")]
pub(crate) enum DailyGameTermination {
    Decisive(DecisiveGameTermination),
    Draw(DrawGameTermination),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DigestedGameCard {
    schema_version: u16,
    pub(crate) digested_game_key: String,
    pub(crate) digest_id: String,
    pub(crate) digested_at: DateTime<Utc>,
    pub(crate) game_import_id: GameImportId,
    pub(crate) source_identity: ProfileGameSourceIdentity,
    pub(crate) ended_at: DateTime<Utc>,
    pub(crate) time_control_raw: String,
    pub(crate) time_control_class: ProfileGameTimeControlClass,
    pub(crate) expected_clock_seconds: Option<u64>,
    pub(crate) review_side: ReviewSide,
    pub(crate) player_outcome: PlayerRelativeOutcome,
    pub(crate) termination: DailyGameTermination,
    pub(crate) opening_eco: Option<String>,
    pub(crate) opening_name: Option<String>,
    pub(crate) opponent_name: Option<String>,
    pub(crate) player_rating: Option<u16>,
    pub(crate) opponent_rating: Option<u16>,
    pub(crate) played_plies: u32,
    pub(crate) source_profile: String,
    pub(crate) window_kind: CoachingWindowKind,
    pub(crate) learning_path_count: u16,
}

impl DigestedGameCard {
    fn new(
        digest_id: &str,
        selected: &ProfileGameWindowEntry,
        reviewed: &FrozenDailyGameReview,
        window_kind: CoachingWindowKind,
        digested_at: DateTime<Utc>,
    ) -> Result<Self, CoachingDigestError> {
        let ended_at_milliseconds = i64::try_from(selected.ended_at_unix_milliseconds)
            .map_err(|_| CoachingDigestError::InvalidAggregate)?;
        let ended_at = DateTime::from_timestamp_millis(ended_at_milliseconds)
            .ok_or(CoachingDigestError::InvalidAggregate)?;
        let card = Self {
            schema_version: DIGESTED_GAME_SCHEMA_VERSION,
            digested_game_key: hashed_path_segment(selected.source_identity.canonical_key()),
            digest_id: digest_id.to_string(),
            digested_at,
            game_import_id: reviewed.game_import_id.clone(),
            source_identity: selected.source_identity.clone(),
            ended_at,
            time_control_raw: selected.time_control_raw.clone(),
            time_control_class: selected.time_control_class,
            expected_clock_seconds: selected.expected_clock_seconds,
            review_side: reviewed.review_side,
            player_outcome: reviewed.player_outcome,
            termination: reviewed.termination,
            opening_eco: reviewed.opening_eco.clone(),
            opening_name: reviewed.opening_name.clone(),
            opponent_name: reviewed.opponent_name.clone(),
            player_rating: reviewed.player_rating,
            opponent_rating: reviewed.opponent_rating,
            played_plies: reviewed.played_plies,
            source_profile: selected.source_profile.clone(),
            window_kind,
            learning_path_count: reviewed.learning_path_count()?,
        };
        card.validate()?;
        Ok(card)
    }

    pub(crate) fn validate(&self) -> Result<(), CoachingDigestError> {
        if self.schema_version != DIGESTED_GAME_SCHEMA_VERSION
            || self.digested_game_key != hashed_path_segment(self.source_identity.canonical_key())
            || self
                .digest_id
                .strip_prefix("daily-")
                .is_none_or(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err())
            || self.digested_at.timestamp_millis() <= 0
            || self.ended_at.timestamp_millis() <= 0
        {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        validate_frozen_game_facts(FrozenGameFacts {
            source_identity: &self.source_identity,
            source_profile: &self.source_profile,
            time_control_raw: &self.time_control_raw,
            time_control_class: self.time_control_class,
            expected_clock_seconds: self.expected_clock_seconds,
            review_side: self.review_side,
            player_outcome: self.player_outcome,
            termination: self.termination,
            opening_eco: self.opening_eco.as_deref(),
            opening_name: self.opening_name.as_deref(),
            opponent_name: self.opponent_name.as_deref(),
            player_rating: self.player_rating,
            opponent_rating: self.opponent_rating,
            played_plies: self.played_plies,
        })
    }

    pub(crate) fn canonical_source_key(&self) -> String {
        self.source_identity.canonical_key()
    }

    pub(crate) fn provider(&self) -> crate::imported_games::ImportedGameProvider {
        match self.source_identity.provider {
            crate::profile_game_feed::ChessProfileProvider::Lichess => {
                crate::imported_games::ImportedGameProvider::Lichess
            }
            crate::profile_game_feed::ChessProfileProvider::ChessCom => {
                crate::imported_games::ImportedGameProvider::ChessCom
            }
        }
    }

    pub(crate) fn review_side(&self) -> crate::imported_games::ImportedGameReviewSide {
        self.review_side.into()
    }

    pub(crate) fn outcome(&self) -> crate::imported_games::ImportedGameOutcome {
        match self.player_outcome {
            PlayerRelativeOutcome::Win => crate::imported_games::ImportedGameOutcome::Win,
            PlayerRelativeOutcome::Loss => crate::imported_games::ImportedGameOutcome::Loss,
            PlayerRelativeOutcome::Draw => crate::imported_games::ImportedGameOutcome::Draw,
        }
    }

    pub(crate) fn opening(&self) -> Option<crate::imported_games::ImportedGameOpening> {
        self.opening_eco
            .as_ref()
            .zip(self.opening_name.as_ref())
            .map(|(eco, name)| crate::imported_games::ImportedGameOpening {
                eco: eco.clone(),
                name: name.clone(),
            })
    }

    pub(crate) fn time_control_class(&self) -> crate::imported_games::ImportedGameTimeControlClass {
        self.time_control_class.into()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CoachingDigest {
    schema_version: u16,
    pub(crate) digest_id: String,
    pub(crate) owner_key: DailyCoachingOwnerKey,
    pub(crate) window_kind: CoachingWindowKind,
    pub(crate) coverage_date: NaiveDate,
    pub(crate) published_at: DateTime<Utc>,
    /// How many times this window was reopened before this publication. It keeps the digest-email
    /// delivery identity distinct from the original send, whose idempotency key is derived from it.
    #[serde(default, skip_serializing_if = "is_zero_count")]
    pub(crate) regeneration_count: u32,
    #[serde(default)]
    pub(crate) email_delivery_eligible: bool,
    pub(crate) timezone: String,
    pub(crate) ordered_card_keys: Vec<String>,
    pub(crate) game_import_ids: Vec<GameImportId>,
    pub(crate) game_count: u8,
    pub(crate) learning_path_count: u16,
    pub(crate) priority_policy_version: CoachingDigestPriorityPolicyVersion,
    pub(crate) learning_plan_selection_policy_version: LearningPlanSelectionPolicyVersion,
    pub(crate) learning_resource_catalog_version: LearningResourceCatalogVersion,
    pub(crate) priorities: Vec<CoachingDigestPriority>,
}

fn is_zero_count(value: &u32) -> bool {
    *value == 0
}

impl CoachingDigest {
    /// The delivery identity for this publication. A rebuilt digest must not reuse the original
    /// send's identity: the provider idempotency key is derived from it and would collapse the
    /// second send into the first.
    pub(crate) fn delivery_id(&self) -> String {
        if self.regeneration_count == 0 {
            self.digest_id.clone()
        } else {
            format!("{}-r{}", self.digest_id, self.regeneration_count)
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "one publication carries its complete owner, window, ordinal, and email context"
    )]
    pub(crate) fn daily(
        owner_key: DailyCoachingOwnerKey,
        digest_id: String,
        coverage_date: NaiveDate,
        published_at: DateTime<Utc>,
        regeneration_count: u32,
        email_delivery_eligible: bool,
        timezone: String,
        reviewed_games: &[FrozenDigestGame],
    ) -> Result<(Self, Vec<DigestedGameCard>), CoachingDigestError> {
        if reviewed_games.is_empty() || reviewed_games.len() > 10 {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        let priority_games = reviewed_games
            .iter()
            .map(|game| (game.selected.clone(), game.review.clone()))
            .collect::<Vec<_>>();
        let priority_projection = priorities::project(&priority_games)?;
        let cards = reviewed_games
            .iter()
            .map(|game| {
                DigestedGameCard::new(
                    &digest_id,
                    &game.selected,
                    &game.review,
                    game.window_kind,
                    published_at,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let learning_path_count = learning_path_count(&cards);
        let digest = Self {
            schema_version: DIGEST_SCHEMA_VERSION,
            digest_id,
            owner_key,
            window_kind: CoachingWindowKind::Daily,
            coverage_date,
            published_at,
            regeneration_count,
            email_delivery_eligible,
            timezone,
            ordered_card_keys: cards
                .iter()
                .map(|card| card.digested_game_key.clone())
                .collect(),
            game_import_ids: cards
                .iter()
                .map(|card| card.game_import_id.clone())
                .collect(),
            game_count: u8::try_from(cards.len())
                .map_err(|_| CoachingDigestError::InvalidAggregate)?,
            learning_path_count: learning_path_count
                .ok_or(CoachingDigestError::InvalidAggregate)?,
            priority_policy_version: PRIORITY_POLICY_VERSION,
            learning_plan_selection_policy_version: priority_projection
                .learning_plan_selection_policy_version,
            learning_resource_catalog_version: priority_projection
                .learning_resource_catalog_version,
            priorities: priority_projection.priorities,
        };
        digest.validate_new(&cards, &priority_games)?;
        Ok((digest, cards))
    }

    pub(crate) fn validate_new(
        &self,
        cards: &[DigestedGameCard],
        reviewed_games: &[(ProfileGameWindowEntry, FrozenDailyGameReview)],
    ) -> Result<(), CoachingDigestError> {
        if self.priority_policy_version != PRIORITY_POLICY_VERSION {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        self.validate(cards)?;
        let expected = priorities::project(reviewed_games)?;
        if self.learning_plan_selection_policy_version
            != expected.learning_plan_selection_policy_version
            || self.learning_resource_catalog_version != expected.learning_resource_catalog_version
            || self.priorities != expected.priorities
        {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        Ok(())
    }

    pub(crate) fn validate(&self, cards: &[DigestedGameCard]) -> Result<(), CoachingDigestError> {
        self.validate_summary()?;
        let card_keys = cards
            .iter()
            .map(|card| card.digested_game_key.clone())
            .collect::<BTreeSet<_>>();
        let import_ids = cards
            .iter()
            .map(|card| card.game_import_id.clone())
            .collect::<BTreeSet<_>>();
        let learning_path_count = learning_path_count(cards);
        if usize::from(self.game_count) != cards.len()
            || card_keys.len() != cards.len()
            || import_ids.len() != cards.len()
            || learning_path_count != Some(self.learning_path_count)
            || self.ordered_card_keys
                != cards
                    .iter()
                    .map(|card| card.digested_game_key.clone())
                    .collect::<Vec<_>>()
            || self.game_import_ids
                != cards
                    .iter()
                    .map(|card| card.game_import_id.clone())
                    .collect::<Vec<_>>()
            || cards
                .iter()
                .any(|card| card.digest_id != self.digest_id || card.validate().is_err())
        {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        Ok(())
    }

    pub(crate) fn validate_summary(&self) -> Result<(), CoachingDigestError> {
        let card_keys = self.ordered_card_keys.iter().collect::<BTreeSet<_>>();
        let import_ids = self.game_import_ids.iter().collect::<BTreeSet<_>>();
        if self.schema_version != DIGEST_SCHEMA_VERSION
            || self.digest_id != format!("daily-{}", self.coverage_date)
            || self.owner_key.as_str().is_empty()
            || self.timezone.parse::<chrono_tz::Tz>().is_err()
            || self.game_count == 0
            || self.game_count > 10
            || usize::from(self.game_count) != self.ordered_card_keys.len()
            || usize::from(self.game_count) != self.game_import_ids.len()
            || card_keys.len() != self.ordered_card_keys.len()
            || self
                .ordered_card_keys
                .iter()
                .any(|key| key.trim().is_empty())
            || import_ids.len() != self.game_import_ids.len()
            || priorities::validate_archived(&self.priorities, &self.game_import_ids).is_err()
        {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        Ok(())
    }
}

struct FrozenGameFacts<'a> {
    source_identity: &'a ProfileGameSourceIdentity,
    source_profile: &'a str,
    time_control_raw: &'a str,
    time_control_class: ProfileGameTimeControlClass,
    expected_clock_seconds: Option<u64>,
    review_side: ReviewSide,
    player_outcome: PlayerRelativeOutcome,
    termination: DailyGameTermination,
    opening_eco: Option<&'a str>,
    opening_name: Option<&'a str>,
    opponent_name: Option<&'a str>,
    player_rating: Option<u16>,
    opponent_rating: Option<u16>,
    played_plies: u32,
}

fn validate_frozen_game_facts(facts: FrozenGameFacts<'_>) -> Result<(), CoachingDigestError> {
    let opening_is_valid = match (facts.opening_eco, facts.opening_name) {
        (Some(eco), Some(name)) => !eco.trim().is_empty() && !name.trim().is_empty(),
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    };
    let ratings_are_valid = [facts.player_rating, facts.opponent_rating]
        .into_iter()
        .flatten()
        .all(|rating| EloRating::try_from(rating).is_ok());
    let outcome_is_valid = matches!(
        (facts.player_outcome, facts.termination),
        (
            PlayerRelativeOutcome::Win | PlayerRelativeOutcome::Loss,
            DailyGameTermination::Decisive(_)
        ) | (PlayerRelativeOutcome::Draw, DailyGameTermination::Draw(_))
    );
    if !facts
        .source_identity
        .is_valid_for_profile(facts.source_profile)
        || !facts
            .time_control_class
            .facts_are_valid(facts.time_control_raw, facts.expected_clock_seconds)
        || !matches!(facts.review_side, ReviewSide::White | ReviewSide::Black)
        || !outcome_is_valid
        || !opening_is_valid
        || facts
            .opponent_name
            .is_some_and(|name| name.trim().is_empty())
        || !ratings_are_valid
        || facts.played_plies == 0
    {
        Err(CoachingDigestError::InvalidAggregate)
    } else {
        Ok(())
    }
}

fn learning_path_count(cards: &[DigestedGameCard]) -> Option<u16> {
    cards.iter().try_fold(0_u16, |count, card| {
        count.checked_add(card.learning_path_count)
    })
}

fn metadata_text(value: &MetadataText) -> Option<String> {
    match value {
        MetadataText::Present { value } => Some(value.clone()),
        MetadataText::Absent => None,
    }
}

fn rating(value: &RatingMetadata) -> Option<u16> {
    match value {
        RatingMetadata::Present { rating } => Some(rating.value()),
        RatingMetadata::Absent => None,
    }
}

fn outcome(
    game_outcome: CompletedGameOutcome,
    player: Color,
) -> (PlayerRelativeOutcome, DailyGameTermination) {
    match game_outcome {
        CompletedGameOutcome::Decisive {
            winner,
            termination,
        } => (
            if winner == player {
                PlayerRelativeOutcome::Win
            } else {
                PlayerRelativeOutcome::Loss
            },
            DailyGameTermination::Decisive(termination),
        ),
        CompletedGameOutcome::Draw { termination } => (
            PlayerRelativeOutcome::Draw,
            DailyGameTermination::Draw(termination),
        ),
    }
}

fn source_identity(provenance: &ImportProvenance) -> Option<ProfileGameSourceIdentity> {
    match provenance {
        ImportProvenance::Lichess {
            canonical_game_id, ..
        } => Some(ProfileGameSourceIdentity::lichess(
            canonical_game_id.as_str().to_string(),
        )),
        ImportProvenance::ChessCom { canonical_url, .. } => {
            ProfileGameSourceIdentity::chess_com_url(canonical_url)
        }
        ImportProvenance::PastedPgn { .. } | ImportProvenance::LocalPgn { .. } => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CoachingDigestError {
    #[error("Coaching Digest aggregate is invalid")]
    InvalidAggregate,
}
