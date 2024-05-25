use std::{sync::mpsc, thread};

use anyhow::{ensure, Context, Result};
use gst::prelude::*;
use gtk::{
    glib::{self, clone, closure_local},
    subclass::prelude::*,
};

use crate::{audio_device, config};

const SAMPLE_WINDOW_SIZE: usize = 2 * 32_000; // 2 seconds
const MODEL_PATH: &str = "./ggml-tiny.en.bin";

mod imp {
    use std::{cell::RefCell, sync::OnceLock};

    use glib::subclass::Signal;
    use gst::bus::BusWatchGuard;

    use super::*;

    #[derive(Default)]
    pub struct Stt {
        pub(super) pipeline: RefCell<Option<(gst::Pipeline, BusWatchGuard)>>,
        pub(super) thread_handle: RefCell<Option<thread::JoinHandle<()>>>,
        pub(super) fut_handle: RefCell<Option<glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Stt {
        const NAME: &'static str = "DeltaStt";
        type Type = super::Stt;
    }

    impl ObjectImpl for Stt {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            if config::is_tts_enabled() {
                if let Err(err) = obj.init() {
                    tracing::error!("Failed to initialize STT: {:?}", err);
                }
            }
        }

        fn dispose(&self) {
            if let Some((pipeline, _bus_watch_guard)) = self.pipeline.take() {
                if let Err(err) = pipeline.set_state(gst::State::Null) {
                    tracing::error!("Failed to set pipeline state to NULL: {:?}", err);
                }
            }

            if let Some(join_handle) = self.thread_handle.take() {
                if let Err(err) = join_handle.join() {
                    tracing::error!("Failed to join thread: {:?}", err);
                }
            }

            if let Some(fut_handle) = self.fut_handle.take() {
                fut_handle.abort();
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![Signal::builder("transcripted")
                    .param_types([String::static_type()])
                    .build()]
            })
        }
    }
}

glib::wrapper! {
    pub struct Stt(ObjectSubclass<imp::Stt>);
}

impl Stt {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_transcripted<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &str) + 'static,
    {
        self.connect_closure(
            "transcripted",
            false,
            closure_local!(|obj: &Self, segment: &str| f(obj, segment)),
        )
    }

    fn init(&self) -> Result<()> {
        let imp = self.imp();

        let caps = gst::Caps::builder("audio/x-raw")
            .field("rate", 16_000)
            .field("format", "S16LE")
            .field("channels", 1)
            .build();

        let pulsesrc = gst::ElementFactory::make("pulsesrc").build()?;
        let appsink = gst::ElementFactory::make("appsink")
            .property("emit-signals", true)
            .build()?;

        let pipeline = gst::Pipeline::new();
        pipeline.add_many([&pulsesrc, &appsink])?;

        let device = audio_device::find_default_source()?;
        device.reconfigure_element(&pulsesrc)?;

        let device_name = pulsesrc
            .property::<Option<String>>("device")
            .context("No device name")?;
        ensure!(!device_name.is_empty(), "Empty device name");

        tracing::debug!("Using device `{}`", device_name);

        pulsesrc.link_filtered(&appsink, &caps)?;

        let (sample_tx, sample_rx) = mpsc::channel();

        appsink.connect("new-sample", false, move |values| {
            let appsink = values[0].get::<gst::Element>().unwrap();

            let raw_sample = appsink
                .emit_by_name::<Option<gst::Sample>>("pull-sample", &[])
                .unwrap();

            let sample = raw_sample
                .buffer()
                .unwrap()
                .map_readable()
                .unwrap()
                .as_slice()
                .to_vec();
            let _ = sample_tx.send(sample);

            Some(gst::FlowReturn::Ok.into())
        });

        let bus = pipeline.bus().unwrap();
        let bus_watch_guard = bus
            .add_watch_local(
                clone!(@weak self as obj => @default-panic, move |_, message| {
                    obj.handle_bus_message(message)
                }),
            )
            .unwrap();

        imp.pipeline
            .replace(Some((pipeline.clone(), bus_watch_guard)));

        let (segment_tx, segment_rx) = async_channel::unbounded();

        let thread_handle = thread::spawn(move || {
            if let Err(err) = run_thread(sample_rx, segment_tx) {
                tracing::debug!("Error from thread: {:?}", err)
            }
        });
        imp.thread_handle.replace(Some(thread_handle));

        let fut_handle = glib::spawn_future_local(clone!(@weak self as obj => async move {
            while let Ok(segment) = segment_rx.recv().await {
                obj.emit_by_name::<()>("transcripted", &[&segment]);
            }
        }));
        imp.fut_handle.replace(Some(fut_handle));

        pipeline.set_state(gst::State::Playing)?;

        Ok(())
    }

    fn handle_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        let imp = self.imp();

        match message.view() {
            gst::MessageView::Eos(..) => {
                tracing::debug!("Received EOS event on bus");

                let pipeline = imp.pipeline.borrow();
                let (pipeline, _) = pipeline.as_ref().unwrap();

                if let Err(err) = pipeline.set_state(gst::State::Null) {
                    tracing::warn!("Failed to set pipeline state to NULL: {:?}", err);
                }

                glib::ControlFlow::Break
            }
            gst::MessageView::Error(err) => {
                tracing::warn!("Error from message bus: {:?}", err);

                let pipeline = imp.pipeline.borrow();
                let (pipeline, _) = pipeline.as_ref().unwrap();

                if let Err(err) = pipeline.set_state(gst::State::Null) {
                    tracing::warn!("Failed to set pipeline state to NULL: {:?}", err);
                }

                glib::ControlFlow::Break
            }
            _ => glib::ControlFlow::Continue,
        }
    }
}

fn run_thread(
    sample_rx: mpsc::Receiver<Vec<u8>>,
    segment_tx: async_channel::Sender<String>,
) -> Result<()> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    let ctx = WhisperContext::new_with_params(MODEL_PATH, WhisperContextParameters::default())?;

    let mut state = ctx.create_state()?;

    let mut accumulated = Vec::with_capacity(SAMPLE_WINDOW_SIZE);

    while let Ok(data) = sample_rx.recv() {
        accumulated.extend(data);

        if accumulated.len() < SAMPLE_WINDOW_SIZE {
            continue;
        }

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_single_segment(true);
        params.set_print_realtime(true);

        let converted = accumulated
            .chunks_exact(2)
            .map(|c| i16::from_ne_bytes([c[0], c[1]]) as f32 / 32768.0)
            .collect::<Vec<_>>();
        state.full(params, &converted)?;

        let num_segments = state.full_n_segments()?;
        for i in 0..num_segments {
            let segment = state.full_get_segment_text(i)?;
            let _ = segment_tx.send_blocking(segment);
        }

        accumulated.clear();
    }

    Ok(())
}

impl Default for Stt {
    fn default() -> Self {
        Self::new()
    }
}
