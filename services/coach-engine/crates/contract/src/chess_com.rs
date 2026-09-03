//! Chess.com Game URL grammar. Pure parsing only — the callback transport and
//! its per-kind contract versions stay in the app crate, the same split
//! `lichess` makes.
//!
//! This module is the single authored statement of which Chess.com Game URLs
//! ChenChess accepts. The Engine's parser walks [`ChessComGameKind::ALL`], and
//! the URL pattern published to TypeScript consumers is built from the same
//! list, so a fourth Game kind reaches every surface from one edit.

/// The literal prefix every accepted Chess.com shared Game URL carries.
///
/// `.` is its only regular-expression metacharacter, which is why
/// [`chess_com_game_url_pattern`] escapes that one and nothing else. The test
/// below pins the whole pattern, so a prefix that grew another would fail here
/// rather than reach a surface.
const CHESS_COM_GAME_URL_PREFIX: &str = "https://www.chess.com/game/";

/// Chess.com's shared Game ids as a regular-expression fragment.
///
/// Deliberately looser than [`chess_com_game_id_is_canonical`], which also
/// requires the id to fit a `u64`. The pattern is a client-side pre-filter;
/// the Engine stays the authority and rejects the remainder with a typed
/// reason a Player can act on.
const CHESS_COM_GAME_ID_PATTERN: &str = "[1-9][0-9]*";

/// A kind of Chess.com Game ChenChess imports, named by its URL path segment.
///
/// Adding a kind means adding it to [`Self::ALL`] and to [`Self::as_path`].
/// Nothing else states which URL forms are accepted — the parser walks this
/// list and the published pattern is built from it. The per-kind transport
/// matches in the app crate and the stored `ImportedChessComGameKind` still
/// name each kind, but the compiler fails them rather than a Player's import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChessComGameKind {
    Computer,
    Daily,
    Live,
}

impl ChessComGameKind {
    pub const ALL: &'static [Self] = &[Self::Computer, Self::Daily, Self::Live];

    pub fn as_path(self) -> &'static str {
        match self {
            Self::Computer => "computer",
            Self::Daily => "daily",
            Self::Live => "live",
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_path() == path)
    }
}

/// Whether a URL's trailing segment is a Chess.com shared Game id ChenChess can
/// canonicalise: decimal, no leading zero, and within the range Chess.com mints.
pub fn chess_com_game_id_is_canonical(game_id: &str) -> bool {
    game_id.bytes().all(|byte| byte.is_ascii_digit())
        && !game_id.starts_with('0')
        && game_id.parse::<u64>().is_ok()
}

/// One accepted Chess.com shared Game URL, canonicalised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChessComGameUrl {
    canonical_game_id: String,
    kind: ChessComGameKind,
}

impl ChessComGameUrl {
    pub fn parse(url: &str) -> Result<Self, ChessComUrlError> {
        let (kind, game_id) = parse_chess_com_game_identity(url)?;
        Ok(Self {
            canonical_game_id: game_id.to_string(),
            kind,
        })
    }

    pub fn canonical_game_id(&self) -> &str {
        &self.canonical_game_id
    }

    pub fn canonical_url(&self) -> String {
        format!(
            "{CHESS_COM_GAME_URL_PREFIX}{}/{}",
            self.kind.as_path(),
            self.canonical_game_id,
        )
    }

    pub fn kind(&self) -> ChessComGameKind {
        self.kind
    }
}

/// The Game a URL names, without building a [`ChessComGameUrl`] for it.
///
/// A stored import and a URL the Player just typed resolve to the same pair,
/// which is what lets a review be found again without a provider call.
pub fn parse_chess_com_game_identity(
    url: &str,
) -> Result<(ChessComGameKind, &str), ChessComUrlError> {
    let (path, game_id) = url
        .strip_prefix(CHESS_COM_GAME_URL_PREFIX)
        .and_then(|rest| rest.split_once('/'))
        .ok_or(ChessComUrlError)?;
    let kind = ChessComGameKind::from_path(path).ok_or(ChessComUrlError)?;
    if !chess_com_game_id_is_canonical(game_id) {
        return Err(ChessComUrlError);
    }
    Ok((kind, game_id))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Chess.com shared Game URL")]
pub struct ChessComUrlError;

/// The accepted Chess.com Game URL forms as one anchored regular expression,
/// built from the kind list above.
///
/// Published into `@chenchess/coach-engine-sdk` by
/// `generate_review_session_contract`, so the web import field and the Coach
/// App tool schema enforce what the Engine imports rather than a copy of it.
pub fn chess_com_game_url_pattern() -> String {
    let prefix = CHESS_COM_GAME_URL_PREFIX.replace('.', r"\.");
    let kinds = ChessComGameKind::ALL
        .iter()
        .map(|kind| kind.as_path())
        .collect::<Vec<_>>()
        .join("|");
    format!("^{prefix}(?:{kinds})/{CHESS_COM_GAME_ID_PATTERN}$")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_every_listed_kind_in_one_anchored_pattern() {
        assert_eq!(
            chess_com_game_url_pattern(),
            r"^https://www\.chess\.com/game/(?:computer|daily|live)/[1-9][0-9]*$"
        );
    }
}
