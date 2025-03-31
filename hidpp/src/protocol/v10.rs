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
    /// the message can be mapped using [`ErrorType::from`].
    Error = 0x8f,
}

impl TryFrom<u8> for MessageType {
    type Error = ();

    fn try_from(
        value: u8,
    ) -> Result<Self, <crate::protocol::v10::MessageType as TryFrom<u8>>::Error> {
        match value {
            x if x == Self::SetRegister as u8 => Ok(Self::SetRegister),
            x if x == Self::GetRegister as u8 => Ok(Self::GetRegister),
            x if x == Self::SetLongRegister as u8 => Ok(Self::SetLongRegister),
            x if x == Self::GetLongRegister as u8 => Ok(Self::GetLongRegister),
            x if x == Self::Error as u8 => Ok(Self::Error),
            _ => Err(()),
        }
    }
}

impl From<MessageType> for u8 {
    fn from(value: MessageType) -> Self {
        value as u8
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

impl From<u8> for ErrorType {
    fn from(value: u8) -> Self {
        match value {
            x if x == Self::Success as u8 => Self::Success,
            x if x == Self::InvalidSubId as u8 => Self::InvalidSubId,
            x if x == Self::InvalidAddress as u8 => Self::InvalidAddress,
            x if x == Self::InvalidValue as u8 => Self::InvalidValue,
            x if x == Self::ConnectFail as u8 => Self::ConnectFail,
            x if x == Self::TooManyDevices as u8 => Self::TooManyDevices,
            x if x == Self::AlreadyExists as u8 => Self::AlreadyExists,
            x if x == Self::Busy as u8 => Self::Busy,
            x if x == Self::UnknownDevice as u8 => Self::UnknownDevice,
            x if x == Self::ResourceError as u8 => Self::ResourceError,
            x if x == Self::RequestUnavailable as u8 => Self::RequestUnavailable,
            x if x == Self::InvalidParamValue as u8 => Self::InvalidParamValue,
            x if x == Self::WrongPinCode as u8 => Self::WrongPinCode,
            _ => Self::Reserved,
        }
    }
}

impl TryFrom<ErrorType> for u8 {
    type Error = ();

    fn try_from(value: ErrorType) -> Result<Self, Self::Error> {
        match value {
            ErrorType::Reserved => Err(()),
            _ => Ok(value as u8),
        }
    }
}
