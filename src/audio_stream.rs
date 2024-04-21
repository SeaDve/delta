use anyhow::{anyhow, ensure, Context, Result};
use futures_util::{AsyncReadExt, AsyncWriteExt};
use gst::prelude::*;
use gtk::{
    gio,
    glib::{self, clone},
};
use libp2p::Stream;

const APPSRC_ELEMENT_NAME: &str = "appsrc";
const APPSINK_ELEMENT_NAME: &str = "appsink";
const PULSESRC_ELEMENT_NAME: &str = "pulsesrc";

pub async fn receive(mut src_stream: Stream) -> Result<()> {
    let pipeline = gst::parse::launch(&format!(
        "appsrc name={} ! oggdemux ! opusdec ! audioconvert ! autoaudiosink",
        APPSRC_ELEMENT_NAME
    ))?
    .downcast::<gst::Pipeline>()
    .unwrap();

    let appsrc = pipeline.by_name(APPSRC_ELEMENT_NAME).unwrap();
    appsrc.set_property("caps", gst::Caps::builder("audio/ogg").build());
    appsrc.set_property_from_str("stream-type", "stream");
    appsrc.set_property("is-live", true);

    let _bus_watch = pipeline
        .bus()
        .unwrap()
        .add_watch(move |_, message| handle_bus_message(message))
        .unwrap();

    pipeline.set_state(gst::State::Playing)?;

    loop {
        let mut raw_buf = vec![0; 16_000];
        let n_bytes = src_stream.read(&mut raw_buf).await?;

        if n_bytes == 0 {
            tracing::debug!("Empty read");
            break;
        }

        let buf = {
            let mut buf = gst::Buffer::with_size(n_bytes)?;
            let buf_mut = buf.get_mut().unwrap();
            buf_mut.append_memory(gst::Memory::from_slice(raw_buf));
            buf_mut.unset_flags(gst::BufferFlags::TAG_MEMORY);
            buf
        };

        appsrc
            .emit_by_name::<gst::FlowReturn>("push-buffer", &[&buf])
            .into_result()?;
    }

    appsrc
        .emit_by_name::<gst::FlowReturn>("end-of-stream", &[])
        .into_result()?;

    Ok(())
}

pub async fn transmit(mut sink_stream: Stream) -> Result<()> {
    let pipeline = gst::parse::launch(&format!(
        "pulsesrc name={} ! audioconvert ! opusenc ! oggmux ! appsink name={}",
        PULSESRC_ELEMENT_NAME, APPSINK_ELEMENT_NAME
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

    let appsink = pipeline.by_name(APPSINK_ELEMENT_NAME).unwrap();
    appsink.set_property("caps", gst::Caps::builder("audio/ogg").build());

    let _bus_watch = pipeline
        .bus()
        .unwrap()
        .add_watch(move |_, message| handle_bus_message(message))
        .unwrap();

    pipeline.set_state(gst::State::Playing)?;

    loop {
        let sample = gio::spawn_blocking(clone!(@strong appsink => move || {
            appsink.emit_by_name::<Option<gst::Sample>>("pull-sample", &[])
        }));

        let Some(sample) = sample.await.unwrap() else {
            tracing::debug!("No sample");
            break;
        };

        let Some(buffer) = sample.buffer() else {
            tracing::debug!("No buffer");
            break;
        };

        sink_stream
            .write_all(buffer.map_readable()?.as_slice())
            .await?;
    }

    sink_stream.close().await?;

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
