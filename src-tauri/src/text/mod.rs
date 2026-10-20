//! Transcript text pipeline ported from the Electron app (KTD15).
//!
//! The JavaScript originals run on UTF-16 strings with JS regex semantics, so
//! the ports work on UTF-16 code units as well: lengths, `\s`, `\w` and
//! backreferences all have to agree with V8 byte for byte. Parity is checked
//! against fixtures generated from the JS by `scripts/gen-text-fixtures.mjs`.

mod chunk;
mod clean;
mod merge;

pub use chunk::{chunk_by_chars, chunk_by_tokens, ChunkError, CHUNK_SIZE, MIN_WORDS};
pub use clean::clean_transcript;
pub use merge::{merge_diarized_segments, Segment};

/// JS `\s`: WhiteSpace plus LineTerminator code points (ECMA-262 22.2.2.9).
pub(crate) fn is_js_space(c: u16) -> bool {
    matches!(
        c,
        0x09..=0x0d
            | 0x20
            | 0xa0
            | 0x1680
            | 0x2000..=0x200a
            | 0x2028
            | 0x2029
            | 0x202f
            | 0x205f
            | 0x3000
            | 0xfeff
    )
}

/// JS `\w` without the `u` flag: ASCII letters, digits and underscore.
pub(crate) fn is_js_word(c: u16) -> bool {
    (c < 0x80 && (c as u8).is_ascii_alphanumeric()) || c == u16::from(b'_')
}

pub(crate) fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

pub(crate) fn from_utf16(units: &[u16]) -> String {
    // Inputs come from valid Rust strings and every rewrite removes or keeps
    // whole BMP characters or whole surrogate pairs, so this never fails.
    String::from_utf16(units).expect("text rewrite split a surrogate pair")
}

/// JS `String.prototype.trim`.
pub(crate) fn js_trim(units: &[u16]) -> &[u16] {
    let start = units
        .iter()
        .position(|&c| !is_js_space(c))
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|&c| !is_js_space(c))
        .map_or(start, |i| i + 1);
    &units[start..end]
}

/// Removes every `[...]` span like JS `.replace(/\[[^\]]*\]/g, "")`.
pub(crate) fn strip_brackets(units: &[u16]) -> Vec<u16> {
    let (open, close) = (u16::from(b'['), u16::from(b']'));
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        if units[i] == open {
            if let Some(end) = units[i + 1..].iter().position(|&c| c == close) {
                i += end + 2;
                continue;
            }
        }
        out.push(units[i]);
        i += 1;
    }
    out
}
