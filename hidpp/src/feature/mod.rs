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

/// Adds a default feature implementation to a device based on its ID and
/// version.
///
/// Returns whether an implementation exists and thus was added or not.
///
/// This does NOT check whether the device actually supports the feature.
pub fn add_implementation(dev: &mut Device, index: u8, id: u16, version: u8) -> bool {
    [
        maybe_add_implementation::<FeatureSetFeatureV0>(dev, id, index, version),
        maybe_add_implementation::<DeviceTypeAndNameFeatureV0>(dev, id, index, version),
    ]
    .iter()
    .any(|elem| *elem)
}

fn maybe_add_implementation<F: CreatableFeature>(
    dev: &mut Device,
    id: u16,
    index: u8,
    version: u8,
) -> bool {
    if id != F::ID || version < F::STARTING_VERSION {
        return false;
    }

    dev.add_feature::<F>(index);
    true
}
