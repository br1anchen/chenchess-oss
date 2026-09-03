//! Player-message gate shared by HostTurn grounding.

use crate::review_session_contract::{ProviderUnavailableReason, ReviewSessionLimits};

/// Size- and control-character gate. Must run before any spend.
/// Newline, carriage return, and tab are allowed; other control characters
/// are not. Empty and oversize `StartHostTurn` text is rejected earlier as
/// `InvalidCommand`.
pub fn gate_player_message(message: &str) -> Result<(), ProviderUnavailableReason> {
    if message.trim().is_empty()
        || message.len() > usize::from(ReviewSessionLimits::V1.max_player_message_bytes)
        || message.chars().any(disallowed_control_character)
    {
        Err(ProviderUnavailableReason::LanguageLayer)
    } else {
        Ok(())
    }
}

fn disallowed_control_character(ch: char) -> bool {
    ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')
}

#[cfg(test)]
mod tests {
    use super::gate_player_message;
    use crate::review_session_contract::ReviewSessionLimits;

    #[test]
    fn control_characters_and_oversize_messages_fail_the_gate() {
        assert!(gate_player_message("How does this hold up?").is_ok());
        assert!(gate_player_message("has\u{0007}bell").is_err());
        assert!(gate_player_message("line\nbreak").is_ok());
        assert!(gate_player_message("tab\tseparated").is_ok());
        assert!(gate_player_message("carriage\rreturn").is_ok());
        assert!(gate_player_message("").is_err());
        assert!(gate_player_message(
            &"x".repeat(usize::from(ReviewSessionLimits::V1.max_player_message_bytes) + 1)
        )
        .is_err());
    }
}
