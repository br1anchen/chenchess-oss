use std::{collections::BTreeSet, sync::Arc};

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

mod played_openings;

use played_openings::aggregate_played_openings;
pub use played_openings::{PlayedOpeningAggregate, PlayedOpeningsResult};

use crate::{
    chess_com::{ChessComGameKind, ChessComGameUrl},
    digested_games::{DigestedGameIndex, DigestedGameLookupError, NoDigestedGames},
    game_import_store::{GameImportStore, GameImportStoreError},
    pgn::parse_pgn_with_metadata,
    profile_game_feed::ProfileGameTimeControlClass,
    review_durability::path::hashed_path_segment,
    review_session_contract::{
        CompletedGameOutcome, GameImportId, ImportProvenance, ImportedGame, MetadataText,
        OpeningMetadata, RatingMetadata, ReviewSide,
    },
    review_session_processor::ProcessorPrincipal,
    reviewed_games::ReviewedGameKey,
};

const IMPORTED_GAME_CARD_SCHEMA_VERSION: u8 = 1;
pub const IMPORTED_GAMES_PAGE_SIZE: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ImportedGameSourceIdentity {
    Lichess {
        canonical_game_id: String,
    },
    ChessCom {
        kind: ImportedChessComGameKind,
        canonical_game_id: String,
    },
    PastedPgn {
        pgn_digest: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ImportedChessComGameKind {
    Computer,
    Daily,
    Live,
}

impl ImportedGameSourceIdentity {
    fn from_imported_game(game: &ImportedGame) -> Result<Self, ImportedGameCardError> {
        match &game.provenance {
            ImportProvenance::Lichess {
                canonical_game_id, ..
            } => Ok(Self::Lichess {
                canonical_game_id: canonical_game_id.as_str().to_string(),
            }),
            ImportProvenance::ChessCom {
                canonical_game_id,
                canonical_url,
                ..
            } => {
                let source = ChessComGameUrl::parse(canonical_url)
                    .map_err(|_| ImportedGameCardError::Invalid)?;
                let kind = match source.kind() {
                    ChessComGameKind::Computer => ImportedChessComGameKind::Computer,
                    ChessComGameKind::Daily => ImportedChessComGameKind::Daily,
                    ChessComGameKind::Live => ImportedChessComGameKind::Live,
                };
                Ok(Self::ChessCom {
                    kind,
                    canonical_game_id: canonical_game_id.as_str().to_string(),
                })
            }
            ImportProvenance::PastedPgn { pgn_digest } => Ok(Self::PastedPgn {
                pgn_digest: pgn_digest.as_str().to_string(),
            }),
            ImportProvenance::LocalPgn { .. } => Err(ImportedGameCardError::Invalid),
        }
    }

    pub(crate) fn for_search(game: &ImportedGame) -> Option<Self> {
        match Self::from_imported_game(game) {
            Ok(identity) => Some(identity),
            Err(_) => match &game.provenance {
                ImportProvenance::LocalPgn { pgn_digest } => Some(Self::PastedPgn {
                    pgn_digest: pgn_digest.as_str().to_string(),
                }),
                ImportProvenance::Lichess { .. }
                | ImportProvenance::ChessCom { .. }
                | ImportProvenance::PastedPgn { .. } => None,
            },
        }
    }

    pub(crate) fn canonical_key(&self) -> String {
        match self {
            Self::Lichess { canonical_game_id } => format!("lichess:{canonical_game_id}"),
            Self::ChessCom {
                kind,
                canonical_game_id,
            } => format!("chessCom:{}:{canonical_game_id}", kind.as_str()),
            Self::PastedPgn { pgn_digest } => format!("pastedPgn:{pgn_digest}"),
        }
    }

    pub(crate) fn provider(&self) -> ImportedGameProvider {
        match self {
            Self::Lichess { .. } => ImportedGameProvider::Lichess,
            Self::ChessCom { .. } => ImportedGameProvider::ChessCom,
            Self::PastedPgn { .. } => ImportedGameProvider::PastedPgn,
        }
    }
}

impl ImportedChessComGameKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Computer => "computer",
            Self::Daily => "daily",
            Self::Live => "live",
        }
    }
}

#[derive(Debug)]
pub(crate) enum ImportedGameCardError {
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ImportedPlayerOutcome {
    Win,
    Loss,
    Draw,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedGameCard {
    schema_version: u8,
    pub(crate) imported_game_key: String,
    pub(crate) game_import_id: GameImportId,
    source_identity: ImportedGameSourceIdentity,
    pub(crate) imported_at: DateTime<Utc>,
    pub(crate) ended_at: DateTime<Utc>,
    time_control_raw: Option<String>,
    time_control_class: Option<ProfileGameTimeControlClass>,
    expected_clock_seconds: Option<u64>,
    review_side: ReviewSide,
    player_outcome: Option<ImportedPlayerOutcome>,
    termination: CompletedGameOutcome,
    opening_eco: Option<String>,
    opening_name: Option<String>,
    opponent_name: Option<String>,
    player_rating: Option<u16>,
    opponent_rating: Option<u16>,
    played_plies: u32,
    learning_path_count: u16,
}

impl ImportedGameCard {
    pub fn game_import_id(&self) -> &GameImportId {
        &self.game_import_id
    }

    pub(crate) fn canonical_source_key(&self) -> String {
        self.source_identity.canonical_key()
    }

    pub(crate) fn provider(&self) -> ImportedGameProvider {
        self.source_identity.provider()
    }

    pub(crate) fn review_side(&self) -> ImportedGameReviewSide {
        self.review_side.into()
    }

    pub(crate) fn outcome(&self) -> Option<ImportedGameOutcome> {
        self.player_outcome.map(Into::into)
    }

    pub(crate) fn opening(&self) -> Option<ImportedGameOpening> {
        self.opening_eco
            .as_ref()
            .zip(self.opening_name.as_ref())
            .map(|(eco, name)| ImportedGameOpening {
                eco: eco.clone(),
                name: name.clone(),
            })
    }

    pub(crate) fn opponent_name(&self) -> Option<String> {
        self.opponent_name.clone()
    }

    pub(crate) fn opponent_rating(&self) -> Option<u16> {
        self.opponent_rating
    }

    pub(crate) fn ended_at(&self) -> DateTime<Utc> {
        self.ended_at
    }

    pub(crate) fn time_control_class(&self) -> Option<ImportedGameTimeControlClass> {
        self.time_control_class.map(Into::into)
    }

    pub(crate) fn learning_path_count(&self) -> u16 {
        self.learning_path_count
    }

    pub(crate) fn new(
        game_import_id: GameImportId,
        game: &ImportedGame,
        pgn: &str,
        learning_path_count: u16,
        imported_at: DateTime<Utc>,
    ) -> Result<Self, ImportedGameCardError> {
        let source_identity = ImportedGameSourceIdentity::from_imported_game(game)?;
        let imported_game_key = imported_game_key(&source_identity, game.review_side);
        let metadata = parse_pgn_with_metadata(pgn)
            .map_err(|_| ImportedGameCardError::Invalid)?
            .metadata;
        let ended_at = pgn_ended_at(&metadata, imported_at);
        let (time_control_raw, time_control_class, expected_clock_seconds) = metadata
            .time_control
            .as_deref()
            .and_then(parse_time_control)
            .map_or((None, None, None), |(raw, class, expected)| {
                (Some(raw), Some(class), expected)
            });
        let (player_outcome, opponent_name, player_rating, opponent_rating) = player_facts(game);
        let (opening_eco, opening_name) = match &game.game.opening {
            OpeningMetadata::Present { eco, name, .. } => (Some(eco.clone()), Some(name.clone())),
            OpeningMetadata::Absent => (None, None),
        };
        let card = Self {
            schema_version: IMPORTED_GAME_CARD_SCHEMA_VERSION,
            imported_game_key,
            game_import_id,
            source_identity,
            imported_at,
            ended_at,
            time_control_raw,
            time_control_class,
            expected_clock_seconds,
            review_side: game.review_side,
            player_outcome,
            termination: game.game.outcome,
            opening_eco,
            opening_name,
            opponent_name,
            player_rating,
            opponent_rating,
            played_plies: u32::try_from(game.game.moves.len())
                .map_err(|_| ImportedGameCardError::Invalid)?,
            learning_path_count,
        };
        if card.is_valid() {
            Ok(card)
        } else {
            Err(ImportedGameCardError::Invalid)
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == IMPORTED_GAME_CARD_SCHEMA_VERSION
            && self.imported_game_key == imported_game_key(&self.source_identity, self.review_side)
            && self.imported_at.timestamp_millis() > 0
            && self.ended_at.timestamp_millis() > 0
            && self.played_plies > 0
            && match (self.time_control_class, self.time_control_raw.as_deref()) {
                (None, None) => self.expected_clock_seconds.is_none(),
                (Some(class), Some(raw)) => class.facts_are_valid(raw, self.expected_clock_seconds),
                _ => false,
            }
    }

    fn reviewed_game_key(&self) -> ReviewedGameKey {
        ReviewedGameKey {
            canonical_source_key: self.canonical_source_key(),
            review_side: self.review_side(),
        }
    }

    fn project(&self) -> ImportedGameListItem {
        ImportedGameListItem {
            game_import_id: self.game_import_id.clone(),
            provider: self.source_identity.provider(),
            review_side: self.review_side.into(),
            outcome: self.player_outcome.map(Into::into),
            opening: self
                .opening_eco
                .as_ref()
                .zip(self.opening_name.as_ref())
                .map(|(eco, name)| ImportedGameOpening {
                    eco: eco.clone(),
                    name: name.clone(),
                }),
            opponent_name: self.opponent_name.clone(),
            ended_at: self
                .ended_at
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            time_control_raw: self.time_control_raw.clone(),
            time_control_class: self.time_control_class.map(Into::into),
            learning_path_count: self.learning_path_count,
        }
    }
}

pub(crate) fn imported_game_key(
    source: &ImportedGameSourceIdentity,
    review_side: ReviewSide,
) -> String {
    let review_side = match review_side {
        ReviewSide::White => "white",
        ReviewSide::Black => "black",
        ReviewSide::Both => "both",
    };
    hashed_path_segment(format!("{}:{review_side}", source.canonical_key()))
}

pub(crate) fn player_facts(
    game: &ImportedGame,
) -> (
    Option<ImportedPlayerOutcome>,
    Option<String>,
    Option<u16>,
    Option<u16>,
) {
    let (player, opponent, player_color) = match game.review_side {
        ReviewSide::White => (
            &game.game.white,
            &game.game.black,
            Some(crate::review_session_contract::Color::White),
        ),
        ReviewSide::Black => (
            &game.game.black,
            &game.game.white,
            Some(crate::review_session_contract::Color::Black),
        ),
        ReviewSide::Both => return (None, None, None, None),
    };
    let outcome = match game.game.outcome {
        CompletedGameOutcome::Draw { .. } => ImportedPlayerOutcome::Draw,
        CompletedGameOutcome::Decisive { winner, .. } if Some(winner) == player_color => {
            ImportedPlayerOutcome::Win
        }
        CompletedGameOutcome::Decisive { .. } => ImportedPlayerOutcome::Loss,
    };
    (
        Some(outcome),
        metadata_text(&opponent.name),
        rating(&player.rating),
        rating(&opponent.rating),
    )
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

fn pgn_ended_at(metadata: &crate::pgn::PgnMetadata, fallback: DateTime<Utc>) -> DateTime<Utc> {
    [
        (metadata.end_date.as_deref(), metadata.end_time.as_deref()),
        (metadata.utc_date.as_deref(), metadata.utc_time.as_deref()),
        (metadata.date.as_deref(), metadata.time.as_deref()),
    ]
    .into_iter()
    .find_map(|(date, time)| parse_pgn_datetime(date?, time))
    .unwrap_or(fallback)
}

fn parse_pgn_datetime(date: &str, time: Option<&str>) -> Option<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(date, "%Y.%m.%d").ok()?;
    let time = time
        .and_then(|value| NaiveTime::parse_from_str(value, "%H:%M:%S").ok())
        .unwrap_or(NaiveTime::MIN);
    Some(DateTime::from_naive_utc_and_offset(
        NaiveDateTime::new(date, time),
        Utc,
    ))
}

pub(crate) fn time_control_class_from_event(
    event: &MetadataText,
) -> Option<ImportedGameTimeControlClass> {
    let MetadataText::Present { value } = event else {
        return None;
    };
    time_control_class_from_event_label(value)
}

fn time_control_class_from_event_label(event: &str) -> Option<ImportedGameTimeControlClass> {
    let folded = event.to_ascii_lowercase();
    const CLASSES: [(&str, ImportedGameTimeControlClass); 7] = [
        ("ultrabullet", ImportedGameTimeControlClass::UltraBullet),
        ("ultra bullet", ImportedGameTimeControlClass::UltraBullet),
        (
            "correspondence",
            ImportedGameTimeControlClass::Correspondence,
        ),
        ("classical", ImportedGameTimeControlClass::Classical),
        ("bullet", ImportedGameTimeControlClass::Bullet),
        ("blitz", ImportedGameTimeControlClass::Blitz),
        ("rapid", ImportedGameTimeControlClass::Rapid),
    ];
    CLASSES
        .into_iter()
        .find_map(|(needle, class)| folded.contains(needle).then_some(class))
}

fn parse_time_control(raw: &str) -> Option<(String, ProfileGameTimeControlClass, Option<u64>)> {
    if let Some((moves, seconds)) = raw.split_once('/') {
        if moves == "1" && seconds.parse::<u64>().ok().is_some_and(|value| value > 0) {
            return Some((
                "correspondence".to_string(),
                ProfileGameTimeControlClass::Correspondence,
                None,
            ));
        }
        return None;
    }
    let (initial, increment) = raw.split_once('+').map_or_else(
        || Some((raw.parse::<u64>().ok()?, 0)),
        |(initial, increment)| Some((initial.parse::<u64>().ok()?, increment.parse::<u64>().ok()?)),
    )?;
    let expected = initial.checked_add(increment.checked_mul(40)?)?;
    if expected == 0 {
        return None;
    }
    let class = match expected {
        0..=29 => ProfileGameTimeControlClass::UltraBullet,
        30..=179 => ProfileGameTimeControlClass::Bullet,
        180..=479 => ProfileGameTimeControlClass::Blitz,
        480..=1499 => ProfileGameTimeControlClass::Rapid,
        _ => ProfileGameTimeControlClass::Classical,
    };
    let normalized = if increment == 0 {
        initial.to_string()
    } else {
        format!("{initial}+{increment}")
    };
    Some((normalized, class, Some(expected)))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedGameListPage {
    pub games: Vec<ImportedGameListItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedGameListItem {
    pub game_import_id: GameImportId,
    pub provider: ImportedGameProvider,
    pub review_side: ImportedGameReviewSide,
    pub outcome: Option<ImportedGameOutcome>,
    pub opening: Option<ImportedGameOpening>,
    pub opponent_name: Option<String>,
    pub ended_at: String,
    pub time_control_raw: Option<String>,
    pub time_control_class: Option<ImportedGameTimeControlClass>,
    pub learning_path_count: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedGameOpening {
    pub eco: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ImportedGameProvider {
    Lichess,
    ChessCom,
    PastedPgn,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum ImportedGameReviewSide {
    White,
    Black,
    Both,
}

impl From<ReviewSide> for ImportedGameReviewSide {
    fn from(value: ReviewSide) -> Self {
        match value {
            ReviewSide::White => Self::White,
            ReviewSide::Black => Self::Black,
            ReviewSide::Both => Self::Both,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ImportedGameOutcome {
    Win,
    Loss,
    Draw,
}

impl From<ImportedPlayerOutcome> for ImportedGameOutcome {
    fn from(value: ImportedPlayerOutcome) -> Self {
        match value {
            ImportedPlayerOutcome::Win => Self::Win,
            ImportedPlayerOutcome::Loss => Self::Loss,
            ImportedPlayerOutcome::Draw => Self::Draw,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ImportedGameTimeControlClass {
    Classical,
    Correspondence,
    Rapid,
    Blitz,
    Bullet,
    UltraBullet,
}

impl From<ProfileGameTimeControlClass> for ImportedGameTimeControlClass {
    fn from(value: ProfileGameTimeControlClass) -> Self {
        match value {
            ProfileGameTimeControlClass::Classical => Self::Classical,
            ProfileGameTimeControlClass::Correspondence => Self::Correspondence,
            ProfileGameTimeControlClass::Rapid => Self::Rapid,
            ProfileGameTimeControlClass::Blitz => Self::Blitz,
            ProfileGameTimeControlClass::Bullet => Self::Bullet,
            ProfileGameTimeControlClass::UltraBullet => Self::UltraBullet,
        }
    }
}

#[derive(Clone)]
pub struct ImportedGamesRuntime {
    store: Arc<dyn GameImportStore>,
    digested_games: Arc<dyn DigestedGameIndex>,
}

impl ImportedGamesRuntime {
    pub(crate) fn new(
        store: Arc<dyn GameImportStore>,
        digested_games: Arc<dyn DigestedGameIndex>,
    ) -> Self {
        Self {
            store,
            digested_games,
        }
    }

    pub fn in_memory() -> Self {
        Self::new(
            Arc::new(crate::game_import_store::InMemoryGameImportStore::default()),
            Arc::new(NoDigestedGames),
        )
    }

    pub(crate) fn store(&self) -> Arc<dyn GameImportStore> {
        self.store.clone()
    }

    pub async fn page(
        &self,
        owner: &crate::review_session_contract::PlayerId,
        cursor: Option<&str>,
    ) -> Result<ImportedGameListPage, ImportedGamesError> {
        let after = cursor.map(decode_cursor).transpose()?;
        let cards = self
            .store
            .list_imported_game_cards(&ProcessorPrincipal::Player(owner.clone()))
            .await?;
        let digested = self.digested_games.digested_games(owner).await?;
        project_page(cards, after, &digested)
    }

    pub async fn played_openings(
        &self,
        owner: &crate::review_session_contract::PlayerId,
    ) -> Result<PlayedOpeningsResult, ImportedGamesError> {
        let cards = self
            .store
            .list_imported_game_cards(&ProcessorPrincipal::Player(owner.clone()))
            .await?;
        Ok(aggregate_played_openings(&cards))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportedGamesError {
    #[error("invalid Imported Games cursor")]
    InvalidCursor,
    #[error(transparent)]
    Store(#[from] GameImportStoreError),
    #[error(transparent)]
    DigestedGames(#[from] DigestedGameLookupError),
}

impl ImportedGamesError {
    /// What failed, for the one log line the route writes. The route answers
    /// every unavailable listing the same way, so only the category differs.
    pub(crate) fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::InvalidCursor => "invalid-cursor",
            Self::Store(error) => error.diagnostic_category(),
            Self::DigestedGames(_) => "daily-coaching",
        }
    }
}

/// The listing is what the Player may act on, and the one action it offers is
/// delete. A Game Daily Coaching digested is refused that — a published
/// Coaching Digest cites its supporting Games — so it is not listed here at
/// all; it lives on the Digest that cites it. Dropping it before the page is
/// cut keeps a page a full page of Games the Player can actually remove.
fn project_page(
    mut cards: Vec<ImportedGameCard>,
    after: Option<(DateTime<Utc>, String)>,
    digested: &BTreeSet<ReviewedGameKey>,
) -> Result<ImportedGameListPage, ImportedGamesError> {
    if cards.iter().any(|card| !card.is_valid()) {
        return Err(ImportedGamesError::Store(
            GameImportStoreError::InvalidRecord,
        ));
    }
    cards.retain(|card| !digested.contains(&card.reviewed_game_key()));
    cards.sort_by(|left, right| {
        right
            .ended_at
            .cmp(&left.ended_at)
            .then_with(|| right.imported_game_key.cmp(&left.imported_game_key))
    });
    if let Some((ended_at, key)) = after {
        cards.retain(|card| {
            card.ended_at < ended_at || (card.ended_at == ended_at && card.imported_game_key < key)
        });
    }
    let has_more = cards.len() > IMPORTED_GAMES_PAGE_SIZE;
    cards.truncate(IMPORTED_GAMES_PAGE_SIZE);
    let next_cursor = if has_more {
        cards
            .last()
            .map(|card| encode_cursor(card.ended_at, &card.imported_game_key))
    } else {
        None
    };
    Ok(ImportedGameListPage {
        games: cards.iter().map(ImportedGameCard::project).collect(),
        next_cursor,
    })
}

fn encode_cursor(ended_at: DateTime<Utc>, key: &str) -> String {
    let material = format!("{}:{key}", ended_at.timestamp_millis());
    crate::review_durability::path::lower_hex(material.as_bytes())
}

fn decode_cursor(cursor: &str) -> Result<(DateTime<Utc>, String), ImportedGamesError> {
    if cursor.is_empty()
        || cursor.len() > 192
        || !cursor.len().is_multiple_of(2)
        || !cursor.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(ImportedGamesError::InvalidCursor);
    }
    let bytes = cursor
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|value| u8::from_str_radix(value, 16).ok())
                .ok_or(ImportedGamesError::InvalidCursor)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let decoded = String::from_utf8(bytes).map_err(|_| ImportedGamesError::InvalidCursor)?;
    let (milliseconds, key) = decoded
        .split_once(':')
        .ok_or(ImportedGamesError::InvalidCursor)?;
    let ended_at = milliseconds
        .parse::<i64>()
        .ok()
        .and_then(DateTime::from_timestamp_millis)
        .filter(|value| value.timestamp_millis() > 0)
        .ok_or(ImportedGamesError::InvalidCursor)?;
    if key.len() != 64
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ImportedGamesError::InvalidCursor);
    }
    Ok((ended_at, key.to_string()))
}

/// The imported Game every test in this module and its children builds from.
#[cfg(test)]
pub(crate) mod test_fixtures {
    use super::ImportedGame;

    pub(crate) const PGN: &str = r#"[Date "2026.08.12"]
[Time "10:00:00"]
[TimeControl "600+5"]

1. e4 e5 2. Nf3 Nc6 *"#;

    pub(crate) fn fixture_game() -> ImportedGame {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
        )))
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixtures::{fixture_game, PGN};
    use super::*;
    use crate::review_session_contract::{ArtifactDigest, EloRating};

    #[test]
    fn event_labels_classify_lichess_time_controls() {
        assert_eq!(
            time_control_class_from_event_label("rated rapid game"),
            Some(ImportedGameTimeControlClass::Rapid)
        );
        assert_eq!(
            time_control_class_from_event_label("Casual ultraBullet game"),
            Some(ImportedGameTimeControlClass::UltraBullet)
        );
        assert_eq!(time_control_class_from_event_label("Live Chess"), None);
    }

    #[test]
    fn time_controls_are_normalized_without_provider_metadata() {
        assert_eq!(
            parse_time_control("600+5"),
            Some((
                "600+5".to_string(),
                ProfileGameTimeControlClass::Rapid,
                Some(800)
            ))
        );
        assert_eq!(
            parse_time_control("1/86400"),
            Some((
                "correspondence".to_string(),
                ProfileGameTimeControlClass::Correspondence,
                None
            ))
        );
    }

    #[test]
    fn cursor_round_trips_the_total_order_tuple() {
        let ended_at = "2026-08-12T10:00:00Z".parse().unwrap();
        let key = "a".repeat(64);
        assert_eq!(
            decode_cursor(&encode_cursor(ended_at, &key)).unwrap(),
            (ended_at, key)
        );
        assert!(matches!(
            decode_cursor("not-opaque"),
            Err(ImportedGamesError::InvalidCursor)
        ));
    }

    #[test]
    fn imported_game_key_includes_review_side_but_not_elo() {
        let mut game = fixture_game();
        let first = ImportedGameCard::new(
            GameImportId::try_from("game-import:fixture:first".to_string()).unwrap(),
            &game,
            PGN,
            0,
            "2026-08-12T11:00:00Z".parse().unwrap(),
        )
        .unwrap();
        game.elo_profile.rating = EloRating::try_from(1800).unwrap();
        let stronger = ImportedGameCard::new(
            GameImportId::try_from("game-import:fixture:stronger".to_string()).unwrap(),
            &game,
            PGN,
            0,
            "2026-08-12T11:00:00Z".parse().unwrap(),
        )
        .unwrap();
        game.review_side = ReviewSide::White;
        let other_side = ImportedGameCard::new(
            GameImportId::try_from("game-import:fixture:white".to_string()).unwrap(),
            &game,
            PGN,
            0,
            "2026-08-12T11:00:00Z".parse().unwrap(),
        )
        .unwrap();

        assert_eq!(first.imported_game_key, stronger.imported_game_key);
        assert_ne!(first.imported_game_key, other_side.imported_game_key);
        assert_ne!(first.game_import_id, stronger.game_import_id);
    }

    #[test]
    fn pages_are_twenty_games_newest_first_with_an_opaque_cursor() {
        let game = fixture_game();
        let mut cards = (0..21)
            .map(|index| {
                let mut imported = game.clone();
                imported.provenance = ImportProvenance::PastedPgn {
                    pgn_digest: ArtifactDigest::try_from(format!("sha256:{index:064x}")).unwrap(),
                };
                ImportedGameCard::new(
                    GameImportId::try_from(format!("game-import:fixture:{index}")).unwrap(),
                    &imported,
                    PGN,
                    0,
                    DateTime::from_timestamp(1_786_531_600 + index, 0).unwrap(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        for (index, card) in cards.iter_mut().enumerate() {
            card.ended_at = DateTime::from_timestamp(1_786_531_600 + index as i64, 0).unwrap();
        }

        let first = project_page(cards.clone(), None, &BTreeSet::new()).unwrap();
        assert_eq!(first.games.len(), IMPORTED_GAMES_PAGE_SIZE);
        assert_eq!(
            first.games[0].game_import_id.as_str(),
            "game-import:fixture:20"
        );
        let after = decode_cursor(first.next_cursor.as_deref().unwrap()).unwrap();
        let second = project_page(cards.clone(), Some(after), &BTreeSet::new()).unwrap();
        assert_eq!(second.games.len(), 1);
        assert_eq!(
            second.games[0].game_import_id.as_str(),
            "game-import:fixture:0"
        );
        assert_eq!(second.next_cursor, None);

        /* A Game Daily Coaching digested is not on this listing at all: the
        one action it offers is a delete the Coach Engine would refuse. */
        let digested = cards
            .iter()
            .take(2)
            .map(ImportedGameCard::reviewed_game_key)
            .collect::<BTreeSet<_>>();
        let without = project_page(cards, None, &digested).unwrap();
        assert_eq!(without.games.len(), IMPORTED_GAMES_PAGE_SIZE - 1);
        assert_eq!(without.next_cursor, None);
        assert!(without.games.iter().all(|game| ![
            "game-import:fixture:0",
            "game-import:fixture:1"
        ]
        .contains(&game.game_import_id.as_str())));
    }
}
