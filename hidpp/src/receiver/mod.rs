//! Implements the different HID++ wireless receivers, including pairing.

use thiserror::Error;

use crate::protocol::v10::Hidpp10Error;

pub mod bolt;

/// The index to use when communicating with the receiver on any HID++ channel.
pub const RECEIVER_DEVICE_INDEX: u8 = 0xff;

/// Represents an error returned by a receiver.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReceiverError {
    /// Indicates that no supported receiver could be identified on a HID++
    /// channel.
    #[error("no (supported) receiver could be found")]
    UnknownReceiver,

    /// Indicates that a HID++1.0 register access resulted in an error.
    #[error("a HID++1.0 error occurred")]
    Protocol(#[from] Hidpp10Error),
}
