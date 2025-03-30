//! Implements the feature starting with version 0.

use std::{cmp::min, sync::Arc};

use crate::{
    channel::{HidppChannel, RawHidChannel},
    feature::{CreatableFeature, Feature},
    nibble::U4,
    protocol::v20::{self, Hidpp20Error},
};

/// Implements the `DeviceTypeAndName` / `0x0005` feature.
///
/// The first version supported by this feature is v0.
#[derive(Clone)]
pub struct DeviceTypeAndNameFeatureV0<T: RawHidChannel> {
    /// The underlying HID++ channel.
    chan: Arc<HidppChannel<T>>,

    /// The index of the device to implement the feature for.
    device_index: u8,

    /// The index of the feature in the feature table.
    feature_index: u8,
}

impl<T: RawHidChannel> CreatableFeature<T> for DeviceTypeAndNameFeatureV0<T> {
    const ID: u16 = 0x0005;
    const STARTING_VERSION: u8 = 0;

    fn new(chan: Arc<HidppChannel<T>>, device_index: u8, feature_index: u8) -> Self {
        Self {
            chan,
            device_index,
            feature_index,
        }
    }
}

impl<T: RawHidChannel> Feature<T> for DeviceTypeAndNameFeatureV0<T> {
}

impl<T: RawHidChannel> DeviceTypeAndNameFeatureV0<T> {
    /// Retrieves the amount of characters in the marketing name of the device.
    pub async fn get_device_name_count(&self) -> Result<u8, Hidpp20Error<T::Error>> {
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
    /// A convenience wrapper implementing this functionality is provided under
    /// [`Self::get_whole_device_name`].
    pub async fn get_device_name(&self, index: u8) -> Result<Vec<u8>, Hidpp20Error<T::Error>> {
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
    pub async fn get_whole_device_name(&self) -> Result<String, Hidpp20Error<T::Error>> {
        let count = self.get_device_name_count().await?;
        let mut string = String::with_capacity(count as usize);

        let mut len = 0;
        while len < count as usize {
            let part = self.get_device_name(len as u8).await?;
            string.push_str(unsafe {
                str::from_utf8_unchecked(&part[..min(part.len(), count as usize - len)])
            });

            len = string.len();
        }

        Ok(string)
    }

    /// Retrieves the marketing type of the device.
    pub async fn get_device_type(&self) -> Result<DeviceType, Hidpp20Error<T::Error>> {
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

        Ok(DeviceType::from(response.extend_payload()[0]))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DeviceType {
    Keyboard,
    RemoteControl,
    Numpad,
    Mouse,
    Trackpad,
    Trackball,
    Presenter,
    Receiver,
    Headset,
    Webcam,
    SteeringWheel,
    Joystick,
    Gamepad,
    Dock,
    Speaker,
    Microphone,
    IlluminationLight,
    ProgrammableController,
    CarSimPedals,
    Adapter,
    Other(u8),
}

impl From<u8> for DeviceType {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Keyboard,
            1 => Self::RemoteControl,
            2 => Self::Numpad,
            3 => Self::Mouse,
            4 => Self::Trackpad,
            5 => Self::Trackball,
            6 => Self::Presenter,
            7 => Self::Receiver,
            8 => Self::Headset,
            9 => Self::Webcam,
            10 => Self::SteeringWheel,
            11 => Self::Joystick,
            12 => Self::Gamepad,
            13 => Self::Dock,
            14 => Self::Speaker,
            15 => Self::Microphone,
            16 => Self::IlluminationLight,
            17 => Self::ProgrammableController,
            18 => Self::CarSimPedals,
            19 => Self::Adapter,
            code => Self::Other(code),
        }
    }
}

impl From<DeviceType> for u8 {
    fn from(value: DeviceType) -> Self {
        match value {
            DeviceType::Keyboard => 0,
            DeviceType::RemoteControl => 1,
            DeviceType::Numpad => 2,
            DeviceType::Mouse => 3,
            DeviceType::Trackpad => 4,
            DeviceType::Trackball => 5,
            DeviceType::Presenter => 6,
            DeviceType::Receiver => 7,
            DeviceType::Headset => 8,
            DeviceType::Webcam => 9,
            DeviceType::SteeringWheel => 10,
            DeviceType::Joystick => 11,
            DeviceType::Gamepad => 12,
            DeviceType::Dock => 13,
            DeviceType::Speaker => 14,
            DeviceType::Microphone => 15,
            DeviceType::IlluminationLight => 16,
            DeviceType::ProgrammableController => 17,
            DeviceType::CarSimPedals => 18,
            DeviceType::Adapter => 19,
            DeviceType::Other(code) => code,
        }
    }
}
