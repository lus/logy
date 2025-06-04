//! Implements the Logi Bolt receiver.
//!
//! Bolt can be seen as a successor to the Unifying receiver. Both of them
//! support up to 6 paired devices, but Bolt uses BTLE technology and introduces
//! so-called passkeys for authenticating devices before pairing them.
//!
//! There is little to no public documentation about what registers Bolt
//! supports (and they seem to differ quite substantially from registers
//! supported by Unifying and other receivers), so this implementation is based
//! largely on information gathered by looking at other codebases (primarily
//! Solaar) and searching registers by fuzzing them.

use std::sync::Arc;

use futures::{FutureExt, pin_mut, select};
use num_enum::{IntoPrimitive, TryFromPrimitive};

use super::{RECEIVER_DEVICE_INDEX, ReceiverError};
use crate::{
    channel::HidppChannel,
    event::EventEmitter,
    protocol::v10::{self, Hidpp10Error},
};

/// Contains all known USB vendor and product ID pairs representing Bolt
/// receivers.
pub const BOLT_VPID_PAIRS: &[(u16, u16)] = &[(0x046d, 0xc548)];

/// Represents the known registers of the Bolt receiver.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

    /// Provides support for discovering devices that are ready to pair.
    DeviceDiscovery = 0xc0,

    /// Provides pairing and unpairing support.
    Pairing = 0xc1,

    /// Provides the unique ID of the receiver.
    ///
    /// Exposed by [`BoltReceiver::get_unique_id`].
    UniqueId = 0xfb,
}

/// Represents the known sub-registers of the [`BoltRegister::ReceiverInfo`]
/// register.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

                let parsed = v10::Message::from(raw);
                let header = parsed.header();
                let payload = parsed.extend_payload();

                if header.device_index != RECEIVER_DEVICE_INDEX && header.sub_id != 0x41 {
                    return;
                }

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
                    // Device discovery
                    0x4f => {
                        match payload[2] {
                            // Device data
                            0 => {
                                let Ok(kind) = BoltDeviceKind::try_from(payload[4] & 0x0f) else {
                                    return;
                                };

                                // I have no idea what payload[2]. payload[12], payload[13] and
                                // payload[15] contain.
                                // Last one seems to change marginally, maybe some kind of
                                // connection metric?

                                emitter.emit(BoltEvent::DeviceDiscoveryDeviceDetails(
                                    BoltDeviceDiscoveryDeviceDetails {
                                        counter: payload[0] as u16 + payload[1] as u16 * 256,
                                        kind,
                                        wpid: u16::from_le_bytes(
                                            payload[5..=6].try_into().unwrap(),
                                        ),
                                        address: payload[7..=12].try_into().unwrap(),
                                        authentication: payload[15],
                                    },
                                ));
                            },
                            // Device name
                            1 => {
                                let Ok(name) =
                                    str::from_utf8(&payload[4..(4 + payload[3] as usize)])
                                else {
                                    return;
                                };

                                emitter.emit(BoltEvent::DeviceDiscoveryDeviceName(
                                    BoltDeviceDiscoveryDeviceName {
                                        counter: payload[0] as u16 + payload[1] as u16 * 256,
                                        name: name.to_string(),
                                    },
                                ));
                            },
                            _ => (),
                        }
                    },
                    // Device discovery status
                    0x53 => {
                        emitter.emit(BoltEvent::DeviceDiscoveryStatus(
                            BoltDeviceDiscoveryStatus {
                                discovery_enabled: payload[0] == 0x00,
                            },
                        ));
                    },
                    // Pairing status
                    0x54 => {
                        // payload[0] contains some kind of information about the status. I don't
                        // know how to map that though.

                        let error = if payload[1] == 0x00 {
                            None
                        } else {
                            let Ok(parsed) = BoltPairingError::try_from(payload[1]) else {
                                return;
                            };

                            Some(parsed)
                        };

                        emitter.emit(BoltEvent::PairingStatus(BoltPairingStatus {
                            device_address: payload[2..=7].try_into().unwrap(),
                            pairing_error: error,
                            slot: if payload[8] == 0x00 {
                                None
                            } else {
                                Some(payload[8])
                            },
                        }));
                    },
                    // Passkey request
                    0x4d => {
                        let Ok(passkey) = str::from_utf8(&payload[1..=6]) else {
                            return;
                        };

                        emitter.emit(BoltEvent::PairingPasskeyRequest(
                            BoltPairingPasskeyRequest {
                                device_address: payload[7..=12].try_into().unwrap(),
                                passkey: passkey.to_string(),
                            },
                        ));
                    },
                    // Passkey pressed
                    0x4e => {
                        let Ok(press_type) = BoltPairingPasskeyPressType::try_from(payload[0])
                        else {
                            return;
                        };

                        emitter.emit(BoltEvent::PairingPasskeyPressed(
                            BoltPairingPasskeyPressed {
                                device_address: payload[1..=6].try_into().unwrap(),
                                press_type,
                            },
                        ));
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

    /// Collects information about all paired devices by calling
    /// [`Self::trigger_device_arrival`] and collecting incoming
    /// [`BoltEvent::DeviceConnection`] events.
    pub async fn collect_paired_devices(&self) -> Result<Vec<BoltDeviceConnection>, ReceiverError> {
        // The idea here is that, when triggering fake device arrival notifications, the
        // receiver will send the register write confirmation message only AFTER sending
        // all arrival notifications.
        // So we will trigger device arrival notifications and continue collecting those
        // until the original future has completed.

        let mut devices = vec![];

        let rx = self.listen();
        let fin = self.trigger_device_arrival().fuse();
        pin_mut!(fin);

        loop {
            select! {
                _ = fin => break,
                res = rx.recv().fuse() => {
                    let Ok(BoltEvent::DeviceConnection(connection)) = res else {
                        continue;
                    };

                    devices.push(connection);
                }
            }
        }

        Ok(devices)
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
        device_index: u8,
    ) -> Result<BoltDevicePairingInformation, ReceiverError> {
        let response = self
            .chan
            .read_long_register(RECEIVER_DEVICE_INDEX, BoltRegister::ReceiverInfo.into(), [
                u8::from(BoltInfoSubRegister::DevicePairingInformation) + (device_index & 0x0f),
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
    pub async fn get_device_codename(&self, device_index: u8) -> Result<String, ReceiverError> {
        // For device names longer than 13 characters this may need to be called
        // multiple times with different parameters. I don't have a device with
        // such a name to be able to test this.

        let response = self
            .chan
            .read_long_register(RECEIVER_DEVICE_INDEX, BoltRegister::ReceiverInfo.into(), [
                u8::from(BoltInfoSubRegister::DeviceCodename) + (device_index & 0x0f),
                0x01,
                0x00,
            ])
            .await?;

        let end_idx = 3 + response[2] as usize;
        Ok(str::from_utf8(&response[3..end_idx])
            .map_err(|_| Hidpp10Error::UnsupportedResponse)?
            .to_string())
    }

    /// Unpairs a device from the receiver by its index.
    pub async fn unpair_device(&self, device_index: u8) -> Result<(), ReceiverError> {
        let mut payload = [0u8; 16];
        payload[0] = 0x03;
        payload[1] = device_index;

        self.chan
            .write_long_register(RECEIVER_DEVICE_INDEX, BoltRegister::Pairing.into(), payload)
            .await?;

        Ok(())
    }

    /// Starts the pairing process for a new device.
    ///
    /// The required `address` and `authentication` values are usually
    /// discovered from the [`BoltEvent::DeviceDiscoveryDeviceDetails`]
    /// event which is emitted regularly when actively discovering available
    /// devices ([`Self::discover_devices`]).
    ///
    /// `entropy` specifies how complex the authentication passkey should be.
    /// For mice this defines the amount of keypresses (left or right) the user
    /// has to perform. Not all values seem to be supported.
    pub async fn pair_device(
        &self,
        slot: u8,
        address: [u8; 6],
        authentication: u8,
        entropy: u8,
    ) -> Result<(), ReceiverError> {
        let mut payload = [0u8; 16];
        payload[0] = 0x01;
        payload[1] = slot;
        payload[2..=7].copy_from_slice(&address);
        payload[8] = authentication;
        payload[9] = entropy;

        self.chan
            .write_long_register(RECEIVER_DEVICE_INDEX, BoltRegister::Pairing.into(), payload)
            .await?;

        Ok(())
    }

    /// Starts device discovery for `timeout` ([`None`] = default, seems to be
    /// 30s) seconds. The maximum supported value is 60s.
    ///
    /// While device discovery is enabled,
    /// [`BoltEvent::DeviceDiscoveryDeviceDetails`] and
    /// [`BoltEvent::DeviceDiscoveryDeviceName`] events are emitted for every
    /// discovered device.
    pub async fn discover_devices(&self, timeout: Option<u8>) -> Result<(), ReceiverError> {
        self.chan
            .write_register(
                RECEIVER_DEVICE_INDEX,
                BoltRegister::DeviceDiscovery.into(),
                [timeout.unwrap_or(0x00), 0x01, 0x00],
            )
            .await?;

        Ok(())
    }

    /// Cancels the device discovery process.
    pub async fn cancel_device_discovery(&self) -> Result<(), ReceiverError> {
        self.chan
            .write_register(
                RECEIVER_DEVICE_INDEX,
                BoltRegister::DeviceDiscovery.into(),
                [0x00, 0x02, 0x00],
            )
            .await?;

        Ok(())
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BoltDevicePairingInformation {
    /// The wireless product ID of the device.
    pub wpid: u16,

    /// The kind of the device.
    pub kind: BoltDeviceKind,

    /// Whether the link to the device is encrypted.
    pub encrypted: bool,

    /// Whether the device is online/reachable.
    pub online: bool,

    /// The unit ID of the device.
    pub unit_id: [u8; 4],
}

/// Represents the kind of a device paired with a Bolt receiver.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum BoltEvent {
    /// Is emitted whenever a device connects to or disconnects from the
    /// receiver.
    ///
    /// Can be triggered for all paired devices using
    /// [`BoltReceiver::trigger_device_arrival`] to allow easy device
    /// enumeration.
    DeviceConnection(BoltDeviceConnection),

    /// Is emitted whenever the device discovery status changes.
    DeviceDiscoveryStatus(BoltDeviceDiscoveryStatus),

    /// Is emitted many times for every device discovered using
    /// [`BoltReceiver::discover_devices`].
    ///
    /// This event contains device details, including its address required to
    /// start pairing. The [`BoltEvent::DeviceDiscoveryDeviceName`] event will
    /// also be emitted and contains the device name.
    DeviceDiscoveryDeviceDetails(BoltDeviceDiscoveryDeviceDetails),

    /// Is emitted many times for every device discovered using
    /// [`BoltReceiver::discover_devices`].
    ///
    /// This event only contains the device name. Device details will be
    /// provided using the [`BoltEvent::DeviceDiscoveryDeviceDetails`] event.
    DeviceDiscoveryDeviceName(BoltDeviceDiscoveryDeviceName),

    /// Is emitted whenever the status of a pairing process changes.
    PairingStatus(BoltPairingStatus),

    /// Is emitted once the receiver requests a passkey to be entered on a
    /// device that should be paired to it.
    PairingPasskeyRequest(BoltPairingPasskeyRequest),

    /// Is emitted for every keypress a user performs while entering a pairing
    /// passkey.
    PairingPasskeyPressed(BoltPairingPasskeyPressed),
}

/// Represents the data of the [`BoltEvent::DeviceConnection`] event.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

/// Represents the data of the [`BoltEvent::DeviceDiscoveryStatus`] event.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BoltDeviceDiscoveryStatus {
    /// Whether device discovery is enabled.
    pub discovery_enabled: bool,
}

/// Represents the data of the [`BoltEvent::DeviceDiscoveryDeviceDetails`]
/// event.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BoltDeviceDiscoveryDeviceDetails {
    /// The incrementing event counter. This can be used to map
    /// [`BoltEvent::DeviceDiscoveryDeviceDetails`] and
    /// [`BoltEvent::DeviceDiscoveryDeviceName`] events.
    pub counter: u16,

    /// The kind of the discovered device.
    pub kind: BoltDeviceKind,

    /// The wireless product ID of the device.
    pub wpid: u16,

    /// The address of the device required to pair it using
    /// [`BoltReceiver::pair_device`].
    ///
    /// This can also be used as the unique device identifier when collecting
    /// discovered devices.
    pub address: [u8; 6],

    /// The authentication type(s) the device supports. Unfortunately, there is
    /// not much information about this value and whether it is a single value
    /// or a bitfield.
    pub authentication: u8,
}

/// Represents the data of the [`BoltEvent::DeviceDiscoveryDeviceName`] event.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BoltDeviceDiscoveryDeviceName {
    /// The incrementing event counter. This can be used to map
    /// [`BoltEvent::DeviceDiscoveryDeviceDetails`] and
    /// [`BoltEvent::DeviceDiscoveryDeviceName`] events.
    pub counter: u16,

    /// The name of the discovered device.
    pub name: String,
}

/// Represents the data of the [`BoltEvent::PairingStatus`] event.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BoltPairingStatus {
    /// The address of the device,
    pub device_address: [u8; 6],

    /// The error that occurred while trying to pair the device.
    pub pairing_error: Option<BoltPairingError>,

    /// The slot of the newly paired device.
    pub slot: Option<u8>,
}

/// Represents an error that occurred while pairing a device.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, TryFromPrimitive, IntoPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
#[repr(u8)]
pub enum BoltPairingError {
    DeviceTimeout = 0x01,
    Failed = 0x02,
}

/// Represents the data of the [`BoltEvent::PairingPasskeyRequest`] event.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BoltPairingPasskeyRequest {
    /// The address of the device.
    pub device_address: [u8; 6],

    /// The passkey the user has to enter in order to pair the device.
    ///
    /// Depending on the device and authentication type, this value has
    /// different implications.
    ///
    /// For mice, this value will be a valid 6-digit number. After parsing this
    /// into an integer, the (least significant) bits represent the sequence
    /// of mouse presses (`0` = left, `1` = right) the user has to perform,
    /// with an additional press of both mouse buttons simultaneously.\
    /// The amount of bits significant to this equals to the `entropy` passed to
    /// [`BoltReceiver::pair_device`].
    pub passkey: String,
}

/// Represents the data of the [`BoltEvent::PairingPasskeyPressed`] event.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BoltPairingPasskeyPressed {
    /// The address of the device.
    pub device_address: [u8; 6],

    /// The type of the keypress the user performed.
    ///
    /// Every passkey sequence starts with an event where this value is set to
    /// [`BoltPairingPasskeyPressType::Initialization`]. Each time the user
    /// presses a key, an event with a press type of
    /// [`BoltPairingPasskeyPressType::Keypress`] is emitted. Once the user
    /// submits their passkey, this value will be
    /// [`BoltPairingPasskeyPressType::Submit`].
    pub press_type: BoltPairingPasskeyPressType,
}

/// The type of a passkey keypress as included in the
/// [`BoltPairingPasskeyPressed`] event data.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, TryFromPrimitive, IntoPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
#[repr(u8)]
pub enum BoltPairingPasskeyPressType {
    Initialization = 0x00,
    Keypress = 0x01,
    Submit = 0x04,
}
