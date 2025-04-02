//! Specific device feature implementations.

use std::{any::Any, sync::Arc};

use device_type_and_name::v0::DeviceTypeAndNameFeatureV0;
use feature_set::v0::FeatureSetFeatureV0;

use crate::{channel::HidppChannel, device::Device};

pub mod device_type_and_name;
pub mod feature_set;
pub mod root;

/// Represents a concrete implementation of a HID++2.0 device feature.
pub trait Feature: Any + Send + Sync {}

/// Represents a [`Feature`] that can be instantiated automatically.
pub trait CreatableFeature: Feature {
    /// The protocol ID of the implemented feature.
    const ID: u16;

    /// The version of the feature the implementation starts to support.
    const STARTING_VERSION: u8;

    /// Creates a new instance of the feature implementation.
    fn new(chan: Arc<HidppChannel>, device_index: u8, feature_index: u8) -> Self;
}

/// A bitfield describing some properties of a feature.
///
/// Documentation is taken from <https://drive.google.com/file/d/1ULmw9uJL8b8iwwUo5xjSS9F5Zvno-86y/view>.
#[derive(Clone, Copy, Hash, Debug)]
pub struct FeatureType {
    /// An obsolete feature is a feature that has been replaced by a newer one,
    /// but is advertised in order for older SWs to still be able to support the
    /// feature (in case the old SW does not know yet the newer one).
    pub obsolete: bool,

    /// A SW hidden feature is a feature that should not be known/managed/used
    /// by end user configuration SW. The host should ignore this type of
    /// features.
    pub hidden: bool,

    /// A hidden feature that has been disabled for user software. Used for
    /// internal testing and manufacturing.
    pub engineering: bool,

    /// A manufacturing feature that can be permanently deactivated. It is
    /// usually also hidden and engineering.
    pub manufacturing_deactivatable: bool,

    /// A compliance feature that can be permanently deactivated. It is usually
    /// also hidden and engineering.
    pub compliance_deactivatable: bool,
}

impl From<u8> for FeatureType {
    fn from(value: u8) -> Self {
        Self {
            obsolete: value & (1 << 7) != 0,
            hidden: value & (1 << 6) != 0,
            engineering: value & (1 << 5) != 0,
            manufacturing_deactivatable: value & (1 << 4) != 0,
            compliance_deactivatable: value & (1 << 3) != 0,
        }
    }
}

impl From<FeatureType> for u8 {
    fn from(value: FeatureType) -> Self {
        let mut raw = 0;

        if value.obsolete {
            raw |= 1 << 7
        }
        if value.hidden {
            raw |= 1 << 6
        }
        if value.engineering {
            raw |= 1 << 5
        }
        if value.manufacturing_deactivatable {
            raw |= 1 << 4
        }
        if value.compliance_deactivatable {
            raw |= 1 << 3
        }

        raw
    }
}

macro_rules! add_features {
    ($device:ident, $id:ident, $version:ident, $index:ident, { $typ:ty, $($types:ty),+ }) => {
        matching_add::<$typ>($device, $id, $version, $index);
        add_features!($device, $id, $version, $index, {$($types),+});
    };

    ($device:ident, $id:ident, $version:ident, $index:ident, { $typ:ty }) => {
        matching_add::<$typ>($device, $id, $version, $index);
    };
}

/// Adds all default feature implementations to a device that support the given
/// parameters.
pub fn add(device: &mut Device, feature_id: u16, feature_version: u8, feature_index: u8) {
    add_features!(device, feature_id, feature_version, feature_index, {
        FeatureSetFeatureV0,
        DeviceTypeAndNameFeatureV0
    });
}

/// Adds a feature implementation to a device only if it supports the ID and
/// version.
pub fn matching_add<F: CreatableFeature>(
    device: &mut Device,
    feature_id: u16,
    feature_version: u8,
    feature_index: u8,
) {
    if feature_id != F::ID || feature_version < F::STARTING_VERSION {
        return;
    }

    device.add_feature::<F>(feature_index);
}
