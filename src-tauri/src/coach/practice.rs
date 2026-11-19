use serde::Serialize;
use ts_rs::TS;

use super::{
    extract::{
        optional_heading, quote_ranges, quotes, sentence_count, split_heading, words, GENDERED,
    },
    spec::{Pronouns, StyleSpec},
};

pub const TRANSCRIPT: &str = include_str!("samples/practice.txt");
pub const MAX_ATTEMPTS: u8 = 3;
// shortcut: very long practice styles may truncate; raise this after measuring native decode time.
pub const NOTE_TOKENS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Check {
    SectionsPresent,
    NoRepeats,
    OptionalOmission,
    Length,
    ExactQuotes,
    QuoteAttribution,
    NoNames,
    Pronouns,
    NoPreamble,
}

impl Check {
    pub fn correction(self) -> &'static str {
        match self {
            Self::SectionsPresent => "Include every required heading, spelled as specified, and give it a body.",
            Self::NoRepeats => "Use each heading at most once.",
            Self::OptionalOmission => "Omit optional sections that have no directly relevant information, including plans without agreed next steps.",
            Self::Length => "Keep the body of every section within the specified sentence range.",
            Self::ExactQuotes => "Follow the quote limit and requirement. Copy exact client words only.",
            Self::QuoteAttribution => "Use exact client quotes only and say in the same sentence that the client said them.",
            Self::NoNames => "Replace personal names with their roles or relationships.",
            Self::Pronouns => "Use only the specified pronouns, or refer to people by role.",
            Self::NoPreamble => "Begin with a specified heading and return only the note.",
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PracticeNote {
    pub text: String,
    pub passed: u8,
    pub total: u8,
    pub failed: Vec<Check>,
}

pub struct PracticeSource<'a> {
    pub transcript: &'a str,
    pub names: &'a [&'a str],
    pub next_steps: bool,
}

pub const PRACTICE_SOURCE: PracticeSource<'static> = PracticeSource {
    transcript: TRANSCRIPT,
    names: &["Morgan"],
    next_steps: false,
};

pub fn corrections(base: &str, failed: &[Check]) -> String {
    if failed.is_empty() {
        return base.into();
    }
    let mut instructions = format!("{base}\n\nPay particular attention to these requirements:");
    for check in failed {
        instructions.push_str("\n- ");
        instructions.push_str(check.correction());
    }
    instructions
}

pub fn check(spec: &StyleSpec, source: &PracticeSource<'_>, text: String) -> PracticeNote {
    let mut bodies = vec![Vec::<String>::new(); spec.sections.len()];
    let mut active: Option<usize> = None;
    let mut preamble = false;
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some((i, rest)) = split_heading(line).and_then(|(head, rest)| {
            let head = head.to_lowercase();
            spec.sections
                .iter()
                .position(|s| s.heading.to_lowercase() == head)
                .map(|i| (i, rest))
        }) {
            active = Some(i);
            bodies[i].push(rest.into());
        } else if let Some(i) = active {
            let body = bodies[i].last_mut().unwrap();
            body.push(' ');
            body.push_str(line);
        } else {
            preamble = true;
        }
    }
    let sections = spec.sections.iter().zip(&bodies);
    let present = |body: &Vec<String>| body.iter().any(|b| !b.trim().is_empty());
    let all_required = sections
        .clone()
        .all(|(s, body)| !s.required || present(body));
    let no_repeats = bodies.iter().all(|b| b.len() <= 1);
    let optional = sections
        .clone()
        .filter(|(s, _)| !s.required)
        .all(|(s, body)| {
            if optional_heading(&s.heading) && !source.next_steps {
                return body.is_empty();
            }
            body.is_empty()
                || body.iter().all(|b| {
                    let lower = b.trim().to_lowercase();
                    words(&lower).count() >= 3
                        && ![
                            "not discussed",
                            "not mentioned",
                            "not provided",
                            "none stated",
                            "no information",
                            "n/a",
                        ]
                        .iter()
                        .any(|placeholder| lower.contains(placeholder))
                })
        });
    let length = sections.clone().all(|(_, body)| {
        body.iter().all(|body| {
            (usize::from(spec.sentence_min)..=usize::from(spec.sentence_max))
                .contains(&sentence_count(body))
        })
    });

    // The fictional dialogue starts with a clinician turn. Labels come from the dialogue,
    // so re-labeling either speaker leaves the role and attribution checks equivalent.
    let turns: Vec<_> = source
        .transcript
        .lines()
        .filter_map(|line| line.split_once(':'))
        .collect();
    let clinician = turns.first().map(|(label, _)| *label).unwrap_or("");
    let clients: Vec<_> = turns
        .iter()
        .filter(|(label, _)| *label != clinician)
        .collect();
    let available = clients
        .iter()
        .any(|(_, said)| quotes(said).any(|q| !q.is_empty()));
    let note_quotes: Vec<_> = quotes(&text).collect();
    let exact = note_quotes.len() <= usize::from(spec.quote_max)
        && (!spec.quote_required || !available || !note_quotes.is_empty())
        && note_quotes
            .iter()
            .all(|q| !q.is_empty() && clients.iter().any(|(_, said)| contains_fragment(said, q)));
    let attribution = quote_ranges(&text)
        .zip(&note_quotes)
        .all(|((start, end), q)| {
            let before = text[..start]
                .rsplit(['\n', '.', '!', '?'])
                .next()
                .unwrap_or("");
            let after = text[end..]
                .split(['\n', '.', '!', '?'])
                .next()
                .unwrap_or("");
            let credit = format!("{before} {after}").to_lowercase();
            let has_client = words(&credit).any(|w| w == "client")
                || clients.iter().any(|(label, _)| has_label(&credit, label));
            let has_clinician = words(&credit).any(|w| matches!(w, "clinician" | "therapist"))
                || has_label(&credit, clinician);
            has_client
                && !has_clinician
                && clients.iter().any(|(_, said)| contains_fragment(said, q))
        });
    let names = !source
        .names
        .iter()
        .any(|name| has_label(&text.to_lowercase(), name));
    let mut unquoted = String::new();
    let mut tail = 0;
    for (start, end) in quote_ranges(&text) {
        unquoted.push_str(&text[tail..start]);
        unquoted.push(' ');
        tail = end;
    }
    unquoted.push_str(&text[tail..]);
    let lower = unquoted.to_lowercase();
    let pronouns = words(&lower).filter(|w| GENDERED.contains(w)).all(|w| {
        spec.pronouns == Pronouns::AsStated
            && clients.iter().any(|(_, said)| {
                // Mere use of a pronoun in the dialogue does not declare the client's pronouns.
                let said = said.to_lowercase();
                (said.contains("my pronouns") || said.contains("my pronoun"))
                    && words(&said).any(|p| p == w)
            })
    });
    let checks = [
        (Check::SectionsPresent, all_required),
        (Check::NoRepeats, no_repeats),
        (Check::OptionalOmission, optional),
        (Check::Length, length),
        (Check::ExactQuotes, exact),
        (Check::QuoteAttribution, attribution),
        (Check::NoNames, names),
        (Check::Pronouns, pronouns),
        (Check::NoPreamble, !preamble && active.is_some()),
    ];
    let failed: Vec<_> = checks
        .iter()
        .filter_map(|(id, ok)| (!ok).then_some(*id))
        .collect();
    PracticeNote {
        text,
        total: checks.len() as u8,
        passed: (checks.len() - failed.len()) as u8,
        failed,
    }
}

fn has_label(text: &str, label: &str) -> bool {
    if label.is_empty() {
        return false;
    }
    contains_fragment(text, &label.to_lowercase())
}

fn contains_fragment(text: &str, fragment: &str) -> bool {
    text.match_indices(fragment).any(|(i, name)| {
        !text[..i]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
            && !text[i + name.len()..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::extract::{extract, SAMPLES};

    #[test]
    fn checks_respect_relabelled_speakers_and_literal_quotes() {
        let (mut spec, _) = extract(SAMPLES[1].text).unwrap();
        spec.sentence_min = 1;
        spec.quote_required = true;
        let note = "Data:\nThe client said, “I can pause and still get things done.”\nAssessment:\nThe client noticed a benefit.".to_string();
        let first = check(&spec, &PRACTICE_SOURCE, note.clone());
        assert_eq!(first.total, 9);
        assert_eq!(first.failed, Vec::<Check>::new());
        let relabelled = TRANSCRIPT
            .replace("Therapist:", "Speaker A:")
            .replace("Client:", "Speaker B:");
        let source = PracticeSource {
            transcript: &relabelled,
            ..PRACTICE_SOURCE
        };
        assert_eq!(check(&spec, &source, note.clone()).failed, first.failed);
        let wrong = check(
            &spec,
            &source,
            note.replace("The client said", "The therapist said")
                .replace("pause", "rest"),
        );
        assert!(wrong.failed.contains(&Check::ExactQuotes));
        assert!(wrong.failed.contains(&Check::QuoteAttribution));
    }
}
