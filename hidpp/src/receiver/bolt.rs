//! Implements the Logi Bolt receiver.

use std::sync::Arc;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use super::{RECEIVER_DEVICE_INDEX, ReceiverError};
use crate::{channel::HidppChannel, nibble::U4, protocol::v10::Hidpp10Error};

/// Contains all known USB vendor and product ID pairs representing Bolt
/// receivers.
pub const BOLT_VPID_PAIRS: &[(u16, u16)] = &[(0x046d, 0xc548)];

/// Represents the known registers of the Bolt receiver.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum BoltRegister {
    /// Provides information about the amount of currently paired devices.
    ///
    /// This count is exposed by [`BoltReceiver::count_pairings`].
    Connections = 0x02,

    /// Provides information about the receiver and paired devices. It uses
    /// sub-registers, as defined in [`BoltInfoSubRegister`], to differentiate
    /// between different kinds of information.
    ReceiverInfo = 0xb5,

    /// Provides the unique ID of the receiver.
    ///
    /// Exposed by [`BoltReceiver::get_unique_id`].
    UniqueId = 0xfb,
}

/// Represents the known sub-registers of the [`BoltRegister::ReceiverInfo`]
/// register.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum BoltInfoSubRegister {
    /// Provides information about a specific paired device.
    ///
    /// Exposed by [`BoltReceiver::get_device_pairing_information`].
    DevicePairingInformation = 0x50, // 0x5N with N = device index

    /// Provides the name of a paired device.
    ///
    /// Exposed by [`BoltReceiver::get_device_codename`].
    DeviceCodename = 0x60, // 0x6N with N = device index
}

/// Implements the Bolt wireless receiver.
#[derive(Clone)]
pub struct BoltReceiver {
    channel: Arc<HidppChannel>,
}

impl BoltReceiver {
    /// Tries to initialize a new [`BoltReceiver`] from a raw HID++ channel.
    ///
    /// If no receiver could be found, or if the vendor and product IDs don't
    /// match the ones of any known Bolt receiver, this function will return
    /// [`ReceiverError::UnknownReceiver`].
    pub fn new(channel: Arc<HidppChannel>) -> Result<Self, ReceiverError> {
        if !BOLT_VPID_PAIRS.contains(&(channel.vendor_id, channel.product_id)) {
            return Err(ReceiverError::UnknownReceiver);
        }

        Ok(BoltReceiver {
            channel,
        })
    }

    /// Counts the amount of devices currently paired to this receiver. The
    /// devices don't have to be online to be included here as pairings are
    /// persistent.
    pub async fn count_pairings(&self) -> Result<u8, ReceiverError> {
        let response = self
            .channel
            .read_register(
                RECEIVER_DEVICE_INDEX,
                BoltRegister::Connections.into(),
                [0u8; 3],
            )
            .await?;

        Ok(response[1])
    }

    /// Triggers device arrival notifications for all devices currently
    /// connected to the receiver. This is useful for device enumeration.
    pub async fn trigger_device_arrival(&self) -> Result<(), ReceiverError> {
        self.channel
            .write_register(RECEIVER_DEVICE_INDEX, BoltRegister::Connections.into(), [
                0x02, 0x00, 0x00,
            ])
            .await?;

        Ok(())
    }

    /// Provides the unique ID of the receiver.
    pub async fn get_unique_id(&self) -> Result<String, ReceiverError> {
        let response = self
            .channel
            .read_long_register(
                RECEIVER_DEVICE_INDEX,
                BoltRegister::UniqueId.into(),
                [0u8; 3],
            )
            .await?;

        // When decoding the last 8 bytes of the response to their ASCII representation
        // we seem to get a valid hex string representing 4 bytes of data.
        // Interpreting this hex string as little endian we seem to get the same decimal
        // value the Options+ software calls `udid` (unique device identifier?). I am
        // not sure what this is about and it may be a (major) coincidence that these
        // values match for my receiver, but it could be worth keeping this in mind.

        // I have no clue how to retrieve the serial number of the receiver.

        Ok(core::str::from_utf8(&response)
            .map_err(|_| Hidpp10Error::UnsupportedResponse)?
            .to_string())
    }

    /// Provides the pairing information of a specific paired device.
    pub async fn get_device_pairing_information(
        &self,
        device_index: U4,
    ) -> Result<BoltDevicePairingInformation, ReceiverError> {
        let response = self
            .channel
            .read_long_register(RECEIVER_DEVICE_INDEX, BoltRegister::ReceiverInfo.into(), [
                u8::from(BoltInfoSubRegister::DevicePairingInformation) + device_index.to_lo(),
                0x00,
                0x00,
            ])
            .await?;

        // response[1] contains the device kind (0x02 in my case), but it seems to
        // contain different values if the device is offline.
        // If my mouse is offline, it contains 0x42. I suspect that the device kind is
        // set only in the 4 rightmost bits and the 4 leftmost ones represent some kind
        // of device status, but I'd need more data for different device kinds to
        // verify this.

        // I unfortunately couldn't find out what the remaining response bytes are for.

        Ok(BoltDevicePairingInformation {
            wpid: u16::from_le_bytes(response[2..=3].try_into().unwrap()),
            kind: BoltDeviceKind::try_from(response[1] & 0x0f)
                .map_err(|_| Hidpp10Error::UnsupportedResponse)?,
            unit_id: response[4..=7].try_into().unwrap(),
        })
    }

    /// Provides the codename of a specific paired device.
    pub async fn get_device_codename(&self, device_index: U4) -> Result<String, ReceiverError> {
        // For device names longer than 13 characters this may need to be called
        // multiple times with different parameters. I don't have a device with
        // such a name to be able to test this.

        let response = self
            .channel
            .read_long_register(RECEIVER_DEVICE_INDEX, BoltRegister::ReceiverInfo.into(), [
                u8::from(BoltInfoSubRegister::DeviceCodename) + device_index.to_lo(),
                0x01,
                0x00,
            ])
            .await?;

        let end_idx = response[2] as usize;
        Ok(core::str::from_utf8(&response[3..end_idx])
            .map_err(|_| Hidpp10Error::UnsupportedResponse)?
            .to_string())
    }
}

/// Represents some information about a specific device pairing as returned by
/// [`BoltReceiver::get_device_pairing_information`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub struct BoltDevicePairingInformation {
    /// The wireless product ID of the device.
    wpid: u16,

    /// The kind of the device.
    kind: BoltDeviceKind,

    /// The unit ID of the device.
    unit_id: [u8; 4],
}

/// Represents the kind of a device paired with a Bolt receiver.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum BoltDeviceKind {
    Unknown = 0x00,
    Keyboard = 0x01,
    Mouse = 0x02,
    Numpad = 0x03,
    Presenter = 0x04,
    Remote = 0x07,
    Trackball = 0x08,
    Touchpad = 0x09,
    Tablet = 0x0a,
    Gamepad = 0x0b,
    Joystick = 0x0c,
    Headset = 0x0d,
}
