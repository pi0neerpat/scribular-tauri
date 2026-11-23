use std::{
    ops::Range,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use ts_rs::TS;

use super::commands;
use crate::{
    jobs::{JobQueue, Recorder, SessionCoachJob},
    store::{SessionStatus, Store},
};

pub const RESULT_EVENT: &str = "session-coach://result";
pub const STOPPED_EVENT: &str = "session-coach://stopped";
const FAILED: &str = "I couldn’t revise that note. Try again.";
const UNSUPPORTED: &str = "I can change length, quotes and sections. Ask for one of those changes.";
const END_NOTE: &str = "<<END_OF_NOTE>>";

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionCoachOutcome {
    Candidate {
        #[serde(rename = "candidateId")]
        candidate_id: String,
        note: String,
    },
    Reply {
        message: &'static str,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionCoachResult {
    pub job_id: String,
    pub session_id: String,
    pub outcome: SessionCoachOutcome,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionCoachStopped {
    pub job_id: String,
    pub session_id: String,
}

#[derive(Default)]
pub struct SessionCoach(Mutex<State>);

#[derive(Default)]
struct State {
    active: Option<(String, String, Arc<AtomicBool>)>,
    candidate: Option<Candidate>,
    previous_candidate: Option<Candidate>,
}

struct Candidate {
    session_id: String,
    id: String,
    job_id: String,
    note: String,
    base: String,
    prompt_id: Option<String>,
    model_id: Option<String>,
}

impl State {
    fn restore_after_stop(&mut self, job_id: &str) {
        if self
            .candidate
            .as_ref()
            .is_some_and(|candidate| candidate.job_id == job_id)
        {
            self.candidate = self.previous_candidate.take();
        }
    }
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect()
}

fn spans(text: &str) -> impl Iterator<Item = Vec<String>> + '_ {
    text.split(['\n', '.', '!', '?'])
        .map(words)
        .filter(|span| !span.is_empty())
}

fn grounded(note: &str, transcript: &str, output: &str) -> bool {
    // shortcut: safe paraphrases may be rejected, upgrade when claim-level validation exists.
    let source: Vec<_> = spans(note).chain(spans(transcript)).collect();
    !words(output).is_empty()
        && spans(output).all(|claim| source.contains(&claim))
        && super::extract::quotes(output)
            .all(|quote| note.contains(quote) || transcript.contains(quote))
}

fn complete_note(text: &str) -> Option<&str> {
    text.trim_end().strip_suffix(END_NOTE).map(str::trim_end)
}

fn section_body(note: &str, heading: &str) -> Option<Range<usize>> {
    // shortcut: blank lines delimit sections, upgrade when notes carry structured section boundaries.
    let mut offset = 0;
    let mut sections = note.split("\n\n").filter_map(|section| {
        let start = offset;
        offset += section.len() + 2;
        let (title, body) = section.split_once('\n')?;
        let (name, inline) = super::extract::split_heading(title)?;
        (inline.is_empty() && name.to_lowercase() == heading).then(|| {
            let body = body.trim_end();
            let body_start = start + title.len() + 1;
            body_start..body_start + body.len()
        })
    });
    let body = sections.next()?;
    (sections.next().is_none()
        && note[body.clone()]
            .lines()
            .all(|line| super::extract::split_heading(line).is_none()))
    .then_some(body)
}

enum SectionEdit {
    Bullets(String),
    Shorten(String),
}

impl SectionEdit {
    fn from_request(draft: &str, request: &str) -> Option<Self> {
        let request = request.to_lowercase();
        let request = request.trim().trim_end_matches(['.', '!']);
        let request = request.strip_prefix("also ").unwrap_or(request);
        let (heading, bullets) = match request.strip_prefix("use bullet points in ") {
            Some(heading) => (heading, true),
            None => (request.strip_prefix("shorten ")?, false),
        };
        let heading = heading.strip_prefix("only ").unwrap_or(heading);
        let heading = heading.strip_prefix("the ").unwrap_or(heading);
        let heading = heading
            .strip_suffix(" by removing one sentence")
            .unwrap_or(heading);
        let heading = heading.strip_suffix(" section").unwrap_or(heading);
        if bullets {
            return Some(Self::Bullets(heading.into()));
        }
        draft
            .lines()
            .any(|line| {
                super::extract::split_heading(line)
                    .is_some_and(|(name, _)| name.to_lowercase() == heading)
            })
            .then(|| Self::Shorten(heading.into()))
    }

    fn heading(&self) -> &str {
        match self {
            Self::Bullets(heading) | Self::Shorten(heading) => heading,
        }
    }
}

fn revise_section(draft: &str, edit: &SectionEdit, output: &str) -> Option<String> {
    let original = section_body(draft, edit.heading())?;
    let proposed = section_body(output, edit.heading())?;
    let replacement = &output[proposed];
    let body = &draft[original.clone()];
    match edit {
        SectionEdit::Bullets(_) => {
            let mut bullets = Vec::new();
            for line in replacement.lines() {
                let line = line.trim();
                let bullet = line
                    .strip_prefix("* ")
                    .or_else(|| line.strip_prefix("- "))
                    .or_else(|| line.strip_prefix("• "))?;
                bullets.push(bullet);
            }
            if bullets.is_empty()
                || bullets
                    .join(" ")
                    .split_whitespace()
                    .ne(body.split_whitespace())
            {
                return None;
            }
        }
        SectionEdit::Shorten(_)
            if replacement.len() >= body.len() || !grounded(body, "", replacement) =>
        {
            return None;
        }
        SectionEdit::Shorten(_) => {}
    }
    Some(format!(
        "{}{replacement}{}",
        &draft[..original.start],
        &draft[original.end..]
    ))
}

fn shortening_options(draft: &str, body_range: Range<usize>) -> Option<Vec<Range<usize>>> {
    // shortcut: use numbered source sentences for plain prose, use the general editor for other layouts.
    let body_start = body_range.start;
    let body = &draft[body_range];
    if body.contains(['\n', '\r', '"', '“', '”', '‘']) || !body.ends_with('.') {
        return None;
    }
    let mut offset = body_start;
    let mut options = Vec::new();
    for sentence in body.split_inclusive(". ") {
        let end = offset + sentence.len();
        let last_sentence = end == body_start + body.len();
        let prose = sentence.trim().strip_suffix('.')?;
        let last_word = prose.split_whitespace().next_back()?;
        if prose.contains(['.', '!', '?'])
            || !prose.starts_with(char::is_uppercase)
            || prose.split_whitespace().count() < 3
            || (!last_sentence && last_word.starts_with(char::is_uppercase))
        {
            return None;
        }
        // Remove the separator after the sentence, or before it if this is the last sentence.
        let start = if last_sentence && !options.is_empty() {
            offset - 1
        } else {
            offset
        };
        options.push(start..end);
        offset = end;
    }
    (options.len() > 1).then_some(options)
}

fn remove_sentence(draft: &str, options: &[Range<usize>], selection: &str) -> Option<String> {
    let line = selection.trim().lines().next()?.trim();
    let choice = line.strip_prefix("REMOVE ").unwrap_or(line);
    let (number, echoed_sentence) = match choice.split_once(". ") {
        Some((number, sentence)) => (number, Some(sentence)),
        None => (choice, None),
    };
    let index = number
        .trim_end_matches('.')
        .parse::<usize>()
        .ok()?
        .checked_sub(1)?;
    let range = options.get(index)?;
    if echoed_sentence.is_some_and(|sentence| sentence != draft[range.clone()].trim()) {
        return None;
    }
    Some(format!("{}{}", &draft[..range.start], &draft[range.end..]))
}

fn supported(request: &str) -> bool {
    words(request).iter().any(|word| {
        matches!(
            word.as_str(),
            "short"
                | "shorter"
                | "shorten"
                | "brief"
                | "length"
                | "long"
                | "longer"
                | "quote"
                | "quotes"
                | "section"
                | "sections"
                | "bullet"
                | "bullets"
        )
    })
}

fn available(app: &AppHandle) -> Result<(), &'static str> {
    let state = commands::available(app);
    if let Some(message) = state.message {
        return Err(message);
    }
    Ok(())
}

#[tauri::command]
pub fn session_coach_start(
    window: WebviewWindow,
    session_id: String,
    request: String,
    prior_requests: Vec<String>,
    prior_candidate_id: Option<String>,
) -> Result<String, &'static str> {
    let app = commands::main_window(&window)?;
    #[cfg(feature = "remote-llm")]
    if crate::llm::remote::remote_url(std::env::var("SCRIBULAR_LLM_URL").ok()).is_some() {
        return Err("Session Coach requires the on-device model.");
    }
    if request.trim().is_empty()
        || request.len() > 2000
        || prior_requests.len() > 10
        || prior_requests.iter().any(|r| r.len() > 2000)
    {
        return Err("Enter a shorter request.");
    }
    let recorder = app.try_state::<Arc<Recorder>>().ok_or(FAILED)?;
    recorder.while_not_recording(|| {
        available(app)?;
        let store = app.try_state::<Arc<Store>>().ok_or(FAILED)?;
        let session = store.get_session(&session_id).map_err(|_| FAILED)?
            .ok_or("This session is no longer available.")?;
        if session.status != SessionStatus::Done || session.summary.trim().is_empty() {
            return Err("Finish this note before asking the coach.");
        }
        let coach = app.try_state::<Arc<SessionCoach>>().ok_or(FAILED)?;
        let mut state = coach.0.lock().unwrap_or_else(|p| p.into_inner());
        if state.active.is_some() {
            return Err("The coach is still working. Try again.");
        }
        let prior = match prior_candidate_id.as_deref() {
            Some(id) => Some(state.candidate.as_ref()
                .filter(|candidate| candidate.session_id == session_id && candidate.id == id)
                .ok_or("That revision is no longer available. Try again.")?.note.clone()),
            None => None,
        };
        let job_id = commands::opaque_id()?;
        if !supported(&request) {
            let app = app.clone();
            let id = session_id.clone();
            let job = job_id.clone();
            std::thread::spawn(move || {
                let _ = app.emit_to("main", RESULT_EVENT, SessionCoachResult {
                    job_id: job, session_id: id,
                    outcome: SessionCoachOutcome::Reply { message: UNSUPPORTED },
                });
            });
            return Ok(job_id);
        }
        let candidate_id = commands::opaque_id()?;
        let draft = prior.as_deref().unwrap_or(&session.summary);
        let section_edit = SectionEdit::from_request(draft, &request);
        let shortening = match &section_edit {
            Some(SectionEdit::Shorten(heading)) => {
                section_body(draft, heading).and_then(|body| shortening_options(draft, body))
            }
            _ => None,
        };
        let prompt = if let Some(options) = &shortening {
            let choices = options.iter().enumerate()
                .map(|(index, range)| format!("{}. {}", index + 1, draft[range.clone()].trim()))
                .collect::<Vec<_>>().join("\n");
            format!(
                "Earlier requests:\n{}\n\nCurrent request:\n{request}\n\n\
                 Which sentence is less important to keep when shortening this section? \
                 Preserve context needed by the remaining sentences. Answer with REMOVE followed by the selected number.\n\n{choices}",
                prior_requests.join("\n")
            )
        } else {
            let draft_context = match &prior {
                Some(draft) => format!(
                    "Saved note (source):\n{}\n\nEarlier requests:\n{}\n\nDraft to edit:\n{draft}",
                    session.summary, prior_requests.join("\n")
                ),
                None => format!("Saved note (draft to edit):\n{}", session.summary),
            };
            format!(
                "Edit the draft clinical note to fulfill the current request. Keep earlier edits. If the request names a section, copy the other sections unchanged, including formatting. For a bullet formatting request, prefix each complete sentence with '* ' and preserve the original wording and order. Use complete source sentences from the saved note or transcript. Keep their facts and speaker attribution intact. When asked to shorten, keep the most important sentences and omit less important details. For a shortening request on a multi-sentence section, remove at least one sentence. Return the full edited note with its headings, separated by blank lines, and no preamble, then {END_NOTE}.\n\nTranscript:\n{}\n\n{draft_context}\n\nCurrent request:\n{request}",
                session.transcript
            )
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let app_for_result = app.clone();
        let job_for_result = job_id.clone();
        let session_for_result = session_id.clone();
        let note = session.summary;
        let prompt_id = session.prompt_id;
        let model_id = session.model_id;
        let transcript = session.transcript;
        let token = Arc::clone(&cancel);
        let completion = move |result: Result<String, &'static str>| {
            let Some(coach) = app_for_result.try_state::<Arc<SessionCoach>>() else { return; };
            let mut state = coach.0.lock().unwrap_or_else(|p| p.into_inner());
            if token.load(Ordering::Relaxed) || state.active.as_ref().is_none_or(|(id, _, _)| id != &job_for_result) {
                return;
            }
            state.active = None;
            let draft = prior.as_deref().unwrap_or(&note);
            let revised = result.as_deref().ok().and_then(|output| {
                if let Some(options) = &shortening {
                    return remove_sentence(draft, options, output);
                }
                let output = complete_note(output)?;
                match &section_edit {
                    Some(edit) => revise_section(draft, edit, output),
                    None => Some(output.to_string()),
                }
            }).filter(|text| {
                text.trim_end() != draft.trim_end()
                    && text.trim_end() != note.trim_end()
                    && grounded(&note, &transcript, text)
            });
            let outcome = match revised {
                Some(text) => {
                    let replaced = state.candidate.replace(Candidate {
                        session_id: session_for_result.clone(), id: candidate_id.clone(),
                        job_id: job_for_result.clone(),
                        note: text.clone(), base: note.clone(),
                        prompt_id, model_id,
                    });
                    state.previous_candidate =
                        replaced.filter(|candidate| Some(&candidate.id) == prior_candidate_id.as_ref());
                    SessionCoachOutcome::Candidate { candidate_id, note: text }
                }
                _ => SessionCoachOutcome::Reply { message: FAILED },
            };
            drop(state);
            let _ = app_for_result.emit_to("main", RESULT_EVENT, SessionCoachResult {
                job_id: job_for_result, session_id: session_for_result, outcome,
            });
        };
        let jobs = app.try_state::<Arc<JobQueue>>().ok_or(FAILED)?;
        jobs.enqueue_session_coach(SessionCoachJob { prompt, cancel: Arc::clone(&cancel), complete: Box::new(completion) })?;
        state.previous_candidate = None;
        state.active = Some((job_id.clone(), session_id, cancel));
        Ok(job_id)
    }).ok_or(super::CoachBlock::Recording.message())?
}

#[tauri::command]
pub fn session_coach_accept(
    window: WebviewWindow,
    session_id: String,
    candidate_id: String,
) -> Result<crate::store::Session, String> {
    let app = commands::main_window(&window).map_err(str::to_string)?;
    let recorder = app.try_state::<Arc<Recorder>>().ok_or(FAILED)?;
    recorder
        .while_not_recording(|| {
            available(app).map_err(str::to_string)?;
            let coach = app.try_state::<Arc<SessionCoach>>().ok_or(FAILED)?;
            let mut state = coach.0.lock().unwrap_or_else(|p| p.into_inner());
            if state.active.is_some() {
                return Err("The coach is still working. Try again.".into());
            }
            let candidate = state
                .candidate
                .as_ref()
                .filter(|candidate| {
                    candidate.session_id == session_id && candidate.id == candidate_id
                })
                .ok_or("That revision is no longer available. Ask for a new one.")?;
            let store = app.try_state::<Arc<Store>>().ok_or(FAILED)?;
            let jobs = app.try_state::<Arc<JobQueue>>().ok_or(FAILED)?;
            let result = jobs
                .while_real_idle(|| {
                    store.accept_coach_candidate(
                        &session_id,
                        (
                            &candidate.base,
                            candidate.prompt_id.as_deref(),
                            candidate.model_id.as_deref(),
                        ),
                        &candidate.note,
                    )
                })
                .ok_or(super::CoachBlock::Busy.message())?
                .map_err(|e| e.to_string());
            if result.is_ok() {
                state.candidate = None;
                state.previous_candidate = None;
            }
            result
        })
        .ok_or_else(|| super::CoachBlock::Recording.message().to_string())?
}

#[tauri::command]
pub fn session_coach_stop(window: WebviewWindow, job_id: String) -> Result<(), &'static str> {
    let app = commands::main_window(&window)?;
    let coach = app.try_state::<Arc<SessionCoach>>().ok_or(FAILED)?;
    let mut state = coach.0.lock().unwrap_or_else(|p| p.into_inner());
    let Some((id, session_id, cancel)) = state.active.take() else {
        // A result may have reached the backend before the user saw it.
        state.restore_after_stop(&job_id);
        return Ok(());
    };
    if id != job_id {
        state.active = Some((id, session_id, cancel));
        return Ok(());
    }
    cancel.store(true, Ordering::Relaxed);
    app.try_state::<Arc<JobQueue>>()
        .ok_or(FAILED)?
        .cancel_session_coach();
    drop(state);
    commands::signal(app);
    let _ = app.emit_to(
        "main",
        STOPPED_EVENT,
        SessionCoachStopped { job_id, session_id },
    );
    Ok(())
}

pub fn cancel_for_gate(app: &AppHandle) {
    let Some(coach) = app.try_state::<Arc<SessionCoach>>() else {
        return;
    };
    let mut state = coach.0.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((job_id, session_id, cancel)) = state.active.take() {
        cancel.store(true, Ordering::Relaxed);
        if let Some(jobs) = app.try_state::<Arc<JobQueue>>() {
            jobs.cancel_session_coach();
        }
        let _ = app.emit_to(
            "main",
            STOPPED_EVENT,
            SessionCoachStopped { job_id, session_id },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortening_a_section_keeps_earlier_edits_and_removes_only_a_source_sentence() {
        let draft = "Assessment:\n* Client felt tired.\n\nPlan:\nReview the sleep diary. Try a consistent bedtime.\n\nFollow-Up:\nMeet again next week.";
        let edit = SectionEdit::from_request(draft, "Also shorten Plan").unwrap();
        let options =
            shortening_options(draft, section_body(draft, edit.heading()).unwrap()).unwrap();
        let keep_first = draft.replace(" Try a consistent bedtime.", "");
        let keep_second = draft.replace("Review the sleep diary. ", "");
        assert_eq!(
            remove_sentence(draft, &options, "2\n"),
            Some(keep_first.clone())
        );
        assert_eq!(
            remove_sentence(draft, &options, "2\nExplanation."),
            Some(keep_first.clone())
        );
        assert_eq!(
            remove_sentence(draft, &options, "REMOVE 2"),
            Some(keep_first.clone())
        );
        assert_eq!(
            remove_sentence(draft, &options, "2. Try a consistent bedtime."),
            Some(keep_first)
        );
        assert_eq!(
            remove_sentence(draft, &options, "2. Review the sleep diary."),
            None
        );
        assert_eq!(remove_sentence(draft, &options, "1."), Some(keep_second));
        assert_eq!(remove_sentence(draft, &options, "3"), None);
        assert_eq!(remove_sentence(draft, &options, "Try a diary."), None);
        for body in [
            "Client said “I will try.” Check in next week.",
            "Follow up with Dr. Moreno about sleep quality.",
            "Meet again next week.",
        ] {
            assert!(shortening_options(body, 0..body.len()).is_none());
        }
        let explicit = SectionEdit::from_request(
            draft,
            "Shorten only the Plan section by removing one sentence",
        )
        .unwrap();
        assert_eq!(explicit.heading(), "plan");
        assert!(SectionEdit::from_request(draft, "Shorten the note").is_none());
    }

    #[test]
    fn a_replacement_bullets_only_the_requested_section_of_the_current_draft() {
        let saved = "Data:\nClient reported working overtime.\n\nAssessment:\nClient felt tired.\n\nPlan:\nReview the sleep diary. Try a consistent bedtime.";
        let previous = saved.replace("Client felt tired.", "* Client felt tired.");
        let model_output = previous
            .replace(
                "Client reported working overtime.",
                "* Client reported working overtime.",
            )
            .replace(
                "Review the sleep diary. Try",
                "* Review the sleep diary.\n* Try",
            );
        let request = "Use bullet points in Plan";
        for draft in [saved, &previous] {
            let edit = SectionEdit::from_request(draft, request).unwrap();
            let expected = draft.replace(
                "Review the sleep diary. Try",
                "* Review the sleep diary.\n* Try",
            );
            assert_eq!(revise_section(draft, &edit, &model_output), Some(expected));
            assert_eq!(
                revise_section(draft, &edit, "Plan:\n* Review the sleep diary."),
                None
            );
            assert_eq!(
                revise_section(draft, &edit, "Data:\n* Client reported working overtime."),
                None
            );
        }
    }

    #[test]
    fn stop_restores_previous_candidate_if_the_follow_up_has_already_finished() {
        let candidate = |id: &str| Candidate {
            session_id: "session".into(),
            id: id.into(),
            job_id: id.into(),
            note: "Revised note".into(),
            base: "Saved note".into(),
            prompt_id: None,
            model_id: None,
        };
        let mut state = State {
            candidate: Some(candidate("second")),
            previous_candidate: Some(candidate("first")),
            ..State::default()
        };
        state.restore_after_stop("unrelated");
        assert_eq!(state.candidate.as_ref().unwrap().id, "second");
        state.restore_after_stop("second");
        state.restore_after_stop("second");
        assert_eq!(state.candidate.unwrap().id, "first");
    }

    #[test]
    fn grounding_rejects_new_facts() {
        assert!(grounded(
            "Client took a break.",
            "Client said rest helps.",
            "Client said rest helps."
        ));
        assert!(!grounded(
            "Client took a break.",
            "Client said rest helps.",
            "Client said medication helps."
        ));
        assert!(!grounded(
            "Client said rest helps.",
            "",
            "Client said ‘helps rest’."
        ));
        assert!(!grounded("Alice slept. Bob cried.", "", "Alice cried."));
        assert!(!grounded(
            "Client said rest helps.",
            "",
            "Client said rest."
        ));
        assert!(grounded("Client took a break.", "", "Client took a break."));
        assert!(supported("Use bullet points in Assessment"));
        assert!(!supported("Diagnose the client"));
        assert!(!supported("Prolonged shortness"));
        assert_eq!(
            complete_note("Client took a break.\n<<END_OF_NOTE>>"),
            Some("Client took a break.")
        );
        assert_eq!(complete_note("Client took a break."), None);
    }
}
