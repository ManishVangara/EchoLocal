//! Optional transcript cleanup via an OpenAI-compatible endpoint.
//! Only transcript text is sent; audio never leaves the machine.

use echolocal_core::ai;
use echolocal_core::{AiSettings, PostProcessing};
use echolocal_engine::CancelFlag;
use std::sync::mpsc;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub enum AiError {
    Cancelled,
    Failed(String),
}

fn request(
    mode: PostProcessing,
    transcript: &str,
    settings: &AiSettings,
) -> Result<String, String> {
    let body = ai::build_request(mode, transcript, settings).ok_or("post-processing is off")?;
    let client = reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = client
        .post(ai::completions_url(&settings.base_url))
        .json(&body);
    if !settings.api_key.trim().is_empty() {
        request = request.bearer_auth(settings.api_key.trim());
    }
    let response = request.send().map_err(|e| e.to_string())?;
    let status = response.status();
    let json: serde_json::Value = response.json().map_err(|e| format!("{status}: {e}"))?;
    if !status.is_success() {
        let detail = json
            .pointer("/error/message")
            .and_then(|m| m.as_str())
            .unwrap_or("request failed");
        return Err(format!("{status}: {detail}"));
    }
    let output = ai::parse_response(&json).map_err(|e| e.to_string())?;
    if ai::accept_output(mode, transcript, &output) {
        Ok(output)
    } else {
        Err("the model's reply didn't look like a cleaned-up transcript".into())
    }
}

/// Run post-processing on a helper thread so cancellation can abandon it.
pub fn post_process(
    mode: PostProcessing,
    transcript: &str,
    settings: &AiSettings,
    cancel: &CancelFlag,
) -> Result<String, AiError> {
    let (tx, rx) = mpsc::channel();
    let (transcript, settings) = (transcript.to_string(), settings.clone());
    std::thread::spawn(move || {
        let _ = tx.send(request(mode, &transcript, &settings));
    });
    loop {
        if cancel.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => return result.map_err(AiError::Failed),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(AiError::Failed("post-processing thread failed".into()))
            }
        }
    }
}
