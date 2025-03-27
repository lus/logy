//! Implements functionality specific to HID++2.0.

use crate::{
    channel::{HidppMessage, LONG_REPORT_LENGTH, SHORT_REPORT_LENGTH},
    nibble::{self, U4},
};

/// Represents the header that every [`HidppMessage`] of HID++2.0 starts with.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MessageHeader {
    /// The index of the device involved in the communication.
    pub device_index: u8,

    /// The index of the feature the message belongs to.
    ///
    /// This is not the same as the feature ID, but the index returned from a
    /// feature enumeration request.
    pub feature_index: u8,

    /// The ID of the function involved in the communication.
    pub function_id: U4,

    /// The ID of the software communicating with the device.
    pub software_id: U4,
}

/// Represents a HID++2.0 message.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Message {
    /// Represents a short HID++2.0 message with 3 bytes of payload.
    Short(MessageHeader, [u8; SHORT_REPORT_LENGTH - 4]),

    /// Represents a long HID++2.0 message with 16 bytes of payload.
    Long(MessageHeader, [u8; LONG_REPORT_LENGTH - 4]),
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
    pub fn extend_payload(&self) -> [u8; LONG_REPORT_LENGTH - 4] {
        match *self {
            Message::Short(_, payload) => {
                let mut data = [0; LONG_REPORT_LENGTH - 4];
                data[..SHORT_REPORT_LENGTH - 4].copy_from_slice(&payload);
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
                    feature_index: payload[1],
                    function_id: U4::from_hi(payload[2]),
                    software_id: U4::from_lo(payload[2]),
                },
                payload[3..].try_into().unwrap(),
            ),
            HidppMessage::Long(payload) => Message::Long(
                MessageHeader {
                    device_index: payload[0],
                    feature_index: payload[1],
                    function_id: U4::from_hi(payload[2]),
                    software_id: U4::from_lo(payload[2]),
                },
                payload[3..].try_into().unwrap(),
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
                data[1] = header.feature_index;
                data[2] = nibble::combine(header.function_id, header.software_id);
                data[3..].copy_from_slice(&payload);

                HidppMessage::Short(data)
            },
            Message::Long(header, payload) => {
                let mut data = [0u8; LONG_REPORT_LENGTH - 1];
                data[0] = header.device_index;
                data[1] = header.feature_index;
                data[2] = nibble::combine(header.function_id, header.software_id);
                data[3..].copy_from_slice(&payload);

                HidppMessage::Long(data)
            },
        }
    }
}
