//! Implements the feature starting with version 0.

use std::{
    collections::HashSet,
    hash::Hash,
    sync::{Arc, Mutex, mpsc},
};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    channel::HidppChannel,
    feature::{CreatableFeature, EmittingFeature, Feature},
    nibble::{self, U4},
    protocol::v20::{self, Hidpp20Error},
};

/// Implements the `UnifiedBattery` / `0x1004` feature.
///
/// The first version supported by this feature is v0.
pub struct UnifiedBatteryFeatureV0 {
    /// The underlying HID++ channel.
    chan: Arc<HidppChannel>,

    /// The index of the device to implement the feature for.
    device_index: u8,

    /// The index of the feature in the feature table.
    feature_index: u8,

    /// A collection of event listeners added via [`Self::listen`].
    listeners: Arc<Mutex<Vec<mpsc::Sender<BatteryInfo>>>>,

    /// The handle assigned to the message listener registered via
    /// [`HidppChannel::add_msg_listener`].
    /// This is used to remove the listener when the feature is dropped.
    msg_listener_hdl: u32,
}

impl CreatableFeature for UnifiedBatteryFeatureV0 {
    const ID: u16 = 0x1004;
    const STARTING_VERSION: u8 = 0;

    fn new(chan: Arc<HidppChannel>, device_index: u8, feature_index: u8) -> Self {
        let listeners_rc = Arc::new(Mutex::new(Vec::<mpsc::Sender<BatteryInfo>>::new()));

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
                let Ok(level) = BatteryLevel::try_from(payload[1]) else {
                    return;
                };
                let Ok(status) = BatteryStatus::try_from(payload[2]) else {
                    return;
                };

                listeners.lock().unwrap().retain(|listener| {
                    listener
                        .send(BatteryInfo {
                            charging_percentage: payload[0],
                            level,
                            status,
                        })
                        .is_ok()
                });
            }
        });

        Self {
            chan,
            device_index,
            feature_index,
            listeners: listeners_rc,
            msg_listener_hdl: hdl,
        }
    }
}

impl Feature for UnifiedBatteryFeatureV0 {
}

impl EmittingFeature<BatteryInfo> for UnifiedBatteryFeatureV0 {
    fn listen(&self) -> mpsc::Receiver<BatteryInfo> {
        let (tx, rx) = mpsc::channel::<BatteryInfo>();
        self.listeners.lock().unwrap().push(tx);
        rx
    }
}

impl Drop for UnifiedBatteryFeatureV0 {
    fn drop(&mut self) {
        self.chan.remove_msg_listener(self.msg_listener_hdl);
    }
}

impl UnifiedBatteryFeatureV0 {
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
        // power source, according to
        // https://github.com/torvalds/linux/blob/a8662bcd2ff152bfbc751cab20f33053d74d0963/drivers/hid/hid-logitech-hidpp.c#L1608
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
        if value[0] & (1 << 0) != 0 {
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
            rechargeable: value[1] & (1 << 0) != 0,
            percentage: value[1] & (1 << 1) != 0,
        }
    }
}

/// Represents infirmation about the current battery charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
#[non_exhaustive]
#[repr(u8)]
pub enum BatteryLevel {
    Critical = 1 << 0,
    Low = 1 << 1,
    Good = 1 << 2,
    Full = 1 << 3,
}

/// Represents the charging status of the battery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum BatteryStatus {
    Discharging = 0,
    Charging = 1,
    ChargingSlow = 2,
    Full = 3,
    Error = 4,
}
