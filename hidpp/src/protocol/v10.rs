//! Implements functionality specific to HID++1.0.

use crate::channel::{HidppMessage, LONG_REPORT_LENGTH, SHORT_REPORT_LENGTH};

/// Represents the header that every [`HidppMessage`] of HID++1.0 starts with.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MessageHeader {
    /// The index of the device involved in the communication.
    pub device_index: u8,

    /// The sub ID of the message.
    pub sub_id: u8,
}

/// Represents a HID++1.0 message.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Message {
    /// Represents a short HID++1.0 message with 4 bytes of payload.
    Short(MessageHeader, [u8; SHORT_REPORT_LENGTH - 3]),

    /// Represents a long HID++1.0 message with 17 bytes of payload.
    Long(MessageHeader, [u8; LONG_REPORT_LENGTH - 3]),
}

impl Message {
    /// Extracts the header of the message.
    pub fn header(&self) -> MessageHeader {
        match *self {
            Message::Short(header, _) => header,
            Message::Long(header, _) => header,
        }
    }

    /// Extracts the payload of the message and fits it into an array capable of
    /// containing the longest possible payload, filling the rest up with
    /// zeroes.
    pub fn extend_payload(&self) -> [u8; LONG_REPORT_LENGTH - 3] {
        match *self {
            Message::Short(_, payload) => {
                let mut data = [0; LONG_REPORT_LENGTH - 3];
                data[..SHORT_REPORT_LENGTH - 3].copy_from_slice(&payload);
                data
            },
            Message::Long(_, payload) => payload,
        }
    }
}

impl From<HidppMessage> for Message {
    fn from(msg: HidppMessage) -> Self {
        match msg {
            HidppMessage::Short(payload) => Message::Short(
                MessageHeader {
                    device_index: payload[0],
                    sub_id: payload[1],
                },
                payload[2..].try_into().unwrap(),
            ),
            HidppMessage::Long(payload) => Message::Long(
                MessageHeader {
                    device_index: payload[0],
                    sub_id: payload[1],
                },
                payload[2..].try_into().unwrap(),
            ),
        }
    }
}

impl From<Message> for HidppMessage {
    fn from(msg: Message) -> Self {
        match msg {
            Message::Short(header, payload) => {
                let mut data = [0u8; SHORT_REPORT_LENGTH - 1];
                data[0] = header.device_index;
                data[1] = header.sub_id;
                data[2..].copy_from_slice(&payload);

                HidppMessage::Short(data)
            },
            Message::Long(header, payload) => {
                let mut data = [0u8; LONG_REPORT_LENGTH - 1];
                data[0] = header.device_index;
                data[1] = header.sub_id;
                data[2..].copy_from_slice(&payload);

                HidppMessage::Long(data)
            },
        }
    }
}

/// Represents a globally defined sub ID of a HID++1.0 message.
///
/// This enum only includes sub IDs that are defined globally across all
/// devices. Most devices (e.g. the Unifying Receiver) define additional sub IDs
/// specific to their functionality.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MessageType {
    /// Used to set a 3-byte register value. A sent message of this type is
    /// usually responded with a response message of the same type (or
    /// [`Self::Error`]).
    SetRegister = 0x80,

    /// Used to retrieve a 3-byte register value. A sent message of this type is
    /// usually responded with a response message of the same type (or
    /// [`Self::Error`]).
    GetRegister = 0x81,

    /// Used to set a 16-byte register value. A sent message of this type is
    /// usually responded with a response message of the same type (or
    /// [`Self::Error`]).
    SetLongRegister = 0x82,

    /// Used to retrieve a 16-byte register value. A sent message of this type
    /// is usually responded with a response message of the same type (or
    /// [`Self::Error`]).
    GetLongRegister = 0x83,

    /// Used to indicate an error response. The error code usually included in
    /// the message can be mapped using [`Error::from_code`].
    Error = 0x8f,
}

impl MessageType {
    /// Tries to construct a [`MessageType`] variant from the raw sub ID of a
    /// HID++1.0 message.
    ///
    /// Returns [`None`] if no variant matches the given sub ID.
    pub fn from_sub_id(sub_id: u8) -> Option<Self> {
        match sub_id {
            0x80 => Some(Self::SetRegister),
            0x81 => Some(Self::GetRegister),
            0x82 => Some(Self::SetLongRegister),
            0x83 => Some(Self::GetLongRegister),
            0x8f => Some(Self::Error),
            _ => None,
        }
    }

    /// Constructs the raw sub ID from a [`MessageType`] variant.
    pub fn to_sub_id(self) -> u8 {
        self as u8
    }
}

/// Represents the type of an error a HID++1.0 device returns as part of a
/// message with the [`MessageType::Error`] type.
///
/// Error code `0x00` is defined as success and is thus not really an error nor
/// included in this enum.
///
/// Error codes `0x0D..=0xFF` are defined as reserved values and are merged into
/// the [`Self::Reserved`] variant.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ErrorType {
    /// No error.
    Success = 0x00,

    /// The sub ID of a sent message is invalid.
    InvalidSubId = 0x01,

    /// The address included in a sent message is invalid.
    InvalidAddress = 0x02,

    /// The value included in a sent message is invalid.
    InvalidValue = 0x03,

    /// A connection request failed on the receiver's side.
    ConnectFail = 0x04,

    /// The receiver indicates that too many devices are connected to it.
    TooManyDevices = 0x05,

    /// The reciever indicates that something already exists. This error is not
    /// further documented, please let me know what it means.
    AlreadyExists = 0x06,

    /// The receiver is currently handling a downstream (to device) message and
    /// cannot process a second one.
    Busy = 0x07,

    /// Trying to send a message to a device (device index) where there is no
    /// device paired.
    UnknownDevice = 0x08,

    /// This error is returned by the receiver when a HID++ command has been
    /// sent to a device that is in disconnected mode. When a device is in
    /// disconnected mode it cannot receive commands from the host until it
    /// reconnects. A device reconnects when the user interacts with it. In most
    /// cases, a device disconnects after several minutes of inactivity.
    ResourceError = 0x09,

    /// A sent request is not available in the current context.
    RequestUnavailable = 0x0a,

    /// A request parameter has an unsupported value.
    InvalidParamValue = 0x0b,

    /// The PIN code a device was wrong.
    WrongPinCode = 0x0c,

    /// An error code in the range `0x0D..=0xFF`.
    Reserved,
}

impl ErrorType {
    /// Constructs an [`Error`] variant from the raw error code included in a
    /// [`MessageType::Error`] message.
    pub fn from_code(code: u8) -> Self {
        match code {
            0x00 => Self::Success,
            0x01 => Self::InvalidSubId,
            0x02 => Self::InvalidAddress,
            0x03 => Self::InvalidValue,
            0x04 => Self::ConnectFail,
            0x05 => Self::TooManyDevices,
            0x06 => Self::AlreadyExists,
            0x07 => Self::Busy,
            0x08 => Self::UnknownDevice,
            0x09 => Self::ResourceError,
            0x0a => Self::RequestUnavailable,
            0x0b => Self::InvalidParamValue,
            0x0c => Self::WrongPinCode,
            0x0d..=0xff => Self::Reserved,
        }
    }

    /// Tries to construct the raw error code from an [`Error`] variant.
    ///
    /// Returns [`None`] for [`Self::Reserved`], as no single error code is
    /// defined for this variant.
    pub fn to_code(self) -> Option<u8> {
        match self {
            Self::Reserved => None,
            _ => Some(self as u8),
        }
    }
}
