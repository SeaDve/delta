use anyhow::{anyhow, Context, Result};
use gst::prelude::*;

pub fn find_default_source() -> Result<gst::Device> {
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
