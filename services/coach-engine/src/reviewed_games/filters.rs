use chrono::NaiveDate;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use crate::imported_games::{
    ImportedGameOutcome, ImportedGameProvider, ImportedGameReviewSide, ImportedGameTimeControlClass,
};

use super::{MergedCard, ReviewedGameSearchError, ReviewedGameSearchRequest};

const MIN_RATING: u16 = 100;
const MAX_RATING: u16 = 3_500;

#[derive(Debug)]
pub(super) struct SearchFilters {
    played_from: Option<NaiveDate>,
    played_to: Option<NaiveDate>,
    provider: Option<ImportedGameProvider>,
    opening_eco_prefix: Option<String>,
    opening_name: Option<String>,
    outcome: Option<ImportedGameOutcome>,
    review_side: Option<ImportedGameReviewSide>,
    time_control_class: Option<ImportedGameTimeControlClass>,
    opponent_name: Option<String>,
    opponent_rating_min: Option<u16>,
    opponent_rating_max: Option<u16>,
}

impl SearchFilters {
    pub(super) fn parse(
        request: &ReviewedGameSearchRequest,
    ) -> Result<Self, ReviewedGameSearchError> {
        let played_from = parse_date(request.played_from.as_deref())?;
        let played_to = parse_date(request.played_to.as_deref())?;
        if played_from
            .zip(played_to)
            .is_some_and(|(from, to)| from > to)
            || request
                .opponent_rating_min
                .zip(request.opponent_rating_max)
                .is_some_and(|(min, max)| min > max)
            || request
                .opponent_rating_min
                .into_iter()
                .chain(request.opponent_rating_max)
                .any(|rating| !(MIN_RATING..=MAX_RATING).contains(&rating))
            || request
                .opening_eco_prefix
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            || request
                .opening_name
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            || request
                .opponent_name
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ReviewedGameSearchError::InvalidRequest);
        }
        Ok(Self {
            played_from,
            played_to,
            provider: request.provider,
            opening_eco_prefix: request.opening_eco_prefix.as_deref().map(unicode_lower),
            opening_name: request.opening_name.as_deref().map(fold_diacritics),
            outcome: request.outcome,
            review_side: request.review_side,
            time_control_class: request.time_control_class,
            opponent_name: request.opponent_name.as_deref().map(unicode_lower),
            opponent_rating_min: request.opponent_rating_min,
            opponent_rating_max: request.opponent_rating_max,
        })
    }

    pub(super) fn matches(&self, card: &MergedCard) -> bool {
        self.matches_played_on(card)
            && self.provider.is_none_or(|value| value == card.provider)
            && self.opening_eco_prefix.as_deref().is_none_or(|prefix| {
                card.opening
                    .as_ref()
                    .is_some_and(|opening| unicode_lower(&opening.eco).starts_with(prefix))
            })
            && self.opening_name.as_deref().is_none_or(|needle| {
                card.opening
                    .as_ref()
                    .is_some_and(|opening| fold_diacritics(&opening.name).contains(needle))
            })
            && self.outcome.is_none_or(|value| card.outcome == Some(value))
            && self
                .review_side
                .is_none_or(|value| value == card.review_side)
            && self
                .time_control_class
                .is_none_or(|value| card.time_control_class == Some(value))
            && self.opponent_name.as_deref().is_none_or(|name| {
                card.opponent_name
                    .as_deref()
                    .is_some_and(|opponent| unicode_lower(opponent) == name)
            })
            && self
                .opponent_rating_min
                .is_none_or(|minimum| card.opponent_rating.is_some_and(|rating| rating >= minimum))
            && self
                .opponent_rating_max
                .is_none_or(|maximum| card.opponent_rating.is_some_and(|rating| rating <= maximum))
    }

    fn matches_played_on(&self, card: &MergedCard) -> bool {
        if self.played_from.is_none() && self.played_to.is_none() {
            return true;
        }
        let Some(ended_at) = card.ended_at else {
            return false;
        };
        let played_on = ended_at.date_naive();
        self.played_from.is_none_or(|from| played_on >= from)
            && self.played_to.is_none_or(|to| played_on <= to)
    }
}

fn parse_date(value: Option<&str>) -> Result<Option<NaiveDate>, ReviewedGameSearchError> {
    value
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| ReviewedGameSearchError::InvalidRequest)
        })
        .transpose()
}

fn unicode_lower(value: &str) -> String {
    value.chars().flat_map(char::to_lowercase).collect()
}

pub(super) fn fold_diacritics(value: &str) -> String {
    value
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .collect()
}
