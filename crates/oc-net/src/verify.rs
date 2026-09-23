//! SHA-256, computed while the bytes stream past (IMPLEMENTATION_PLAN PHASE 9 detail 6).
//!
//! The file on disk is never re-read to be checked: the digest is of exactly the bytes that were
//! written, as they were written, so there is no window in which a verified name holds bytes the
//! check did not see.

use std::io::{Read, Write};

use sha2::{Digest, Sha256};

/// How much is read at a time. An I/O size, not a tunable: it changes no output and no decision.
const CHUNK: usize = 1 << 16;

/// Why a streamed copy stopped.
#[derive(Debug)]
pub enum CopyError {
    /// More bytes arrived than `limit` allows.
    TooLarge,
    /// `stop` said to stop.
    Stopped,
    Read(std::io::Error),
    Write(std::io::Error),
}

/// Copy `reader` into `writer`, hashing as it goes, and refusing to go past `limit` bytes.
/// `progress` is told the running total after every chunk, and `stop` is asked before every read,
/// so a caller can end the copy between two chunks. Returns the byte count and the lowercase hex
/// SHA-256.
pub fn copy_hashed(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    limit: u64,
    progress: &mut dyn FnMut(u64),
    stop: &dyn Fn() -> bool,
) -> Result<(u64, String), CopyError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    let mut total: u64 = 0;
    loop {
        if stop() {
            return Err(CopyError::Stopped);
        }
        let n = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(CopyError::Read(error)),
        };
        total = total.saturating_add(n as u64);
        if total > limit {
            return Err(CopyError::TooLarge);
        }
        let chunk = buffer.get(..n).unwrap_or_default();
        hasher.update(chunk);
        writer.write_all(chunk).map_err(CopyError::Write)?;
        progress(total);
    }
    Ok((total, hex(&hasher.finalize())))
}

/// Lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}
