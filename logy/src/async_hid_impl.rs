//! Implements HID communication using the `async-hid` crate.

use std::{error::Error, fs::File, io::Read};

use anyhow::{Result, anyhow};
use async_hid::{
    AsyncHidRead,
    AsyncHidWrite,
    Device,
    DeviceId,
    DeviceInfo,
    DeviceReader,
    DeviceWriter,
    HidBackend,
};
use futures_lite::StreamExt;
use hidpp::{
    async_trait,
    channel::{ChannelError, HidppChannel, RawHidChannel},
};
use itertools::Itertools;
use tokio::sync::Mutex;

struct AsyncHidDevice(Mutex<DeviceReader>, Mutex<DeviceWriter>, DeviceInfo);

#[async_trait]
impl RawHidChannel for AsyncHidDevice {
    fn vendor_id(&self) -> u16 {
        self.2.vendor_id
    }

    fn product_id(&self) -> u16 {
        self.2.product_id
    }

    async fn write_report(&self, src: &[u8]) -> Result<usize, Box<dyn Error + Sync + Send>> {
        let mut guard = self.1.lock().await;
        guard.write_output_report(src).await?;
        Ok(src.len())
    }

    async fn read_report(&self, buf: &mut [u8]) -> Result<usize, Box<dyn Error + Sync + Send>> {
        let mut guard = self.0.lock().await;
        Ok(guard.read_input_report(buf).await?)
    }

    fn supports_short_long_hidpp(&self) -> Option<(bool, bool)> {
        None
    }

    async fn get_report_descriptor(
        &self,
        buf: &mut [u8],
    ) -> Result<usize, Box<dyn Error + Sync + Send>> {
        let DeviceId::DevPath(ref path) = self.2.id else {
            return Err(
                anyhow!("report descriptors are currently only supported on Linux")
                    .into_boxed_dyn_error(),
            );
        };

        let descriptor_path = path.join("device/report_descriptor");
        let mut file = File::open(descriptor_path)?;
        Ok(file.read(buf)?)
    }
}

/// Tries to find all [`HidppChannel`]s on the local machine.
pub async fn enumerate_hidpp() -> Result<Vec<HidppChannel>> {
    let hid = HidBackend::default();
    let devices: Vec<Device> = hid
        .enumerate()
        .await?
        .collect::<Vec<Device>>()
        .await
        .into_iter()
        .unique_by(|x| x.id.clone())
        .collect();

    let mut channels = Vec::new();
    for dev in devices.into_iter() {
        let opened = dev.open().await?;

        let channel = match HidppChannel::from_raw_channel(AsyncHidDevice(
            Mutex::new(opened.0),
            Mutex::new(opened.1),
            dev.to_device_info(),
        ))
        .await
        {
            Ok(channel) => channel,
            Err(ChannelError::HidppNotSupported) => continue,
            Err(other) => {
                return Err(
                    anyhow::Error::new(other).context("could not initialize the HID++ channel")
                );
            },
        };
        channels.push(channel);
    }

    Ok(channels)
}
