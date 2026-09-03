//! Lichess Game URL grammar. Pure parsing only — export transport stays in
//! the app crate, which is why `export_request` lives there as a free fn.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LichessSide {
    White,
    Black,
}

impl LichessSide {
    fn as_path(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LichessGameUrl {
    source_url: String,
    game_identifier: String,
    canonical_game_id: String,
    side: Option<LichessSide>,
}

impl LichessGameUrl {
    pub fn parse(url: &str) -> Result<Self, LichessUrlError> {
        let path = url
            .strip_prefix("https://lichess.org/")
            .ok_or(LichessUrlError)?;
        if path.contains(['?', '#']) {
            return Err(LichessUrlError);
        }
        let segments = path.split('/').collect::<Vec<_>>();
        let (game_identifier, side) = match segments.as_slice() {
            [game_identifier] => (*game_identifier, None),
            [game_identifier, "white"] => (*game_identifier, Some(LichessSide::White)),
            [game_identifier, "black"] => (*game_identifier, Some(LichessSide::Black)),
            _ => return Err(LichessUrlError),
        };
        if !matches!(game_identifier.len(), 8 | 12)
            || !game_identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric())
        {
            return Err(LichessUrlError);
        }
        Ok(Self {
            source_url: url.to_string(),
            game_identifier: game_identifier.to_string(),
            canonical_game_id: game_identifier[..8].to_string(),
            side,
        })
    }

    pub fn canonical_game_id(&self) -> &str {
        &self.canonical_game_id
    }

    pub fn canonical_url(&self) -> String {
        format!("https://lichess.org/{}", self.canonical_game_id)
    }

    pub fn side(&self) -> Option<LichessSide> {
        self.side
    }

    pub fn has_qualified_side(&self) -> bool {
        self.side.is_some()
    }

    pub fn side_qualified_url(&self, side: LichessSide) -> String {
        match self.side {
            Some(_) => self.source_url.clone(),
            None => format!(
                "https://lichess.org/{}/{}",
                self.game_identifier,
                side.as_path()
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Lichess Game URL")]
pub struct LichessUrlError;
