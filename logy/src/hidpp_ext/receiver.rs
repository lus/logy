use anyhow::Result;
use hidpp::receiver::{
    Receiver,
    bolt::{DeviceConnection, DeviceKind},
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

impl From<DeviceConnection> for PairedDevice {
    fn from(value: DeviceConnection) -> Self {
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

impl From<DeviceKind> for PairedDeviceKind {
    fn from(value: DeviceKind) -> Self {
        match value {
            DeviceKind::Unknown => Self::Unknown,
            DeviceKind::Keyboard => Self::Keyboard,
            DeviceKind::Mouse => Self::Mouse,
            DeviceKind::Numpad => Self::Numpad,
            DeviceKind::Presenter => Self::Presenter,
            DeviceKind::Remote => Self::Remote,
            DeviceKind::Trackball => Self::Trackball,
            DeviceKind::Touchpad => Self::Touchpad,
            DeviceKind::Tablet => Self::Tablet,
            DeviceKind::Gamepad => Self::Gamepad,
            DeviceKind::Joystick => Self::Joystick,
            DeviceKind::Headset => Self::Headset,
            _ => Self::Unknown,
        }
    }
}
