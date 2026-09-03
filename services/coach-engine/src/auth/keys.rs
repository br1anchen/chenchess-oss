use anyhow::Context;
use jsonwebtoken::{jwk::JwkSet, DecodingKey};

#[derive(Clone)]
pub(super) struct VerificationKey {
    pub(super) kid: Option<String>,
    pub(super) decoding_key: DecodingKey,
}

impl VerificationKey {
    /// The emulator profile decodes with signature validation off, so no key
    /// material is read; `jsonwebtoken::decode` still takes one.
    pub(super) fn unread() -> Self {
        Self {
            kid: None,
            decoding_key: DecodingKey::from_secret(&[]),
        }
    }
}

pub(super) fn matching_keys(keys: &[VerificationKey], kid: &str) -> Vec<VerificationKey> {
    keys.iter()
        .filter(|key| key.kid.as_deref() == Some(kid))
        .cloned()
        .collect()
}

pub(super) fn parse_jwks(value: &str, name: &str) -> anyhow::Result<JwkSet> {
    serde_json::from_str(value).with_context(|| format!("{name} must contain valid JWKS JSON"))
}

pub(super) fn verification_keys(jwks: &JwkSet) -> anyhow::Result<Vec<VerificationKey>> {
    let keys = jwks
        .keys
        .iter()
        .map(
            |jwk| -> Result<VerificationKey, jsonwebtoken::errors::Error> {
                Ok(VerificationKey {
                    kid: jwk.common.key_id.clone(),
                    decoding_key: DecodingKey::from_jwk(jwk)?,
                })
            },
        )
        .collect::<Result<Vec<_>, _>>()
        .context("JWKS contains an unsupported verification key")?;
    anyhow::ensure!(!keys.is_empty(), "JWKS must contain a key");
    Ok(keys)
}
