use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use tokio::task::JoinSet;

use crate::{
    daily_coaching::{
        DailyCoachingOwnerKey, DailyCoachingRunStoreError, DailyCoachingRuntime, DigestedGameCard,
    },
    game_import_store::{
        GameImportLookup, GameImportRecord, GameImportStore, GameImportStoreError,
    },
    imported_games::{
        player_facts, time_control_class_from_event, ImportedGameCard, ImportedGameOpening,
        ImportedGameOutcome, ImportedGameProvider, ImportedGameReviewSide,
        ImportedGameSourceIdentity, ImportedGameTimeControlClass, ImportedGamesRuntime,
    },
    review_durability::path::hashed_path_segment,
    review_session_contract::{
        GameImportId, GameReview, LearningTrackKey, OpeningMetadata, PlayerId,
    },
    review_session_processor::ProcessorPrincipal,
};

mod filters;
mod model;
#[cfg(test)]
use filters::fold_diacritics;
use filters::SearchFilters;
mod key;
pub use key::ReviewedGameKey;
pub use model::{
    ReviewedGameSearchCard, ReviewedGameSearchCoverage, ReviewedGameSearchError,
    ReviewedGameSearchRequest, ReviewedGameSearchResult, ReviewedGameSearchTruncation,
};

#[cfg(test)]
#[path = "reviewed_games/tests.rs"]
mod tests;

pub const REVIEWED_GAME_SEARCH_LIMIT: usize = 20;

pub async fn search_reviewed_games(
    daily_coaching: &DailyCoachingRuntime,
    imported_games: &ImportedGamesRuntime,
    player_id: &PlayerId,
    request: ReviewedGameSearchRequest,
) -> Result<ReviewedGameSearchResult, ReviewedGameSearchError> {
    let filters = SearchFilters::parse(&request)?;
    let owner_key = DailyCoachingOwnerKey::for_player(player_id);
    let principal = ProcessorPrincipal::Player(player_id.clone());
    let run_store = daily_coaching.run_store();
    let game_import_store = imported_games.store();
    let (digested, imported, records) = tokio::join!(
        run_store.list_digested_game_cards(&owner_key),
        game_import_store.list_imported_game_cards(&principal),
        game_import_store.list_game_import_records(&principal),
    );
    let digested = listed_digested_cards(digested)?;
    let imported = listed_imported_cards(imported)?;
    let records = listed_game_import_records(records)?;
    let record_by_id = records
        .iter()
        .map(|record| (record.game_import_id.clone(), record.clone()))
        .collect();
    let merged = merge_cards(digested, imported, &records);
    /* Resolve every card against its Game Import Record before anything is
    counted: a card that cannot hydrate is invisible, and count, coverage,
    and page must all describe the same survivor set or the client-side
    contract decoder rejects the whole response. */
    let resolved = resolve_records(merged, principal, game_import_store, record_by_id).await;
    let coverage = coverage(&resolved)?;
    let selected = select_matches(resolved, &filters)?;
    let truncation = if selected.total_match_count as usize > REVIEWED_GAME_SEARCH_LIMIT {
        ReviewedGameSearchTruncation::Truncated {
            total_match_count: selected.total_match_count,
            oldest_returned_at: selected.oldest_returned_at.ok_or({
                ReviewedGameSearchError::GameImportStore(GameImportStoreError::InvalidRecord)
            })?,
        }
    } else {
        ReviewedGameSearchTruncation::Complete {
            total_match_count: selected.total_match_count,
        }
    };
    Ok(ReviewedGameSearchResult {
        games: selected.games,
        coverage,
        truncation,
    })
}

#[derive(Debug, Clone)]
struct MergedCard {
    reviewed_game_key: String,
    canonical_source_key: String,
    game_import_id: GameImportId,
    provider: ImportedGameProvider,
    review_side: ImportedGameReviewSide,
    outcome: Option<ImportedGameOutcome>,
    opening: Option<ImportedGameOpening>,
    opponent_name: Option<String>,
    opponent_rating: Option<u16>,
    ended_at: Option<DateTime<Utc>>,
    imported_at: DateTime<Utc>,
    time_control_class: Option<ImportedGameTimeControlClass>,
    learning_path_count: u16,
    digested: bool,
    imported: bool,
    digest_id: Option<String>,
    digest_date: Option<String>,
}

impl MergedCard {
    fn identity_key(&self) -> ReviewedGameKey {
        ReviewedGameKey {
            canonical_source_key: self.canonical_source_key.clone(),
            review_side: self.review_side,
        }
    }

    fn recency_at(&self) -> DateTime<Utc> {
        self.ended_at.unwrap_or(self.imported_at)
    }

    fn from_digested(card: DigestedGameCard) -> Result<Self, ReviewedGameSearchError> {
        let digest_date = card
            .digest_id
            .strip_prefix("daily-")
            .ok_or(GameImportStoreError::InvalidRecord)?
            .to_string();
        let canonical_source_key = card.canonical_source_key();
        let review_side = card.review_side();
        let provider = card.provider();
        let outcome = card.outcome();
        let opening = card.opening();
        let time_control_class = card.time_control_class();
        Ok(Self {
            reviewed_game_key: reviewed_game_key(&canonical_source_key, review_side),
            canonical_source_key,
            game_import_id: card.game_import_id,
            provider,
            review_side,
            outcome: Some(outcome),
            opening,
            opponent_name: card.opponent_name,
            opponent_rating: card.opponent_rating,
            ended_at: Some(card.ended_at),
            imported_at: card.ended_at,
            time_control_class: Some(time_control_class),
            learning_path_count: card.learning_path_count,
            digested: true,
            imported: false,
            digest_id: Some(card.digest_id),
            digest_date: Some(digest_date),
        })
    }

    fn from_imported(card: ImportedGameCard) -> Self {
        let canonical_source_key = card.canonical_source_key();
        let review_side = card.review_side();
        Self {
            reviewed_game_key: reviewed_game_key(&canonical_source_key, review_side),
            canonical_source_key,
            game_import_id: card.game_import_id().clone(),
            provider: card.provider(),
            review_side,
            outcome: card.outcome(),
            opening: card.opening(),
            opponent_name: card.opponent_name(),
            opponent_rating: card.opponent_rating(),
            ended_at: Some(card.ended_at()),
            imported_at: card.imported_at,
            time_control_class: card.time_control_class(),
            learning_path_count: card.learning_path_count(),
            digested: false,
            imported: true,
            digest_id: None,
            digest_date: None,
        }
    }

    fn from_record(record: &GameImportRecord) -> Option<Self> {
        let game = &record.imported_game;
        let source = ImportedGameSourceIdentity::for_search(game)?;
        let canonical_source_key = source.canonical_key();
        let review_side = ImportedGameReviewSide::from(game.review_side);
        let (outcome, opponent_name, _, opponent_rating) = player_facts(game);
        let opening = match &game.game.opening {
            OpeningMetadata::Present { eco, name, .. } => Some(ImportedGameOpening {
                eco: eco.clone(),
                name: name.clone(),
            }),
            OpeningMetadata::Absent => None,
        };
        Some(Self {
            reviewed_game_key: reviewed_game_key(&canonical_source_key, review_side),
            canonical_source_key,
            game_import_id: record.game_import_id.clone(),
            provider: source.provider(),
            review_side,
            outcome: outcome.map(Into::into),
            opening,
            opponent_name,
            opponent_rating,
            ended_at: None,
            imported_at: record.created_at,
            time_control_class: time_control_class_from_event(&game.game.event),
            learning_path_count: learning_path_count(record.review())?,
            digested: false,
            imported: true,
            digest_id: None,
            digest_date: None,
        })
    }
}

fn listed_digested_cards(
    result: Result<Vec<DigestedGameCard>, DailyCoachingRunStoreError>,
) -> Result<Vec<DigestedGameCard>, ReviewedGameSearchError> {
    match result {
        Ok(cards) => Ok(cards),
        Err(DailyCoachingRunStoreError::InvalidRecord) => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn listed_imported_cards(
    result: Result<Vec<ImportedGameCard>, GameImportStoreError>,
) -> Result<Vec<ImportedGameCard>, ReviewedGameSearchError> {
    match result {
        Ok(cards) => Ok(cards),
        Err(GameImportStoreError::InvalidRecord) => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn listed_game_import_records(
    result: Result<Vec<GameImportRecord>, GameImportStoreError>,
) -> Result<Vec<GameImportRecord>, ReviewedGameSearchError> {
    match result {
        Ok(records) => Ok(records),
        Err(GameImportStoreError::InvalidRecord) => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn merge_cards(
    digested: Vec<DigestedGameCard>,
    imported: Vec<ImportedGameCard>,
    records: &[GameImportRecord],
) -> BTreeMap<ReviewedGameKey, MergedCard> {
    let mut merged = BTreeMap::new();
    for card in digested {
        if card.validate().is_err() {
            continue;
        }
        if let Ok(card) = MergedCard::from_digested(card) {
            merged.insert(card.identity_key(), card);
        }
    }
    for card in imported {
        if card.is_valid() {
            merge_imported_card(&mut merged, MergedCard::from_imported(card));
        }
    }
    /* One Game can be imported more than once at different Elo Profiles. The
    Elo is part of a Game Import's identity, so each import is its own record,
    while an Imported Game card and this merge both key on the Game and the
    reviewed side alone. The newest import is the one shown. Choosing it here
    rather than letting the store's listing order choose is what makes "the
    latest review" a rule instead of whichever document Firestore happened to
    return first; the Game Import ID breaks a tie so the answer is stable. */
    let mut newest_first = records.iter().collect::<Vec<_>>();
    newest_first.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.game_import_id.cmp(&left.game_import_id))
    });
    for record in newest_first {
        if let Some(card) = MergedCard::from_record(record) {
            merged.entry(card.identity_key()).or_insert(card);
        }
    }
    merged
}

fn merge_imported_card(
    merged: &mut BTreeMap<ReviewedGameKey, MergedCard>,
    mut imported: MergedCard,
) {
    let key = imported.identity_key();
    if let Some(digested) = merged.remove(&key) {
        imported.digested = true;
        imported.digest_id = digested.digest_id;
        imported.digest_date = digested.digest_date;
    }
    merged.insert(key, imported);
}

fn select_matches(
    resolved: Vec<ResolvedCard>,
    filters: &SearchFilters,
) -> Result<SelectedMatches, ReviewedGameSearchError> {
    let mut matches = resolved
        .into_iter()
        .filter(|resolved| filters.matches(&resolved.card))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .card
            .recency_at()
            .cmp(&left.card.recency_at())
            .then_with(|| {
                right
                    .card
                    .reviewed_game_key
                    .cmp(&left.card.reviewed_game_key)
            })
    });
    let total_match_count = u32::try_from(matches.len()).map_err(|_| {
        ReviewedGameSearchError::GameImportStore(GameImportStoreError::InvalidRecord)
    })?;
    matches.truncate(REVIEWED_GAME_SEARCH_LIMIT);
    let oldest_returned_at = matches
        .last()
        .map(|resolved| timestamp(resolved.card.recency_at()));
    Ok(SelectedMatches {
        total_match_count,
        games: matches
            .into_iter()
            .map(|resolved| resolved.projected)
            .collect(),
        oldest_returned_at,
    })
}

struct SelectedMatches {
    total_match_count: u32,
    games: Vec<ReviewedGameSearchCard>,
    oldest_returned_at: Option<String>,
}

/// A card whose Game Import Record answered, projected and ready to serve.
struct ResolvedCard {
    card: MergedCard,
    projected: ReviewedGameSearchCard,
}

fn coverage(
    resolved: &[ResolvedCard],
) -> Result<ReviewedGameSearchCoverage, ReviewedGameSearchError> {
    let reviewed_game_count = u32::try_from(resolved.len()).map_err(|_| {
        ReviewedGameSearchError::GameImportStore(GameImportStoreError::InvalidRecord)
    })?;
    let earliest = resolved
        .iter()
        .filter_map(|resolved| resolved.card.ended_at)
        .min();
    let latest = resolved
        .iter()
        .filter_map(|resolved| resolved.card.ended_at)
        .max();
    Ok(ReviewedGameSearchCoverage {
        reviewed_game_count,
        earliest_played_at: earliest.map(timestamp),
        latest_played_at: latest.map(timestamp),
    })
}

async fn resolve_records(
    cards: BTreeMap<ReviewedGameKey, MergedCard>,
    principal: ProcessorPrincipal,
    store: std::sync::Arc<dyn GameImportStore>,
    records: BTreeMap<GameImportId, GameImportRecord>,
) -> Vec<ResolvedCard> {
    let mut resolved = Vec::new();
    let mut reads = JoinSet::new();
    for card in cards.into_values() {
        if let Some(record) = records.get(&card.game_import_id) {
            if let Some(projected) = project_from_record(card.clone(), record) {
                resolved.push(ResolvedCard { card, projected });
            }
            continue;
        }
        let store = store.clone();
        let principal = principal.clone();
        reads.spawn(async move {
            let lookup = store.find(&principal, &card.game_import_id).await;
            (card, lookup)
        });
    }
    while let Some(joined) = reads.join_next().await {
        let Ok((card, lookup)) = joined else {
            continue;
        };
        let Ok(GameImportLookup::Found(record)) = lookup else {
            continue;
        };
        if let Some(projected) = project_from_record(card.clone(), &record) {
            resolved.push(ResolvedCard { card, projected });
        }
    }
    resolved
}

fn project_from_record(
    card: MergedCard,
    record: &GameImportRecord,
) -> Option<ReviewedGameSearchCard> {
    let learning_path_count = learning_path_count(record.review())?;
    let learning_track_keys = record
        .review()
        .learning_plan
        .tracks
        .iter()
        .map(|track| track.key.clone())
        .collect();
    let mut projected = project_card(card, learning_track_keys);
    projected.learning_path_count = learning_path_count;
    Some(projected)
}

fn learning_path_count(review: &GameReview) -> Option<u16> {
    review
        .learning_plan
        .tracks
        .iter()
        .try_fold(0_u16, |count, track| {
            count.checked_add(u16::try_from(track.support.len()).ok()?)
        })
}

fn project_card(
    card: MergedCard,
    learning_track_keys: Vec<LearningTrackKey>,
) -> ReviewedGameSearchCard {
    ReviewedGameSearchCard {
        reviewed_game_key: card.reviewed_game_key,
        game_import_id: card.game_import_id,
        provider: card.provider,
        review_side: card.review_side,
        outcome: card.outcome,
        opening: card.opening,
        opponent_name: card.opponent_name,
        opponent_rating: card.opponent_rating,
        ended_at: card.ended_at.map(timestamp),
        time_control_class: card.time_control_class,
        learning_path_count: card.learning_path_count,
        learning_track_keys,
        digested: card.digested,
        imported: card.imported,
        digest_id: card.digest_id,
        digest_date: card.digest_date,
    }
}

fn reviewed_game_key(canonical_source_key: &str, review_side: ImportedGameReviewSide) -> String {
    hashed_path_segment(
        ReviewedGameKey {
            canonical_source_key: canonical_source_key.to_string(),
            review_side,
        }
        .to_string(),
    )
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
