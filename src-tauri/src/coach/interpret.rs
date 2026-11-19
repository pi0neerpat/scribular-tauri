use super::{
    extract::{number, numbers, words, CoachQuestion},
    spec::{compile, section, Emphasis, StyleSection, StyleSpec},
};

pub const SUGGESTIONS: &[&str] = &[
    "Make it shorter",
    "One sentence per section",
    "Include an exact quote when available",
    "Add a section",
    "Remove a section",
    "Make a section optional",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Patch {
    Length {
        before: (u8, u8),
        after: (u8, u8),
    },
    Quotes {
        before: (u8, bool),
        after: (u8, bool),
    },
    Emphasis {
        before: Vec<Emphasis>,
        after: Vec<Emphasis>,
    },
    Section {
        before: Option<StyleSection>,
        after: Option<StyleSection>,
    },
}

impl Patch {
    pub fn apply(&self, spec: &StyleSpec) -> Result<(StyleSpec, String), &'static str> {
        let mut next = spec.clone();
        let description = match self {
            Self::Length { before, after } if *before == (spec.sentence_min, spec.sentence_max) => {
                (next.sentence_min, next.sentence_max) = *after;
                format!("Use {}–{} sentences per section.", after.0, after.1)
            }
            Self::Quotes { before, after } if *before == (spec.quote_max, spec.quote_required) => {
                (next.quote_max, next.quote_required) = *after;
                match after {
                    (0, _) => "Leave out direct quotes.".into(),
                    (n, false) => format!("Allow up to {n} exact quotes."),
                    (n, true) => {
                        format!("Include an exact quote when available, with a limit of {n}.")
                    }
                }
            }
            Self::Emphasis { before, after } if *before == spec.emphasis => {
                next.emphasis.clone_from(after);
                "Change what the note emphasizes.".into()
            }
            Self::Section {
                before: None,
                after: Some(new),
            } => {
                next.sections.push(new.clone());
                format!("Add {}.", new.heading)
            }
            Self::Section {
                before: Some(old),
                after,
            } => {
                let index = next
                    .sections
                    .iter()
                    .position(|s| s == old)
                    .ok_or("This style already changed. Ask again from the current version.")?;
                if let Some(new) = after {
                    next.sections[index] = new.clone();
                    if old.heading != new.heading {
                        format!("Rename {} to {}.", old.heading, new.heading)
                    } else if new.required {
                        format!("Always include {}.", new.heading)
                    } else {
                        format!("Include {} only when discussed.", new.heading)
                    }
                } else {
                    next.sections.remove(index);
                    format!("Remove {}.", old.heading)
                }
            }
            _ => return Err("This style already changed. Ask again from the current version."),
        };
        if &next == spec {
            return Err("The style already does that. Ask for another change.");
        }
        next.compiler_version = super::spec::COMPILER_VERSION;
        compile(&next)?;
        Ok((next, description))
    }
}

pub enum Interpretation {
    Patch(Patch),
    Question(CoachQuestion),
    Unsupported,
}

fn question(text: &str, options: Vec<String>) -> Interpretation {
    Interpretation::Question(CoachQuestion {
        text: text.into(),
        options,
        allow_skip: false,
    })
}

enum SectionEdit {
    Add,
    Remove,
    Rename,
    Optional,
    Required,
}

impl SectionEdit {
    fn request(&self, heading: &str) -> String {
        match self {
            Self::Add => format!("Add section {heading}"),
            Self::Remove => format!("Remove section {heading}"),
            Self::Rename => format!("Rename section {heading} to"),
            Self::Optional => format!("Make {heading} optional"),
            Self::Required => format!("Make {heading} required"),
        }
    }

    fn choose(&self, spec: &StyleSpec) -> Interpretation {
        if matches!(self, Self::Add) {
            return question(
                "What should I call the new section? Type “Add section” followed by its heading.",
                ["Goals", "Progress", "Next Steps"]
                    .iter()
                    .filter(|h| {
                        !spec
                            .sections
                            .iter()
                            .any(|s| s.heading.eq_ignore_ascii_case(h))
                    })
                    .map(|h| self.request(h))
                    .collect(),
            );
        }
        question(
            "Which section do you mean?",
            spec.sections
                .iter()
                .map(|s| self.request(&s.heading))
                .collect(),
        )
    }

    fn interpret(
        &self,
        spec: &StyleSpec,
        heading: &str,
        new_heading: Option<&str>,
    ) -> Interpretation {
        let heading = heading
            .trim()
            .trim_matches(['"', '“', '”', '\'', '’', '.', ':']);
        if heading.is_empty()
            || matches!(
                heading.to_lowercase().as_str(),
                "a section" | "the section" | "section" | "a new section"
            )
        {
            return self.choose(spec);
        }
        if matches!(self, Self::Add) {
            return Interpretation::Patch(Patch::Section {
                before: None,
                after: Some(section(heading, true)),
            });
        }
        let Some(old) = spec
            .sections
            .iter()
            .find(|s| s.heading.to_lowercase() == heading.to_lowercase())
        else {
            return self.choose(spec);
        };
        let mut new = old.clone();
        let after = match self {
            Self::Add => unreachable!(),
            Self::Remove => None,
            Self::Optional => {
                new.required = false;
                Some(new)
            }
            Self::Required => {
                new.required = true;
                Some(new)
            }
            Self::Rename => {
                let Some(name) = new_heading.filter(|s| !s.is_empty()) else {
                    return question(
                        &format!("What should I call {}? Type “Rename section {} to” followed by the new heading.", old.heading, old.heading),
                        Vec::new(),
                    );
                };
                new.heading = name.trim_matches(['"', '“', '”', '.', ':']).into();
                Some(new)
            }
        };
        Interpretation::Patch(Patch::Section {
            before: Some(old.clone()),
            after,
        })
    }
}

pub fn interpret(spec: &StyleSpec, request: &str) -> Interpretation {
    let request = request.trim().trim_end_matches(['.', '!', '?']);
    let request = if request.to_ascii_lowercase().starts_with("please ") {
        &request[7..]
    } else {
        request
    };
    let lower = request.to_ascii_lowercase();
    let length = |after| {
        Interpretation::Patch(Patch::Length {
            before: (spec.sentence_min, spec.sentence_max),
            after,
        })
    };
    let quotes = |after| {
        Interpretation::Patch(Patch::Quotes {
            before: (spec.quote_max, spec.quote_required),
            after,
        })
    };

    // Resolve exact heading matches before trying general requests.
    if let Some(tail) = lower.strip_prefix("add ") {
        if matches!(tail, "a section" | "a new section" | "section") {
            return SectionEdit::Add.choose(spec);
        }
        if let Some(name) = [
            "a section called ",
            "a new section called ",
            "section called ",
            "section ",
        ]
        .iter()
        .find_map(|prefix| tail.strip_prefix(prefix))
        {
            let original = &request[request.len() - name.len()..];
            return SectionEdit::Add.interpret(spec, original, None);
        }
    }
    for (prefix, edit) in [
        ("remove ", SectionEdit::Remove),
        ("rename ", SectionEdit::Rename),
    ] {
        if let Some(tail) = lower.strip_prefix(prefix) {
            if tail.contains("quote") && !tail.starts_with("section ") {
                break;
            }
            let tail = tail
                .strip_prefix("a section called ")
                .or_else(|| tail.strip_prefix("section "))
                .or_else(|| tail.strip_prefix("the section "))
                .unwrap_or(tail);
            let original = &request[request.len() - tail.len()..];
            let (name, replacement) = match edit {
                SectionEdit::Rename => spec
                    .sections
                    .iter()
                    .filter_map(|section| {
                        let offset = section.heading.len();
                        let name = original.get(..offset)?;
                        if !name.eq_ignore_ascii_case(&section.heading) {
                            return None;
                        }
                        let replacement = match &tail[offset..] {
                            "" | " to" => None,
                            suffix if suffix.starts_with(" to ") => {
                                Some(original[offset + " to ".len()..].trim())
                            }
                            _ => return None,
                        };
                        Some((name, replacement))
                    })
                    .max_by_key(|(name, replacement)| (replacement.is_some(), name.len()))
                    .unwrap_or((original, None)),
                _ => (original, None),
            };
            return edit.interpret(spec, name, replacement);
        }
    }
    if let Some(tail) = lower.strip_prefix("make ") {
        for (suffix, edit) in [
            (" optional", SectionEdit::Optional),
            (" required", SectionEdit::Required),
        ] {
            if let Some(name) = tail.strip_suffix(suffix) {
                return edit.interpret(spec, name.trim_start_matches("section "), None);
            }
        }
    }
    if lower.contains("shorter") || lower == "more concise" {
        let max = spec.sentence_max.saturating_sub(1).max(1);
        return length((spec.sentence_min.min(max), max));
    }
    if lower.contains("longer") {
        return length((spec.sentence_min, spec.sentence_max.saturating_add(1)));
    }
    if lower.contains("sentence") {
        if lower.contains("one sentence") || lower.contains("single sentence") {
            return length((1, 1));
        }
        let mut bounds = numbers(&lower);
        if let Some(min) = bounds.next() {
            return length((min, bounds.next().unwrap_or(min)));
        }
        return question(
            "How long should each section be?",
            [1, 2, 3]
                .map(|n| format!("{n} sentences per section"))
                .to_vec(),
        );
    }
    if lower.contains("quote") {
        if lower.contains("no quote")
            || lower.contains("without quotes")
            || lower == "remove quotes"
        {
            return quotes((0, false));
        }
        if lower.contains("optional") {
            return quotes((spec.quote_max.max(1), false));
        }
        if lower.contains("when available")
            || lower.contains("require")
            || lower.contains("at least one")
        {
            return quotes((number(&lower).unwrap_or(1), true));
        }
        if let Some(n) = number(&lower) {
            return quotes((n, false));
        }
        return question(
            "Would you like to leave quotes out or include one when available?",
            vec![
                "No quotes".into(),
                "Include an exact quote when available".into(),
            ],
        );
    }
    if lower.starts_with("emphasize ")
        || lower.starts_with("focus on ")
        || lower.starts_with("less emphasis on ")
    {
        for (term, emphasis) in [
            ("client", Emphasis::ClientExperience),
            ("intervention", Emphasis::Interventions),
            ("next step", Emphasis::NextSteps),
        ] {
            if lower.contains(term) {
                let mut after = spec.emphasis.clone();
                after.retain(|e| *e != emphasis);
                if !lower.starts_with("less") {
                    after.push(emphasis);
                }
                return Interpretation::Patch(Patch::Emphasis {
                    before: spec.emphasis.clone(),
                    after,
                });
            }
        }
        return question(
            "What should the note emphasize?",
            [
                "Emphasize client experience",
                "Emphasize interventions",
                "Emphasize next steps",
            ]
            .map(String::from)
            .to_vec(),
        );
    }
    if words(&lower).all(|w| {
        [
            "make",
            "it",
            "a",
            "bit",
            "different",
            "better",
            "change",
            "something",
            "please",
        ]
        .contains(&w)
    }) {
        return question(
            "What would you like to change?",
            SUGGESTIONS.iter().map(|s| (*s).into()).collect(),
        );
    }
    Interpretation::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::extract::{extract, SAMPLES};

    #[test]
    fn section_edits_resolve_exactly_and_refuse_stale_or_invalid_changes() {
        let (spec, _) = extract(SAMPLES[2].text).unwrap();
        let Interpretation::Patch(patch) = interpret(&spec, "Rename section Plan to Suivi") else {
            panic!()
        };
        let (next, _) = patch.apply(&spec).unwrap();
        assert_eq!(next.sections[3].heading, "Suivi");
        assert!(patch.apply(&next).is_err());
        assert!(
            matches!(interpret(&spec, "Remove Unknown"), Interpretation::Question(q) if q.options.contains(&"Remove section Plan".into()))
        );
        assert!(matches!(
            interpret(&spec, "make it better"),
            Interpretation::Question(_)
        ));
        for request in ["Translate it", "Add a personal detail to the note"] {
            assert!(matches!(
                interpret(&spec, request),
                Interpretation::Unsupported
            ));
        }
        for request in [
            "11 sentences per section",
            "Use -1 quotes",
            "Use 7 quotes",
            "Add section Plan",
            "Add section \"fake\nheading\"",
        ] {
            let Interpretation::Patch(patch) = interpret(&spec, request) else {
                panic!("{request}")
            };
            assert!(patch.apply(&spec).is_err(), "{request}");
        }
    }

    #[test]
    fn rename_matches_headings_that_contain_to() {
        let (mut spec, _) = extract(SAMPLES[0].text).unwrap();
        spec.sections[0].heading = "Going to School".into();
        spec.sections
            .push(crate::coach::spec::section("Going", false));
        let Interpretation::Patch(patch) = interpret(&spec, "Rename Going to School to Education")
        else {
            panic!()
        };
        let (next, _) = patch.apply(&spec).unwrap();
        assert_eq!(next.sections[0].heading, "Education");
    }

    #[test]
    fn rename_requires_a_heading_boundary_and_prefers_a_complete_instruction() {
        let (mut spec, _) = extract(SAMPLES[2].text).unwrap();
        assert!(matches!(
            interpret(&spec, "Rename Planning to Notes"),
            Interpretation::Question(q) if q.text == "Which section do you mean?"
        ));
        assert!(matches!(
            interpret(&spec, "Rename section Plan to"),
            Interpretation::Question(q) if q.text.starts_with("What should I call Plan?")
        ));
        spec.sections.push(section("Plan to Notes", false));
        let Interpretation::Patch(Patch::Section {
            before: Some(old),
            after: Some(new),
        }) = interpret(&spec, "Rename Plan TO Notes")
        else {
            panic!()
        };
        assert_eq!(
            (old.heading.as_str(), new.heading.as_str()),
            ("Plan", "Notes")
        );
    }
}
