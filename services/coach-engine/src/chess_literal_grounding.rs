use std::collections::BTreeSet;

use shakmaty::san::SanPlus;

/// Every chess move and square a Language Layer is allowed to name.
///
/// The allowlist is built from a purpose-built projection of the facts, not by
/// walking their serialized form. Whitespace-splitting JSON is an accident of
/// how the facts happen to be stored: it yields `Nxd4` but never `d4`, so
/// "the knight on d4" was rejected, and it never yields SAN for the engine
/// line at all because the principal variation is stored as UCI. Descriptive
/// prose needs both, and only a deliberate projection can say which squares
/// are actually grounded.
#[derive(Clone)]
pub(crate) struct ChessLiteralGrounding {
    allowed: BTreeSet<String>,
}

impl ChessLiteralGrounding {
    pub(crate) fn empty() -> Self {
        Self {
            allowed: BTreeSet::new(),
        }
    }

    pub(crate) fn allow(&mut self, value: &str) {
        self.allowed.insert(value.to_string());
        self.allowed.extend(
            value
                .split_whitespace()
                .map(normalized_chess_token)
                .filter(|token| !token.is_empty()),
        );
    }

    pub(crate) fn allow_all<'a>(&mut self, values: impl IntoIterator<Item = &'a String>) {
        for value in values {
            self.allow(value);
        }
    }

    /// Allows a move in SAN together with the square it lands on, so prose can
    /// say "the knight on d4" about a move recorded as `Nxd4`.
    pub(crate) fn allow_move_san(&mut self, san: &str) {
        self.allow(san);
        if let Some(square) = destination_square(san) {
            self.allow_square(&square);
        }
    }

    pub(crate) fn allow_moves_san<'a>(&mut self, values: impl IntoIterator<Item = &'a String>) {
        for value in values {
            self.allow_move_san(value);
        }
    }

    /// Allows both squares a UCI move touches without allowing the UCI token
    /// itself, which stays banned as coordinate notation no coach speaks.
    pub(crate) fn allow_uci_squares(&mut self, uci: &str) {
        let bytes = uci.as_bytes();
        if bytes.len() >= 4 {
            self.allow_square(&uci[0..2]);
            self.allow_square(&uci[2..4]);
        }
    }

    pub(crate) fn allow_uci_squares_all<'a>(
        &mut self,
        values: impl IntoIterator<Item = &'a String>,
    ) {
        for value in values {
            self.allow_uci_squares(value);
        }
    }

    pub(crate) fn allow_square(&mut self, square: &str) {
        if is_square(square) {
            self.allowed.insert(square.to_string());
        }
    }

    /// The allowlist itself, because it is a prompt input as well as a gate:
    /// the model is shown exactly what it may name.
    pub(crate) fn allowed(&self) -> impl Iterator<Item = &str> {
        self.allowed.iter().map(String::as_str)
    }

    pub(crate) fn validate(&self, text: &str) -> bool {
        !text
            .split_whitespace()
            .map(normalized_chess_token)
            .filter(|token| !token.is_empty())
            .filter(|token| SanPlus::from_ascii(token.as_bytes()).is_ok())
            .any(|token| !self.allowed.contains(&token))
    }
}

fn normalized_chess_token(token: &str) -> String {
    token
        .trim_matches(|character: char| {
            !character.is_ascii_alphanumeric() && !matches!(character, '+' | '#' | '=' | '-' | 'O')
        })
        .to_string()
}

/// The square a SAN move lands on. Castling has no destination worth naming.
fn destination_square(san: &str) -> Option<String> {
    let trimmed = san.trim_end_matches(['+', '#']);
    let trimmed = trimmed.split('=').next().unwrap_or(trimmed);
    let bytes = trimmed.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let square = &trimmed[bytes.len() - 2..];
    is_square(square).then(|| square.to_string())
}

fn is_square(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 2 && matches!(bytes[0], b'a'..=b'h') && matches!(bytes[1], b'1'..=b'8')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_grounds_the_square_it_lands_on() {
        let mut grounding = ChessLiteralGrounding::empty();
        grounding.allow_move_san("Nxd4");

        assert!(grounding.validate("the knight on d4 was the bigger piece"));
        assert!(!grounding.validate("the knight on d5 was the bigger piece"));
    }

    #[test]
    fn promotion_and_check_glyphs_do_not_hide_the_destination() {
        let mut grounding = ChessLiteralGrounding::empty();
        grounding.allow_move_san("e8=Q+");
        grounding.allow_move_san("Rxh7#");

        assert!(grounding.validate("the pawn reaches e8 and the rook lands on h7"));
    }

    #[test]
    fn uci_grounds_squares_without_grounding_the_notation() {
        let mut grounding = ChessLiteralGrounding::empty();
        grounding.allow_uci_squares("f3d4");

        assert!(grounding.validate("from f3 to d4"));
        assert!(!grounding.allowed.contains("f3d4"));
    }

    #[test]
    fn castling_grounds_no_square() {
        let mut grounding = ChessLiteralGrounding::empty();
        grounding.allow_move_san("O-O");

        assert!(grounding.validate("O-O was still available"));
    }
}
