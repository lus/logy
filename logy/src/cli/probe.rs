use std::{
    io::{BufWriter, Write},
    sync::Arc,
};

use anyhow::Result;
use clap::Args;
use hidpp::{
    channel::HidppChannel,
    device::Device,
    feature::{
        device_friendly_name::DeviceFriendlyNameFeature,
        device_information::DeviceInformationFeature,
        device_type_and_name::{DeviceType, DeviceTypeAndNameFeature},
        unified_battery::{BatteryLevel, BatteryStatus, UnifiedBatteryFeature},
    },
    receiver,
};
use owo_colors::OwoColorize;
use serde::Serialize;
use serde_json::json;

use super::Cli;
use crate::{
    async_hid_impl::enumerate_hidpp,
    hidpp_ext::receiver::{LogyReceiver, PairedDeviceKind},
};

/// Detect and view general information about connected devices.
#[derive(Args)]
pub struct ProbeCommand {}

impl ProbeCommand {
    pub async fn execute(&self, root: &Cli) -> Result<()> {
        let mut stdout = BufWriter::new(anstream::stdout());

        let receivers = probe_receivers().await?;

        if root.json {
            writeln!(stdout, "{}", json!(receivers)).unwrap();
            return Ok(());
        }

        if receivers.is_empty() {
            writeln!(stdout, "{}", "No HID++ devices were found.".bright_black()).unwrap();
            return Ok(());
        }

        for (receiver_i, receiver) in receivers.into_iter().enumerate() {
            if receiver_i != 0 {
                writeln!(stdout).unwrap();
            }

            writeln!(
                stdout,
                "{}: {} ({:#06x}:{:#06x})",
                receiver.unique_id.bright_black(),
                receiver.name,
                receiver.vendor_id.bright_black(),
                receiver.product_id.bright_black()
            )
            .unwrap();
            writeln!(stdout, " │").unwrap();

            if receiver.paired_devices.is_empty() {
                writeln!(
                    stdout,
                    " ╰─ {}",
                    "No devices were found.".bright_black().italic()
                )
                .unwrap();
                return Ok(());
            }

            let devices_len = receiver.paired_devices.len();
            for (device_i, device) in receiver.paired_devices.into_iter().enumerate() {
                if device_i != 0 {
                    writeln!(stdout, " │").unwrap();
                }

                writeln!(
                    stdout,
                    "{} {}: {} {} ({:?}) ({:#06x})",
                    if device_i == devices_len - 1 {
                        " ╰─"
                    } else {
                        " ├─"
                    },
                    device.slot.bright_blue(),
                    if device.online {
                        "●".green().into_styled()
                    } else {
                        "●".red().into_styled()
                    },
                    if device.online {
                        device.name
                    } else {
                        device.name.bright_black().italic().to_string()
                    },
                    device.kind.green(),
                    device.wpid.bright_black(),
                )
                .unwrap();

                if !device.online {
                    continue;
                }

                let mut properties = Vec::new();
                if let Some(kind) = device.properties.kind {
                    properties.push(format!("TYPE: {:?}", kind.bright_black()));
                }
                if let Some(full_name) = device.properties.full_name {
                    properties.push(format!("FULL NAME: {}", full_name.bright_black()));
                }
                if let Some(friendly_name) = device.properties.friendly_name {
                    properties.push(format!("FRIENDLY NAME: {}", friendly_name.bright_black()));
                }
                if let Some(battery_percentage) = device.properties.battery_percentage {
                    if let Some(battery_level) = device.properties.battery_level {
                        if let Some(battery_status) = device.properties.battery_status {
                            properties.push(format!(
                                "BATTERY: {:?} ({}), {:?}",
                                match battery_level {
                                    BatteryLevel::Full | BatteryLevel::Good =>
                                        battery_level.green().into_styled(),
                                    BatteryLevel::Low => battery_level.yellow().into_styled(),
                                    BatteryLevel::Critical =>
                                        battery_level.bright_red().into_styled(),
                                    _ => battery_level.default_color().into_styled(),
                                },
                                format!("{}%", battery_percentage).blue(),
                                battery_status.bright_black()
                            ));
                        }
                    }
                }
                if let Some(serial_number) = device.properties.serial_number {
                    properties.push(format!("SERIAL NUMBER: {}", serial_number.bright_black()));
                }

                let properties_len = properties.len();
                for (propery_i, property) in properties.into_iter().enumerate() {
                    writeln!(
                        stdout,
                        "{}{} {}",
                        if device_i == devices_len - 1 {
                            "         "
                        } else {
                            " │       "
                        },
                        if propery_i == properties_len - 1 {
                            "╰─"
                        } else {
                            "├─"
                        },
                        property
                    )
                    .unwrap();
                }
            }
        }

        stdout.flush().unwrap();

        Ok(())
    }
}

async fn probe_receivers() -> Result<Vec<ProbedReceiver>> {
    let channels: Vec<Arc<HidppChannel>> =
        enumerate_hidpp().await?.into_iter().map(Arc::new).collect();

    let mut receivers = Vec::with_capacity(channels.len());
    for channel in channels {
        let Some(receiver) = receiver::detect(Arc::clone(&channel)) else {
            continue;
        };

        let mut paired_devices = receiver.get_paired_devices().await?;
        paired_devices.sort_by_key(|x| x.slot);

        let mut probed_devices = Vec::with_capacity(paired_devices.len());
        for device in paired_devices {
            let properties = if device.online {
                let mut dev = Device::new(Arc::clone(&channel), device.slot).await?;
                dev.enumerate_features().await?;
                probe_properties(dev).await?
            } else {
                ProbedDeviceProperties::default()
            };

            let name = receiver.get_paired_device_name(device.slot).await?;

            probed_devices.push(ProbedPairedDevice {
                slot: device.slot,
                name,
                kind: device.kind,
                wpid: device.wpid,
                online: device.online,
                properties,
            });
        }

        receivers.push(ProbedReceiver {
            name: receiver.name(),
            unique_id: receiver.get_unique_id().await?,
            vendor_id: channel.vendor_id,
            product_id: channel.product_id,
            paired_devices: probed_devices,
        });
    }

    Ok(receivers)
}

async fn probe_properties(device: Device) -> Result<ProbedDeviceProperties> {
    let mut properties = ProbedDeviceProperties::default();

    if let Some(feature) = device.get_feature::<DeviceTypeAndNameFeature>() {
        properties.kind.replace(feature.get_device_type().await?);
        properties
            .full_name
            .replace(feature.get_whole_device_name().await?);
    }

    if let Some(feature) = device.get_feature::<DeviceFriendlyNameFeature>() {
        let default_friendly_name = feature.get_whole_default_friendly_name().await?;
        let friendly_name = feature.get_whole_friendly_name().await?;

        if default_friendly_name != friendly_name {
            properties.friendly_name.replace(friendly_name);
        }
    }

    if let Some(feature) = device.get_feature::<UnifiedBatteryFeature>() {
        let battery = feature.get_battery_info().await?;
        properties
            .battery_percentage
            .replace(battery.charging_percentage);
        properties.battery_level.replace(battery.level);
        properties.battery_status.replace(battery.status);
    }

    if let Some(feature) = device.get_feature::<DeviceInformationFeature>() {
        let info = feature.get_device_info().await?;

        if info.capabilities.serial_number {
            properties
                .serial_number
                .replace(feature.get_serial_number().await?);
        }
    }

    Ok(properties)
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
struct ProbedReceiver {
    name: String,
    unique_id: String,
    vendor_id: u16,
    product_id: u16,
    paired_devices: Vec<ProbedPairedDevice>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
struct ProbedPairedDevice {
    slot: u8,
    name: String,
    kind: PairedDeviceKind,
    wpid: u16,
    online: bool,
    properties: ProbedDeviceProperties,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize)]
struct ProbedDeviceProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<DeviceType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    full_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    friendly_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    battery_percentage: Option<u8>,

    #[serde(skip_serializing_if = "Option::is_none")]
    battery_level: Option<BatteryLevel>,

    #[serde(skip_serializing_if = "Option::is_none")]
    battery_status: Option<BatteryStatus>,

    #[serde(skip_serializing_if = "Option::is_none")]
    serial_number: Option<String>,
}
