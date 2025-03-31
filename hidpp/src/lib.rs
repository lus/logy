//! An implementation of Logitech's HID++ protocol.
//!
//! Many of Logitech's more modern peripheral devices (mice, keyboards etc.)
//! support advanced features increasing the user experience. These include, but
//! are not limited to, things like:
//!
//! - scroll wheels dynamically switching between ratchet and freespin mode ([SmartShift](https://support.logi.com/hc/en-us/articles/360052340194-What-is-SmartShift-on-MX-Anywhere-3))
//! - [mouse gestures](https://support.logi.com/hc/en-us/articles/360023359813-How-to-customize-mouse-buttons-with-Logitech-Options#gesture)
//! - custom actions for specific mouse buttons
//! - several customizability options for keyboards, audio devices and touchpads
//!
//! All of these features can be managed using their (more or less) proprietary
//! HID++-protocol which extends standard [HID](https://en.wikipedia.org/wiki/Human_interface_device).
//!
//! Logitech kindly provided a [public Google Drive folder](https://drive.google.com/drive/folders/0BxbRzx7vEV7eWmgwazJ3NUFfQ28)
//! with a lot of documentation on HID++ and several device features. These
//! documents were heavily used during the development of this crate.

pub use async_trait::async_trait;

pub mod channel;
pub mod device;
pub mod feature;
pub mod nibble;
pub mod protocol;
