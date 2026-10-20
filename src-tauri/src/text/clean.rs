//! Port of the `processTranscript` cleanup chain in Electron's
//! `src/service/summarize.js`. Each pass mirrors one `.replace` call. The JS
//! patterns use backreferences, which the `regex` crate cannot express, so
//! every pass is a small scanner whose control flow reproduces the backtracking
//! outcome of V8 for that specific pattern.

use super::{from_utf16, is_js_space, is_js_word, strip_brackets, utf16};

const DOT: u16 = b'.' as u16;

/// Cleans a raw transcript exactly as Electron's `processTranscript` does.
pub fn clean_transcript(transcript: &str) -> String {
    let s = strip_brackets(&utf16(transcript));
    let s = strip_sound_descriptions(&s);
    let s = collapse_repeated_words(&s);
    let s = collapse_repeated_chars(&s);
    let s = collapse_dot_sequences(&s);
    let s = collapse_dot_runs(&s);
    from_utf16(&collapse_spaces(&s))
}

const SOUND_WORDS: [&[u8]; 10] = [
    b"music",
    b"chirping",
    b"singing",
    b"playing",
    b"laughter",
    b"applause",
    b"noise",
    b"silence",
    b"inaudible",
    b"crosstalk",
];

fn ascii_ci_eq(a: u16, b: u16) -> bool {
    // Without the `u` flag, case folding never maps a non-ASCII unit onto an
    // ASCII one, so ASCII-only folding is exact for these patterns.
    a == b || (a < 0x80 && b < 0x80 && (a as u8).eq_ignore_ascii_case(&(b as u8)))
}

fn starts_with_ci(units: &[u16], at: usize, word: &[u8]) -> bool {
    units.len() >= at + word.len()
        && word
            .iter()
            .enumerate()
            .all(|(k, &w)| ascii_ci_eq(units[at + k], u16::from(w)))
}

/// `/\([^)]{1,40}(?:music|…|crosstalk)[^)]*\)/gi` → `""`.
///
/// Every unit between `(` and the keyword is non-`)` and the keyword contains
/// no `)`, so a match always ends at the first `)` after the `(`; the match
/// exists when some keyword starts after 1..=40 units past the `(` and ends at or
/// before that `)`.
fn strip_sound_descriptions(units: &[u16]) -> Vec<u16> {
    let (open, close) = (u16::from(b'('), u16::from(b')'));
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        if units[i] == open {
            if let Some(rel) = units[i + 1..].iter().position(|&c| c == close) {
                let close_at = i + 1 + rel;
                let first = i + 2;
                let last = (i + 42).min(close_at);
                let hit = (first..last).any(|p| {
                    SOUND_WORDS
                        .iter()
                        .any(|w| p + w.len() <= close_at && starts_with_ci(units, p, w))
                });
                if hit {
                    i = close_at + 1;
                    continue;
                }
            }
        }
        out.push(units[i]);
        i += 1;
    }
    out
}

/// `/(\b\w+\b)(?:[,\s]+\1){3,}/gi` → `"$1"`.
///
/// The capture is always a whole ASCII word starting at a word boundary. Each
/// repetition greedily eats separators and then needs the word again
/// (case-insensitively, with no trailing boundary, so `the theory` repeats
/// `the`). Separators can never overlap the word, so greedy is the only
/// outcome that matters.
fn collapse_repeated_words(units: &[u16]) -> Vec<u16> {
    let is_sep = |c: u16| c == u16::from(b',') || is_js_space(c);
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let at_word_start = is_js_word(units[i]) && (i == 0 || !is_js_word(units[i - 1]));
        if at_word_start {
            let word_end = i + units[i..].iter().take_while(|&&c| is_js_word(c)).count();
            let word = &units[i..word_end];
            let mut end = word_end;
            let mut repeats = 0;
            loop {
                let seps = units[end..].iter().take_while(|&&c| is_sep(c)).count();
                let at = end + seps;
                let matches = seps > 0
                    && units.len() >= at + word.len()
                    && word
                        .iter()
                        .enumerate()
                        .all(|(k, &w)| ascii_ci_eq(units[at + k], w));
                if !matches {
                    break;
                }
                end = at + word.len();
                repeats += 1;
            }
            if repeats >= 3 {
                out.extend_from_slice(word);
                i = end;
                continue;
            }
        }
        out.push(units[i]);
        i += 1;
    }
    out
}

/// `/(\S)(?:\s*\1){3,}/g` → `"$1"`. Lone surrogate halves can never repeat
/// in valid UTF-16, so astral characters are left alone, as in V8.
fn collapse_repeated_chars(units: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let c = units[i];
        if !is_js_space(c) {
            let mut end = i + 1;
            let mut repeats = 0;
            loop {
                let at = end + units[end..].iter().take_while(|&&u| is_js_space(u)).count();
                if units.get(at) != Some(&c) {
                    break;
                }
                end = at + 1;
                repeats += 1;
            }
            if repeats >= 3 {
                out.push(c);
                i = end;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// `/(?:\s*\.\s*){2,}/g` → `"."`. A match may begin on whitespace.
fn collapse_dot_sequences(units: &[u16]) -> Vec<u16> {
    let space_run = |from: usize| {
        units[from..]
            .iter()
            .take_while(|&&c| is_js_space(c))
            .count()
    };
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let mut end = i;
        let mut dots = 0;
        loop {
            let at = end + space_run(end);
            if units.get(at) != Some(&DOT) {
                break;
            }
            end = at + 1 + space_run(at + 1);
            dots += 1;
        }
        if dots >= 2 {
            out.push(DOT);
            i = end;
            continue;
        }
        if dots == 0 && is_js_space(units[i]) {
            // Every start inside this whitespace run reaches the same non-dot
            // unit and fails, so copy the run and skip past it.
            let run = space_run(i);
            out.extend_from_slice(&units[i..i + run]);
            i += run;
            continue;
        }
        out.push(units[i]);
        i += 1;
    }
    out
}

/// `/\.{2,}/g` → `"."`.
fn collapse_dot_runs(units: &[u16]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let mut run = 0;
    for &c in units {
        run = if c == DOT { run + 1 } else { 0 };
        if run < 2 {
            out.push(c);
        }
    }
    out
}

/// `/ +/g` → `" "` (ASCII space only, unlike `\s`).
fn collapse_spaces(units: &[u16]) -> Vec<u16> {
    let space = u16::from(b' ');
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    for &c in units {
        if !(c == space && out.last() == Some(&space)) {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_word_matches_prefix_of_longer_word() {
        assert_eq!(clean_transcript("the the the the theory"), "theory");
    }

    #[test]
    fn non_ascii_letters_end_a_js_word() {
        assert_eq!(
            clean_transcript("café café café café"),
            "café café café café"
        );
    }
}
