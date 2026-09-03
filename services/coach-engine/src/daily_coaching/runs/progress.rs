use super::{DailyCoachingGameProgress, DailyCoachingRunGame};

impl DailyCoachingRunGame {
    pub(crate) fn attempts(&self) -> u8 {
        self.attempts
    }

    pub(super) fn is_valid(&self) -> bool {
        self.selected.is_valid()
            && match &self.progress {
                DailyCoachingGameProgress::Pending => true,
                DailyCoachingGameProgress::Reviewed { review } => {
                    self.attempts > 0 && review.validate_for_selection(&self.selected).is_ok()
                }
                DailyCoachingGameProgress::TerminalUnreviewed => self.attempts > 0,
                DailyCoachingGameProgress::RetryExhaustedUnreviewed => self.attempts > 0,
                DailyCoachingGameProgress::DeadlineUnreviewed => true,
            }
    }
}
