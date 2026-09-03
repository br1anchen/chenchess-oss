use std::{collections::HashMap, iter, sync::OnceLock};

use schemars::JsonSchema;
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::{
    chess_com::ChessComGameUrl,
    lichess::LichessGameUrl,
    pgn::{parse_pgn_with_metadata, PgnMetadata},
    review_session_contract::{
        ImportProvenance, OpeningCatalogVersion, OpeningIdentificationProvenance, OpeningMetadata,
        OpeningServiceAttribution, OpeningServiceProvider,
    },
    types::Game,
};

pub const OPENING_CATALOG_VERSION: OpeningCatalogVersion = OpeningCatalogVersion::V2026_04_16;
pub const OPENING_CATALOG_RELEASE: &str = "2026.04.16";
pub const OPENING_CATALOG_SOURCE_COMMIT: &str = "a470acc9d1cdcb26018affa90459a6ec8689af79";
pub const OPENING_CATALOG_SOURCE_DIGEST: &str =
    "sha256:2c0f0fe3f6a9a6e08d0e7b264785b9b3f67da9f1134d841fe42e16bad527be70";
pub const OPENING_CATALOG_POSITION_COUNT: usize = 3_690;

const OPENING_CATALOG_SOURCES: [&[u8]; 5] = [
    include_bytes!("../data/chess-openings/2026.04.16/a.tsv"),
    include_bytes!("../data/chess-openings/2026.04.16/b.tsv"),
    include_bytes!("../data/chess-openings/2026.04.16/c.tsv"),
    include_bytes!("../data/chess-openings/2026.04.16/d.tsv"),
    include_bytes!("../data/chess-openings/2026.04.16/e.tsv"),
];

static OPENING_CATALOG: OnceLock<Result<OpeningCatalog, OpeningCatalogError>> = OnceLock::new();

pub const OPENING_LINE_FIND_LIMIT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CatalogEntry {
    eco: String,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogLine {
    pub eco: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug)]
pub(crate) struct OpeningCatalog {
    pub lines: Vec<CatalogLine>,
    positions: HashMap<String, CatalogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayedOpening {
    pub eco: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindOpeningLinesRequest {
    pub query: String,
    #[serde(default)]
    pub played: Vec<PlayedOpening>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpeningLineFindMatch {
    pub eco: String,
    pub name: String,
    pub path: String,
    pub played: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OpeningLineFindTruncation {
    Complete { total_match_count: u32 },
    Truncated { total_match_count: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpeningLineFindResult {
    pub matches: Vec<OpeningLineFindMatch>,
    pub truncation: OpeningLineFindTruncation,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
enum OpeningCatalogError {
    #[error("the bundled Opening Catalog digest does not match its pinned source digest")]
    DigestMismatch,
    #[error("the bundled Opening Catalog contains invalid UTF-8")]
    InvalidEncoding,
    #[error("the bundled Opening Catalog has an invalid row")]
    InvalidRow,
    #[error("the bundled Opening Catalog contains an invalid representative line: {0}")]
    InvalidLine(String),
    #[error("the bundled Opening Catalog contains a duplicate exact EPD")]
    DuplicatePosition,
    #[error("the bundled Opening Catalog does not contain the pinned number of Positions")]
    PositionCountMismatch,
}

pub fn identify_opening(
    metadata: &PgnMetadata,
    provenance: &ImportProvenance,
    game: &Game,
) -> OpeningMetadata {
    if let (Some((eco, name)), Some(provenance)) = (
        complete_metadata(metadata),
        authoritative_service_provenance(metadata, provenance),
    ) {
        return OpeningMetadata::Present {
            eco,
            name,
            provenance,
        };
    }

    identify_from_catalog(game)
}

fn complete_metadata(metadata: &PgnMetadata) -> Option<(String, String)> {
    let eco = metadata.eco.as_deref()?.trim();
    let name = metadata.opening.as_deref()?.trim();
    (!eco.is_empty() && !name.is_empty()).then(|| (eco.to_string(), name.to_string()))
}

fn authoritative_service_provenance(
    metadata: &PgnMetadata,
    provenance: &ImportProvenance,
) -> Option<OpeningIdentificationProvenance> {
    let direct_provider = match provenance {
        ImportProvenance::Lichess { .. } => Some(OpeningServiceProvider::Lichess),
        ImportProvenance::ChessCom { .. } => Some(OpeningServiceProvider::ChessCom),
        ImportProvenance::PastedPgn { .. } | ImportProvenance::LocalPgn { .. } => None,
    };
    if let Some(provider) = direct_provider {
        return Some(OpeningIdentificationProvenance::Service {
            provider,
            attribution: OpeningServiceAttribution::DirectImport,
        });
    }

    let mut attributions = metadata
        .opening_attribution_headers
        .iter()
        .map(String::as_str)
        .filter_map(service_url_attribution);
    let (provider, canonical_url) = attributions.next()?;
    if attributions.next().is_some() {
        return None;
    }
    Some(OpeningIdentificationProvenance::Service {
        provider,
        attribution: OpeningServiceAttribution::PgnUrl { canonical_url },
    })
}

fn service_url_attribution(value: &str) -> Option<(OpeningServiceProvider, String)> {
    if let Ok(source) = LichessGameUrl::parse(value) {
        return Some((OpeningServiceProvider::Lichess, source.canonical_url()));
    }
    ChessComGameUrl::parse(value)
        .ok()
        .map(|source| (OpeningServiceProvider::ChessCom, source.canonical_url()))
}

fn identify_from_catalog(game: &Game) -> OpeningMetadata {
    let catalog = catalog();
    let positions = iter::once((game.moves.len(), game.final_position.as_str())).chain(
        game.moves
            .iter()
            .enumerate()
            .rev()
            .filter(|(index, _)| *index > 0)
            .map(|(index, game_move)| (index, game_move.position.as_str())),
    );
    for (matched_ply, fen) in positions {
        let epd = epd_from_fen(fen).expect("imported Positions are normalized FEN");
        if let Some(entry) = catalog.positions.get(&epd) {
            return OpeningMetadata::Present {
                eco: entry.eco.clone(),
                name: entry.name.clone(),
                provenance: OpeningIdentificationProvenance::Catalog {
                    catalog_version: OPENING_CATALOG_VERSION,
                    matched_ply: u16::try_from(matched_ply)
                        .expect("an import-size-bounded Game fits the public ply limit"),
                },
            };
        }
    }
    OpeningMetadata::Absent
}

pub(crate) fn catalog() -> &'static OpeningCatalog {
    OPENING_CATALOG
        .get_or_init(build_catalog)
        .as_ref()
        .expect("the tested bundled Opening Catalog is valid")
}

static OPENING_LINE_REFS: OnceLock<HashMap<String, usize>> = OnceLock::new();

/// The Opening Line address: `<eco>-<name-slug>-<digest4>` over the move
/// path, matching the constructor Central Host mints in `openingLineRef.ts`.
/// The digest is identity; the slug is legibility. (#493 aligned the engine
/// root to the same constructor.)
pub fn opening_line_reference(eco: &str, name: &str, path: &str) -> String {
    format!(
        "{}-{}-{}",
        eco.to_uppercase(),
        opening_name_slug(name),
        opening_line_digest4(path)
    )
}

fn opening_name_slug(name: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for character in name.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(character);
        } else {
            pending_dash = true;
        }
    }
    if slug.is_empty() {
        "opening".to_string()
    } else {
        slug
    }
}

/// FNV-1a 32-bit over the path's UTF-16 code units, first four hex
/// characters — byte-for-byte the TS `openingLineDigest4`. Catalog paths are
/// ASCII, so code units equal bytes.
fn opening_line_digest4(path: &str) -> String {
    let mut hash: u32 = 2_166_136_261;
    for unit in path.encode_utf16() {
        hash ^= u32::from(unit);
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("{hash:08x}")[..4].to_string()
}

/// Resolve an Opening Line address back to its catalog row.
pub(crate) fn resolve_opening_line(reference: &str) -> Option<&'static CatalogLine> {
    let refs = OPENING_LINE_REFS.get_or_init(|| {
        catalog()
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                (
                    opening_line_reference(&line.eco, &line.name, &line.path),
                    index,
                )
            })
            .collect()
    });
    refs.get(reference).map(|index| &catalog().lines[*index])
}

/// The canonical order of a named line: the shortest move path among the
/// catalog rows sharing its ECO and name. A played opening is known only as
/// that colliding pair, so this ranks rows; it never claims the Player
/// played that exact line.
pub(crate) fn shortest_line_for(eco: &str, name: &str) -> Option<&'static CatalogLine> {
    catalog()
        .lines
        .iter()
        .filter(|line| line.eco == eco && line.name == name)
        .min_by(|left, right| {
            left.path
                .len()
                .cmp(&right.path.len())
                .then_with(|| left.path.cmp(&right.path))
        })
}

pub fn find_opening_lines(query: &str, played: &[PlayedOpening]) -> OpeningLineFindResult {
    let needle = query.trim();
    if needle.is_empty() {
        return OpeningLineFindResult {
            matches: Vec::new(),
            truncation: OpeningLineFindTruncation::Complete {
                total_match_count: 0,
            },
        };
    }

    let played_keys: std::collections::HashSet<String> = played
        .iter()
        .map(|opening| played_key(&opening.eco, &opening.name))
        .collect();
    let mut matches: Vec<OpeningLineFindMatch> = catalog()
        .lines
        .iter()
        .filter(|line| line_matches(line, needle))
        .map(|line| OpeningLineFindMatch {
            eco: line.eco.clone(),
            name: line.name.clone(),
            path: line.path.clone(),
            played: played_keys.contains(&played_key(&line.eco, &line.name)),
        })
        .collect();
    matches.sort_by(|left, right| {
        right
            .played
            .cmp(&left.played)
            .then_with(|| left.name.len().cmp(&right.name.len()))
            .then_with(|| left.eco.cmp(&right.eco))
            .then_with(|| left.path.cmp(&right.path))
    });
    let total_match_count = u32::try_from(matches.len()).unwrap_or(u32::MAX);
    let truncated = matches.len() > OPENING_LINE_FIND_LIMIT;
    matches.truncate(OPENING_LINE_FIND_LIMIT);
    let truncation = if truncated {
        OpeningLineFindTruncation::Truncated { total_match_count }
    } else {
        OpeningLineFindTruncation::Complete { total_match_count }
    };
    OpeningLineFindResult {
        matches,
        truncation,
    }
}

fn line_matches(line: &CatalogLine, needle: &str) -> bool {
    if eco_shaped(needle) {
        return line
            .eco
            .to_ascii_uppercase()
            .starts_with(&needle.to_ascii_uppercase());
    }
    // Full Unicode folding: catalog names carry accents, and the TS local
    // lookup folds the same way.
    line.name.to_lowercase().contains(&needle.to_lowercase())
}

fn eco_shaped(query: &str) -> bool {
    let bytes = query.as_bytes();
    matches!(bytes.len(), 1..=3)
        && matches!(bytes[0], b'A'..=b'E' | b'a'..=b'e')
        && bytes[1..].iter().all(u8::is_ascii_digit)
}

fn played_key(eco: &str, name: &str) -> String {
    format!("{}:{}", eco.to_ascii_uppercase(), name.to_ascii_lowercase())
}

fn build_catalog() -> Result<OpeningCatalog, OpeningCatalogError> {
    let mut source_digest = Sha256::new();
    for source in OPENING_CATALOG_SOURCES {
        source_digest.update(source);
    }
    if format!("sha256:{:x}", source_digest.finalize()) != OPENING_CATALOG_SOURCE_DIGEST {
        return Err(OpeningCatalogError::DigestMismatch);
    }

    let mut positions = HashMap::with_capacity(OPENING_CATALOG_POSITION_COUNT);
    let mut lines = Vec::with_capacity(OPENING_CATALOG_POSITION_COUNT);
    for source in OPENING_CATALOG_SOURCES {
        let source =
            std::str::from_utf8(source).map_err(|_| OpeningCatalogError::InvalidEncoding)?;
        let mut source_lines = source.lines();
        if source_lines.next() != Some("eco\tname\tpgn") {
            return Err(OpeningCatalogError::InvalidRow);
        }
        for line in source_lines {
            let mut fields = line.splitn(3, '\t');
            let eco = fields.next().filter(|value| !value.is_empty());
            let name = fields.next().filter(|value| !value.is_empty());
            let pgn = fields.next().filter(|value| !value.is_empty());
            let (Some(eco), Some(name), Some(pgn)) = (eco, name, pgn) else {
                return Err(OpeningCatalogError::InvalidRow);
            };
            let representative_game = format!("{pgn} *");
            let parsed = parse_pgn_with_metadata(&representative_game).map_err(|error| {
                OpeningCatalogError::InvalidLine(format!("{eco} {name}: {error}"))
            })?;
            let epd = epd_from_fen(&parsed.game.final_position)
                .ok_or_else(|| OpeningCatalogError::InvalidLine(format!("{eco} {name}")))?;
            let entry = CatalogEntry {
                eco: eco.to_string(),
                name: name.to_string(),
            };
            if positions.insert(epd, entry).is_some() {
                return Err(OpeningCatalogError::DuplicatePosition);
            }
            lines.push(CatalogLine {
                eco: eco.to_string(),
                name: name.to_string(),
                path: pgn.to_string(),
            });
        }
    }
    if positions.len() != OPENING_CATALOG_POSITION_COUNT {
        return Err(OpeningCatalogError::PositionCountMismatch);
    }
    Ok(OpeningCatalog { lines, positions })
}

pub(crate) fn epd_from_fen(fen: &str) -> Option<String> {
    let fields = fen.split_whitespace().take(4).collect::<Vec<_>>();
    (fields.len() == 4).then(|| fields.join(" "))
}

#[cfg(test)]
mod tests {
    use crate::{
        pgn::parse_pgn_with_metadata,
        review_session_contract::{
            ArtifactDigest, ImportProvenance, OpeningIdentificationProvenance, OpeningMetadata,
        },
    };

    use super::*;

    fn local_provenance() -> ImportProvenance {
        ImportProvenance::PastedPgn {
            pgn_digest: ArtifactDigest::try_from(format!("sha256:{}", "0".repeat(64))).unwrap(),
        }
    }

    #[test]
    fn pinned_catalog_has_the_exact_source_digest_and_position_count() {
        let catalog = catalog();

        assert_eq!(catalog.positions.len(), OPENING_CATALOG_POSITION_COUNT);
        assert_eq!(catalog.lines.len(), OPENING_CATALOG_POSITION_COUNT);
        assert_eq!(OPENING_CATALOG_RELEASE, "2026.04.16");
        assert_eq!(
            OPENING_CATALOG_SOURCE_COMMIT,
            "a470acc9d1cdcb26018affa90459a6ec8689af79"
        );
        assert_eq!(
            OPENING_CATALOG_SOURCE_DIGEST,
            "sha256:2c0f0fe3f6a9a6e08d0e7b264785b9b3f67da9f1134d841fe42e16bad527be70"
        );
    }

    #[test]
    fn fallback_returns_the_last_named_exact_position() {
        let parsed = parse_pgn_with_metadata(
            "[Result \"*\"]\n\n1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 6. Re1 b5 7. Bb3 d6 8. c3 O-O 9. h3 Kh8 10. a4",
        )
        .unwrap();

        let opening = identify_opening(&parsed.metadata, &local_provenance(), &parsed.game);

        assert_eq!(
            opening,
            OpeningMetadata::Present {
                eco: "C92".to_string(),
                name: "Ruy Lopez: Closed".to_string(),
                provenance: OpeningIdentificationProvenance::Catalog {
                    catalog_version: OpeningCatalogVersion::V2026_04_16,
                    matched_ply: 17,
                },
            }
        );
    }

    #[test]
    fn unmatched_nonstandard_position_is_absent() {
        let parsed = parse_pgn_with_metadata(
            "[FEN \"7k/8/8/8/8/8/P7/K7 w - - 0 1\"]\n[Result \"*\"]\n\n1. a4",
        )
        .unwrap();

        assert_eq!(
            identify_opening(&parsed.metadata, &local_provenance(), &parsed.game),
            OpeningMetadata::Absent
        );
    }

    #[test]
    fn colliding_eco_and_name_keep_distinct_paths() {
        let french: Vec<_> = catalog()
            .lines
            .iter()
            .filter(|line| line.eco == "C00" && line.name == "French Defense")
            .collect();

        assert_eq!(french.len(), 2);
        assert_ne!(french[0].path, french[1].path);
    }

    #[test]
    fn eco_shaped_query_is_a_prefix_and_anything_else_is_a_name_substring() {
        let by_eco = find_opening_lines("B90", &[]);
        assert!(!by_eco.matches.is_empty());
        assert!(by_eco
            .matches
            .iter()
            .all(|line| line.eco.starts_with("B90")));

        let by_name = find_opening_lines("Najdorf", &[]);
        assert!(!by_name.matches.is_empty());
        assert!(by_name
            .matches
            .iter()
            .all(|line| line.name.to_ascii_lowercase().contains("najdorf")));

        let both = find_opening_lines("B90 Najdorf", &[]);
        assert!(both.matches.is_empty());
        assert_eq!(
            both.truncation,
            OpeningLineFindTruncation::Complete {
                total_match_count: 0
            }
        );
    }

    #[test]
    fn najdorf_truncates_at_ten_with_the_existing_marker_shape() {
        let found = find_opening_lines("Najdorf", &[]);

        assert_eq!(found.matches.len(), OPENING_LINE_FIND_LIMIT);
        match found.truncation {
            OpeningLineFindTruncation::Truncated { total_match_count } => {
                assert!(total_match_count > OPENING_LINE_FIND_LIMIT as u32);
            }
            OpeningLineFindTruncation::Complete { .. } => {
                panic!("Najdorf matches more than ten catalog rows")
            }
        }
    }

    #[test]
    fn ties_break_toward_shorter_names() {
        let found = find_opening_lines("French Defense", &[]);

        assert_eq!(found.matches[0].name, "French Defense");
        assert!(found
            .matches
            .windows(2)
            .all(|pair| pair[0].name.len() <= pair[1].name.len()));
    }

    #[test]
    fn played_matches_rank_first_only_when_they_match_the_query() {
        let played = [PlayedOpening {
            eco: "A00".to_string(),
            name: "Saragossa Opening".to_string(),
        }];

        let saragossa = find_opening_lines("Saragossa", &played);
        assert_eq!(saragossa.matches[0].eco, "A00");
        assert_eq!(saragossa.matches[0].name, "Saragossa Opening");
        assert!(saragossa.matches[0].played);

        let najdorf = find_opening_lines("Najdorf", &played);
        assert!(najdorf.matches.iter().all(|line| !line.played));
        assert!(najdorf
            .matches
            .iter()
            .all(|line| line.name.to_ascii_lowercase().contains("najdorf")));
    }

    #[test]
    fn opening_line_reference_matches_the_central_host_constructor() {
        // Pinned against Central Host's openingLineRefFromPath twin; its
        // openingLineFind tests carry the same literals.
        assert_eq!(
            opening_line_reference(
                "B90",
                "Sicilian Defense: Najdorf Variation",
                "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6"
            ),
            "B90-sicilian-defense-najdorf-variation-a203"
        );
        assert_eq!(
            opening_line_reference("C00", "French Defense", "1. e4 e6"),
            "C00-french-defense-1564"
        );
        assert_eq!(
            opening_line_reference("A00", "Amar Opening", "1. Nh3"),
            "A00-amar-opening-b2ca"
        );
    }

    #[test]
    fn every_catalog_line_resolves_by_its_own_reference() {
        let mut references = std::collections::HashSet::new();
        for line in &catalog().lines {
            let reference = opening_line_reference(&line.eco, &line.name, &line.path);
            assert!(
                references.insert(reference.clone()),
                "duplicate Opening Line address {reference}"
            );
            let resolved = resolve_opening_line(&reference)
                .unwrap_or_else(|| panic!("{reference} should resolve"));
            assert_eq!(resolved.path, line.path);
        }
        assert!(resolve_opening_line("B90-sicilian-defense-0000").is_none());
    }

    #[test]
    fn empty_query_creates_nothing() {
        let found = find_opening_lines("   ", &[]);

        assert!(found.matches.is_empty());
        assert_eq!(
            found.truncation,
            OpeningLineFindTruncation::Complete {
                total_match_count: 0
            }
        );
    }
}
