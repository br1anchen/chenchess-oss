//! The Firebase Auth emulator identity path, armed by loopback (ADR 0060).
//!
//! Coach Engine remains the single authority on Firebase identity. The
//! emulator changes one thing: an ID token it mints carries
//! `{"alg":"none","typ":"JWT"}` and an empty signature. Issuer, audience,
//! expiry, `iat`, `auth_time`, and `sub` are checked by the same `Validation`
//! that reads a Google-signed token.

use std::net::ToSocketAddrs;

use anyhow::Context;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;

const EMULATOR_HOST: &str = "FIREBASE_AUTH_EMULATOR_HOST";
const READABLE_HEADER: &[u8] = br#"{"alg":"RS256","typ":"JWT"}"#;

#[derive(Deserialize)]
struct EmulatorTokenHeader {
    alg: String,
    #[serde(default)]
    typ: Option<String>,
}

/// Refuses a `FIREBASE_AUTH_EMULATOR_HOST` that names anything but this
/// machine. Loopback is the property that matters and the deployment
/// environment name is not: staging is deployed and publicly reachable, while
/// an address on this machine cannot be a token mint someone else controls.
/// Every address the value resolves to must be loopback, and Railway sets the
/// variable in no environment at all.
pub(super) fn loopback_emulator_host(value: &str) -> anyhow::Result<()> {
    let addresses = value
        .to_socket_addrs()
        .with_context(|| format!("{EMULATOR_HOST} must be a resolvable host:port, not {value}"))?
        .collect::<Vec<_>>();
    anyhow::ensure!(
        !addresses.is_empty() && addresses.iter().all(|address| address.ip().is_loopback()),
        "{EMULATOR_HOST} must resolve to loopback, and {value} does not"
    );
    Ok(())
}

/// An emulator-minted ID token, with its algorithm restated so `jsonwebtoken`
/// can read it: that crate parses a header into an `Algorithm` enum with no
/// `none` variant, and refuses the token before a single claim is read.
/// Nothing else about the token moves — the signature stays unread because the
/// emulator profile's `Validation` has signature validation off.
///
/// `None` for anything the emulator did not mint, including a signed token,
/// which the Google path already reads unaided.
pub(super) fn readable_emulator_token(token: &str) -> Option<String> {
    let (header, rest) = token.split_once('.')?;
    let (payload, signature) = rest.split_once('.')?;
    if !signature.is_empty() {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(header).ok()?;
    let header: EmulatorTokenHeader = serde_json::from_slice(&decoded).ok()?;
    if header.alg != "none" || header.typ.as_deref() != Some("JWT") {
        return None;
    }
    Some(format!(
        "{}.{payload}.",
        URL_SAFE_NO_PAD.encode(READABLE_HEADER)
    ))
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    use super::{loopback_emulator_host, readable_emulator_token};

    #[test]
    fn a_non_loopback_emulator_host_is_refused_at_configuration_time() {
        let error = loopback_emulator_host("192.0.2.10:9099").unwrap_err();

        assert_eq!(
            error.to_string(),
            "FIREBASE_AUTH_EMULATOR_HOST must resolve to loopback, and 192.0.2.10:9099 does not"
        );
        assert_eq!(
            loopback_emulator_host("securetoken.example.test")
                .unwrap_err()
                .to_string(),
            "FIREBASE_AUTH_EMULATOR_HOST must be a resolvable host:port, not securetoken.example.test"
        );
    }

    #[test]
    fn a_loopback_emulator_host_arms_the_local_identity_path() {
        for host in ["127.0.0.1:9099", "[::1]:9099"] {
            loopback_emulator_host(host).expect("a loopback emulator host is the armed case");
        }
    }

    #[test]
    fn only_an_unsigned_emulator_header_is_restated() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"player"}"#);
        let emulator = format!("{}.{payload}.", encoded(br#"{"alg":"none","typ":"JWT"}"#));

        assert_eq!(
            readable_emulator_token(&emulator),
            Some(format!(
                "{}.{payload}.",
                encoded(br#"{"alg":"RS256","typ":"JWT"}"#)
            ))
        );
        let refused = [
            format!(
                "{}.{payload}.signature",
                encoded(br#"{"alg":"none","typ":"JWT"}"#)
            ),
            format!("{}.{payload}.", encoded(br#"{"alg":"RS256","typ":"JWT"}"#)),
            format!(
                "{}.{payload}.",
                encoded(br#"{"alg":"none","typ":"at+jwt"}"#)
            ),
            format!("{}.{payload}.", encoded(b"not-json")),
            format!("{payload}."),
        ];
        for (case, token) in refused.into_iter().enumerate() {
            assert_eq!(
                readable_emulator_token(&token),
                None,
                "token case {case} was restated"
            );
        }
    }

    fn encoded(header: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(header)
    }
}
