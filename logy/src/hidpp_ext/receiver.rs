use anyhow::Result;
use hidpp::receiver::{
    Receiver,
    bolt::{BoltDeviceConnection, BoltDeviceKind},
};
use itertools::Itertools;
use serde::Serialize;

pub trait LogyReceiver {
    async fn get_paired_devices(&self) -> Result<Vec<PairedDevice>>;
    async fn get_paired_device_name(&self, index: u8) -> Result<String>;
}

impl LogyReceiver for Receiver {
    async fn get_paired_devices(&self) -> Result<Vec<PairedDevice>> {
        Ok(match self {
            Self::Bolt(bolt) => bolt
                .collect_paired_devices()
                .await?
                .into_iter()
                .map_into()
                .collect_vec(),
            _ => vec![],
        })
    }

    async fn get_paired_device_name(&self, index: u8) -> Result<String> {
        Ok(match self {
            Self::Bolt(bolt) => bolt.get_device_codename(index).await?,
            _ => String::new(),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct PairedDevice {
    pub slot: u8,
    pub kind: PairedDeviceKind,
    pub online: bool,
    pub wpid: u16,
}

impl From<BoltDeviceConnection> for PairedDevice {
    fn from(value: BoltDeviceConnection) -> Self {
        Self {
            slot: value.index,
            kind: value.kind.into(),
            online: value.online,
            wpid: value.wpid,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize)]
pub enum PairedDeviceKind {
    Unknown,
    Keyboard,
    Mouse,
    Numpad,
    Presenter,
    Remote,
    Trackball,
    Touchpad,
    Tablet,
    Gamepad,
    Joystick,
    Headset,
}

impl From<BoltDeviceKind> for PairedDeviceKind {
    fn from(value: BoltDeviceKind) -> Self {
        match value {
            BoltDeviceKind::Unknown => Self::Unknown,
            BoltDeviceKind::Keyboard => Self::Keyboard,
            BoltDeviceKind::Mouse => Self::Mouse,
            BoltDeviceKind::Numpad => Self::Numpad,
            BoltDeviceKind::Presenter => Self::Presenter,
            BoltDeviceKind::Remote => Self::Remote,
            BoltDeviceKind::Trackball => Self::Trackball,
            BoltDeviceKind::Touchpad => Self::Touchpad,
            BoltDeviceKind::Tablet => Self::Tablet,
            BoltDeviceKind::Gamepad => Self::Gamepad,
            BoltDeviceKind::Joystick => Self::Joystick,
            BoltDeviceKind::Headset => Self::Headset,
            _ => Self::Unknown,
        }
    }
}
