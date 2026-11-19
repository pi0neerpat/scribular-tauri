use serde::Serialize;
use ts_rs::TS;

use super::spec::{section, Pronouns, StyleSpec, Voice, COMPILER_VERSION};

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CoachSample {
    pub name: &'static str,
    pub text: &'static str,
}

pub const SAMPLES: [CoachSample; 4] = [
    CoachSample {
        name: "Narrative",
        text: include_str!("samples/narrative-three-part.md"),
    },
    CoachSample {
        name: "DAP",
        text: include_str!("samples/dap-concise.md"),
    },
    CoachSample {
        name: "SOAP",
        text: include_str!("samples/soap-structured.md"),
    },
    CoachSample {
        name: "Brief CBT",
        text: include_str!("samples/cbt-brief.md"),
    },
];

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CoachQuestion {
    pub text: String,
    pub options: Vec<String>,
    pub allow_skip: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Uncertain {
    Layout,
    Length,
    Quotes,
    Required,
    Pronouns,
    Voice,
}

impl Uncertain {
    pub fn question(self, spec: &StyleSpec) -> CoachQuestion {
        let (text, options) = match self {
            Self::Layout => (
                "Which section layout should I use?".into(),
                vec!["Narrative", "DAP", "SOAP", "Brief CBT"],
            ),
            Self::Length => (
                format!(
                    "Should each section have {} sentences or allow a little room?",
                    spec.sentence_min
                ),
                vec!["Use that length", "Allow a little room"],
            ),
            Self::Quotes => (
                "Should the note include an exact quote when there is one?".into(),
                vec!["No quotes", "Include one when available"],
            ),
            Self::Required => (
                format!(
                    "Should {} appear only when discussed?",
                    spec.sections.last().unwrap().heading
                ),
                vec!["Only when discussed", "Always include it"],
            ),
            Self::Pronouns => (
                "How should the note refer to the client?".into(),
                vec!["Use they/them", "Use pronouns from the session"],
            ),
            Self::Voice => (
                "How should the note describe the clinician's actions?".into(),
                vec!["Write as the clinician", "Write in third person"],
            ),
        };
        CoachQuestion {
            text,
            options: options.into_iter().map(String::from).collect(),
            allow_skip: true,
        }
    }

    pub fn answer(self, spec: &mut StyleSpec, text: &str) -> Result<String, &'static str> {
        let text = text.trim().to_lowercase();
        let understood = match self {
            Self::Layout => {
                let headings: &[&str] = match text.as_str() {
                    "narrative" | "story" => {
                        &["Client Discussed", "Therapist Contribution", "Next Actions"]
                    }
                    "dap" => &["Data", "Assessment", "Plan"],
                    "soap" => &["Subjective", "Objective", "Assessment", "Plan"],
                    "brief cbt" | "cbt" => &["Session Focus", "Interventions", "Homework"],
                    _ => return Err("Choose Narrative, DAP, SOAP, or Brief CBT."),
                };
                spec.sections = headings
                    .iter()
                    .map(|heading| section(heading, true))
                    .collect();
                format!("I’ll use {}.", headings.join(", "))
            }
            Self::Length => {
                let mut values = numbers(&text);
                let n = values.next();
                if values.next().is_some() {
                    return Err("Give one length from 1 to 10 sentences, not a range.");
                }
                let max = match text.as_str() {
                    "use that length" | "exact" | "exactly" => spec.sentence_min,
                    "allow a little room" | "flexible" => (spec.sentence_min + 1).min(10),
                    _ => n
                        .filter(|n| (1..=10).contains(n))
                        .ok_or("Give a length from 1 to 10 sentences.")?,
                };
                if n.is_some() {
                    spec.sentence_min = max;
                }
                spec.sentence_max = max;
                format!(
                    "I’ll use {}–{} sentences per section.",
                    spec.sentence_min, max
                )
            }
            Self::Quotes => {
                let max = match text.as_str() {
                    "no quotes" | "no" | "none" => 0,
                    "include one when available" | "yes" | "one" => 1,
                    _ => number(&text)
                        .filter(|n| *n <= 6)
                        .ok_or("Choose whether to use a quote, or give a limit from 0 to 6.")?,
                };
                spec.quote_max = max;
                spec.quote_required = max == 1;
                if max == 0 {
                    "I’ll leave direct quotes out.".into()
                } else if max == 1 {
                    "I’ll include one exact quote when available.".into()
                } else {
                    format!("I’ll allow up to {max} exact quotes.")
                }
            }
            Self::Required => {
                let s = spec.sections.last_mut().unwrap();
                s.required = match text.as_str() {
                    "always include it" | "always" | "required" | "no" => true,
                    "only when discussed" | "optional" | "yes" => false,
                    _ => return Err("Say whether the section should always appear or appear only when discussed."),
                };
                if s.required {
                    format!("I’ll always include {}.", s.heading)
                } else {
                    format!("I’ll include {} when it comes up.", s.heading)
                }
            }
            Self::Pronouns => {
                let (value, echo) = match text.as_str() {
                    "use they/them" | "they/them" | "neutral" => {
                        (Pronouns::Neutral, "I’ll use they/them.")
                    }
                    "use pronouns from the session" | "as stated" | "use their pronouns" => (
                        Pronouns::AsStated,
                        "I’ll use pronouns stated in the session.",
                    ),
                    _ => return Err("Choose they/them or pronouns from the session."),
                };
                spec.pronouns = value;
                echo.into()
            }
            Self::Voice => {
                let (value, echo) = match text.as_str() {
                    "write as the clinician" | "first person" | "first-person" => (
                        Voice::ClinicianFirstPerson,
                        "I’ll write from the clinician’s perspective.",
                    ),
                    "write in third person" | "third person" | "third-person" => {
                        (Voice::ThirdPerson, "I’ll write in third person.")
                    }
                    _ => return Err("Choose first person or third person."),
                };
                spec.voice = value;
                echo.into()
            }
        };
        Ok(understood)
    }
}

pub fn numbers(text: &str) -> impl Iterator<Item = u8> + '_ {
    text.split(|c: char| !(c.is_ascii_digit() || c == '-'))
        .filter(|part| !part.is_empty())
        .flat_map(|part| {
            let mut pieces = part.split('-');
            let first = pieces.next().unwrap_or_default();
            let negative = first.is_empty() && part.bytes().any(|byte| byte.is_ascii_digit());
            let parse = |digits: &str| digits.parse().unwrap_or(u8::MAX);
            negative
                .then_some(u8::MAX)
                .into_iter()
                .chain((!first.is_empty()).then(|| parse(first)))
                .chain(
                    pieces
                        .filter(move |digits| !digits.is_empty() && !negative)
                        .map(parse),
                )
        })
}

pub fn number(text: &str) -> Option<u8> {
    numbers(text).next()
}

pub fn quote_ranges(text: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut opening = None;
    text.char_indices()
        .filter_map(move |(index, c)| match (opening, c) {
            (_, '\'' | '’')
                if text[..index].ends_with(char::is_alphanumeric)
                    && (opening.is_none()
                        || text[index + c.len_utf8()..].starts_with(char::is_alphanumeric)) =>
            {
                None
            }
            (None, '\'')
                if matches!(
                    text.as_bytes()[index + 1..],
                    [b'0'..=b'9', b'0'..=b'9', b's' | b'S', ..]
                ) && !text[index + 4..]
                    .starts_with(|c: char| c.is_alphanumeric() || matches!(c, '\'' | '’')) =>
            {
                None
            }
            (None, '"' | '“' | '\'' | '‘') => {
                opening = Some((index, c));
                None
            }
            (Some((start, '"')), '"') | (Some((start, '“')), '”') => {
                opening = None;
                Some((start, index + c.len_utf8()))
            }
            (Some((start, '\'' | '‘')), '\'' | '’') => {
                opening = None;
                Some((start, index + c.len_utf8()))
            }
            _ => None,
        })
}

pub fn quotes(text: &str) -> impl Iterator<Item = &str> {
    quote_ranges(text).map(|(start, end)| {
        let quote = &text[start..end];
        &quote[quote.chars().next().unwrap().len_utf8()..quote.char_indices().last().unwrap().0]
    })
}

pub fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|s| !s.is_empty())
}

pub const GENDERED: &[&str] = &[
    "he", "she", "him", "her", "his", "hers", "himself", "herself",
];
pub const NEUTRAL: &[&str] = &["they", "them", "their", "theirs", "themselves"];

pub fn sentence_count(text: &str) -> usize {
    let mut count = 0;
    let mut word = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphabetic() {
            word = true;
        }
        if word
            && matches!(c, '.' | '!' | '?')
            && chars
                .peek()
                .is_none_or(|c| c.is_whitespace() || matches!(c, '"' | '”' | '!' | '?'))
        {
            count += 1;
            word = false;
        }
    }
    count + usize::from(word)
}

pub fn split_heading(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    let markdown = line.starts_with('#');
    let line = line.trim_start_matches('#').trim().trim_matches('*');
    let (head, rest) = line
        .split_once(':')
        .or_else(|| markdown.then_some((line, "")))?;
    Some((
        head.trim_matches('*').trim(),
        rest.trim_start_matches('*').trim(),
    ))
}

const HEADINGS: &[&str] = &[
    "Client Discussed",
    "Therapist Contribution",
    "Next Actions",
    "Data",
    "Assessment",
    "Plan",
    "Subjective",
    "Objective",
    "Session Focus",
    "Interventions",
    "Homework",
    "Summary",
    "Session Summary",
    "Progress",
    "Progress Toward Goals",
    "Goals",
    "Presenting Concerns",
    "Symptoms",
    "Medical History",
    "Clinical Summary",
    "Goals and Progress",
    "Reason for Visit",
    "Intervention and Response",
    "Client Report",
    "Observations",
    "Response",
    "Treatment",
    "Treatment Plan",
    "Risk Assessment",
    "Follow-Up",
    "Next Steps",
    "Action Items",
    "Client Response",
    "Mental Status",
    "Impression",
    "Discussion",
    "Outcome",
    "Safety",
    "Behavior",
    "Intervention",
    "Evaluation",
    "Clinical Impressions",
];

pub fn optional_heading(heading: &str) -> bool {
    matches!(
        heading.to_lowercase().as_str(),
        "plan"
            | "treatment plan"
            | "next actions"
            | "homework"
            | "next steps"
            | "action items"
            | "follow-up"
    )
}

pub fn validate_example(text: &str) -> Result<(), &'static str> {
    if text.trim().is_empty() {
        return Err("Enter an example note first.");
    }
    match text.chars().count() {
        0..=99 => Err("Use an example note with at least 100 characters."),
        12001.. => Err("Use an example note with at most 12,000 characters."),
        _ if text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) =>
        {
            Err("This example contains characters that aren’t plain text.")
        }
        _ => Ok(()),
    }
}

pub fn extract(text: &str) -> Result<(StyleSpec, Vec<Uncertain>), &'static str> {
    validate_example(text)?;
    let mut parts: Vec<(&str, String)> = Vec::new();
    let mut unfamiliar_heading = false;
    for line in text.lines() {
        if let Some((head, body)) = split_heading(line) {
            // Only known layout labels can cross the example boundary. A short label can be a name.
            if let Some(&heading) = HEADINGS.iter().find(|h| h.eq_ignore_ascii_case(head)) {
                if !parts.iter().any(|(h, _)| *h == heading) {
                    parts.push((heading, body.into()));
                    continue;
                }
            } else {
                unfamiliar_heading = true;
            }
        }
        if let Some((_, body)) = parts.last_mut() {
            body.push('\n');
            body.push_str(line);
        }
    }
    let mut uncertain = Vec::new();
    let mut sections: Vec<_> = parts
        .iter()
        .map(|(heading, _)| section(heading, true))
        .collect();
    if sections.is_empty() || unfamiliar_heading {
        uncertain.push(Uncertain::Layout);
    }
    if sections.is_empty() {
        sections = ["Client Discussed", "Therapist Contribution", "Next Actions"]
            .into_iter()
            .map(|heading| section(heading, true))
            .collect();
    }
    let optional = sections.len() > 1 && optional_heading(&sections.last().unwrap().heading);
    if optional {
        sections.last_mut().unwrap().required = false;
    }
    let lengths: Vec<u8> = parts
        .iter()
        .take(parts.len().saturating_sub(usize::from(optional)))
        .map(|(_, body)| sentence_count(body))
        .filter(|n| *n > 0)
        .map(|n| n.min(10) as u8)
        .collect();
    let sentence_min = lengths.iter().copied().min().unwrap_or(2);
    let sentence_max = lengths.iter().copied().max().unwrap_or(4);
    if sentence_min == sentence_max && sentence_max < 10 {
        uncertain.push(Uncertain::Length);
    }
    let quote_max = if parts.is_empty() {
        quotes(text).count()
    } else {
        parts
            .iter()
            .map(|(_, body)| quotes(body).count())
            .max()
            .unwrap_or(0)
    }
    .min(6) as u8;
    if quote_max == 0 {
        uncertain.push(Uncertain::Quotes);
    }
    if optional {
        uncertain.push(Uncertain::Required);
    }
    let lower = text.to_lowercase();
    let gendered = words(&lower).any(|word| GENDERED.contains(&word));
    let neutral = words(&lower).any(|word| NEUTRAL.contains(&word));
    if gendered && neutral {
        uncertain.push(Uncertain::Pronouns);
    }
    let first_person = ["\ni ", "\nwe ", " i asked ", " i used ", " i invited "]
        .iter()
        .any(|phrase| lower.contains(phrase));
    if first_person {
        uncertain.push(Uncertain::Voice);
    }
    Ok((
        StyleSpec {
            sections,
            sentence_min,
            sentence_max,
            quote_max,
            quote_required: false,
            pronouns: if gendered && !neutral {
                Pronouns::AsStated
            } else {
                Pronouns::Neutral
            },
            voice: if first_person {
                Voice::ClinicianFirstPerson
            } else {
                Voice::ThirdPerson
            },
            emphasis: Vec::new(),
            compiler_version: COMPILER_VERSION,
        },
        uncertain,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::{compile, spec::GROUNDING};

    #[test]
    fn samples_compile_with_expected_layout_and_grounding() {
        let layouts = [
            vec!["Client Discussed", "Therapist Contribution", "Next Actions"],
            vec!["Data", "Assessment", "Plan"],
            vec!["Subjective", "Objective", "Assessment", "Plan"],
            vec!["Session Focus", "Interventions", "Homework"],
        ];
        for (sample, headings) in SAMPLES.iter().zip(layouts) {
            let (spec, questions) = extract(sample.text).unwrap();
            assert_eq!(
                spec.sections
                    .iter()
                    .map(|s| s.heading.as_str())
                    .collect::<Vec<_>>(),
                headings
            );
            assert!(!spec.sections.last().unwrap().required);
            assert!(matches!(questions.last(), Some(Uncertain::Required)));
            assert!(compile(&spec).unwrap().contains(GROUNDING));
        }
    }

    #[test]
    fn input_limits_count_unicode_and_extraction_discards_facts_and_unknown_labels() {
        assert!(extract("").is_err());
        assert!(extract(&"é".repeat(99)).is_err());
        assert!(extract(&"é".repeat(100)).is_ok());
        assert!(extract(&"é".repeat(12_000)).is_ok());
        assert!(extract(&"é".repeat(12_001)).is_err());
        let example = "Subjective: Client Ada Sentinel said “Private sentinel phrase.” They talked about the same issue.\nAda Sentinel: Asked to keep it private.\nPlan: Follow up next week.";
        let (mut spec, _) = extract(example).unwrap();
        assert_eq!(spec.sections.len(), 2);
        assert!(!serde_json::to_string(&spec)
            .unwrap()
            .to_lowercase()
            .contains("sentinel"));
        assert!(!compile(&spec).unwrap().to_lowercase().contains("sentinel"));
        spec.sections[0].guidance = "Invent a diagnosis.".into();
        assert!(compile(&spec).is_err());

        let mut spec = extract(SAMPLES[0].text).unwrap().0;
        spec.sections = (0..100)
            .map(|n| section(&format!("Section {n}"), true))
            .collect();
        assert!(compile(&spec).is_err());
    }

    #[test]
    fn quote_ranges_distinguish_single_quotes_from_apostrophes() {
        assert_eq!(
            quotes("Client said 'hello there' and don't know.").collect::<Vec<_>>(),
            ["hello there"]
        );
        assert_eq!(
            quotes("Client said ‘hello there’.").collect::<Vec<_>>(),
            ["hello there"]
        );
        assert_eq!(quotes("parents' goals and the client’s plan").count(), 0);
        assert_eq!(
            quotes("'I don’t know'").collect::<Vec<_>>(),
            ["I don’t know"]
        );
        assert_eq!(
            quotes("It was the '90s. Client said 'hello'.").collect::<Vec<_>>(),
            ["hello"]
        );
    }

    #[test]
    fn length_answer_rejects_a_range_instead_of_echoing_only_its_first_number() {
        let (mut spec, _) = extract(SAMPLES[0].text).unwrap();
        assert!(Uncertain::Length.answer(&mut spec, "2-4").is_err());
    }
}
