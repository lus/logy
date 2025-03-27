//! Implements peripheral devices connected to HID++ channels.

use std::{error::Error, sync::Arc};

use thiserror::Error;

use crate::{
    channel::{ChannelError, HidppChannel, RawHidChannel},
    protocol::{self, ProtocolError, ProtocolVersion},
};

/// Represents a single HID++ device connected to a [`HidppChannel`].
///
/// This is used only for peripheral devices and not receivers.
pub struct Device<T: RawHidChannel> {
    chan: Arc<HidppChannel<T>>,

    /// The index of the device on the HID++ channel.
    pub device_index: u8,

    /// The supported protocol version reported by the device.
    pub protocol_version: ProtocolVersion,
}

impl<T: RawHidChannel> Device<T> {
    /// Tries to initialize a device on a HID++ channel.
    ///
    /// This will automatically ping the device to determine the protocol
    /// version it supports via [`protocol::determine_version`].
    ///
    /// Returns [`DeviceError::DeviceNotFound`] if there is no device with the
    /// specified index connected to the channel.
    ///
    /// Returns [`DeviceError::UnsupportedProtocolVersion`] if the device only
    /// supports [`ProtocolVersion::V10`].
    pub async fn new(
        chan: Arc<HidppChannel<T>>,
        device_index: u8,
    ) -> Result<Self, DeviceError<T::Error>> {
        let protocol_version = protocol::determine_version(&*chan, device_index)
            .await
            .map_err(|err| match err {
                ProtocolError::Channel(src) => DeviceError::Channel(src),
                ProtocolError::DeviceNotFound => DeviceError::DeviceNotFound,
            })?;

        if let ProtocolVersion::V10 = protocol_version {
            return Err(DeviceError::UnsupportedProtocolVersion);
        }

        Ok(Self {
            chan,
            device_index,
            protocol_version,
        })
    }
}

/// Represents a device-specific error.
#[derive(Debug, Error)]
pub enum DeviceError<T: Error> {
    /// Indicates that the underlying [`HidppChannel`] returned an error.
    #[error("the HID++ channel returned an error")]
    Channel(#[from] ChannelError<T>),

    /// Indicates that the specified device index points to no device.
    #[error("there is no device with the specified device index")]
    DeviceNotFound,

    /// Indicates that the addressed device does only support HID++1.0.
    #[error("the device does not support HID++2.0 or newer")]
    UnsupportedProtocolVersion,
}
