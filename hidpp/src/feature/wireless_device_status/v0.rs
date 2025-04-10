//! Implements the feature starting with version 0.

use std::sync::{Arc, Mutex, mpsc};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    channel::HidppChannel,
    feature::{CreatableFeature, EmittingFeature, Feature},
    nibble,
    protocol::v20,
};

/// Implements the `WirelessDeviceStatus` / `0x1d4b` feature.
///
/// The first version supported by this feature is v0.
pub struct WirelessDeviceStatusFeatureV0 {
    /// The underlying HID++ channel.
    chan: Arc<HidppChannel>,

    /// A collection of event listeners added via [`Self::listen`].
    listeners: Arc<Mutex<Vec<mpsc::Sender<WirelessDeviceStatusEvent>>>>,

    /// The handle assigned to the message listener registered via
    /// [`HidppChannel::add_msg_listener`].
    /// This is used to remove the listener when the feature is dropped.
    msg_listener_hdl: u32,
}

impl CreatableFeature for WirelessDeviceStatusFeatureV0 {
    const ID: u16 = 0x1d4b;
    const STARTING_VERSION: u8 = 0;

    fn new(chan: Arc<HidppChannel>, device_index: u8, feature_index: u8) -> Self {
        let listeners_rc = Arc::new(Mutex::new(
            Vec::<mpsc::Sender<WirelessDeviceStatusEvent>>::new(),
        ));

        let hdl = chan.add_msg_listener({
            let listeners = Arc::clone(&listeners_rc);

            move |raw, matched| {
                if matched {
                    return;
                }

                let msg = v20::Message::from(raw);

                let header = msg.header();
                if header.device_index != device_index
                    || header.feature_index != feature_index
                    || nibble::combine(header.software_id, header.function_id) != 0
                {
                    return;
                }

                let payload = msg.extend_payload();
                let Ok(status) = WirelessDeviceStatus::try_from(payload[0]) else {
                    return;
                };
                let Ok(request) = WirelessDeviceStatusRequest::try_from(payload[1]) else {
                    return;
                };
                let Ok(reason) = WirelessDeviceStatusReason::try_from(payload[2]) else {
                    return;
                };

                listeners.lock().unwrap().retain(|listener| {
                    listener
                        .send(WirelessDeviceStatusEvent::StatusBroadcast(
                            WirelessDeviceStatusBroadcast {
                                status,
                                request,
                                reason,
                            },
                        ))
                        .is_ok()
                });
            }
        });

        Self {
            chan,
            listeners: listeners_rc,
            msg_listener_hdl: hdl,
        }
    }
}

impl Feature for WirelessDeviceStatusFeatureV0 {
}

impl EmittingFeature<WirelessDeviceStatusEvent> for WirelessDeviceStatusFeatureV0 {
    fn listen(&self) -> mpsc::Receiver<WirelessDeviceStatusEvent> {
        let (tx, rx) = mpsc::channel::<WirelessDeviceStatusEvent>();
        self.listeners.lock().unwrap().push(tx);
        rx
    }
}

impl Drop for WirelessDeviceStatusFeatureV0 {
    fn drop(&mut self) {
        self.chan.remove_msg_listener(self.msg_listener_hdl);
    }
}

/// Represents an event emitted by the [`WirelessDeviceStatusFeatureV0`]
/// feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WirelessDeviceStatusEvent {
    /// Is emitted whenever a device (re)connects to the host.
    ///
    /// This event is always enabled.
    StatusBroadcast(WirelessDeviceStatusBroadcast),
}

/// Represents the data of the [`WirelessDeviceStatusEvent::StatusBroadcast`]
/// event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct WirelessDeviceStatusBroadcast {
    /// The status the device reports to be in.
    pub status: WirelessDeviceStatus,

    /// The request the devices expresses towards the host.
    pub request: WirelessDeviceStatusRequest,

    /// The reason for the status broadcast.
    pub reason: WirelessDeviceStatusReason,
}

/// Represents a device status as reported in
/// [`WirelessDeviceStatusBroadcastEvent::status`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum WirelessDeviceStatus {
    Unknown = 0x00,
    Reconnection = 0x01,
}

/// Represents a request as reported in
/// [`WirelessDeviceStatusBroadcastEvent::request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum WirelessDeviceStatusRequest {
    NoRequest = 0x00,
    SoftwareReconfigurationNeeded = 0x01,
}

/// Represents a broadcast reason as reported in
/// [`WirelessDeviceStatusBroadcastEvent::reason`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum WirelessDeviceStatusReason {
    Unknown = 0x00,
    PowerSwitchActivated = 0x01,
}
