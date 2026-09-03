use sha2::{Digest, Sha256};

/// Renders opaque semantic material as a Firestore-safe document ID.
///
/// Callers supply the exact canonical bytes for their key. The returned value
/// is always a lowercase 64-character SHA-256 digest, so semantic identifiers
/// never become path data.
pub(crate) fn hashed_path_segment(material: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(material.as_ref()))
}

/// Renders bytes as lowercase hex.
///
/// Beside [`hashed_path_segment`] because everything addressed here is written
/// in the same alphabet: a digest, a random secret, and the segments they
/// become all read the same way and are validated by the same predicate.
pub(crate) fn lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_segments_are_fixed_length_opaque_lowercase_hex() {
        let raw = "firebase/player:with/path?characters";
        let segment = hashed_path_segment(raw);

        assert_eq!(segment.len(), 64);
        assert!(segment.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(segment, segment.to_ascii_lowercase());
        assert!(!segment.contains(raw));
    }
}
