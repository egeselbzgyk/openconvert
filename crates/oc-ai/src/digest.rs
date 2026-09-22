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

/// The 32 bytes a 64-digit hex string spells, or `None` when it spells anything else.
pub fn unhex32(text: &str) -> Option<[u8; 32]> {
    let digits = text.as_bytes();
    let mut out = [0u8; 32];
    if digits.len() != out.len() * 2 {
        return None;
    }
    for (index, pair) in digits.chunks(2).enumerate() {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        out[index] = u8::try_from((high << 4) | low).ok()?;
    }
    Some(out)
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

    let digest = sha256(b"abc");
    assert_eq!(unhex32(&hex(&digest)), Some(digest));
    assert_eq!(unhex32("00"), None);
    assert_eq!(unhex32(&"g".repeat(64)), None);
}
