//! Implements the feature starting with version 0.

use std::sync::Arc;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    channel::HidppChannel,
    feature::{CreatableFeature, Feature},
    nibble::U4,
    protocol::v20::{self, Hidpp20Error},
};

/// Implements the `DeviceTypeAndName` / `0x0005` feature.
///
/// The first version supported by this feature is v0.
#[derive(Clone)]
pub struct DeviceTypeAndNameFeatureV0 {
    /// The underlying HID++ channel.
    chan: Arc<HidppChannel>,

    /// The index of the device to implement the feature for.
    device_index: u8,

    /// The index of the feature in the feature table.
    feature_index: u8,
}

impl CreatableFeature for DeviceTypeAndNameFeatureV0 {
    const ID: u16 = 0x0005;
    const STARTING_VERSION: u8 = 0;

    fn new(chan: Arc<HidppChannel>, device_index: u8, feature_index: u8) -> Self {
        Self {
            chan,
            device_index,
            feature_index,
        }
    }
}

impl Feature for DeviceTypeAndNameFeatureV0 {
}

impl DeviceTypeAndNameFeatureV0 {
    /// Retrieves the amount of characters in the marketing name of the device.
    pub async fn get_device_name_count(&self) -> Result<u8, Hidpp20Error> {
        let response = self
            .chan
            .send_v20(v20::Message::Short(
                v20::MessageHeader {
                    device_index: self.device_index,
                    feature_index: self.feature_index,
                    function_id: U4::from_lo(0),
                    software_id: self.chan.get_sw_id(),
                },
                [0x00, 0x00, 0x00],
            ))
            .await?;

        Ok(response.extend_payload()[0])
    }

    /// Retrieves a chunk of characters of the marketing name of the device,
    /// starting at a specific index (inclusive).
    ///
    /// Depending on the device and channel capabilities, this function will
    /// return at most 3 or 16 characters of the device name.
    ///
    /// Use this function in conjunction with [`Self::get_device_name_count`] to
    /// retrieve the whole device name.\
    /// A convenience wrapper implementing this functionality is provided as
    /// [`Self::get_whole_device_name`].
    pub async fn get_device_name(&self, index: u8) -> Result<Vec<u8>, Hidpp20Error> {
        let response = self
            .chan
            .send_v20(v20::Message::Short(
                v20::MessageHeader {
                    device_index: self.device_index,
                    feature_index: self.feature_index,
                    function_id: U4::from_lo(1),
                    software_id: self.chan.get_sw_id(),
                },
                [index, 0x00, 0x00],
            ))
            .await?;

        match response {
            v20::Message::Long(_, payload) => Ok(payload.to_vec()),
            v20::Message::Short(_, payload) => Ok(payload.to_vec()),
        }
    }

    /// Retrieves the whole marketing name of the device by first calling
    /// [`Self::get_device_name_count`] once and then repeatedly calling
    /// [`Self::get_device_name`] until all characters were received.
    pub async fn get_whole_device_name(&self) -> Result<String, Hidpp20Error> {
        let count = self.get_device_name_count().await?;
        let mut string = String::with_capacity(count as usize);

        let mut len = 0;
        while len < count as usize {
            let part = self.get_device_name(len as u8).await?;
            string.push_str(str::from_utf8(&part).map_err(|_| Hidpp20Error::UnsupportedResponse)?);
            len = string.len();
        }

        Ok(string)
    }

    /// Retrieves the marketing type of the device.
    pub async fn get_device_type(&self) -> Result<DeviceType, Hidpp20Error> {
        let response = self
            .chan
            .send_v20(v20::Message::Short(
                v20::MessageHeader {
                    device_index: self.device_index,
                    feature_index: self.feature_index,
                    function_id: U4::from_lo(2),
                    software_id: self.chan.get_sw_id(),
                },
                [0x00, 0x00, 0x00],
            ))
            .await?;

        DeviceType::try_from(response.extend_payload()[0])
            .map_err(|_| Hidpp20Error::UnsupportedResponse)
    }
}

/// Represents the type of a HID++2.0 device as returned by the
/// [`DeviceTypeAndNameFeatureV0`] feature.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum DeviceType {
    Keyboard = 0,
    RemoteControl = 1,
    Numpad = 2,
    Mouse = 3,
    Trackpad = 4,
    Trackball = 5,
    Presenter = 6,
    Receiver = 7,
    Headset = 8,
    Webcam = 9,
    SteeringWheel = 10,
    Joystick = 11,
    Gamepad = 12,
    Dock = 13,
    Speaker = 14,
    Microphone = 15,
    IlluminationLight = 16,
    ProgrammableController = 17,
    CarSimPedals = 18,
    Adapter = 19,
}
