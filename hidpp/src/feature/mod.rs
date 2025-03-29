//! Specific device feature implementations.

use std::any::Any;

use crate::channel::RawHidChannel;

pub mod root;

/// Represents a concrete implementation of a HID++2.0 device feature.
pub trait Feature<T: RawHidChannel>: Any + Send + Sync {
    /// Provides the protocol ID of the feature.
    fn id(&self) -> u16;
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
