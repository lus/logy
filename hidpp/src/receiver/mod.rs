//! Implements the different HID++ wireless receivers.
//!
//! Because of the lack of public documentation about the different receivers
//! and their capabilities, and because I currently only own a single Bolt
//! receiver, this module is largely incomplete. I would be more than happy for
//! anyone who owns a different receiver, with Unifying having the highest
//! priority, and who is willing to actively support its implementation by
//! providing information and testing.
//!
//! Receivers can generally only be differentiated by their USB vendor and
//! product IDs, so the [`detect`] function does nothing more than matching
//! those values to the sets of known vendor and product ID pairs of the
//! different receivers.

use std::sync::Arc;

use bolt::{BOLT_VPID_PAIRS, BoltReceiver};
use thiserror::Error;

use crate::{channel::HidppChannel, protocol::v10::Hidpp10Error};

pub mod bolt;

/// The index to use when communicating with the receiver on any HID++ channel.
pub const RECEIVER_DEVICE_INDEX: u8 = 0xff;

/// Represents a HID++ wireless receiver.
#[derive(Clone)]
#[non_exhaustive]
pub enum Receiver {
    Bolt(BoltReceiver),
}

/// Tries to detect the receiver present on a HID++ channel.
pub fn detect(chan: Arc<HidppChannel>) -> Option<Receiver> {
    if BOLT_VPID_PAIRS.contains(&(chan.vendor_id, chan.product_id)) {
        if let Ok(bolt) = BoltReceiver::new(chan) {
            return Some(Receiver::Bolt(bolt));
        }
        return None;
    }

    None
}

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
