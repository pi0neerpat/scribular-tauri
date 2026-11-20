use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use super::*;
use crate::{
    commands::{prompt_list, PromptList},
    jobs::{JobQueue, PracticeJob, Recorder},
    license::Licensing,
    models::ModelManager,
    store::Store,
};

const DRAFT_GONE: &str = "This style is no longer open. Start again from Note Styles.";
const UNAVAILABLE: &str = "The coach isn’t available right now.";

pub(super) fn opaque_id() -> Result<String, &'static str> {
    let bytes = crate::store::key::new_key().map_err(|_| UNAVAILABLE)?;
    Ok(bytes[..16].iter().map(|b| format!("{b:02x}")).collect())
}

pub(super) fn main_window(window: &WebviewWindow) -> Result<&AppHandle, &'static str> {
    (window.label() == "main")
        .then(|| window.app_handle())
        .ok_or(UNAVAILABLE)
}

fn get_state<T: Send + Sync + 'static>(
    app: &AppHandle,
) -> Result<tauri::State<'_, Arc<T>>, &'static str> {
    app.try_state::<Arc<T>>().ok_or(UNAVAILABLE)
}

pub(super) fn available(app: &AppHandle) -> CoachAvailability {
    let components = (
        get_state::<Licensing>(app),
        get_state::<JobQueue>(app),
        get_state::<ModelManager>(app),
    );
    let (Ok(licensing), Ok(jobs), Ok(models)) = components else {
        return CoachAvailability::from_reason(Some(CoachBlock::AccessDenied));
    };
    CoachAvailability::current(
        false,
        licensing.require_access().is_ok(),
        jobs.is_busy(),
        models.summary_availability(),
    )
}

#[tauri::command]
pub fn get_coach_availability(app: AppHandle) -> CoachAvailability {
    availability(&app)
}

pub fn availability(app: &AppHandle) -> CoachAvailability {
    let Ok(recorder) = get_state::<Recorder>(app) else {
        return CoachAvailability::from_reason(Some(CoachBlock::AccessDenied));
    };
    recorder
        .while_not_recording(|| available(app))
        .unwrap_or_else(|| CoachAvailability::from_reason(Some(CoachBlock::Recording)))
}

pub fn signal(app: &AppHandle) {
    if let Ok(coach) = get_state::<Coach>(app) {
        coach.signal();
    }
}

// Serialize admission with capture start. No model work runs under either lock.
fn admitted<T>(
    app: &AppHandle,
    work: impl FnOnce(&mut Option<Draft>) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let recorder = get_state::<Recorder>(app)?;
    let coach = get_state::<Coach>(app)?;
    recorder
        .while_not_recording(|| {
            if let Some(message) = available(app).message {
                return Err(message);
            }
            work(&mut coach.lock())
        })
        .ok_or(CoachBlock::Recording.message())?
}

fn draft_mut<'a>(draft: &'a mut Option<Draft>, id: &str) -> Result<&'a mut Draft, &'static str> {
    draft
        .as_mut()
        .filter(|draft| draft.id == id)
        .ok_or(DRAFT_GONE)
}

#[tauri::command]
pub fn coach_samples(window: WebviewWindow) -> Result<Vec<CoachSample>, &'static str> {
    main_window(&window)?;
    Ok(extract::SAMPLES.to_vec())
}

#[tauri::command]
pub fn coach_open(
    window: WebviewWindow,
    style_id: Option<String>,
) -> Result<CoachDraft, &'static str> {
    let app = main_window(&window)?;
    admitted(app, |current| {
        if current.is_some() {
            return Err("Finish or close the open style first.");
        }
        let original = if let Some(id) = style_id {
            let p = get_state::<Store>(app)?
                .get_prompt(&id)
                .map_err(|_| "I couldn’t open that style.")?
                .filter(|p| !p.is_default)
                .ok_or("This style is no longer available to adjust.")?;
            compile(
                p.style_spec
                    .as_ref()
                    .ok_or("This style is no longer available to adjust.")?,
            )?;
            Some(p)
        } else {
            None
        };
        let draft = Draft::new(opaque_id()?, original);
        let snapshot = draft.snapshot();
        *current = Some(draft);
        signal(app);
        Ok(snapshot)
    })
}

#[tauri::command]
pub async fn coach_build(
    window: WebviewWindow,
    example_text: String,
    draft_id: Option<String>,
) -> Result<CoachDraft, &'static str> {
    main_window(&window)?;
    extract::validate_example(&example_text)?;
    let standalone = draft_id.is_none();
    let draft_id = match draft_id {
        Some(id) => id,
        None => coach_open(window.clone(), None)?.id,
    };
    let built = build(&window, &draft_id, example_text).await;
    if standalone && built.is_err() {
        let _ = coach_cancel(window, draft_id);
    }
    built
}

async fn build(
    window: &WebviewWindow,
    draft_id: &str,
    example_text: String,
) -> Result<CoachDraft, &'static str> {
    let app = window.app_handle();
    admitted(app, |current| {
        let draft = draft_mut(current, draft_id)?;
        if draft.spec.is_some() || draft.reading {
            return Err("The coach is already reading or has read the example.");
        }
        draft.reading = true;
        Ok(())
    })?;
    let _ = window.emit_to(
        "main",
        PROGRESS_EVENT,
        CoachProgress {
            draft_id: draft_id.into(),
            job_id: None,
            attempt: 0,
        },
    );
    let extracted = tauri::async_runtime::spawn_blocking(move || {
        crate::diag::panic::catch_unwind(|| extract::extract(&example_text))
    })
    .await;
    let coach = get_state::<Coach>(app)?;
    // Release the reading reservation even if real work arrived while extraction ran.
    let recorder = get_state::<Recorder>(app)?;
    recorder
        .while_not_recording(|| {
            let mut current = coach.lock();
            let draft = draft_mut(&mut current, draft_id)?;
            draft.reading = false;
            if let Some(message) = available(app).message {
                return Err(message);
            }
            let (spec, questions) = extracted
                .ok()
                .and_then(Result::ok)
                .ok_or("I couldn’t read that example. Try again.")??;
            compile(&spec)?;
            draft.spec = Some(spec);
            draft.questions = questions;
            Ok(draft.snapshot())
        })
        .ok_or(CoachBlock::Recording.message())?
}

#[tauri::command]
pub fn coach_answer(
    window: WebviewWindow,
    draft_id: String,
    answer: Option<String>,
) -> Result<CoachReply, &'static str> {
    let app = main_window(&window)?;
    admitted(app, |current| {
        let draft = draft_mut(current, &draft_id)?;
        let question = *draft
            .questions
            .first()
            .ok_or("There are no more example questions.")?;
        let spec = draft.spec.as_mut().ok_or(DRAFT_GONE)?;
        let message = if let Some(answer) = answer {
            validate_reply(&answer)?;
            question.answer(spec, &answer)?
        } else {
            "I’ll use my best guess.".into()
        };
        draft.questions.remove(0);
        Ok(CoachReply::Answer {
            draft: draft.snapshot(),
            message,
        })
    })
}

fn validate_reply(reply: &str) -> Result<(), &'static str> {
    if reply.trim().is_empty() || reply.chars().count() > 500 {
        return Err("Write a reply of 1 to 500 characters.");
    }
    Ok(())
}

#[tauri::command]
pub fn coach_practice(window: WebviewWindow, draft_id: String) -> Result<String, &'static str> {
    let app = main_window(&window)?;
    admitted(app, |current| {
        let draft = draft_mut(current, &draft_id)?;
        if !draft.questions.is_empty()
            || draft.note.is_some()
            || draft.proposal.is_some()
            || draft.writing.is_some()
        {
            return Err("Finish the current step first.");
        }
        let spec = draft
            .spec
            .clone()
            .ok_or("Give the coach an example first.")?;
        let base = match &draft.original {
            Some(original) => original.text.clone(),
            None => compile(&spec)?,
        };
        start_practice(&window, draft, spec, base, None)
    })
}

#[tauri::command]
pub fn coach_interpret(
    window: WebviewWindow,
    draft_id: String,
    request: String,
) -> Result<CoachReply, &'static str> {
    let app = main_window(&window)?;
    validate_reply(&request)?;
    admitted(app, |current| {
        let draft = draft_mut(current, &draft_id)?;
        if draft.note.is_none() || draft.writing.is_some() || draft.proposal.is_some() {
            return Err("Finish the current step first.");
        }
        let spec = draft.spec.as_ref().ok_or(DRAFT_GONE)?;
        match interpret::interpret(spec, &request) {
            interpret::Interpretation::Question(question) => Ok(CoachReply::Question { question }),
            interpret::Interpretation::Unsupported => Ok(CoachReply::Unsupported {
                suggestions: interpret::SUGGESTIONS.iter().map(|s| (*s).into()).collect(),
            }),
            interpret::Interpretation::Patch(patch) => {
                let (next, change) = patch.apply(spec)?;
                let text = compile(&next)?;
                let job_id = start_practice(&window, draft, next, text, Some(change))?;
                Ok(CoachReply::Writing { job_id })
            }
        }
    })
}

fn start_practice(
    window: &WebviewWindow,
    draft: &mut Draft,
    spec: StyleSpec,
    instructions: String,
    change: Option<String>,
) -> Result<String, &'static str> {
    let job_id = opaque_id()?;
    let cancel = Arc::new(AtomicBool::new(false));
    let draft_id = draft.id.clone();
    let event = window.clone();
    let progress = {
        let draft_id = draft_id.clone();
        let job_id = job_id.clone();
        Box::new(move |attempt| {
            let _ = event.emit_to(
                "main",
                PROGRESS_EVENT,
                CoachProgress {
                    draft_id: draft_id.clone(),
                    job_id: Some(job_id.clone()),
                    attempt,
                },
            );
        })
    };
    let complete = {
        let window = window.clone();
        let job_id = job_id.clone();
        let spec = spec.clone();
        Box::new(move |result| {
            let app = window.app_handle();
            let Ok(coach) = get_state::<Coach>(app) else {
                return;
            };
            let mut current = coach.lock();
            let Ok(draft) = draft_mut(&mut current, &draft_id) else {
                return;
            };
            if !draft
                .writing
                .as_ref()
                .is_some_and(|job| job.id == job_id && !job.cancel.load(Ordering::Relaxed))
            {
                return;
            }
            draft.writing = None;
            let error = match result {
                Ok(note) => {
                    if let Some(change) = change {
                        draft.proposal = Some((
                            CoachProposal {
                                id: job_id.clone(),
                                change,
                                note,
                            },
                            spec,
                        ));
                    } else {
                        draft.note = Some(note);
                    }
                    None
                }
                Err(message) => Some(message),
            };
            let _ = window.emit_to(
                "main",
                RESULT_EVENT,
                CoachResult {
                    job_id,
                    draft: draft.snapshot(),
                    error,
                },
            );
        })
    };
    get_state::<JobQueue>(window.app_handle())?.enqueue_practice(PracticeJob {
        spec,
        instructions,
        source: practice::PRACTICE_SOURCE,
        cancel: Arc::clone(&cancel),
        progress,
        complete,
    })?;
    draft.writing = Some(Writing {
        id: job_id.clone(),
        cancel,
    });
    Ok(job_id)
}

#[tauri::command]
pub fn coach_choose(
    window: WebviewWindow,
    draft_id: String,
    proposal_id: String,
    use_this: bool,
) -> Result<CoachDraft, &'static str> {
    let app = main_window(&window)?;
    admitted(app, |current| {
        let draft = draft_mut(current, &draft_id)?;
        if !draft
            .proposal
            .as_ref()
            .is_some_and(|(proposal, _)| proposal.id == proposal_id)
        {
            return Err("This change is no longer available.");
        }
        let (proposal, spec) = draft.proposal.take().unwrap();
        if use_this {
            draft.spec = Some(spec);
            draft.note = Some(proposal.note);
            draft.accepted_change = true;
        }
        Ok(draft.snapshot())
    })
}

#[tauri::command]
pub fn coach_save(
    window: WebviewWindow,
    draft_id: String,
    name: String,
    use_next: bool,
) -> Result<PromptList, &'static str> {
    let app = main_window(&window)?;
    admitted(app, |current| {
        let draft = draft_mut(current, &draft_id)?;
        if draft.note.is_none()
            || draft.writing.is_some()
            || draft.proposal.is_some()
            || (draft.original.is_some() && !draft.accepted_change)
        {
            return Err("Finish checking the practice note before saving.");
        }
        let spec = draft.spec.as_ref().ok_or(DRAFT_GONE)?;
        let store = get_state::<Store>(app)?;
        let jobs = get_state::<JobQueue>(app)?;
        let id = jobs
            .while_real_idle(|| {
                store.save_coach_style(draft.original.as_ref(), &name, spec, use_next)
            })
            .ok_or(CoachBlock::Busy.message())?
            .map_err(|error| match error {
                crate::store::StoreError::CoachStyle(message) => message,
                _ => "I couldn’t save that style. Try again.",
            })?;
        *current = None;
        signal(app);
        prompt_list(&store, Some(id))
            .map_err(|_| "The style was saved. Reopen Note Styles to see it.")
    })
}

#[tauri::command]
pub fn coach_cancel(window: WebviewWindow, draft_id: String) -> Result<(), &'static str> {
    let app = main_window(&window)?;
    let coach = get_state::<Coach>(app)?;
    let mut current = coach.lock();
    if current.as_ref().is_some_and(|draft| draft.id == draft_id) {
        *current = None;
        get_state::<JobQueue>(app)?.cancel_practice();
        coach.signal();
        let _ = window.emit_to(
            "main",
            CANCELLED_EVENT,
            CoachCancelled {
                draft_id,
                reason: CoachCancelReason::Cancelled,
            },
        );
    }
    Ok(())
}

pub fn discard(app: &AppHandle, reason: CoachCancelReason) {
    let Ok(coach) = get_state::<Coach>(app) else {
        return;
    };
    let mut current = coach.lock();
    if let Some(draft) = current.take() {
        coach.signal();
        if let Ok(jobs) = get_state::<JobQueue>(app) {
            jobs.cancel_practice();
        }
        let _ = app.emit_to(
            "main",
            CANCELLED_EVENT,
            CoachCancelled {
                draft_id: draft.id,
                reason,
            },
        );
    }
}

pub fn refresh_availability(app: &AppHandle, last: &mut Option<(CoachAvailability, bool)>) {
    let availability = availability(app);
    if availability.message.is_some() {
        super::session::cancel_for_gate(app);
    }
    if availability.reason == Some(CoachBlock::AccessDenied) {
        discard(app, CoachCancelReason::AccessDenied);
    }
    let open = get_state::<Coach>(app).is_ok_and(|coach| coach.lock().is_some());
    let previous = last.replace((availability, open));
    if previous.is_none_or(|(old, _)| old != availability) {
        let _ = app.emit_to("main", AVAILABILITY_EVENT, availability);
    }
    if previous != *last {
        let app = app.clone();
        let _ = app.clone().run_on_main_thread(move || {
            crate::shell::menu::set_coach_state(
                &app,
                !open && availability.reason.is_none(),
                open,
                availability.message,
            )
        });
    }
    if matches!(
        availability.reason,
        Some(CoachBlock::Downloading | CoachBlock::LoadFailed | CoachBlock::Unsupported)
    ) {
        let Ok(coach) = get_state::<Coach>(app) else {
            return;
        };
        let mut current = coach.lock();
        if let Some(draft) = current.as_mut() {
            if let Some(writing) = draft.writing.take() {
                if let Ok(jobs) = get_state::<JobQueue>(app) {
                    jobs.cancel_practice();
                }
                let _ = app.emit_to(
                    "main",
                    RESULT_EVENT,
                    CoachResult {
                        job_id: writing.id,
                        draft: draft.snapshot(),
                        error: availability.message,
                    },
                );
            }
        }
    }
}

pub fn is_open(app: &AppHandle) -> bool {
    get_state::<Coach>(app).is_ok_and(|coach| coach.lock().is_some())
}
