use anyhow::{anyhow, ensure, Context, Result};
use futures_util::future;
use gst::prelude::*;
use gtk::glib;
use libp2p::Stream;

use crate::{input_stream::InputStream, output_stream::OutputStream};

const STREAMSRC_ELEMENT_NAME: &str = "giostreamsrc";

const PULSESRC_ELEMENT_NAME: &str = "pulsesrc";
const STREAMSINK_ELEMENT_NAME: &str = "giostreamsink";

pub async fn receive(src_stream: Stream) -> Result<()> {
    let pipeline = gst::parse::launch(&format!(
        "giostreamsrc name={STREAMSRC_ELEMENT_NAME} ! application/x-rtp ! rtpopusdepay ! opusdec ! audioconvert ! autoaudiosink",
    ))?
    .downcast::<gst::Pipeline>()
    .unwrap();

    let streamsrc = pipeline.by_name(STREAMSRC_ELEMENT_NAME).unwrap();
    streamsrc.set_property("stream", InputStream::new(src_stream));

    let _bus_watch = pipeline
        .bus()
        .unwrap()
        .add_watch(move |_, message| handle_bus_message(message))
        .unwrap();

    pipeline.set_state(gst::State::Playing)?;

    future::pending::<()>().await;

    Ok(())
}

pub async fn transmit(sink_stream: Stream) -> Result<()> {
    let pipeline = gst::parse::launch(&format!(
        "pulsesrc name={PULSESRC_ELEMENT_NAME} ! audioconvert ! opusenc ! rtpopuspay ! giostreamsink name={STREAMSINK_ELEMENT_NAME}",
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
    streamsink.set_property("stream", OutputStream::new(sink_stream));

    let _bus_watch = pipeline
        .bus()
        .unwrap()
        .add_watch(move |_, message| handle_bus_message(message))
        .unwrap();

    pipeline.set_state(gst::State::Playing)?;

    future::pending::<()>().await;

    Ok(())
}

fn handle_bus_message(message: &gst::Message) -> glib::ControlFlow {
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

pub fn find_default_source_device() -> Result<gst::Device> {
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
