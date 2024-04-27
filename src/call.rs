use std::cell::OnceCell;

use anyhow::{anyhow, ensure, Context, Result};
use gst::prelude::*;
use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use crate::{input_stream::InputStream, output_stream::OutputStream, peer::Peer};

const STREAMSRC_ELEMENT_NAME: &str = "giostreamsrc";

const PULSESRC_ELEMENT_NAME: &str = "pulsesrc";
const STREAMSINK_ELEMENT_NAME: &str = "giostreamsink";

mod imp {
    use std::cell::RefCell;

    use gst::bus::BusWatchGuard;

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Call)]
    pub struct Call {
        #[property(get, set, construct_only)]
        pub(super) peer: OnceCell<Peer>,

        pub(super) input: RefCell<Option<(InputStream, gst::Pipeline, BusWatchGuard)>>,
        pub(super) output: RefCell<Option<(OutputStream, gst::Pipeline, BusWatchGuard)>>,
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

            glib::spawn_future_local(clone!(@weak obj => async move {
                if let Err(err) = obj.end().await {
                    tracing::error!("Failed to close call on dispose: {}", err);
                }
            }));
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

    pub async fn end(&self) -> Result<()> {
        let imp = self.imp();

        if let Some((input_stream, pipeline, _)) = imp.input.take() {
            if let Err(err) = pipeline.set_state(gst::State::Null) {
                tracing::error!("Failed to set input pipeline to null: {}", err);
            }

            glib::spawn_future_local(async move {
                if let Err(err) = input_stream
                    .close_future(glib::Priority::DEFAULT_IDLE)
                    .await
                {
                    tracing::error!("Failed to close input stream: {}", err);
                }
            });
        }

        if let Some((output_stream, pipeline, _)) = imp.output.take() {
            if let Err(err) = pipeline.set_state(gst::State::Null) {
                tracing::error!("Failed to set output pipeline to null: {}", err);
            }

            glib::spawn_future_local(async move {
                if let Err(err) = output_stream
                    .close_future(glib::Priority::DEFAULT_IDLE)
                    .await
                {
                    tracing::error!("Failed to close output stream: {}", err);
                }
            });
        }

        Ok(())
    }

    pub fn set_input_stream(&self, input_stream: InputStream) -> Result<()> {
        let imp = self.imp();

        let pipeline = gst::parse::launch(&format!(
            "giostreamsrc name={STREAMSRC_ELEMENT_NAME} ! matroskademux ! opusdec ! audioconvert ! autoaudiosink",
        ))?
        .downcast::<gst::Pipeline>()
        .unwrap();

        let streamsrc = pipeline.by_name(STREAMSRC_ELEMENT_NAME).unwrap();
        streamsrc.set_property("stream", &input_stream);

        let bus_watch_guard = pipeline
            .bus()
            .unwrap()
            .add_watch_local(
                clone!(@weak self as obj => @default-panic,move |_, message| {
                    obj.handle_bus_message(message)
                }),
            )
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
            "pulsesrc name={PULSESRC_ELEMENT_NAME} ! audioconvert ! opusenc ! matroskamux ! giostreamsink name={STREAMSINK_ELEMENT_NAME}",
        ))?
        .downcast::<gst::Pipeline>()
        .unwrap();

        let pulsesrc = pipeline.by_name(PULSESRC_ELEMENT_NAME).unwrap();
        let device = find_default_source_device()?;
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
            .add_watch_local(
                clone!(@weak self as obj => @default-panic,move |_, message| {
                    obj.handle_bus_message(message)
                }),
            )
            .unwrap();
        pipeline.set_state(gst::State::Playing)?;

        let prev_output = imp
            .output
            .replace(Some((output_stream, pipeline, bus_watch_guard)));
        debug_assert!(prev_output.is_none());

        Ok(())
    }

    fn handle_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        match message.view() {
            gst::MessageView::Eos(..) => {
                tracing::debug!("End of stream");
                glib::ControlFlow::Break
            }
            gst::MessageView::Error(err) => {
                tracing::debug!("Error from bus: {:?}", err);
                glib::ControlFlow::Break
            }
            _ => glib::ControlFlow::Continue,
        }
    }
}

fn find_default_source_device() -> Result<gst::Device> {
    let provider = gst::DeviceProviderFactory::by_name("pulsedeviceprovider")
        .context("Missing pulseaudio device provider")?;

    provider.start()?;
    let devices = provider.devices();
    provider.stop();

    for device in devices {
        if !device.has_classes("Audio/Source") {
            tracing::debug!(
                "Skipping device `{}` as it has unknown device class `{}`",
                device.name(),
                device.device_class()
            );
            continue;
        }

        let Some(properties) = device.properties() else {
            tracing::warn!(
                "Skipping device `{}` as it has no properties",
                device.name()
            );
            continue;
        };

        let is_default = match properties.get::<bool>("is-default") {
            Ok(is_default) => is_default,
            Err(err) => {
                tracing::warn!(
                    "Skipping device `{}` as it has no `is-default` property: {:?}",
                    device.name(),
                    err
                );
                continue;
            }
        };

        if !is_default {
            tracing::debug!(
                "Skipping device `{}` as it is not the default",
                device.name()
            );
            continue;
        }

        return Ok(device);
    }

    Err(anyhow!("Failed to find a default device"))
}
