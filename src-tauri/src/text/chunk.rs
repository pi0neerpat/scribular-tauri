//! Transcript chunking. `chunk_by_chars` is a port of `chunkTranscript` in
//! Electron's `src/service/summarize.js` and is kept for parity; the
//! summarizer uses `chunk_by_tokens`, which budgets with the model's own
//! tokenizer (KTD3).

use std::fmt;

/// Electron's `CHUNK_SIZE`, in UTF-16 code units.
pub const CHUNK_SIZE: usize = 3500;

/// Transcripts with fewer space-separated words than this are rejected.
pub const MIN_WORDS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkError {
    /// Same condition and message as Electron's `chunkTranscript` throw.
    InsufficientAudio,
    /// A single word tokenizes to more than the budget on its own.
    WordExceedsBudget { tokens: usize, budget: usize },
}

impl fmt::Display for ChunkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientAudio => {
                f.write_str("Audio capture insufficient. Please record a bit more.")
            }
            Self::WordExceedsBudget { tokens, budget } => {
                write!(
                    f,
                    "A single word needs {tokens} tokens, over the chunk budget of {budget}."
                )
            }
        }
    }
}

impl std::error::Error for ChunkError {}

fn words(transcript: &str) -> Result<Vec<&str>, ChunkError> {
    // `split(" ")`, empty words included, exactly like the JS.
    let words: Vec<&str> = transcript.split(' ').collect();
    if words.len() < MIN_WORDS {
        return Err(ChunkError::InsufficientAudio);
    }
    Ok(words)
}

/// Port of `chunkTranscript`. Lengths are UTF-16 code units, and a first
/// word longer than `CHUNK_SIZE` yields a leading empty chunk, as in the JS.
pub fn chunk_by_chars(transcript: &str) -> Result<Vec<String>, ChunkError> {
    let mut chunks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    // Length of `current.join(" ")`, tracked instead of re-joining per word.
    let mut joined_len = 0;
    for word in words(transcript)? {
        let word_len = word.encode_utf16().count();
        if joined_len + word_len > CHUNK_SIZE {
            chunks.push(current.join(" "));
            current.clear();
            joined_len = 0;
        }
        joined_len += word_len + usize::from(!current.is_empty());
        current.push(word);
    }
    if !current.is_empty() {
        chunks.push(current.join(" "));
    }
    Ok(chunks)
}

/// Greedy word packing where each chunk's `count_tokens` stays within
/// `budget`. Words are never split; chunks rejoin words with single spaces
/// so concatenating chunks with `" "` restores the input.
///
/// A transcript that fits costs one count. Otherwise each chunk doubles its
/// word count until the text overflows, then binary-searches, so it costs
/// O(log w) counts for w words. Only text that was counted and fit is kept,
/// so a non-additive counter can leave a chunk short but never over budget.
pub fn chunk_by_tokens(
    transcript: &str,
    budget: usize,
    mut count_tokens: impl FnMut(&str) -> usize,
) -> Result<Vec<String>, ChunkError> {
    let words = words(transcript)?;
    if count_tokens(transcript) <= budget {
        return Ok(vec![transcript.to_owned()]);
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let rest = &words[start..];
        let tokens = count_tokens(rest[0]);
        if tokens > budget {
            return Err(ChunkError::WordExceedsBudget { tokens, budget });
        }
        let mut fits = |k: usize| count_tokens(&rest[..k].join(" ")) <= budget;
        // `lo` words were counted and fit; `hi` words overflowed.
        let (mut lo, mut hi) = (1, None);
        while lo < rest.len() {
            let k = (lo * 2).min(rest.len());
            if fits(k) {
                lo = k;
            } else {
                hi = Some(k);
                break;
            }
        }
        if let Some(mut hi) = hi {
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                if fits(mid) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
        }
        chunks.push(rest[..lo].join(" "));
        start += lo;
    }
    Ok(chunks)
}
