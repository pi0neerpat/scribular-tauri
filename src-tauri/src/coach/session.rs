use std::{
    collections::HashSet,
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
    candidate: Option<(String, String, String)>,
}

fn words(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect()
}

fn grounded(note: &str, transcript: &str, output: &str) -> bool {
    let source = words(&format!("{note}\n{transcript}"));
    !output.trim().is_empty()
        && words(output).is_subset(&source)
        && super::extract::quotes(output)
            .all(|quote| note.contains(quote) || transcript.contains(quote))
}

fn supported(request: &str) -> bool {
    let request = request.to_lowercase();
    [
        "short", "brief", "length", "long", "quote", "section", "bullet",
    ]
    .iter()
    .any(|term| request.contains(term))
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
        let prior = match prior_candidate_id {
            Some(id) => Some(state.candidate.as_ref()
                .filter(|(session, candidate, _)| session == &session_id && candidate == &id)
                .ok_or("That revision is no longer available. Try again.")?.2.clone()),
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
        let prompt = format!(
            "Revise the saved clinical note for writing form only. Preserve every clinical fact and attribution. Use only words already in the saved note or transcript. Return the full revised note, no preamble.\nSaved note:\n{}\nTranscript:\n{}\nEarlier requests:\n{}\nEarlier candidate:\n{}\nCurrent request:\n{}",
            session.summary, session.transcript, prior_requests.join("\n"), prior.unwrap_or_default(), request
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let app_for_result = app.clone();
        let job_for_result = job_id.clone();
        let session_for_result = session_id.clone();
        let note = session.summary;
        let transcript = session.transcript;
        let token = Arc::clone(&cancel);
        let completion = move |result: Result<String, &'static str>| {
            let Some(coach) = app_for_result.try_state::<Arc<SessionCoach>>() else { return; };
            let mut state = coach.0.lock().unwrap_or_else(|p| p.into_inner());
            if token.load(Ordering::Relaxed) || state.active.as_ref().is_none_or(|(id, _, _)| id != &job_for_result) {
                return;
            }
            state.active = None;
            let outcome = match result {
                Ok(text) if grounded(&note, &transcript, &text) => {
                    state.candidate = Some((session_for_result.clone(), candidate_id.clone(), text.clone()));
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
        state.active = Some((job_id.clone(), session_id, cancel));
        Ok(job_id)
    }).ok_or(super::CoachBlock::Recording.message())?
}

#[tauri::command]
pub fn session_coach_stop(window: WebviewWindow, job_id: String) -> Result<(), &'static str> {
    let app = commands::main_window(&window)?;
    let coach = app.try_state::<Arc<SessionCoach>>().ok_or(FAILED)?;
    let mut state = coach.0.lock().unwrap_or_else(|p| p.into_inner());
    let Some((id, session_id, cancel)) = state.active.take() else {
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
        assert!(supported("Use bullet points in Assessment"));
        assert!(!supported("Diagnose the client"));
    }
}
