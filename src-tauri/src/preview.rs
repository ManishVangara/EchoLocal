//! Live transcript while the user is speaking.
//!
//! The recorder hands over the speech recorded since the last finished piece
//! about twice a second. A worker thread transcribes the newest snapshot and
//! shows "finished pieces + current snapshot" in the overlay. The preview is
//! display-only: the inserted text still comes from the normal release path.
//! It only runs when the model is idle (never waits for it), and is aborted
//! the moment recording stops so the final transcription isn't delayed.

use crate::background::SharedTexts;
use crate::overlay;
use crate::state::{lock, AppState};
use echolocal_audio::PreviewCallback;
use echolocal_core::audio::pad_to_min_duration;
use echolocal_core::text::{join_transcripts, normalize_transcript};
use echolocal_engine::CancelFlag;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use tauri::{AppHandle, Manager};

/// Only the most recent snapshot matters; older ones are dropped.
type Slot = Arc<(Mutex<Option<Vec<f32>>>, Condvar)>;

pub struct LivePreview {
    slot: Slot,
    stop: Arc<AtomicBool>,
    cancel: CancelFlag,
    thread: Option<JoinHandle<()>>,
}

impl LivePreview {
    pub fn start(app: &AppHandle, pieces: SharedTexts) -> (Self, PreviewCallback) {
        let slot: Slot = Arc::new((Mutex::new(None), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = CancelFlag::default();

        let worker = {
            let (app, slot, stop, cancel) =
                (app.clone(), slot.clone(), stop.clone(), cancel.clone());
            std::thread::Builder::new()
                .name("echolocal-preview".into())
                .spawn(move || run(app, slot, stop, cancel, pieces))
                .expect("failed to start live preview")
        };

        let callback_slot = slot.clone();
        let callback: PreviewCallback = Box::new(move |audio: &[f32]| {
            let (latest, ready) = &*callback_slot;
            *lock(latest) = Some(audio.to_vec());
            ready.notify_one();
        });
        (
            Self {
                slot,
                stop,
                cancel,
                thread: Some(worker),
            },
            callback,
        )
    }

    /// Stop previewing and abort an in-flight preview transcription, so the
    /// engine is free for the final one. Waits briefly for the worker.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.cancel.cancel();
        self.slot.1.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(app: AppHandle, slot: Slot, stop: Arc<AtomicBool>, cancel: CancelFlag, pieces: SharedTexts) {
    let state = app.state::<AppState>();
    loop {
        let audio = {
            let (latest, ready) = &*slot;
            let mut guard = lock(latest);
            while guard.is_none() && !stop.load(Ordering::SeqCst) {
                guard = ready.wait(guard).unwrap_or_else(|e| e.into_inner());
            }
            if stop.load(Ordering::SeqCst) {
                return;
            }
            guard.take()
        };
        let Some(audio) = audio else { continue };

        let model = state.settings().model;
        let text = {
            let Some(mut engine) = state.try_engine() else {
                continue; // a piece is being transcribed or the model is loading
            };
            let Some(engine) = engine.as_mut().filter(|e| e.model_id() == model) else {
                continue;
            };
            match engine.transcribe(&pad_to_min_duration(&audio, 1000), &cancel) {
                Ok(t) => normalize_transcript(&t.text),
                Err(_) => continue,
            }
        };
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let finished = lock(&pieces).clone();
        let combined = join_transcripts(finished.iter().map(String::as_str).chain([text.as_str()]));
        overlay::set_text(&app, &combined);
    }
}
