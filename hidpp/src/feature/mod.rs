//! Specific device feature implementations.

use std::{any::Any, sync::Arc};

use feature_set::v0::FeatureSetFeatureV0;
use root::RootFeature;

use crate::{
    channel::{HidppChannel, RawHidChannel},
    device::Device,
};

pub mod feature_set;
pub mod root;

/// Represents a concrete implementation of a HID++2.0 device feature.
pub trait Feature<T: RawHidChannel>: Any + Send + Sync {}

/// Represents a [`Feature`] that can be instantiated automatically.
pub trait CreatableFeature<T: RawHidChannel>: Feature<T> {
    /// Creates a new instance of the feature implementation.
    fn new(chan: Arc<HidppChannel<T>>, device_index: u8, feature_index: u8) -> Self;
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

impl FeatureType {
    /// Constructs a new [`FeatureType`] from the raw byte.
    pub fn from_bits(raw: u8) -> Self {
        Self {
            obsolete: raw & (1 << 7) != 0,
            hidden: raw & (1 << 6) != 0,
            engineering: raw & (1 << 5) != 0,
            manufacturing_deactivatable: raw & (1 << 4) != 0,
            compliance_deactivatable: raw & (1 << 3) != 0,
        }
    }
}

/// Adds a default feature implementation to a device based on its ID and
/// version.
///
/// Returns whether an implementation exists and thus was added or not.
///
/// This does NOT check whether the device actually supports the feature.
pub fn add_implementation<T: RawHidChannel>(
    dev: &mut Device<T>,
    feature_index: u8,
    feature_id: u16,
    feature_version: u8,
) -> bool {
    match feature_id {
        root::FEATURE_ID => {
            dev.add_feature::<RootFeature<T>>(feature_index);
            true
        },
        feature_set::FEATURE_ID => match feature_version {
            0..=2 => {
                dev.add_feature::<FeatureSetFeatureV0<T>>(feature_index);
                true
            },
            _ => false,
        },
        _ => false,
    }
}
