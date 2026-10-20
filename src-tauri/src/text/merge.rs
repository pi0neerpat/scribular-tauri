//! Port of `mergeDiarizedSegments` in Electron's
//! `src/service/audio/audioWorker.js`: interleaves the remote ("Client") and
//! microphone ("You") transcripts by start time into speaker-labelled turns.

use serde::Deserialize;

use super::{from_utf16, js_trim, strip_brackets, utf16};

/// One timed transcript segment from a single audio source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    /// `None` mirrors a missing or null `text` in the JS segment objects.
    #[serde(default)]
    pub text: Option<String>,
}

/// Merges two sources into `Speaker: text` lines joined by `\n`.
pub fn merge_diarized_segments(client: &[Segment], you: &[Segment]) -> String {
    let mut labeled: Vec<(&str, &Segment)> = client
        .iter()
        .map(|s| ("Client", s))
        .chain(you.iter().map(|s| ("You", s)))
        .collect();
    // Stable, like Array.prototype.sort: ties keep Client before You.
    labeled.sort_by_key(|(_, s)| s.start_ms);

    let mut lines: Vec<String> = Vec::new();
    let mut current: Option<(&str, Vec<String>)> = None;
    for (speaker, segment) in labeled {
        let stripped = strip_brackets(&utf16(segment.text.as_deref().unwrap_or("")));
        let text = js_trim(&stripped);
        if text.is_empty() {
            continue;
        }
        let text = from_utf16(text);
        match &mut current {
            Some((name, texts)) if *name == speaker => texts.push(text),
            _ => {
                if let Some((name, texts)) = current.take() {
                    lines.push(format!("{name}: {}", texts.join(" ")));
                }
                current = Some((speaker, vec![text]));
            }
        }
    }
    if let Some((name, texts)) = current {
        lines.push(format!("{name}: {}", texts.join(" ")));
    }
    lines.join("\n")
}
