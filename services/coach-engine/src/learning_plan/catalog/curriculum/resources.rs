use super::registry::PracticeResource;

// Exact external resource definitions; concept recognition lives in Decision Explanation.

macro_rules! practice {
    ($id:literal, $title:literal, $path:literal) => {
        PracticeResource {
            id: $id,
            title: $title,
            path: $path,
        }
    };
}

pub(super) const NO_PRACTICE: &[PracticeResource] = &[];
pub(super) const PIECE_CHECKMATE_PRACTICE: &[PracticeResource] = &[
    practice!(
        "BJy6fEDf",
        "Piece Checkmates I",
        "checkmates/piece-checkmates-i/BJy6fEDf"
    ),
    practice!(
        "Rg2cMBZ6",
        "Piece Checkmates II",
        "checkmates/piece-checkmates-ii/Rg2cMBZ6"
    ),
];
pub(super) const CHECKMATE_PATTERN_PRACTICE: &[PracticeResource] = &[
    practice!(
        "fE4k21MW",
        "Checkmate Patterns I",
        "checkmates/checkmate-patterns-i/fE4k21MW"
    ),
    practice!(
        "8yadFPpU",
        "Checkmate Patterns II",
        "checkmates/checkmate-patterns-ii/8yadFPpU"
    ),
    practice!(
        "PDkQDt6u",
        "Checkmate Patterns III",
        "checkmates/checkmate-patterns-iii/PDkQDt6u"
    ),
    practice!(
        "96Lij7wH",
        "Checkmate Patterns IV",
        "checkmates/checkmate-patterns-iv/96Lij7wH"
    ),
];
pub(super) const KNIGHT_BISHOP_MATE_PRACTICE: &[PracticeResource] = &[practice!(
    "ByhlXnmM",
    "Knight & Bishop Mate",
    "checkmates/knight--bishop-mate/ByhlXnmM"
)];
pub(super) const PIN_PRACTICE: &[PracticeResource] = &[practice!(
    "9ogFv8Ac",
    "The Pin",
    "fundamental-tactics/the-pin/9ogFv8Ac"
)];
pub(super) const SKEWER_PRACTICE: &[PracticeResource] = &[practice!(
    "tuoBxVE5",
    "The Skewer",
    "fundamental-tactics/the-skewer/tuoBxVE5"
)];
pub(super) const FORK_PRACTICE: &[PracticeResource] = &[practice!(
    "Qj281y1p",
    "The Fork",
    "fundamental-tactics/the-fork/Qj281y1p"
)];
pub(super) const DISCOVERED_ATTACK_PRACTICE: &[PracticeResource] = &[practice!(
    "MnsJEWnI",
    "Discovered Attacks",
    "fundamental-tactics/discovered-attacks/MnsJEWnI"
)];
pub(super) const DOUBLE_CHECK_PRACTICE: &[PracticeResource] = &[practice!(
    "RUQASaZm",
    "Double Check",
    "fundamental-tactics/double-check/RUQASaZm"
)];
pub(super) const OVERLOADED_PIECE_PRACTICE: &[PracticeResource] = &[practice!(
    "o734CNqp",
    "Overloaded Pieces",
    "fundamental-tactics/overloaded-pieces/o734CNqp"
)];
pub(super) const INTERMEZZO_PRACTICE: &[PracticeResource] = &[practice!(
    "ITWY4GN2",
    "Zwischenzug",
    "fundamental-tactics/zwischenzug/ITWY4GN2"
)];
pub(super) const X_RAY_PRACTICE: &[PracticeResource] = &[practice!(
    "lyVYjhPG",
    "X-Ray",
    "fundamental-tactics/x-ray/lyVYjhPG"
)];
pub(super) const ZUGZWANG_PRACTICE: &[PracticeResource] = &[practice!(
    "9cKgYrHb",
    "Zugzwang",
    "advanced-tactics/zugzwang/9cKgYrHb"
)];
pub(super) const INTERFERENCE_PRACTICE: &[PracticeResource] = &[practice!(
    "g1fxVZu9",
    "Interference",
    "advanced-tactics/interference/g1fxVZu9"
)];
pub(super) const GREEK_GIFT_PRACTICE: &[PracticeResource] = &[practice!(
    "s5pLU7Of",
    "Greek Gift",
    "advanced-tactics/greek-gift/s5pLU7Of"
)];
pub(super) const DEFLECTION_PRACTICE: &[PracticeResource] = &[practice!(
    "kdKpaYLW",
    "Deflection",
    "advanced-tactics/deflection/kdKpaYLW"
)];
pub(super) const ATTRACTION_PRACTICE: &[PracticeResource] = &[practice!(
    "jOZejFWk",
    "Attraction",
    "advanced-tactics/attraction/jOZejFWk"
)];
pub(super) const UNDERPROMOTION_PRACTICE: &[PracticeResource] = &[practice!(
    "49fDW0wP",
    "Underpromotion",
    "advanced-tactics/underpromotion/49fDW0wP"
)];
pub(super) const DESPERADO_PRACTICE: &[PracticeResource] = &[practice!(
    "0YcGiH4Y",
    "Desperado",
    "advanced-tactics/desperado/0YcGiH4Y"
)];
pub(super) const COUNTER_CHECK_PRACTICE: &[PracticeResource] = &[practice!(
    "CgjKPvxQ",
    "Counter Check",
    "advanced-tactics/counter-check/CgjKPvxQ"
)];
pub(super) const CAPTURING_DEFENDER_PRACTICE: &[PracticeResource] = &[practice!(
    "udx042D6",
    "Undermining",
    "advanced-tactics/undermining/udx042D6"
)];
pub(super) const CLEARANCE_PRACTICE: &[PracticeResource] = &[practice!(
    "Grmtwuft",
    "Clearance",
    "advanced-tactics/clearance/Grmtwuft"
)];
pub(super) const KEY_SQUARES_PRACTICE: &[PracticeResource] = &[practice!(
    "xebrDvFe",
    "Key Squares",
    "pawn-endgames/key-squares/xebrDvFe"
)];
pub(super) const OPPOSITION_PRACTICE: &[PracticeResource] = &[practice!(
    "A4ujYOer",
    "Opposition",
    "pawn-endgames/opposition/A4ujYOer"
)];
pub(super) const SEVENTH_RANK_ROOK_PAWN_PRACTICE: &[PracticeResource] = &[practice!(
    "pt20yRkT",
    "7th-Rank Rook Pawn",
    "pawn-endgames/7th-rank-rook-pawn/pt20yRkT"
)];
pub(super) const PASSIVE_ROOK_PRACTICE: &[PracticeResource] = &[practice!(
    "MkDViieT",
    "7th-Rank Rook Pawn",
    "rook-endgames/7th-rank-rook-pawn/MkDViieT"
)];
pub(super) const BASIC_ROOK_ENDGAME_PRACTICE: &[PracticeResource] = &[practice!(
    "pqUSUw8Y",
    "Basic Rook Endgames",
    "rook-endgames/basic-rook-endgames/pqUSUw8Y"
)];
pub(super) const INTERMEDIATE_ROOK_PRACTICE: &[PracticeResource] = &[practice!(
    "heQDnvq7",
    "Intermediate Rook Endings",
    "rook-endgames/intermediate-rook-endings/heQDnvq7"
)];
pub(super) const PRACTICAL_ROOK_PRACTICE: &[PracticeResource] = &[practice!(
    "wS23j5Tm",
    "Practical Rook Endings",
    "rook-endgames/practical-rook-endings/wS23j5Tm"
)];
