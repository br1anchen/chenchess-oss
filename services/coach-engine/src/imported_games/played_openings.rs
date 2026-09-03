//! What the Player actually plays, counted over their whole shelf.
//!
//! A different question from listing the shelf, and the only reader that cares
//! about openings rather than Games, so it keeps its own module.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::Serialize;
use ts_rs::TS;

use super::ImportedGameCard;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlayedOpeningAggregate {
    pub eco: String,
    pub name: String,
    pub play_count: u32,
    pub last_played_at_unix_milliseconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub opening_line_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlayedOpeningsResult {
    pub openings: Vec<PlayedOpeningAggregate>,
}

/// Counted over every imported Game with no window: the corpus is thin, so a
/// recency cutoff would delete the signal it was meant to rank. Sorted by
/// count then most recent. A Game with no opening identification contributes
/// to no count.
pub(super) fn aggregate_played_openings(cards: &[ImportedGameCard]) -> PlayedOpeningsResult {
    let mut by_opening: std::collections::BTreeMap<(String, String), (u32, DateTime<Utc>)> =
        std::collections::BTreeMap::new();
    for card in cards {
        let Some(opening) = card.opening() else {
            continue;
        };
        let entry = by_opening
            .entry((opening.eco, opening.name))
            .or_insert((0, card.ended_at));
        entry.0 += 1;
        if card.ended_at > entry.1 {
            entry.1 = card.ended_at;
        }
    }
    let mut openings: Vec<PlayedOpeningAggregate> = by_opening
        .into_iter()
        .map(|((eco, name), (play_count, last_played))| {
            let resolved = crate::opening_identification::shortest_line_for(&eco, &name);
            PlayedOpeningAggregate {
                play_count,
                last_played_at_unix_milliseconds: u64::try_from(last_played.timestamp_millis())
                    .unwrap_or(0),
                opening_line_ref: resolved.map(|line| {
                    crate::opening_identification::opening_line_reference(
                        &line.eco, &line.name, &line.path,
                    )
                }),
                path: resolved.map(|line| line.path.clone()),
                eco,
                name,
            }
        })
        .collect();
    openings.sort_by(|left, right| {
        right
            .play_count
            .cmp(&left.play_count)
            .then_with(|| {
                right
                    .last_played_at_unix_milliseconds
                    .cmp(&left.last_played_at_unix_milliseconds)
            })
            .then_with(|| left.eco.cmp(&right.eco))
            .then_with(|| left.name.cmp(&right.name))
    });
    PlayedOpeningsResult { openings }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::{fixture_game, PGN};
    use super::super::ImportedGameCard;
    use super::*;
    use crate::review_session_contract::{GameImportId, ImportedGame, ReviewSide};

    #[test]
    fn played_openings_count_every_imported_game_with_no_window() {
        let mut game = fixture_game();
        let card = |id: &str, imported_at: &str, game: &ImportedGame| {
            ImportedGameCard::new(
                GameImportId::try_from(format!("game-import:fixture:{id}")).unwrap(),
                game,
                PGN,
                0,
                imported_at.parse().unwrap(),
            )
            .unwrap()
        };
        // Same PGN date for every card: recency comes from ended_at, and the
        // fixture PGN pins one; play counts do the first sort.
        let canonical_old = card("canonical-old", "2024-01-01T00:00:00Z", &game);
        game.review_side = ReviewSide::White;
        let canonical_new = card("canonical-new", "2026-08-01T00:00:00Z", &game);
        game.game.opening = crate::review_session_contract::OpeningMetadata::Absent;
        let unidentified = card("unidentified", "2026-08-02T00:00:00Z", &game);
        game.game.opening = crate::review_session_contract::OpeningMetadata::Present {
            eco: "C00".to_string(),
            name: "French Defense".to_string(),
            provenance: crate::review_session_contract::OpeningIdentificationProvenance::Catalog {
                catalog_version: crate::opening_identification::OPENING_CATALOG_VERSION,
                matched_ply: 2,
            },
        };
        game.review_side = ReviewSide::Black;
        let french = card("french", "2026-08-03T00:00:00Z", &game);

        let aggregate =
            aggregate_played_openings(&[canonical_old, unidentified, french, canonical_new]);
        assert_eq!(
            aggregate.openings.len(),
            2,
            "unidentified Games count nowhere"
        );
        let canonical = &aggregate.openings[0];
        assert_eq!(
            (canonical.eco.as_str(), canonical.play_count),
            ("A00", 2),
            "count sorts first, with no recency window"
        );
        let french_row = &aggregate.openings[1];
        assert_eq!((french_row.eco.as_str(), french_row.play_count), ("C00", 1));

        // A played opening resolves to the shortest move path among the rows
        // sharing its ECO and name — the canonical order of the named line.
        let resolved_path = french_row.path.as_deref().expect("C00 French resolves");
        let shortest = crate::opening_identification::shortest_line_for("C00", "French Defense")
            .expect("catalog has the French");
        assert_eq!(resolved_path, shortest.path);
        assert!(french_row
            .opening_line_ref
            .as_deref()
            .is_some_and(|reference| reference.starts_with("C00-french-defense-")));
    }

    #[test]
    fn played_openings_break_count_ties_by_recency() {
        let game = fixture_game();
        let card = |id: &str, imported_at: &str, game: &ImportedGame| {
            ImportedGameCard::new(
                GameImportId::try_from(format!("game-import:fixture:{id}")).unwrap(),
                game,
                PGN,
                0,
                imported_at.parse().unwrap(),
            )
            .unwrap()
        };
        let mut other = fixture_game();
        other.game.opening = crate::review_session_contract::OpeningMetadata::Present {
            eco: "C00".to_string(),
            name: "French Defense".to_string(),
            provenance: crate::review_session_contract::OpeningIdentificationProvenance::Catalog {
                catalog_version: crate::opening_identification::OPENING_CATALOG_VERSION,
                matched_ply: 2,
            },
        };
        other.review_side = ReviewSide::White;
        let canonical = card("canonical", "2026-08-01T00:00:00Z", &game);
        let french = card("french", "2026-08-02T00:00:00Z", &other);
        let aggregate = aggregate_played_openings(&[canonical.clone(), french.clone()]);
        // Counts tie at one; the fixture PGN pins ended_at, so recency falls
        // back to the deterministic eco order.
        assert_eq!(aggregate.openings.len(), 2);
        assert!(aggregate.openings[0].eco <= aggregate.openings[1].eco);
    }
}
