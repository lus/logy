//! Implements basic messaging across HID and HID++ channels.
//!
//! This includes mapping incoming messages to previously sent requests.

use std::{
    collections::VecDeque,
    error::Error,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use futures::{FutureExt, channel::oneshot, select};
use hidreport::{Field, Report, ReportDescriptor, Usage, UsageId, UsagePage};
use thiserror::Error;

/// hidapi defines this as the maximum EXPECTED size of report descriptors.
/// We will trust this for now, but a workaround may be required if devices do
/// in fact return longer descriptors.
const MAX_REPORT_DESCRIPTOR_LENGTH: usize = 4096;

/// This is the size of the buffer incoming reports are read into.
/// As we only care about HID++ reports, this equals to [`LONG_REPORT_LENGTH`].
const MAX_REPORT_LENGTH: usize = LONG_REPORT_LENGTH;

/// The ID of the HID report that is used to transmit short HID++ messages.
pub const SHORT_REPORT_ID: u8 = 0x10;

/// The HID usage page ID of short HID++ message reports.
pub const SHORT_REPORT_USAGE_PAGE: u16 = 0xff00;

/// The HID usage ID of short HID++ message reports.
pub const SHORT_REPORT_USAGE: u16 = 0x0001;

/// The length of short HID++ message reports (including report ID).
pub const SHORT_REPORT_LENGTH: usize = 7;

/// The ID of the HID report that is used to transmit long HID++ messages.
pub const LONG_REPORT_ID: u8 = 0x11;

/// The HID usage page ID of long HID++ message reports.
pub const LONG_REPORT_USAGE_PAGE: u16 = 0xff00;

/// The HID usage ID of long HID++ message reports.
pub const LONG_REPORT_USAGE: u16 = 0x0002;

/// The length of long HID++ message reports (including report ID).
pub const LONG_REPORT_LENGTH: usize = 20;

/// Represents an arbitrary HID communication channel that is both readable and
/// writable. It has to support async I/O.
///
/// Any type this trait is implemented for can be used for HID(++)
/// communication. If a specific channel supports HID++ is determined at a later
/// stage and is not directly related to potential implementations of this
/// trait.
pub trait RawHidChannel: Send + Sync + 'static {
    /// An implementation-specific error type.
    type Error: Error;

    /// Writes a raw report to the channel.
    ///
    /// Returns the exact amount of written bytes on success.
    fn write_report(&self, src: &[u8]) -> impl Future<Output = Result<usize, Self::Error>> + Send;

    /// Reads a raw report from the channel.
    ///
    /// If the buffer is not large enough to fit the whole report, its remainder
    /// should be discarded and must not be returned by any succeeding call to
    /// [`Self::read_report`].
    ///
    /// Returns the exact amount or read bytes on success.
    fn read_report(
        &self,
        buf: &mut [u8],
    ) -> impl Future<Output = Result<usize, Self::Error>> + Send;

    /// If the implementation already knows whether the underlying HID channel
    /// supports HID++ messages, it should return `Some((supports_short,
    /// supports_long))` from this method.
    ///
    /// In this case, the report descriptor will not be read and parsed.
    fn supports_short_long_hidpp(&self) -> Option<(bool, bool)>;

    /// Retrieves the raw HID report descriptor from the channel.
    ///
    /// This is used to determine whether the channel supports HID++.
    ///
    /// Returns the exact size of the report descriptor on success.
    fn get_report_descriptor(
        &self,
        buf: &mut [u8],
    ) -> impl Future<Output = Result<usize, Self::Error>> + Send;
}

/// Checks whether a raw channel supports short or long HID++ messages.
async fn supports_short_long_hidpp<T: RawHidChannel>(
    chan: &T,
) -> Result<(bool, bool), ChannelError<T::Error>> {
    if let Some((supports_short, supports_long)) = chan.supports_short_long_hidpp() {
        return Ok((supports_short, supports_long));
    }

    let mut raw_descriptor = vec![0u8; MAX_REPORT_DESCRIPTOR_LENGTH];
    let descriptor_size = chan.get_report_descriptor(&mut raw_descriptor).await?;

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

/// Represents an unversioned HID++ message.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HidppMessage {
    /// Represents a short HID++ message.
    ///
    /// Please check [`HidppChannel::supports_short`] before sending this kind
    /// of message.
    Short([u8; SHORT_REPORT_LENGTH - 1]),

    /// Represents a long HID++ message.
    ///
    /// Please check [`HidppChannel::supports_long`] before sending this kind of
    /// message.
    Long([u8; LONG_REPORT_LENGTH - 1]),
}

impl HidppMessage {
    /// Tries to read a HID++ message from raw data.
    pub fn read_raw(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        if data[0] == SHORT_REPORT_ID {
            if data.len() != SHORT_REPORT_LENGTH {
                return None;
            }

            return Some(HidppMessage::Short(data[1..].try_into().unwrap()));
        } else if data[0] == LONG_REPORT_ID {
            if data.len() != LONG_REPORT_LENGTH {
                return None;
            }

            return Some(HidppMessage::Long(data[1..].try_into().unwrap()));
        }

        None
    }

    /// Writes a HID++ message in its raw byte form into a buffer.
    ///
    /// Returns the amount of written bytes.
    pub fn write_raw(&self, buf: &mut [u8]) -> usize {
        match self {
            Self::Short(payload) => {
                buf[0] = SHORT_REPORT_ID;
                buf[1..SHORT_REPORT_LENGTH].copy_from_slice(payload);
                SHORT_REPORT_LENGTH
            },
            Self::Long(payload) => {
                buf[0] = LONG_REPORT_ID;
                buf[1..LONG_REPORT_LENGTH].copy_from_slice(payload);
                LONG_REPORT_LENGTH
            },
        }
    }
}

/// Represents a HID communication channel supporting HID++.
pub struct HidppChannel<T: RawHidChannel> {
    /// Whether the channel supports short (7 bytes) HID++ messages.
    pub supports_short: bool,

    /// Whether the channel supports long (20 bytes) HID++ messages.
    pub supports_long: bool,

    /// The underlying raw HID channel.
    raw_channel: Arc<T>,

    /// All sent messages that are waiting for a response.
    pending_messages: Arc<Mutex<VecDeque<PendingMessage>>>,

    /// The sender signaling the read thread to stop.
    read_thread_close: Option<oneshot::Sender<()>>,

    /// The handle to the read thread. Should be joined after signaling
    /// [`Self::read_thread_close`].
    read_thread_hdl: Option<JoinHandle<()>>,
}

impl<T: RawHidChannel> Drop for HidppChannel<T> {
    fn drop(&mut self) {
        if let Some(read_thread_close) = self.read_thread_close.take() {
            // This only fails if the receiving end, which is owned by the read thread in
            // this case, is dropped.
            // This just means that the read thread is already stopped, so we can ignore the
            // error here.
            let _ = read_thread_close.send(());
        }

        if let Some(read_thread_hdl) = self.read_thread_hdl.take() {
            read_thread_hdl.join().unwrap();
        }
    }
}

/// Represents a message that was sent and is waiting for a response.
struct PendingMessage {
    /// The predicate that has to match for an incoming message to be classified
    /// as the response.
    response_predicate: Box<dyn Fn(&HidppMessage) -> bool + Send>,

    /// The oneshot sender used to provide the response message to the receiving
    /// end.
    sender: oneshot::Sender<HidppMessage>,
}

impl<T: RawHidChannel> HidppChannel<T> {
    /// Tries to construct a HID++ channel from a raw HID channel.
    ///
    /// If the given HID channel does not support HID++,
    /// [`ChannelError::HidppNotSupported`] will be returned.
    pub async fn of_raw_channel(raw: T) -> Result<Self, ChannelError<T::Error>> {
        let (supports_short, supports_long) = supports_short_long_hidpp(&raw).await?;

        if !supports_short && !supports_long {
            return Err(ChannelError::HidppNotSupported);
        }

        let raw_channel_rc = Arc::new(raw);
        let pending_messages_rc = Arc::new(Mutex::new(VecDeque::<PendingMessage>::new()));

        let (close_sender, mut close_receiver) = oneshot::channel::<()>();

        let read_thread_hdl = thread::spawn({
            let raw_channel = Arc::clone(&raw_channel_rc);
            let pending_messages = Arc::clone(&pending_messages_rc);

            move || {
                futures::executor::block_on(async {
                    let mut buf = [0u8; MAX_REPORT_LENGTH];

                    loop {
                        let res = select! {
                            _ = close_receiver => {
                                break;
                            },
                            res = raw_channel.read_report(&mut buf).fuse() => res
                        };

                        let Ok(len) = res else {
                            continue;
                        };

                        let Some(msg) = HidppMessage::read_raw(&buf[..len]) else {
                            continue;
                        };

                        let Ok(mut guard) = pending_messages.lock() else {
                            continue;
                        };

                        if let Some(pos) = guard
                            .iter()
                            .position(|elem| (elem.response_predicate)(&msg))
                        {
                            let waiting = guard.remove(pos).unwrap();
                            let _ = waiting.sender.send(msg);
                        }
                    }
                });
            }
        });

        Ok(Self {
            supports_short,
            supports_long,
            raw_channel: raw_channel_rc,
            pending_messages: pending_messages_rc,
            read_thread_close: Some(close_sender),
            read_thread_hdl: Some(read_thread_hdl),
        })
    }

    /// Checks whether the channel supports the given HID++ message.
    pub fn supports_msg(&self, msg: &HidppMessage) -> bool {
        match msg {
            HidppMessage::Short(_) => self.supports_short,
            HidppMessage::Long(_) => self.supports_long,
        }
    }

    /// Sends a HID++ message across the channel and waits for a response.
    ///
    /// If no response is expected/required, use [`Self::send_and_forget`].
    ///
    /// The future resolves to `Ok(None)` if no response was received.
    pub async fn send(
        &self,
        msg: HidppMessage,
        response_predicate: impl Fn(&HidppMessage) -> bool + Send + 'static,
    ) -> Result<Option<HidppMessage>, ChannelError<T::Error>> {
        if !self.supports_msg(&msg) {
            return Err(ChannelError::MessageTypeNotSupported);
        }

        let (sender, receiver) = oneshot::channel::<HidppMessage>();

        self.pending_messages
            .lock()
            .unwrap()
            .push_back(PendingMessage {
                response_predicate: Box::new(response_predicate),
                sender,
            });

        self.send_and_forget(msg).await?;

        Ok(receiver.await.ok())
    }

    /// Sends a HID++ message across the channel and does not wait for a
    /// response.
    ///
    /// If a response is expected, use [`Self::send`],
    pub async fn send_and_forget(&self, msg: HidppMessage) -> Result<(), ChannelError<T::Error>> {
        if !self.supports_msg(&msg) {
            return Err(ChannelError::MessageTypeNotSupported);
        }

        let mut buf = [0u8; LONG_REPORT_LENGTH];
        let len = msg.write_raw(&mut buf);
        self.raw_channel
            .write_report(&buf[..len])
            .await
            .map(|_| ())
            .map_err(ChannelError::Implementation)
    }
}

/// Represents an error that occurred when creating or interacting with a HID or
/// HID++ communication channel.
#[derive(Debug, Error)]
pub enum ChannelError<T: Error> {
    /// Indicates that the concrete implementation of [`RawHidChannel`] returned
    /// an error of type [`RawHidChannel::Error`].
    #[error("the HID channel implementation returned an error")]
    Implementation(#[from] T),

    /// Indicates that the HID report descriptor could not be parsed.
    #[error("the report descriptor could not be parsed")]
    ReportDescriptor(hidreport::ParserError),

    /// Indicates that the channel in question does not support HID++.
    #[error("the HID channel does not support HID++")]
    HidppNotSupported,

    /// Indicates that the HID++ channel does not support messages of the given
    /// type (short/long).
    #[error("the channel does not support the given HID++ message type")]
    MessageTypeNotSupported,
}
