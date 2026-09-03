pub(crate) const DAILY_COACHING_PATH: &str = "daily-coaching";
pub(crate) const GAME_IMPORT_PATH: &str = "game-import";

const CONTACT_ENV: &str = "PROVIDER_CONTACT";
const DEFAULT_CONTACT: &str = "https://github.com/br1anchen/chenchess-oss";

/// What Lichess and Chess.com see when this instance reads a public profile.
///
/// Both ask for a contact in the User-Agent so they can reach whoever is
/// making the requests. That is the operator, not this repository, so
/// `PROVIDER_CONTACT` names it; the default points at the source rather than
/// at anyone's mailbox.
pub(crate) fn provider_user_agent(path: &str) -> String {
    let contact = std::env::var(CONTACT_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CONTACT.to_string());
    format!("ChenChess/{} {path} ({contact})", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_the_crate_path_and_a_contact() {
        for path in [DAILY_COACHING_PATH, GAME_IMPORT_PATH] {
            let agent = provider_user_agent(path);
            assert!(agent.starts_with(&format!("ChenChess/{}", env!("CARGO_PKG_VERSION"))));
            assert!(agent.contains(path));
            // A provider that cannot reach the operator throttles the client,
            // so the contact is never allowed to be empty.
            let contact = agent
                .rsplit_once('(')
                .and_then(|(_, tail)| tail.strip_suffix(')'))
                .expect("the agent ends in a parenthesised contact");
            assert!(!contact.trim().is_empty());
        }
    }
}
