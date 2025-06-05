//! Implements the `UnifiedBattery` feature (ID `0x1004`) that provides
//! information about the battery status of the device.

use std::{collections::HashSet, hash::Hash, sync::Arc};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    channel::HidppChannel,
    event::EventEmitter,
    feature::{CreatableFeature, EmittingFeature, Feature},
    nibble::{self, U4},
    protocol::v20::{self, Hidpp20Error},
};

/// Implements the `UnifiedBattery` / `0x1004` feature.
pub struct UnifiedBatteryFeature {
    /// The underlying HID++ channel.
    chan: Arc<HidppChannel>,

    /// The index of the device to implement the feature for.
    device_index: u8,

    /// The index of the feature in the feature table.
    feature_index: u8,

    /// The emitter used to emit events.
    emitter: Arc<EventEmitter<BatteryEvent>>,

    /// The handle assigned to the message listener registered via
    /// [`HidppChannel::add_msg_listener`].
    /// This is used to remove the listener when the feature is dropped.
    msg_listener_hdl: u32,
}

impl CreatableFeature for UnifiedBatteryFeature {
    const ID: u16 = 0x1004;
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
                let Ok(level) = BatteryLevel::try_from(payload[1]) else {
                    return;
                };
                let Ok(status) = BatteryStatus::try_from(payload[2]) else {
                    return;
                };

                emitter.emit(BatteryEvent::InfoUpdate(BatteryInfo {
                    charging_percentage: payload[0],
                    level,
                    status,
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

impl Feature for UnifiedBatteryFeature {
}

impl EmittingFeature<BatteryEvent> for UnifiedBatteryFeature {
    fn listen(&self) -> async_channel::Receiver<BatteryEvent> {
        self.emitter.create_receiver()
    }
}

impl Drop for UnifiedBatteryFeature {
    fn drop(&mut self) {
        self.chan.remove_msg_listener(self.msg_listener_hdl);
    }
}

impl UnifiedBatteryFeature {
    /// Retrieves the capabilities of this feature and the battery in general.
    pub async fn get_battery_capabilities(&self) -> Result<BatteryCapabilities, Hidpp20Error> {
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

        let payload: [u8; 2] = response.extend_payload()[..2].try_into().unwrap();

        Ok(BatteryCapabilities::from(payload))
    }

    /// Retrieves the current information about the battery status.
    pub async fn get_battery_info(&self) -> Result<BatteryInfo, Hidpp20Error> {
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

        // payload[3] contains some kind of information about the status of the external
        // power source (maybe 0 = disconnected and 1 = connected, I don't have enough
        // info about that), according to https://github.com/torvalds/linux/blob/a8662bcd2ff152bfbc751cab20f33053d74d0963/drivers/hid/hid-logitech-hidpp.c#L1608
        // and
        // https://github.com/torvalds/linux/blob/a8662bcd2ff152bfbc751cab20f33053d74d0963/drivers/hid/hid-logitech-hidpp.c#L1679

        Ok(BatteryInfo {
            charging_percentage: payload[0],
            level: BatteryLevel::try_from(payload[1])
                .map_err(|_| Hidpp20Error::UnsupportedResponse)?,
            status: BatteryStatus::try_from(payload[2])
                .map_err(|_| Hidpp20Error::UnsupportedResponse)?,
        })
    }
}

/// Represents the capabilites of this feature and the battery itself.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BatteryCapabilities {
    /// All [`BatteryLevel`] variants the feature supports and reports.
    pub reported_levels: HashSet<BatteryLevel>,

    /// Whether the battery is rechargeable.
    pub rechargeable: bool,

    /// Whether the device supports reporting the current battery charge
    /// percentage in [`BatteryInfo::charging_percentage`].
    pub percentage: bool,
}

impl From<[u8; 2]> for BatteryCapabilities {
    fn from(value: [u8; 2]) -> Self {
        let mut reported_levels = HashSet::new();
        if value[0] & 1 != 0 {
            reported_levels.insert(BatteryLevel::Critical);
        }
        if value[0] & (1 << 1) != 0 {
            reported_levels.insert(BatteryLevel::Low);
        }
        if value[0] & (1 << 2) != 0 {
            reported_levels.insert(BatteryLevel::Good);
        }
        if value[0] & (1 << 3) != 0 {
            reported_levels.insert(BatteryLevel::Full);
        }

        Self {
            reported_levels,
            rechargeable: value[1] & 1 != 0,
            percentage: value[1] & (1 << 1) != 0,
        }
    }
}

/// Represents infirmation about the current battery charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BatteryInfo {
    /// The current charge of the battery in percent.
    ///
    /// If [`BatteryCapabilities::percentage`] is set to `false`, this is always
    /// zero.
    pub charging_percentage: u8,

    /// The current (approximate) level of the battery.
    ///
    /// This can only reach values present in
    /// [`BatteryCapabilities::reported_levels`].
    pub level: BatteryLevel,

    /// The current charging status of the battery.
    pub status: BatteryStatus,
}

/// Represents an approximate level of the battery charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
#[repr(u8)]
pub enum BatteryLevel {
    Critical = 1,
    Low = 1 << 1,
    Good = 1 << 2,
    Full = 1 << 3,
}

/// Represents the charging status of the battery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
#[repr(u8)]
pub enum BatteryStatus {
    Discharging = 0,
    Charging = 1,
    ChargingSlow = 2,
    Full = 3,
    Error = 4,
}

/// Represents an event emitted by the [`UnifiedBatteryFeature`] feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum BatteryEvent {
    /// Is emitted whenever the battery information changes.
    ///
    /// This event is always enabled.
    InfoUpdate(BatteryInfo),
}
