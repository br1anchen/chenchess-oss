pub mod chess_com;
mod core_integrity;
mod critical_moment_comment;
mod decision_explanation;
mod delivery;
mod evidence;
mod game_review;
mod grounded_moment;
mod host_turn;
mod learning;
pub mod lichess;
mod model;
mod opening;
mod operations;
mod player_plan_evaluation;
mod position_builder;
mod position_content;
mod position_integrity;
mod position_phase;
mod presentation;

pub use critical_moment_comment::*;
pub use decision_explanation::*;
pub use delivery::encode_delivery_frame;
pub use evidence::*;
pub use game_review::*;
pub use grounded_moment::*;
pub use host_turn::*;
pub use learning::*;
pub use model::*;
pub use opening::*;
pub use operations::*;
pub use player_plan_evaluation::*;
pub use position_builder::{build_position_snapshot, STANDARD_STARTING_FEN};
pub use position_content::{CanonicalPosition, PositionContentId};
pub use position_phase::*;
pub use presentation::*;

// pub (not pub(crate)): the app crate's critical_moment_comment enforces the
// same byte budget when composing player messages.
pub fn json_text_bytes_within_limit(value: &str, max_bytes: u16) -> bool {
    serde_json::to_vec(value)
        .is_ok_and(|encoded| encoded.len().saturating_sub(2) <= usize::from(max_bytes))
}
