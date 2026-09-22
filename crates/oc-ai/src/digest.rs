//! SHA-256, and the lowercase hex every hash in this crate is written as.
//!
//! One place for both, because a hash appears in four artifacts that have to agree byte for byte:
//! the cache key (D13.8), a cassette's name and its `prompt_sha256`/`grammar_sha256` fields
//! (Appendix B), an `LlmTrace`, and the prompt pin manifest.

use sha2::{Digest, Sha256};

/// The SHA-256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Lowercase hex, two digits per byte.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// The empty string's hash is the one every SHA-256 implementation agrees on, and the hex is the
/// form every artifact carries.
#[test]
fn sha256_hex_is_the_standard_digest_in_lowercase() {
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(hex(&[0x00, 0x0f, 0xf0, 0xff]), "000ff0ff");
}
