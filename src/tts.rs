use anyhow::Result;
use std::sync::{Mutex, OnceLock};

static TTS: OnceLock<Mutex<Result<tts::Tts>>> = OnceLock::new();

pub fn speak(text: impl Into<String>, interrupt: bool) {
    let instance = TTS.get_or_init(|| Mutex::new(tts::Tts::default().map_err(|err| err.into())));

    match *instance.lock().unwrap() {
        Ok(ref mut tts) => {
            if let Err(err) = tts.speak(text, interrupt) {
                tracing::error!("Failed to speak: {:?}", err);
            }
        }
        Err(ref err) => {
            tracing::error!("Failed to setup TTS: {:?}", err);
        }
    }
}
