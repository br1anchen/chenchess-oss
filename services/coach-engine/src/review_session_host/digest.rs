use sha2::{Digest, Sha256};

pub fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes.as_ref()))
}

pub fn digest_canonical_json(value: &serde_json::Value) -> String {
    let canonical =
        serde_json_canonicalizer::to_string(value).expect("schema values are canonicalizable");
    sha256_hex(canonical)
}

pub fn digest_templates(system: &str, user: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(system.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(user.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}
