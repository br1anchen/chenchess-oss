use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UserLevel {
    Beginner,
    Intermediate,
    Advanced,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(try_from = "u16")]
pub struct EloProfile(u16);

impl EloProfile {
    const MIN: u16 = 100;
    const MAX: u16 = 3500;

    pub fn rating(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for EloProfile {
    type Error = String;

    fn try_from(rating: u16) -> Result<Self, Self::Error> {
        if (Self::MIN..=Self::MAX).contains(&rating) {
            Ok(Self(rating))
        } else {
            Err(format!(
                "Elo Profile must be between {} and {}",
                Self::MIN,
                Self::MAX
            ))
        }
    }
}

impl UserLevel {
    pub fn from_elo(elo: u16) -> Self {
        match elo {
            0..=1199 => Self::Beginner,
            1200..=1899 => Self::Intermediate,
            _ => Self::Advanced,
        }
    }

    pub fn coaching_focus(self) -> &'static str {
        match self {
            Self::Beginner => {
                "Focus on forcing moves, one-move threats, hanging pieces, and simple opening principles."
            }
            Self::Intermediate => {
                "Focus on candidate moves, tactical motifs, pawn breaks, and converting advantages into plans."
            }
            Self::Advanced => {
                "Focus on calculation depth, prophylaxis, move-order nuance, structural tradeoffs, and practical decision quality."
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Game {
    pub white: Option<String>,
    pub black: Option<String>,
    pub event: Option<String>,
    pub site: Option<String>,
    pub result: Option<String>,
    pub moves: Vec<ImportedMove>,
    pub final_position: String,
    pub is_terminal: bool,
}

impl Game {
    pub fn summary(&self) -> GameSummary {
        GameSummary {
            white: self.white.clone(),
            black: self.black.clone(),
            event: self.event.clone(),
            site: self.site.clone(),
            result: self.result.clone(),
            move_count: self.moves.len().div_ceil(2),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSummary {
    pub white: Option<String>,
    pub black: Option<String>,
    pub event: Option<String>,
    pub site: Option<String>,
    pub result: Option<String>,
    pub move_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedMove {
    pub ply: usize,
    pub move_number: u32,
    pub side: MoveSide,
    pub san: String,
    pub uci: String,
    pub position: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewableMove {
    pub ply: usize,
    pub move_number: u32,
    pub side: MoveSide,
    pub san: String,
}

impl From<&ImportedMove> for ReviewableMove {
    fn from(game_move: &ImportedMove) -> Self {
        Self {
            ply: game_move.ply,
            move_number: game_move.move_number,
            side: game_move.side,
            san: game_move.san.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MoveSide {
    White,
    Black,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSide {
    White,
    Black,
    Both,
}

impl ReviewSide {
    pub fn includes(self, side: MoveSide) -> bool {
        matches!(
            (self, side),
            (Self::White, MoveSide::White) | (Self::Black, MoveSide::Black) | (Self::Both, _)
        )
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanMoveCandidate {
    pub uci: String,
    pub probability: f64,
    pub rank: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerProfile {
    pub elo: u16,
    pub level: UserLevel,
    pub coaching_focus: String,
}

impl PlayerProfile {
    pub fn from_elo(elo: EloProfile) -> Self {
        let elo = elo.rating();
        let level = UserLevel::from_elo(elo);

        Self {
            elo,
            level,
            coaching_focus: level.coaching_focus().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EloProfile, PlayerProfile, UserLevel};

    #[test]
    fn elo_profile_maps_to_pipeline_assumption_bands() {
        assert_eq!(profile(1199).level, UserLevel::Beginner);
        assert_eq!(profile(1200).level, UserLevel::Intermediate);
        assert_eq!(profile(1899).level, UserLevel::Intermediate);
        assert_eq!(profile(1900).level, UserLevel::Advanced);
    }

    fn profile(rating: u16) -> PlayerProfile {
        PlayerProfile::from_elo(EloProfile::try_from(rating).expect("test Elo should be valid"))
    }
}
