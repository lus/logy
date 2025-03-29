//! Implements peripheral devices connected to HID++ channels.

use std::{any::TypeId, collections::HashMap, error::Error, sync::Arc};

use thiserror::Error;

use crate::{
    channel::{ChannelError, HidppChannel, RawHidChannel},
    feature::{CreatableFeature, Feature, root::RootFeature},
    protocol::{self, ProtocolError, ProtocolVersion},
};

/// Represents a single HID++ device connected to a [`HidppChannel`].
///
/// This is used only for peripheral devices and not receivers.
#[derive(Clone)]
pub struct Device<T: RawHidChannel> {
    /// The underlying HID++ channel.
    chan: Arc<HidppChannel<T>>,

    /// The initialized implementation of features the device supports.
    features: HashMap<TypeId, Arc<dyn Feature<T>>>,

    /// The index of the device on the HID++ channel.
    pub device_index: u8,

    /// The supported protocol version reported by the device.
    pub protocol_version: ProtocolVersion,
}

impl<T: RawHidChannel> Device<T> {
    /// Tries to initialize a device on a HID++ channel.
    ///
    /// This will automatically ping the device to determine the protocol
    /// version it supports via [`protocol::determine_version`].
    ///
    /// Returns [`DeviceError::DeviceNotFound`] if there is no device with the
    /// specified index connected to the channel.
    ///
    /// Returns [`DeviceError::UnsupportedProtocolVersion`] if the device only
    /// supports [`ProtocolVersion::V10`].
    pub async fn new(
        chan: Arc<HidppChannel<T>>,
        device_index: u8,
    ) -> Result<Self, DeviceError<T::Error>> {
        let protocol_version = protocol::determine_version(&*chan, device_index)
            .await
            .map_err(|err| match err {
                ProtocolError::Channel(src) => DeviceError::Channel(src),
                ProtocolError::DeviceNotFound => DeviceError::DeviceNotFound,
            })?;

        if let ProtocolVersion::V10 = protocol_version {
            return Err(DeviceError::UnsupportedProtocolVersion);
        }

        let mut device = Self {
            chan: Arc::clone(&chan),
            features: HashMap::new(),
            device_index,
            protocol_version,
        };

        // Every HID++2.0 device supports the root feature.
        // We implicitly verified that using [`protocol::determine_version`].
        device.add_feature::<RootFeature<T>>(0);

        Ok(device)
    }

    /// Adds a new feature implementation to the list of available features.
    /// This will override an existing implementation of the same type.
    /// The caller is responsible for making sure the device actually supports
    /// the feature.
    pub fn add_feature_instance<F: Feature<T>>(&mut self, feature: F) -> Arc<F> {
        let feat_rc: Arc<dyn Feature<T>> = Arc::new(feature);

        self.features
            .insert(TypeId::of::<F>(), Arc::clone(&feat_rc));

        Arc::downcast::<F>(feat_rc).unwrap()
    }

    /// Adds a new feature implementation to the list of available features.
    /// This will override an existing implementation of the same type.
    /// The caller is responsible for making sure the device actually supports
    /// the feature.
    ///
    /// This method uses [`CreatableFeature`] to automatically create an
    /// instance of the feature implementation and adds it using
    /// [`Self::add_feature_instance`].
    pub fn add_feature<F: CreatableFeature<T>>(&mut self, feature_index: u8) -> Arc<F> {
        self.add_feature_instance(F::new(
            Arc::clone(&self.chan),
            self.device_index,
            feature_index,
        ))
    }

    /// Checks whether a specific feature implementation is provided by the
    /// device.
    pub fn provides_feature<F: Feature<T>>(&self) -> bool {
        self.features.contains_key(&TypeId::of::<F>())
    }

    /// Tries to retrieve a feature implementation from the device.
    ///
    /// Returns [`None`] if the requested feature implementation is not
    /// provided.
    pub fn get_feature<F: Feature<T>>(&self) -> Option<Arc<F>> {
        self.features
            .get(&TypeId::of::<F>())
            .cloned()
            .and_then(|feat| Arc::downcast::<F>(feat).ok())
    }
}

/// Represents a device-specific error.
#[derive(Debug, Error)]
pub enum DeviceError<T: Error> {
    /// Indicates that the underlying [`HidppChannel`] returned an error.
    #[error("the HID++ channel returned an error")]
    Channel(#[from] ChannelError<T>),

    /// Indicates that the specified device index points to no device.
    #[error("there is no device with the specified device index")]
    DeviceNotFound,

    /// Indicates that the addressed device does only support HID++1.0.
    #[error("the device does not support HID++2.0 or newer")]
    UnsupportedProtocolVersion,
}
