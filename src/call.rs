use std::{cell::OnceCell, time::Duration};

use anyhow::{ensure, Context, Result};
use gst::prelude::*;
use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use crate::{audio_device, input_stream::InputStream, output_stream::OutputStream, peer::Peer};

const STREAMSRC_ELEMENT_NAME: &str = "giostreamsrc";

const PULSESRC_ELEMENT_NAME: &str = "pulsesrc";
const STREAMSINK_ELEMENT_NAME: &str = "giostreamsink";

const DURATION_SECS_NOTIFTY_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "DeltaCallState")]
pub enum CallState {
    #[default]
    Init,
    Incoming,
    Outgoing,
    Ongoing,
    Ended(CallEndReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallEndReason {
    PeerInAnotherCall,
    PeerRejected,
    PeerMuted,
    Other,
}

mod imp {
    use std::{
        cell::{Cell, RefCell},
        marker::PhantomData,
        time::Instant,
    };

    use gst::bus::BusWatchGuard;

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Call)]
    pub struct Call {
        #[property(get, set, construct_only)]
        pub(super) peer: OnceCell<Peer>,
        #[property(get, set = Self::set_state, explicit_notify)]
        pub(super) state: Cell<CallState>,
        #[property(get = Self::duration_secs)]
        pub(super) duration_secs: PhantomData<u64>,

        pub(super) ongoing_time: Cell<Option<Instant>>,
        pub(super) ongoing_timer_id: RefCell<Option<glib::SourceId>>,

        pub(super) input: RefCell<Option<(InputStream, gst::Pipeline, BusWatchGuard)>>,
        pub(super) output: RefCell<Option<(OutputStream, gst::Pipeline, BusWatchGuard)>>,

        pub(super) input_closed: Cell<bool>,
        pub(super) output_closed: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Call {
        const NAME: &'static str = "DeltaCall";
        type Type = super::Call;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Call {
        fn dispose(&self) {
            let obj = self.obj();

            tracing::debug!("Started to end call on dispose");

            obj.start_end();
        }
    }

    impl Call {
        fn set_state(&self, state: CallState) {
            let obj = self.obj();

            if state == obj.state() {
                return;
            }

            match state {
                CallState::Ongoing => {
                    debug_assert!(self.ongoing_time.get().is_none());

                    self.ongoing_time.set(Some(Instant::now()));

                    let source_id = glib::timeout_add_local_full(
                        DURATION_SECS_NOTIFTY_INTERVAL,
                        glib::Priority::LOW,
                        clone!(
                            #[weak]
                            obj,
                            #[upgrade_or_panic]
                            move || {
                                obj.notify_duration_secs();
                                glib::ControlFlow::Continue
                            }
                        ),
                    );
                    self.ongoing_timer_id.replace(Some(source_id));
                }
                CallState::Ended(_) => {
                    self.ongoing_time.set(None);

                    if let Some(source_id) = self.ongoing_timer_id.take() {
                        source_id.remove();
                    }
                }
                _ => {}
            }

            self.state.set(state);
            obj.notify_state();
        }

        fn duration_secs(&self) -> u64 {
            self.ongoing_time
                .get()
                .map(|start_time| start_time.elapsed().as_secs())
                .unwrap_or(0)
        }
    }
}

glib::wrapper! {
    pub struct Call(ObjectSubclass<imp::Call>);
}

impl Call {
    pub fn new(peer: &Peer) -> Self {
        glib::Object::builder().property("peer", peer).build()
    }

    pub fn start_end(&self) {
        let imp = self.imp();

        if let Some((_, ref pipeline, _)) = *imp.input.borrow() {
            pipeline.send_event(gst::event::Eos::new());

            tracing::debug!("Sent EOS to input pipeline");
        }

        if let Some((_, ref pipeline, _)) = *imp.output.borrow() {
            pipeline.send_event(gst::event::Eos::new());

            tracing::debug!("Sent EOS to output pipeline");
        }
    }

    pub fn set_input_stream(&self, input_stream: InputStream) -> Result<()> {
        let imp = self.imp();

        let pipeline = gst::parse::launch(&format!(
            "giostreamsrc name={STREAMSRC_ELEMENT_NAME} ! matroskademux ! vorbisdec ! audioconvert ! autoaudiosink",
        ))?
        .downcast::<gst::Pipeline>()
        .unwrap();

        let streamsrc = pipeline.by_name(STREAMSRC_ELEMENT_NAME).unwrap();
        streamsrc.set_property("stream", &input_stream);

        let bus_watch_guard = pipeline
            .bus()
            .unwrap()
            .add_watch_local(clone!(
                #[weak(rename_to = obj)]
                self,
                #[upgrade_or_panic]
                move |_, message| obj.handle_input_bus_message(message)
            ))
            .unwrap();

        pipeline.set_state(gst::State::Playing)?;

        let prev_input = imp
            .input
            .replace(Some((input_stream, pipeline, bus_watch_guard)));
        debug_assert!(prev_input.is_none());

        Ok(())
    }

    pub fn set_output_stream(&self, output_stream: OutputStream) -> Result<()> {
        let imp = self.imp();

        let pipeline = gst::parse::launch(&format!(
            "pulsesrc name={PULSESRC_ELEMENT_NAME} ! audioconvert ! vorbisenc ! matroskamux ! giostreamsink name={STREAMSINK_ELEMENT_NAME}",
        ))?
        .downcast::<gst::Pipeline>()
        .unwrap();

        let pulsesrc = pipeline.by_name(PULSESRC_ELEMENT_NAME).unwrap();
        let device = audio_device::find_default_source()?;
        device.reconfigure_element(&pulsesrc)?;

        let device_name = pulsesrc
            .property::<Option<String>>("device")
            .context("No device name")?;
        ensure!(!device_name.is_empty(), "Empty device name");

        tracing::debug!("Using device `{}`", device_name);

        let streamsink = pipeline.by_name(STREAMSINK_ELEMENT_NAME).unwrap();
        streamsink.set_property("stream", &output_stream);

        let bus_watch_guard = pipeline
            .bus()
            .unwrap()
            .add_watch_local(clone!(
                #[weak(rename_to = obj)]
                self,
                #[upgrade_or_panic]
                move |_, message| obj.handle_output_bus_message(message)
            ))
            .unwrap();
        pipeline.set_state(gst::State::Playing)?;

        let prev_output = imp
            .output
            .replace(Some((output_stream, pipeline, bus_watch_guard)));
        debug_assert!(prev_output.is_none());

        Ok(())
    }

    fn handle_input_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        match message.view() {
            gst::MessageView::Eos(..) => {
                tracing::debug!("Received EOS event on input bus");

                glib::spawn_future_local(clone!(
                    #[weak(rename_to = obj)]
                    self,
                    async move {
                        obj.dispose_input().await;
                    }
                ));

                glib::ControlFlow::Break
            }
            gst::MessageView::Error(err) => {
                tracing::warn!("Error from input bus: {:?}", err);

                glib::spawn_future_local(clone!(
                    #[weak(rename_to = obj)]
                    self,
                    async move {
                        obj.dispose_input().await;
                    }
                ));

                glib::ControlFlow::Break
            }
            _ => glib::ControlFlow::Continue,
        }
    }

    fn handle_output_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        match message.view() {
            gst::MessageView::Eos(..) => {
                tracing::debug!("Received EOS event on output bus");

                glib::spawn_future_local(clone!(
                    #[weak(rename_to = obj)]
                    self,
                    async move {
                        obj.dispose_output().await;
                    }
                ));

                glib::ControlFlow::Break
            }
            gst::MessageView::Error(err) => {
                tracing::warn!("Error from output bus: {:?}", err);

                glib::spawn_future_local(clone!(
                    #[weak(rename_to = obj)]
                    self,
                    async move {
                        obj.dispose_output().await;
                    }
                ));

                glib::ControlFlow::Break
            }
            _ => glib::ControlFlow::Continue,
        }
    }

    async fn dispose_input(&self) {
        let imp = self.imp();

        let (input_stream, pipeline, _bus_watch_guard) = imp.input.take().unwrap();

        if let Err(err) = input_stream.close_future(glib::Priority::LOW).await {
            tracing::error!("Failed to close input stream: {:?}", err);
        }

        pipeline.set_state(gst::State::Null).unwrap();

        imp.input_closed.set(true);

        if imp.output_closed.get() {
            self.set_state(CallState::Ended(CallEndReason::Other));
        }
    }

    async fn dispose_output(&self) {
        let imp = self.imp();

        let (output_stream, pipeline, _bus_watch_guard) = imp.output.take().unwrap();

        if let Err(err) = output_stream.close_future(glib::Priority::LOW).await {
            tracing::error!("Failed to close output stream: {:?}", err);
        }

        pipeline.set_state(gst::State::Null).unwrap();

        imp.output_closed.set(true);

        if imp.input_closed.get() {
            self.set_state(CallState::Ended(CallEndReason::Other));
        }
    }
}
