# hidpp

[![docs.rs](https://img.shields.io/docsrs/hidpp)](https://docs.rs/hidpp)
[![Crates.io Version](https://img.shields.io/crates/v/hidpp)](https://crates.io/crates/hidpp)

This crate implements the HID++ protocol used by a lot of Logitech peripheral devices and wireless receivers.
It is the protocol backing their Windows- and Mac-only Options+ software used to configure the different device features.

## Quickstart

```rs
use std::sync::Arc;

use hidpp::{
    channel::HidppChannel,
    device::Device,
    feature::{
        CreatableFeature,
        EmittingFeature,
        feature_set::v0::FeatureSetFeatureV0,
        thumbwheel::v0::{ThumbwheelEvent, ThumbwheelFeatureV0, ThumbwheelReportingMode},
    },
    nibble::U4,
    receiver::{self, Receiver, bolt::BoltEvent},
};

// First, we will create the HID++ channel.
// This function will return `ChannelError::HidppNotSupported`
// if the passed HID channel does not support HID++.
let channel = Arc::new(
    HidppChannel::from_raw_channel(my_hid_channel)
        .await
        .expect("could not establish HID++ communication"),
);

// HID++2.0 includes an arbitrary "software ID" in every message.
// This ID is meant to differentiate messages of different
// softwares, but it can also be used to ease the mapping of
// incoming messages to previously sent outgoing messages by
// rotating it after every sent message.
// By default, the software ID is `0x01` and will not rotate.
channel.set_rotating_sw_id(true);

// You can also set a custom software ID.
channel.set_sw_id(U4::from_lo(0xa));

// If a wireless receiver is handling the HID++ communication,
// we can detect it.
let receiver = receiver::detect(Arc::clone(&channel)).expect("no receiver was found");

// Assuming we have a Bolt receiver, we will now detect all connected devices.
let Receiver::Bolt(bolt) = receiver else {
    panic!("no Bolt receiver");
};
tokio::spawn({
    let rx = bolt.listen();

    async move {
        while let Ok(BoltEvent::DeviceConnection(event)) = rx.recv() {
            println!("Paired device found: {:x?}", event);
        }
    }
});
bolt.trigger_device_arrival()
    .await
    .expect("could not trigger device arrival notification");

// Let's say we found a device with the index 0x02 using this enumeration. We
// can now initialize it:
let mut device = Device::new(Arc::clone(&channel), 0x02)
    .await
    .expect("could not initialize device");

// The device is a HID++2.0 one, meaning it supports so-called HID++2.0
// features. Every device supports the standardized `IRoot` feature, which
// we can access like this:
let root = device.root();
assert_eq!(0x2, root.ping(0x2).await.unwrap());

// Additional features are accessed by their feature index in the
// device-internal feature map, not by their globally unique feature ID.
// The root feature also supports looking up a specific feature by its ID.
// The resulting value will contain some information about the feature,
// including its index:
let info = root
    .get_feature(FeatureSetFeatureV0::ID)
    .await
    .expect("could not look up feature")
    .expect("FeatureSet feature is not supported");

// As there are a lot of possible features and a given device only supports a
// small subset of these, looking up every single feature ID using this
// technique is not practicable. That's why the `IFeatureSet` feature can be
// used to enumerate over all supported features, but only if this feature
// itself is supported by the device.
let infos = device
    .enumerate_features()
    .await
    .expect("could not look up features")
    .expect("FeatureSet feature is not supported");

// This crate provides Rust implementations for many HID++2.0 features. A
// registry in the `hidpp::feature::registry` module maintains a list of all
// known features and, if provided, a link to its implementation. The
// `enumerate_features` function we just called automatically registers these
// implementations for our device and we can now access them like this:
let thumbwheel = device
    .get_feature::<ThumbwheelFeatureV0>()
    .expect("Thumbwheel feature is not supported");
thumbwheel
    .set_thumbwheel_reporting(ThumbwheelReportingMode::Diverted, false)
    .await
    .expect("could not divert thumbwheel");
```
