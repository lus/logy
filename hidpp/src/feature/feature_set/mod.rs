//! Implements the FeatureSet feature (ID `0x0001`) that allow enumerating all
//! the features supported by a device.

pub mod v0;

/// The protocol ID of the feature.
pub const FEATURE_ID: u16 = 0x0001;
