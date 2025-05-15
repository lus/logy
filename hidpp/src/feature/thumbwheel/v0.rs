//! Implements the feature starting with version 0.

use std::sync::Arc;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    channel::HidppChannel,
    event::EventEmitter,
    feature::{CreatableFeature, EmittingFeature, Feature},
    nibble::{self, U4},
    protocol::v20::{self, Hidpp20Error},
};

/// Implements the `Thumbwheel` / `0x2150` feature.
///
/// The first version supported by this feature is v0.
pub struct ThumbwheelFeatureV0 {
    /// The underlying HID++ channel.
    chan: Arc<HidppChannel>,

    /// The index of the device to implement the feature for.
    device_index: u8,

    /// The index of the feature in the feature table.
    feature_index: u8,

    /// The emitter used to emit events.
    emitter: Arc<EventEmitter<ThumbwheelEvent>>,

    /// The handle assigned to the message listener registered via
    /// [`HidppChannel::add_msg_listener`].
    /// This is used to remove the listener when the feature is dropped.
    msg_listener_hdl: u32,
}

impl CreatableFeature for ThumbwheelFeatureV0 {
    const ID: u16 = 0x2150;
    const STARTING_VERSION: u8 = 0;

    fn new(chan: Arc<HidppChannel>, device_index: u8, feature_index: u8) -> Self {
        let emitter = Arc::new(EventEmitter::new());

        let hdl = chan.add_msg_listener({
            let emitter = Arc::clone(&emitter);

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
                let Ok(rotation_status) = ThumbwheelRotationStatus::try_from(payload[4]) else {
                    return;
                };

                emitter.emit(ThumbwheelEvent::StatusUpdate(ThumbwheelStatusUpdate {
                    rotation: i16::from_be_bytes(payload[0..=1].try_into().unwrap()),
                    time_elapsed: u16::from_be_bytes(payload[2..=3].try_into().unwrap()),
                    rotation_status,
                    touch: payload[5] & (1 << 1) != 0,
                    proxy: payload[5] & (1 << 2) != 0,
                    single_tap: payload[5] & (1 << 3) != 0,
                }));
            }
        });

        Self {
            chan,
            device_index,
            feature_index,
            emitter,
            msg_listener_hdl: hdl,
        }
    }
}

impl Feature for ThumbwheelFeatureV0 {
}

impl EmittingFeature<ThumbwheelEvent> for ThumbwheelFeatureV0 {
    fn listen(&self) -> async_channel::Receiver<ThumbwheelEvent> {
        self.emitter.create_receiver()
    }
}

impl Drop for ThumbwheelFeatureV0 {
    fn drop(&mut self) {
        self.chan.remove_msg_listener(self.msg_listener_hdl);
    }
}

impl ThumbwheelFeatureV0 {
    /// Retrieves some information about the thumbwheel.
    pub async fn get_thumbwheel_info(&self) -> Result<ThumbwheelInfo, Hidpp20Error> {
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

        let payload = response.extend_payload();

        Ok(ThumbwheelInfo {
            native_resolution: u16::from_be_bytes(payload[0..=1].try_into().unwrap()),
            diverted_resolution: u16::from_be_bytes(payload[2..=3].try_into().unwrap()),
            time_unit: u16::from_be_bytes(payload[6..=7].try_into().unwrap()),
            default_direction: ThumbwheelDirection::try_from(payload[4] & 1)
                .map_err(|_| Hidpp20Error::UnsupportedResponse)?,
            capabilities: ThumbwheelCapabilities::from(payload[5]),
        })
    }

    /// Retrieves the custom status of the thumbwheel.
    pub async fn get_thumbwheel_status(&self) -> Result<ThumbwheelStatus, Hidpp20Error> {
        let response = self
            .chan
            .send_v20(v20::Message::Short(
                v20::MessageHeader {
                    device_index: self.device_index,
                    feature_index: self.feature_index,
                    function_id: U4::from_lo(1),
                    software_id: self.chan.get_sw_id(),
                },
                [0x00, 0x00, 0x00],
            ))
            .await?;

        let payload = response.extend_payload();

        Ok(ThumbwheelStatus {
            reporting_mode: ThumbwheelReportingMode::try_from(payload[0])
                .map_err(|_| Hidpp20Error::UnsupportedResponse)?,
            direction_inverted: payload[1] & 1 != 0,
            touch: payload[1] & (1 << 1) != 0,
            proxy: payload[1] & (1 << 2) != 0,
        })
    }

    /// Sets the reporting mode of the thumbwheel.
    ///
    /// This can be used to divert the thumbwheel notifications to HID++.
    ///
    /// If `invert_direction` is set, the [`ThumbwheelStatusUpdate::rotation`]
    /// field will be the inverse of that would be expected if following
    /// [`ThumbwheelInfo::default_direction`].
    pub async fn set_thumbwheel_reporting(
        &self,
        mode: ThumbwheelReportingMode,
        invert_direction: bool,
    ) -> Result<(), Hidpp20Error> {
        self.chan
            .send_v20(v20::Message::Short(
                v20::MessageHeader {
                    device_index: self.device_index,
                    feature_index: self.feature_index,
                    function_id: U4::from_lo(2),
                    software_id: self.chan.get_sw_id(),
                },
                [
                    mode.into(),
                    if invert_direction {
                        1
                    } else {
                        0
                    },
                    0x00,
                ],
            ))
            .await?;

        Ok(())
    }
}

/// Represents information about the thumbwheel as reported by
/// [`ThumbwheelFeatureV0::get_thumbwheel_info`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ThumbwheelInfo {
    /// The number of ratchets generated by revolution when in native (HID)
    /// mode.
    pub native_resolution: u16,

    /// The number of rotation increments generated by revolution when in
    /// diverted (HID++) mode
    pub diverted_resolution: u16,

    /// If [`ThumbwheelCapabilities::time_stamp`] is set, this is set to the
    /// timestamp unit used for [`ThumbwheelStatusUpdate::time_elapsed`] in
    /// microseconds. If the capability is not supported, this will always be
    /// `0`.
    pub time_unit: u16,

    /// The default rotation direction. This determines which rotation direction
    /// corresponds to which number range (positive or negative) for the
    /// [`ThumbwheelStatusUpdate::rotation`] value.
    pub default_direction: ThumbwheelDirection,

    /// The capabilites of the thumbwheel.
    pub capabilities: ThumbwheelCapabilities,
}

/// Determines which thumbwheel rotation corresponds to which number range
/// (positive or negative) for the [`ThumbwheelStatusUpdate::rotation`] value.
///
/// The direction descriptors (`LeftOrBack`, `RightOrFront`) are
/// specific to the device orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum ThumbwheelDirection {
    PositiveWhenLeftOrBack = 0,
    PositiveWhenRightOrFront = 1,
}

/// Represents the capabilities the thumbwheel may support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ThumbwheelCapabilities {
    /// Whether the thumbwheel supports emitting the elapsed time between two
    /// events via [`ThumbwheelStatusUpdate::time_elapsed`].
    pub time_stamp: bool,

    /// Whether the thumbwheel is equipped with a touch sensor.
    ///
    /// If this capability is supported, [`ThumbwheelStatusUpdate::touch`] will
    /// be set to whether the user touches the thumbwheel.
    pub touch: bool,

    /// Whether the thumbwheel is equipped with a proximity sensor.
    ///
    /// If this capability is supported, [`ThumbwheelStatusUpdate::proxy`] will
    /// be set to whether the user is close to the thumbwheel.
    pub proxy: bool,

    /// Whether the thumbwheel supports detecting single taps.
    ///
    /// If this capability is supported, [`ThumbwheelStatusUpdate::single_tap`]
    /// will be set to whether the user tapped the thumbwheel.
    pub single_tap: bool,
}

impl From<u8> for ThumbwheelCapabilities {
    fn from(value: u8) -> Self {
        Self {
            time_stamp: value & 1 != 0,
            touch: value & (1 << 1) != 0,
            proxy: value & (1 << 2) != 0,
            single_tap: value & (1 << 3) != 0,
        }
    }
}

/// Represents information about the thumbwheel status as reported by
/// [`ThumbwheelFeatureV0::get_thumbwheel_status`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ThumbwheelStatus {
    /// The mode how thumbwheel events are reported (native/HID or
    /// diverted/HID++).
    pub reporting_mode: ThumbwheelReportingMode,

    /// Whether the default direction as reported by
    /// [`ThumbwheelInfo::default_direction`] is inverted.
    pub direction_inverted: bool,

    /// Whether the user touches the thumbwheel.
    ///
    /// This is only set if the device supports touch detection as reported by
    /// [`ThumbwheelCapabilities::touch`].
    pub touch: bool,

    /// Whether the user is close to the thumbwheel.
    ///
    /// This is only set if the device supports proximity detection as reported
    /// by [`ThumbwheelCapabilities::proxy`].
    pub proxy: bool,
}

/// Represents the mode how the thumbwheel reports its events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum ThumbwheelReportingMode {
    /// Thumbwheel events are reported only to the native HID channel.
    Native = 0,

    /// Thumbwheel events are reported only to the diverted HID++ channel.
    ///
    /// This mode is required for [`ThumbwheelFeatureV0::listen`] to report any
    /// events.
    Diverted = 1,
}

/// Represents an event emitted by the [`ThumbwheelFeatureV0`] feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ThumbwheelEvent {
    /// Is emitted whenever the thumbwheel status updates.
    ///
    /// Requires the thumbwheel to be in diverted reporting mode.
    StatusUpdate(ThumbwheelStatusUpdate),
}

/// Represents the data of the [`ThumbwheelEvent::StatusUpdate`] event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ThumbwheelStatusUpdate {
    /// The rotation in relation to [`ThumbwheelInfo::native_resolution`] or
    /// [`ThumbwheelInfo::diverted_resolution`].
    pub rotation: i16,

    /// The time elapsed since the last event.
    ///
    /// The unit of this value is reported in [`ThumbwheelInfo::time_unit`].
    ///
    /// If [`ThumbwheelCapabilities::time_stamp`] is not supported, this value
    /// will be `0`.
    pub time_elapsed: u16,

    /// The status of the current rotation.
    pub rotation_status: ThumbwheelRotationStatus,

    /// Whether the user touches the thumbwheel.
    ///
    /// This is only set if the device supports touch detection as reported by
    /// [`ThumbwheelCapabilities::touch`].
    pub touch: bool,

    /// Whether the user is close to the thumbwheel.
    ///
    /// This is only set if the device supports proximity detection as reported
    /// by [`ThumbwheelCapabilities::proxy`].
    pub proxy: bool,

    /// Whether the user single-tapped the thumbwheel.
    ///
    /// This is only set if the device supports single-tap detection as reported
    /// by [`ThumbwheelCapabilities::single_tap`].
    pub single_tap: bool,
}

/// Represents a thumbwheel rotation status as reported in
/// [`ThumbwheelStatusUpdate::rotation_status`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum ThumbwheelRotationStatus {
    /// The thumbwheel was not rotated.
    Inactive = 0,

    /// The thumbwheel rotation was started.
    Start = 1,

    /// The thumbwheel rotation is ongoing.
    Active = 2,

    /// The thumbwheel was released.
    Stop = 3,
}
