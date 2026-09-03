use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;

pub(super) const FIREBASE_PROJECT_ID: &str = "chenchess-test";
pub(super) const COACH_ISSUER: &str = "https://beta.chenchess.test";
pub(super) const COACH_RESOURCE: &str = "https://beta.chenchess.test/mcp";
pub(super) const COACH_SCOPE: &str = "coach:review";
pub(super) const MCP_CONFORMANCE_PLAYER_ID: &str =
    "benchmark-issue-335-mcp-conformance:019c1510-391c-4d67-8ff1-35775e85c504";
pub(super) fn jwt_jwks() -> &'static str {
    crate::certification_keys::jwks()
}

const ISSUER: &str = "https://securetoken.google.com/chenchess-test";
const KID: &str = "firebase-test-key";

pub(super) fn firebase_token(player_id: &str) -> String {
    signed_token(player_id, None, None, None, None, None)
}

pub(super) fn verified_firebase_token(
    player_id: &str,
    email_verified: bool,
    sign_in_provider: &str,
) -> String {
    signed_token(
        player_id,
        None,
        Some(email_verified),
        Some(sign_in_provider),
        None,
        None,
    )
}

pub(super) fn firebase_token_with_email(
    player_id: &str,
    email: &str,
    email_verified: bool,
    sign_in_provider: &str,
) -> String {
    signed_token(
        player_id,
        Some(email),
        Some(email_verified),
        Some(sign_in_provider),
        None,
        None,
    )
}

pub(super) fn administrator_token(
    player_id: &str,
    email_verified: bool,
    chenchess_admin: Option<bool>,
) -> String {
    signed_token(
        player_id,
        None,
        Some(email_verified),
        Some("password"),
        chenchess_admin,
        None,
    )
}

pub(super) fn coach_token(player_id: &str) -> String {
    coach_token_with_conformance_claim(player_id, None)
}

pub(super) fn mcp_conformance_firebase_token(player_id: &str) -> String {
    signed_token(player_id, None, None, Some("custom"), None, Some(true))
}

pub(super) fn firebase_token_with_conformance_claim(
    player_id: &str,
    sign_in_provider: &str,
    claim: Option<bool>,
) -> String {
    signed_token(player_id, None, None, Some(sign_in_provider), None, claim)
}

pub(super) fn mcp_conformance_coach_token(player_id: &str) -> String {
    coach_token_with_conformance_claim(player_id, Some(true))
}

fn coach_token_with_conformance_claim(
    player_id: &str,
    chenchess_mcp_conformance: Option<bool>,
) -> String {
    let issued_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("test clock must be after the Unix epoch")
        .as_secs() as usize;
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_string());
    header.typ = Some("at+jwt".to_string());
    encode(
        &header,
        &CoachTestClaims {
            sub: player_id,
            exp: issued_at + 600,
            iat: issued_at,
            iss: COACH_ISSUER,
            aud: COACH_RESOURCE,
            jti: "coach-access-token-test",
            scope: COACH_SCOPE,
            chenchess_mcp_conformance,
        },
        &EncodingKey::from_rsa_pem(crate::certification_keys::private_key_pem().as_bytes())
            .expect("valid private key"),
    )
    .expect("valid Coach token")
}

fn signed_token(
    player_id: &str,
    email: Option<&str>,
    email_verified: Option<bool>,
    sign_in_provider: Option<&str>,
    chenchess_admin: Option<bool>,
    chenchess_mcp_conformance: Option<bool>,
) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_string());
    encode(
        &header,
        &TestClaims {
            sub: player_id,
            exp: 4_102_444_800,
            iat: 1_700_000_000,
            auth_time: 1_700_000_000,
            email,
            email_verified,
            firebase: sign_in_provider
                .map(|sign_in_provider| FirebaseTestClaims { sign_in_provider }),
            chenchess_admin,
            chenchess_mcp_conformance,
            iss: ISSUER,
            aud: FIREBASE_PROJECT_ID,
        },
        &EncodingKey::from_rsa_pem(crate::certification_keys::private_key_pem().as_bytes())
            .expect("valid private key"),
    )
    .expect("valid token")
}

#[derive(Serialize)]
struct TestClaims<'a> {
    sub: &'a str,
    exp: usize,
    iat: usize,
    auth_time: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    firebase: Option<FirebaseTestClaims<'a>>,
    #[serde(rename = "chenchessAdmin", skip_serializing_if = "Option::is_none")]
    chenchess_admin: Option<bool>,
    #[serde(
        rename = "chenchessMcpConformance",
        skip_serializing_if = "Option::is_none"
    )]
    chenchess_mcp_conformance: Option<bool>,
    iss: &'a str,
    aud: &'a str,
}

#[derive(Serialize)]
struct FirebaseTestClaims<'a> {
    sign_in_provider: &'a str,
}

#[derive(Serialize)]
struct CoachTestClaims<'a> {
    sub: &'a str,
    exp: usize,
    iat: usize,
    iss: &'a str,
    aud: &'a str,
    jti: &'a str,
    scope: &'a str,
    #[serde(
        rename = "chenchessMcpConformance",
        skip_serializing_if = "Option::is_none"
    )]
    chenchess_mcp_conformance: Option<bool>,
}
