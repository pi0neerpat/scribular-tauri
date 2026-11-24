use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::{
    commands,
    jobs::{RecordError, Recorder},
    store::{Session, Store},
};

#[derive(Default)]
struct Transition {
    sequence: u32,
    running: bool,
    pending: Option<(u32, Instant, bool)>,
}

impl Transition {
    fn begin(&mut self, request: Option<u32>, now: Instant) -> Result<bool, &'static str> {
        if self.pending.is_some_and(|(_, until, _)| now >= until) {
            self.pending = None;
        }
        let notify = match (self.pending, request) {
            (Some((id, _, notify)), Some(request)) if id == request => notify,
            (None, None) if !self.running => false,
            _ => return Err("A recording action is already in progress or has expired."),
        };
        self.pending = None;
        self.running = true;
        Ok(notify)
    }
}

static TRANSITION: Mutex<Transition> = Mutex::new(Transition {
    sequence: 0,
    running: false,
    pending: None,
});

pub(crate) struct Action(bool);

impl Action {
    pub(crate) fn begin(request: Option<u32>) -> Result<(Self, bool), &'static str> {
        let notify = TRANSITION.lock().unwrap().begin(request, Instant::now())?;
        Ok((Self(true), notify))
    }

    fn request_start(mut self, app: &AppHandle, notify: bool) {
        let id = {
            let mut state = TRANSITION.lock().unwrap();
            state.sequence = state.sequence.wrapping_add(1);
            let id = state.sequence;
            // shortcut: expire abandoned starts after 10s, add webview lifecycle cancellation if reloads become frequent.
            state.pending = Some((id, Instant::now() + Duration::from_secs(10), notify));
            state.running = false;
            self.0 = false;
            id
        };
        if app.emit(commands::TRAY_START_RECORDING_EVENT, id).is_err() {
            cancel_start(id);
        }
    }
}

impl Drop for Action {
    fn drop(&mut self) {
        if self.0 {
            TRANSITION.lock().unwrap().running = false;
        }
    }
}

pub(crate) fn cancel_start(id: u32) {
    let mut state = TRANSITION.lock().unwrap();
    if state.pending.is_some_and(|(pending, _, _)| pending == id) {
        state.pending = None;
    }
}

pub(crate) enum BackgroundAction {
    Start,
    Stop(Option<u8>),
    Toggle,
}

pub(crate) fn background(app: &AppHandle, action: BackgroundAction) {
    let Ok((lease, _)) = Action::begin(None) else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(recorder) = app.try_state::<std::sync::Arc<Recorder>>() else {
            if matches!(action, BackgroundAction::Start) {
                crate::shell::show_main(&app);
            }
            return;
        };
        let (speakers, notify) = match action {
            BackgroundAction::Start if recorder.current().is_some() => return,
            BackgroundAction::Start => return lease.request_start(&app, false),
            BackgroundAction::Stop(speakers) => (speakers, false),
            BackgroundAction::Toggle if recorder.current().is_none() => {
                return lease.request_start(&app, true);
            }
            BackgroundAction::Toggle => (None, true),
        };
        let result = recorder.stop_with_speakers(speakers);
        finish_stop(
            result,
            notify,
            |message| notice(&app, "Recording stopped", message),
            |kind, message| {
                crate::diag::log::log("ERROR", "shell", kind);
                let _ = app.emit(commands::TRAY_STOP_FAILED_EVENT, message);
                notice(&app, "Scribular", message);
            },
        );
    });
}

fn finish_stop(
    result: Result<String, RecordError>,
    notify: bool,
    success: impl FnOnce(&str),
    failure: impl FnOnce(&'static str, &'static str),
) {
    match result {
        Ok(_) if notify => success("Your note is being written."),
        Ok(_) => {}
        Err(RecordError::NotRecording) => failure("not recording", "Nothing is recording."),
        Err(RecordError::NoAudio) => failure("empty recording", "No audio was recorded."),
        Err(RecordError::StopPreparationFailed) => failure(
            "recording stop preparation failed",
            "Couldn’t prepare to stop. Recording is still running.",
        ),
        Err(_) => failure(
            "recording stop failed",
            "Couldn't finish recording. Check History for the saved session.",
        ),
    }
}

pub(crate) fn started(app: &AppHandle, session: &Session) {
    let style = app.try_state::<std::sync::Arc<Store>>().and_then(|store| {
        store
            .get_prompt(session.prompt_id.as_deref()?)
            .ok()
            .flatten()
    });
    notice(
        app,
        "Recording started",
        style.as_ref().map_or("", |style| style.name.as_str()),
    );
}

fn notice(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_and_in_flight_actions_are_exclusive() {
        let now = Instant::now();
        let mut state = Transition {
            pending: Some((7, now + Duration::from_secs(10), true)),
            ..Default::default()
        };
        assert!(state.begin(None, now).is_err());
        assert!(state.begin(Some(6), now).is_err());
        assert_eq!(state.begin(Some(7), now), Ok(true));
        assert!(state.begin(None, now + Duration::from_secs(2)).is_err());
        assert!(state.begin(Some(7), now).is_err());
        state.running = false;
        assert_eq!(state.begin(None, now), Ok(false));
        assert!(state.begin(None, now + Duration::from_secs(2)).is_err());
        state.running = false;
        state.pending = Some((8, now, true));
        assert!(state.begin(Some(8), now).is_err());
        assert_eq!(state.begin(None, now), Ok(false));
    }

    #[test]
    fn only_a_completed_shortcut_stop_notifies_and_failure_discards_error_details() {
        let mut count = 0;
        finish_stop(
            Ok("session ID".into()),
            true,
            |_| count += 1,
            |_, _| panic!(),
        );
        finish_stop(
            Ok("session ID".into()),
            false,
            |_| count += 1,
            |_, _| panic!(),
        );
        assert_eq!(count, 1);
        let private = "private audio path, transcript, note, custom style and session title";
        finish_stop(
            Err(RecordError::Audio(crate::audio::AudioError::Io(
                std::io::Error::other(private),
            ))),
            true,
            |_| panic!("failed stop emitted success"),
            |kind, message| {
                assert_eq!(kind, "recording stop failed");
                assert!(!message.contains(private));
            },
        );
    }
}
