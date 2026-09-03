use std::sync::{Arc, Mutex, PoisonError};

use super::CoachTurnId;

/// The at-most-one in-flight Coach Turn for one Player on one Game Import.
///
/// A Coach Turn belongs to the Player and the reviewed Game, not to the
/// conversation that started it. Every Review Session over one Game Import
/// shares a single scope, so a concurrent second turn is refused however it
/// arrives — including from another conversation.
#[derive(Debug, Default)]
pub struct CoachTurnActivity {
    in_flight: Mutex<Option<CoachTurnId>>,
}

/// Holds the scope for one admitted Coach Turn and releases it when dropped.
pub(crate) struct CoachTurnLease {
    activity: Arc<CoachTurnActivity>,
    coach_turn_id: CoachTurnId,
}

impl CoachTurnActivity {
    pub(crate) fn acquire(self: &Arc<Self>, coach_turn_id: &CoachTurnId) -> Option<CoachTurnLease> {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if in_flight.is_some() {
            return None;
        }
        *in_flight = Some(coach_turn_id.clone());
        Some(CoachTurnLease {
            activity: self.clone(),
            coach_turn_id: coach_turn_id.clone(),
        })
    }
}

impl CoachTurnLease {
    /// Rebinds an already-held scope to the turn that replaces its holder.
    ///
    /// The scope is never released in between, so no other conversation can
    /// take it while a rollback reinstates the turn it superseded.
    pub(super) fn transfer(&mut self, coach_turn_id: &CoachTurnId) {
        let mut in_flight = self
            .activity
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        debug_assert_eq!(
            in_flight.as_ref(),
            Some(&self.coach_turn_id),
            "only the holder of a scope can hand it on"
        );
        *in_flight = Some(coach_turn_id.clone());
        drop(in_flight);
        self.coach_turn_id = coach_turn_id.clone();
    }
}

impl Drop for CoachTurnLease {
    fn drop(&mut self) {
        let mut in_flight = self
            .activity
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if in_flight.as_ref() == Some(&self.coach_turn_id) {
            in_flight.take();
        }
    }
}
