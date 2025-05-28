//! Implements the Logi Bolt receiver.
//!
//! Bolt can be seen as a successor to the Unifying receiver. Both of them
//! support up to 6 paired devices, but Bolt uses BTLE technology and introduces
//! so-called passkeys for authenticating devices before pairing them.
//!
//! There is little to no public documentation about what registers Bolt support
//! (and they seem to differ quite substantially from registers supported by
//! Unifying and other receivers), so this implementation is based largely on
//! information gathered by looking at other codebases (primarily Solaar) and
//! searching registers by fuzzing them.

use std::sync::Arc;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use super::{RECEIVER_DEVICE_INDEX, ReceiverError};
use crate::{
    channel::HidppChannel,
    event::EventEmitter,
    nibble::U4,
    protocol::v10::{self, Hidpp10Error},
};

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
    /// The underlying HID++ channel.
    chan: Arc<HidppChannel>,

    /// The emitter used to emit events.
    emitter: Arc<EventEmitter<BoltEvent>>,

    /// The handle assigned to the message listener registered via
    /// [`HidppChannel::add_msg_listener`].
    /// This is used to remove the listener when the receiver is dropped.
    msg_listener_hdl: u32,
}

impl BoltReceiver {
    /// Tries to initialize a new [`BoltReceiver`] from a raw HID++ channel.
    ///
    /// If no receiver could be found, or if the vendor and product IDs don't
    /// match the ones of any known Bolt receiver, this function will return
    /// [`ReceiverError::UnknownReceiver`].
    pub fn new(chan: Arc<HidppChannel>) -> Result<Self, ReceiverError> {
        if !BOLT_VPID_PAIRS.contains(&(chan.vendor_id, chan.product_id)) {
            return Err(ReceiverError::UnknownReceiver);
        }

        let emitter = Arc::new(EventEmitter::new());

        let hdl = chan.add_msg_listener({
            let emitter = Arc::clone(&emitter);

            move |raw, matched| {
                if matched {
                    return;
                }

                let v10::Message::Short(header, payload) = v10::Message::from(raw) else {
                    return;
                };

                match header.sub_id {
                    // Device connection
                    0x41 => {
                        let Ok(kind) = BoltDeviceKind::try_from(payload[1] & 0x0f) else {
                            return;
                        };

                        emitter.emit(BoltEvent::DeviceConnection(BoltDeviceConnection {
                            index: header.device_index,
                            kind,
                            encrypted: payload[1] & (1 << 5) != 0,
                            online: payload[1] & (1 << 6) == 0,
                            wpid: u16::from_le_bytes(payload[2..=3].try_into().unwrap()),
                        }));
                    },
                    _ => (),
                }
            }
        });

        Ok(BoltReceiver {
            chan,
            emitter,
            msg_listener_hdl: hdl,
        })
    }

    /// Creates a new listener for receiving Bolt receiver events.
    pub fn listen(&self) -> async_channel::Receiver<BoltEvent> {
        self.emitter.create_receiver()
    }

    /// Counts the amount of devices currently paired to this receiver. The
    /// devices don't have to be online to be included here as pairings are
    /// persistent.
    pub async fn count_pairings(&self) -> Result<u8, ReceiverError> {
        let response = self
            .chan
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
        self.chan
            .write_register(RECEIVER_DEVICE_INDEX, BoltRegister::Connections.into(), [
                0x02, 0x00, 0x00,
            ])
            .await?;

        Ok(())
    }

    /// Provides the unique ID of the receiver.
    pub async fn get_unique_id(&self) -> Result<String, ReceiverError> {
        let response = self
            .chan
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

        Ok(str::from_utf8(&response)
            .map_err(|_| Hidpp10Error::UnsupportedResponse)?
            .to_string())
    }

    /// Provides the pairing information of a specific paired device.
    pub async fn get_device_pairing_information(
        &self,
        device_index: U4,
    ) -> Result<BoltDevicePairingInformation, ReceiverError> {
        let response = self
            .chan
            .read_long_register(RECEIVER_DEVICE_INDEX, BoltRegister::ReceiverInfo.into(), [
                u8::from(BoltInfoSubRegister::DevicePairingInformation) + device_index.to_lo(),
                0x00,
                0x00,
            ])
            .await?;

        Ok(BoltDevicePairingInformation {
            wpid: u16::from_le_bytes(response[2..=3].try_into().unwrap()),
            kind: BoltDeviceKind::try_from(response[1] & 0x0f)
                .map_err(|_| Hidpp10Error::UnsupportedResponse)?,
            encrypted: response[1] & (1 << 5) != 0,
            online: response[1] & (1 << 6) == 0,
            unit_id: response[4..=7].try_into().unwrap(),
        })
    }

    /// Provides the codename of a specific paired device.
    pub async fn get_device_codename(&self, device_index: U4) -> Result<String, ReceiverError> {
        // For device names longer than 13 characters this may need to be called
        // multiple times with different parameters. I don't have a device with
        // such a name to be able to test this.

        let response = self
            .chan
            .read_long_register(RECEIVER_DEVICE_INDEX, BoltRegister::ReceiverInfo.into(), [
                u8::from(BoltInfoSubRegister::DeviceCodename) + device_index.to_lo(),
                0x01,
                0x00,
            ])
            .await?;

        let end_idx = response[2] as usize;
        Ok(str::from_utf8(&response[3..end_idx])
            .map_err(|_| Hidpp10Error::UnsupportedResponse)?
            .to_string())
    }
}

impl Drop for BoltReceiver {
    fn drop(&mut self) {
        self.chan.remove_msg_listener(self.msg_listener_hdl);
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
    pub kind: BoltDeviceKind,

    /// Whether the link to the device is encrypted.
    pub encrypted: bool,

    /// Whether the device is online/reachable.
    pub online: bool,

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

/// Represents an event emitted by a Bolt receiver.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum BoltEvent {
    /// Is emitted whenever a device connects to or disconnects from the
    /// receiver.
    ///
    /// Can be triggered for all paired devices using
    /// [`BoltReceiver::trigger_device_arrival`] to allow easy device
    /// enumeration.
    DeviceConnection(BoltDeviceConnection),
}

/// Represents the data of the [`BoltEvent::DeviceConnection`] event.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub struct BoltDeviceConnection {
    /// The index of the device used to communicate with it.
    pub index: u8,

    /// The kind of the device.
    pub kind: BoltDeviceKind,

    /// Whether the link to the device is encrypted.
    pub encrypted: bool,

    /// Whether the device is online/reachable.
    pub online: bool,

    /// The wireless product ID of the device.
    pub wpid: u16,
}
