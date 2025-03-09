//! Implements messaging across a HID++ channel.
//!
//! This includes mapping incoming messages to previously sent requests.

use std::error::Error;

use hidreport::{Field, Report, ReportDescriptor, Usage, UsageId, UsagePage};
use thiserror::Error;

const MAX_REPORT_DESCRIPTOR_LENGTH: usize = 0x1000;

const SHORT_REPORT_ID: u8 = 0x10;
const SHORT_REPORT_USAGE_PAGE: u16 = 0xff00;
const SHORT_REPORT_USAGE: u16 = 0x0001;

const LONG_REPORT_ID: u8 = 0x11;
const LONG_REPORT_USAGE_PAGE: u16 = 0xff00;
const LONG_REPORT_USAGE: u16 = 0x0002;

/// Represents an arbitrary HID communication channel that is both readable and
/// writable.
///
/// Any type this trait is implemented for can be used for HID(++)
/// communication. If a specific channel supports HID++ is determined at a later
/// stage and is not directly related to potential implementations of this
/// trait.
pub trait RawHidChannel {
    /// An implementation-specific error type.
    type Error: Error;

    /// Writes raw data to the channel.
    /// Returns the exact amount of written bytes on success.
    fn write(&self, src: &[u8]) -> Result<usize, Self::Error>;

    /// Reads raw data from the channel.
    /// Returns the exact amount or read bytes on success.
    fn read(&self, buf: &mut [u8]) -> Result<usize, Self::Error>;

    /// Reads the HID report descriptor from the channel.
    /// Returns the exact size of the read report descriptor on success.
    /// This is used to determine whether the channel supports HID++.
    fn read_report_descriptor(&self, buf: &mut [u8]) -> Result<usize, Self::Error>;
}

/// Represents the header that starts every HID++ message.
pub struct HidppMessageHeader {
    /// The index of the device involved in the communication.
    pub device_index: u8,

    /// The index of the feature the message belongs to.
    /// This is not the same as the feature ID, but the index returned from a
    /// feature enumeration request.
    pub feature_index: u8,

    /// The function (leftmost 4 bits) and software (rightmost 4 bits) IDs.
    pub function_and_sw_id: u8,
}

/// Represents a HID++ message consisting of a header and payload.
pub enum HidppMessage {
    /// Represents a short HID++ message that has 3 bytes of payload.
    /// Please check [`HidppChannel::supports_short`] before sending this kind
    /// of message.
    Short(HidppMessageHeader, [u8; 3]),

    /// Represents a long HID++ message that has 16 bytes of payload.
    /// Please check [`HidppChannel::supports_long`] before sending this kind of
    /// message.
    Long(HidppMessageHeader, [u8; 16]),
}

impl HidppMessage {
    /// Tries to read a HID++ message from raw data.
    pub fn read_raw(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        let header = HidppMessageHeader {
            device_index: data[1],
            feature_index: data[2],
            function_and_sw_id: data[3],
        };

        if data[0] == SHORT_REPORT_ID {
            if data.len() != 7 {
                return None;
            }

            return Some(HidppMessage::Short(header, data[4..].try_into().unwrap()));
        } else if data[0] == LONG_REPORT_ID {
            if data.len() != 20 {
                return None;
            }

            return Some(HidppMessage::Long(header, data[4..].try_into().unwrap()));
        }

        None
    }

    /// Writes a HID++ message in its raw byte form into a buffer.
    /// Returns the amount of written bytes.
    pub fn write_raw(&self, buf: &mut [u8]) -> usize {
        let (id, header) = match self {
            Self::Short(header, _) => (SHORT_REPORT_ID, header),
            Self::Long(header, _) => (LONG_REPORT_ID, header),
        };

        buf[0] = id;
        buf[1] = header.device_index;
        buf[2] = header.feature_index;
        buf[3] = header.function_and_sw_id;

        match self {
            Self::Short(_, payload) => {
                buf[4..7].copy_from_slice(payload);
                7
            },
            Self::Long(_, payload) => {
                buf[4..20].copy_from_slice(payload);
                20
            },
        }
    }
}

/// Represents a HID communication channel supporting HID++.
pub struct HidppChannel<T: RawHidChannel> {
    channel: T,

    /// Whether the channel supports short (7 bytes) HID++ messages.
    pub supports_short: bool,

    /// Whether the channel supports long (20 bytes) HID++ messages.
    pub supports_long: bool,
}

impl<T> HidppChannel<T>
where T: RawHidChannel
{
    /// Tries to construct a HID++ channel from a raw HID channel.
    /// If the given HID channel does not support HID++,
    /// [`ChannelError::HidppNotSupported`] will be returned.
    pub fn of_raw_channel(raw: T) -> Result<Self, ChannelError<T::Error>> {
        let (supports_short, supports_long) = Self::supports_short_long_hidpp(&raw)?;

        if !supports_short && !supports_long {
            return Err(ChannelError::HidppNotSupported);
        }

        Ok(Self {
            channel: raw,
            supports_short,
            supports_long,
        })
    }

    /// Checks whether a raw channel supports short or long HID++ messages.
    fn supports_short_long_hidpp(chan: &T) -> Result<(bool, bool), ChannelError<T::Error>> {
        let mut raw_descriptor = vec![0u8; MAX_REPORT_DESCRIPTOR_LENGTH];
        let descriptor_size = chan.read_report_descriptor(&mut raw_descriptor)?;

        let descriptor = match ReportDescriptor::try_from(&raw_descriptor[..descriptor_size]) {
            Ok(val) => val,
            Err(err) => return Err(ChannelError::ReportDescriptor(err)),
        };

        let supports_short = descriptor
            .find_input_report(&[SHORT_REPORT_ID])
            .and_then(|report| report.fields().first())
            .and_then(|field| match field {
                Field::Array(arr) => Some(arr.usage_range()),
                _ => None,
            })
            .is_some_and(|range| {
                range
                    .lookup_usage(&Usage::from_page_and_id(
                        UsagePage::from(SHORT_REPORT_USAGE_PAGE),
                        UsageId::from(SHORT_REPORT_USAGE),
                    ))
                    .is_some()
            });

        let supports_long = descriptor
            .find_input_report(&[LONG_REPORT_ID])
            .and_then(|report| report.fields().first())
            .and_then(|field| match field {
                Field::Array(arr) => Some(arr.usage_range()),
                _ => None,
            })
            .is_some_and(|range| {
                range
                    .lookup_usage(&Usage::from_page_and_id(
                        UsagePage::from(LONG_REPORT_USAGE_PAGE),
                        UsageId::from(LONG_REPORT_USAGE),
                    ))
                    .is_some()
            });

        Ok((supports_short, supports_long))
    }

    /// Sends a HID++ message across the channel.
    pub fn send(&self, msg: &HidppMessage) -> Result<(), ChannelError<T::Error>> {
        // TODO: Return future for response

        match msg {
            HidppMessage::Short(..) => {
                let mut buf = [0u8; 7];
                msg.write_raw(&mut buf);
                self.channel.write(&buf)
            },
            HidppMessage::Long(..) => {
                let mut buf = [0u8; 20];
                msg.write_raw(&mut buf);
                self.channel.write(&buf)
            },
        }
        .map(|_| ())
        .map_err(ChannelError::Implementation)
    }
}

/// Represents an error that occurred when creating or interacting with a HID or
/// HID++ communication channel.
#[derive(Debug, Error)]
pub enum ChannelError<T> {
    /// Indicates that the channel in question does not support HID++.
    #[error("the HID channel does not support HID++")]
    HidppNotSupported,

    /// Indicates that the HID report descriptor could not be parsed.
    #[error("the report descriptor could not be parsed")]
    ReportDescriptor(hidreport::ParserError),

    /// Indicates that the concrete implementation of [`RawHidChannel`] returned
    /// an error of type [`RawHidChannel::Error`].
    #[error("the HID channel implementation returned an error")]
    Implementation(#[from] T),
}
