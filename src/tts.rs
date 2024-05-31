use anyhow::{Context, Result};
use gtk::gio;
use once_cell::unsync::OnceCell;
use speech_dispatcher::{Connection, Mode, Priority};

thread_local! {
    static TTS: OnceCell<Connection> = const { OnceCell::new() };
}

fn instance() -> Result<Connection> {
    TTS.with(|tts| {
        tts.get_or_try_init(|| {
            let conn = Connection::open("delta", "delta", "delta", Mode::Threaded)?;

            tracing::debug!("Speech dispatcher connection initialized");

            Ok(conn)
        })
        .cloned()
    })
}

pub fn speak(text: impl Into<String>) {
    let text = text.into();

    gio::spawn_blocking(move || {
        if let Err(err) = cancel() {
            tracing::warn!("Failed to stop: {:?}", err);
        }

        if let Err(err) = say(text, Priority::Important) {
            tracing::warn!("Failed to say: {:?}", err);
        }
    });
}

fn say(text: impl Into<String>, priority: Priority) -> Result<()> {
    let instance = instance()?;

    instance
        .say(priority, text.into())
        .context("Null utterance id")?;

    Ok(())
}

fn cancel() -> Result<()> {
    let instance = instance()?;

    instance.cancel()?;

    Ok(())
}
