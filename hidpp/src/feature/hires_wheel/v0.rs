//! Implements the feature starting with version 0.

use std::{hash::Hash, sync::Arc};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    channel::HidppChannel,
    event::EventEmitter,
    feature::{CreatableFeature, EmittingFeature, Feature},
    nibble::U4,
    protocol::v20::{self, Hidpp20Error},
};

/// Implements the `HiResWheel` / `0x2121` feature.
///
/// The first version supported by this feature is v0.
///
/// The analytics part of the feature is not implemented here as its data
/// structure lacks any documentation.
pub struct HiResWheelFeatureV0 {
    /// The underlying HID++ channel.
    chan: Arc<HidppChannel>,

    /// The index of the device to implement the feature for.
    device_index: u8,

    /// The index of the feature in the feature table.
    feature_index: u8,

    /// The emitter used to emit events.
    emitter: Arc<EventEmitter<HiResWheelEvent>>,

    /// The handle assigned to the message listener registered via
    /// [`HidppChannel::add_msg_listener`].
    /// This is used to remove the listener when the feature is dropped.
    msg_listener_hdl: u32,
}

impl CreatableFeature for HiResWheelFeatureV0 {
    const ID: u16 = 0x2121;
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
                    || header.software_id.to_lo() != 0
                {
                    return;
                }

                let payload = msg.extend_payload();

                let event = match header.function_id.to_lo() {
                    0 => {
                        let Ok(resolution) =
                            WheelResolution::try_from((payload[0] & (1 << 4)) >> 4)
                        else {
                            return;
                        };

                        HiResWheelEvent::WheelMovement(WheelMovementData {
                            resolution,
                            periods: U4::from_lo(payload[0]),
                            delta_vertical: i16::from_be_bytes(payload[1..=2].try_into().unwrap()),
                        })
                    },
                    1 => {
                        let Ok(state) = WheelRatchetState::try_from(payload[0] & 1) else {
                            return;
                        };

                        HiResWheelEvent::RatchetSwitch(state)
                    },
                    _ => return,
                };

                emitter.emit(event);
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

impl Feature for HiResWheelFeatureV0 {
}

impl EmittingFeature<HiResWheelEvent> for HiResWheelFeatureV0 {
    fn listen(&self) -> async_channel::Receiver<HiResWheelEvent> {
        self.emitter.create_receiver()
    }
}

impl Drop for HiResWheelFeatureV0 {
    fn drop(&mut self) {
        self.chan.remove_msg_listener(self.msg_listener_hdl);
    }
}

impl HiResWheelFeatureV0 {
    /// Retrieves the capabilities of the hi-res wheel and this feature.
    pub async fn get_wheel_capabilities(&self) -> Result<WheelCapabilities, Hidpp20Error> {
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

        Ok(WheelCapabilities {
            multiplier: payload[0],
            has_invert: payload[1] & (1 << 3) != 0,
            has_switch: payload[1] & (1 << 2) != 0,
            ratches_per_rotation: payload[2],
            wheel_diameter: payload[3],
        })
    }

    /// Retrieves the current mode of the hi-res wheel.
    pub async fn get_wheel_mode(&self) -> Result<WheelMode, Hidpp20Error> {
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

        Ok(WheelMode {
            inverted: payload[0] & (1 << 2) != 0,
            resolution: WheelResolution::try_from((payload[0] & (1 << 1)) >> 1)
                .map_err(|_| Hidpp20Error::UnsupportedResponse)?,
            target: WheelEventTarget::try_from(payload[0] & 1)
                .map_err(|_| Hidpp20Error::UnsupportedResponse)?,
        })
    }

    /// Sets the mode of the hi-res wheel.
    ///
    /// Setting the bit to control analytics collection is not supported in this
    /// feature implementation as the analytics data structure is completely
    /// undocumented.\
    /// If this is implemented in the future, a new implementation will do so to
    /// not break this one.
    pub async fn set_wheel_mode(
        &self,
        target: WheelEventTarget,
        resolution: WheelResolution,
        inverted: bool,
    ) -> Result<WheelMode, Hidpp20Error> {
        let mut mode_byte = 0u8;
        if inverted {
            mode_byte |= 1 << 2;
        }
        mode_byte |= u8::from(resolution) << 1;
        mode_byte |= u8::from(target);

        let response = self
            .chan
            .send_v20(v20::Message::Short(
                v20::MessageHeader {
                    device_index: self.device_index,
                    feature_index: self.feature_index,
                    function_id: U4::from_lo(2),
                    software_id: self.chan.get_sw_id(),
                },
                [mode_byte, 0x00, 0x00],
            ))
            .await?;

        let payload = response.extend_payload();

        Ok(WheelMode {
            inverted: payload[0] & (1 << 2) != 0,
            resolution: WheelResolution::try_from((payload[0] & (1 << 1)) >> 1)
                .map_err(|_| Hidpp20Error::UnsupportedResponse)?,
            target: WheelEventTarget::try_from(payload[0] & 1)
                .map_err(|_| Hidpp20Error::UnsupportedResponse)?,
        })
    }

    /// Retrieves the current state of the ratchet switch.
    pub async fn get_ratchet_switch_state(&self) -> Result<WheelRatchetState, Hidpp20Error> {
        let response = self
            .chan
            .send_v20(v20::Message::Short(
                v20::MessageHeader {
                    device_index: self.device_index,
                    feature_index: self.feature_index,
                    function_id: U4::from_lo(3),
                    software_id: self.chan.get_sw_id(),
                },
                [0x00, 0x00, 0x00],
            ))
            .await?;

        let payload = response.extend_payload();

        WheelRatchetState::try_from(payload[0] & 1).map_err(|_| Hidpp20Error::UnsupportedResponse)
    }
}

/// Represents the capabilities of the hi-res wheel and this feature as reported
/// by [`HiResWheelFeatureV0::get_wheel_capabilities`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct WheelCapabilities {
    /// The report multiplier for the high-resolution mode. A single ratchet
    /// distance will produce this amount of wheel movement reports in hi-res
    /// mode.
    pub multiplier: u8,

    /// Whether the device supports inverting the scrolling direction when in
    /// native HID reporting mode.
    ///
    /// Inverting is never supported in diverted HID++ mode.
    pub has_invert: bool,

    /// Whether the device has a switch to control the ratchet mode.
    pub has_switch: bool,

    /// The amount of ratches that would be generated by a whole rotation of the
    /// scroll wheel.
    pub ratches_per_rotation: u8,

    /// The nominal wheel diameter in millimeters.
    pub wheel_diameter: u8,
}

/// Represents the wheel mode as reported by
/// [`HiResWheelFeatureV0::get_wheel_mode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct WheelMode {
    /// Whether the scrolling direction is inverted.
    /// Only applies when in native HID mode.
    pub inverted: bool,

    /// The current scrolling resolution.
    pub resolution: WheelResolution,

    /// The target of wheel movement reports (native or diverted).
    pub target: WheelEventTarget,
}

/// Represents the resolution of the hi-res wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
#[repr(u8)]
pub enum WheelResolution {
    Low = 0,
    High = 1,
}

/// Represents the target of wheel movement reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
#[repr(u8)]
pub enum WheelEventTarget {
    Native = 0,
    Diverted = 1,
}

/// Represents the state of the wheel ratchet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
#[repr(u8)]
pub enum WheelRatchetState {
    Freespin = 0,
    Ratchet = 1,
}

/// Represents an event emitted by the [`HiResWheelFeatureV0`] feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum HiResWheelEvent {
    /// Is emitted whenever the scroll wheel is moved in diverted HID++ mode.
    WheelMovement(WheelMovementData),

    /// Is emitted whenever the wheel ratchet mode is changed.
    ///
    /// This event is always enabled.
    RatchetSwitch(WheelRatchetState),
}

/// Represents the data of the [`HiResWheelEvent::WheelMovement`] event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct WheelMovementData {
    /// The current resolution of the wheel.
    pub resolution: WheelResolution,

    /// The amount of sampling periods for this event. Maxes at 15.
    pub periods: U4,

    /// The vertical movement delta. Moving away from the user produces positive
    /// values.
    pub delta_vertical: i16,
}
