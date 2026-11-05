use std::collections::BTreeMap;

use super::{from_utf16, js_trim, strip_brackets, utf16, Segment};

pub type SpeakerMap = BTreeMap<String, String>;

pub fn default_speaker_map(ids: impl IntoIterator<Item = String>) -> SpeakerMap {
    ids.into_iter()
        .map(|id| {
            let number = id
                .strip_prefix("SPEAKER_")
                .unwrap_or("0")
                .parse::<u32>()
                .unwrap_or(0)
                + 1;
            (id, format!("Speaker {number}"))
        })
        .collect()
}

pub fn build_raw_transcript(turns: &[(String, Segment)]) -> String {
    let mut lines: Vec<(String, Vec<String>)> = Vec::new();
    for (id, segment) in turns {
        let units = utf16(segment.text.as_deref().unwrap_or(""));
        let stripped = strip_brackets(&units);
        let text = js_trim(&stripped);
        if text.is_empty() {
            continue;
        }
        let text = from_utf16(text);
        match lines.last_mut() {
            Some((speaker, words)) if speaker == id => words.push(text),
            _ => lines.push((id.clone(), vec![text])),
        }
    }
    lines
        .into_iter()
        .map(|(speaker, words)| format!("{speaker}: {}", words.join(" ")))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn apply_speaker_map(raw: &str, map: &SpeakerMap) -> String {
    let mut lines: Vec<(String, String)> = Vec::new();
    for line in raw.lines() {
        let (speaker, text) = match line.split_once(": ") {
            Some((id, text)) if id.starts_with("SPEAKER_") => {
                (map.get(id).cloned().unwrap_or_else(|| id.to_string()), text)
            }
            _ => (String::new(), line),
        };
        if !speaker.is_empty() {
            match lines.last_mut() {
                Some((last, body)) if last == &speaker => {
                    body.push(' ');
                    body.push_str(text);
                }
                _ => lines.push((speaker, text.to_string())),
            }
        } else {
            lines.push((speaker, text.to_string()));
        }
    }
    lines
        .into_iter()
        .map(|(speaker, text)| {
            if speaker.is_empty() {
                text
            } else {
                format!("{speaker}: {text}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn substitute_speaker_names(summary: &str, old: &SpeakerMap, new: &SpeakerMap) -> String {
    old.iter().fold(summary.to_string(), |summary, (id, from)| {
        let to = new.get(id).map(String::as_str).unwrap_or(from);
        if from == to || from.is_empty() {
            return summary;
        }
        replace_whole_words(&summary, from, to)
    })
}

fn replace_whole_words(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(from) {
        let end = at + from.len();
        let left_word = rest[..at].chars().next_back().is_some_and(is_word);
        let right_word = rest[end..].chars().next().is_some_and(is_word);
        out.push_str(&rest[..at]);
        if !left_word && !right_word {
            out.push_str(to);
        } else {
            out.push_str(from);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_only_line_prefixes_and_merges_same_labels() {
        let raw = "SPEAKER_0: one\nSPEAKER_1: SPEAKER_0: mid\nSPEAKER_0: two\nSPEAKER_9: stays";
        let map = SpeakerMap::from([
            ("SPEAKER_0".into(), "Therapist".into()),
            ("SPEAKER_1".into(), "Client".into()),
        ]);
        assert_eq!(
            apply_speaker_map(raw, &map),
            "Therapist: one\nClient: SPEAKER_0: mid\nTherapist: two\nSPEAKER_9: stays"
        );
    }

    #[test]
    fn substitutes_whole_names_only() {
        let old = SpeakerMap::from([("SPEAKER_0".into(), "Mom".into())]);
        let new = SpeakerMap::from([("SPEAKER_0".into(), "Maria".into())]);
        assert_eq!(
            substitute_speaker_names("Mom spoke a Moment ago", &old, &new),
            "Maria spoke a Moment ago"
        );
    }
}
