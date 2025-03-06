//! Provides functionality for interacting with HID++-capable interfaces
//! connected to the local machine.
//!
//! This module makes heavy use of [`hidapi`](https://docs.rs/hidapi), an
//! abstraction over the [libusb/hidapi](https://github.com/libusb/hidapi) C
//! library providing cross-platform support for interacting with HID devices.

use std::{collections::HashSet, ffi::CStr};

use hidapi::{DeviceInfo, HidApi, HidDevice, MAX_REPORT_DESCRIPTOR_SIZE};
use hidreport::{Field, Report, ReportDescriptor, Usage, UsageId, UsagePage};
use thiserror::Error;

const SHORT_REPORT_ID: u8 = 0x10;
const SHORT_REPORT_USAGE_PAGE: u16 = 0xff00;
const SHORT_REPORT_USAGE: u16 = 0x0001;

const LONG_REPORT_ID: u8 = 0x11;
const LONG_REPORT_USAGE_PAGE: u16 = 0xff00;
const LONG_REPORT_USAGE: u16 = 0x0002;

/// Represents an HID interface supporting HID++ communication.
#[derive(Debug)]
pub struct HidppInterface {
    device: HidDevice,
    pub supports_short: bool,
    pub supports_long: bool,
}

impl HidppInterface {
    /// Tries to search for all HID++-capable interfaces connected to the local
    /// machine.
    pub fn find_all() -> Result<Vec<Self>, InterfaceError> {
        let api = HidApi::new()?;

        // hidapi returns different entries for every usage of every device, resulting
        // in multiple entries for the same device.
        // We don't really care about usages, so we just collect all distinct device
        // paths for further inspection.
        let device_paths = api
            .device_list()
            .map(&DeviceInfo::path)
            .collect::<HashSet<&CStr>>();

        let mut hidpp_interfaces = Vec::<HidppInterface>::with_capacity(device_paths.len());

        for device_path in device_paths {
            let device = api.open_path(device_path)?;

            if device.supports_hidpp()? {
                hidpp_interfaces.push(Self {
                    device,
                    supports_short: false,
                    supports_long: false,
                });
            }
        }

        Ok(hidpp_interfaces)
    }
}

trait HidDeviceExt {
    fn supports_short_long_hidpp(&self) -> Result<(bool, bool), InterfaceError>;
}

impl HidDeviceExt for HidDevice {
    fn supports_short_long_hidpp(&self) -> Result<(bool, bool), InterfaceError> {
        let mut raw_descriptor = vec![0u8; MAX_REPORT_DESCRIPTOR_SIZE];
        let descriptor_size = self.get_report_descriptor(&mut raw_descriptor)?;
        let descriptor = ReportDescriptor::try_from(&raw_descriptor[..descriptor_size])?;

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
}

/// Represents anything that could theoretically be an HID++ interface.
pub trait PossibleHidppInterface {
    /// Checks whether the target supports HID++.
    fn supports_hidpp(&self) -> Result<bool, InterfaceError>;

    /// Tries to retrieve the HID++ interface representing the target.
    /// If [`Self::supports_hidpp`] returns [`false`], this method MUST return
    /// [`None`].
    fn to_hidpp_interface(self) -> Result<Option<HidppInterface>, InterfaceError>;
}

impl PossibleHidppInterface for HidDevice {
    fn supports_hidpp(&self) -> Result<bool, InterfaceError> {
        let (supports_short, supports_long) = self.supports_short_long_hidpp()?;
        Ok(supports_short || supports_long)
    }

    fn to_hidpp_interface(self) -> Result<Option<HidppInterface>, InterfaceError> {
        let (supports_short, supports_long) = self.supports_short_long_hidpp()?;
        if !supports_short && !supports_long {
            return Ok(None);
        }

        Ok(Some(HidppInterface {
            device: self,
            supports_short,
            supports_long,
        }))
    }
}

/// Represents an error that occurred when creating or interacting with an HID++
/// interface.
#[derive(Debug, Error)]
pub enum InterfaceError {
    /// Indicates that hidapi returned an error.
    #[error("hidapi returned an error")]
    HidApi(#[from] hidapi::HidError),

    /// Indicates that the HID report descriptor could not be parsed.
    #[error("the report descriptor could not be parsed")]
    ReportDescriptor(#[from] hidreport::ParserError),
}
