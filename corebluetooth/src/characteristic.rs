//! A GATT characteristic.

use btuuid::BluetoothUuid;
use dispatch_executor::{SyncClone, SyncDrop};
use objc2::rc::Retained;
use objc2_core_bluetooth::{CBCharacteristic, CBCharacteristicProperties};

use crate::descriptor::Descriptor;
use crate::service::Service;

/// A characteristic of a remote peripheral's service.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Characteristic {
    pub(crate) characteristic: Retained<CBCharacteristic>,
}

unsafe impl SyncDrop for Characteristic {}
unsafe impl SyncClone for Characteristic {}

impl Characteristic {
    pub(crate) fn new(characteristic: Retained<CBCharacteristic>) -> Self {
        Self { characteristic }
    }

    /// The Bluetooth-specific UUID of the characteristic.
    ///
    /// See [`-[CBAttribute UUID]`](https://developer.apple.com/documentation/corebluetooth/cbattribute/uuid).
    pub fn uuid(&self) -> BluetoothUuid {
        let data = unsafe { self.characteristic.UUID().data() };
        BluetoothUuid::from_be_slice(unsafe { data.as_bytes_unchecked() }).unwrap()
    }

    /// The service that this characteristic belongs to.
    ///
    /// See [`-[CBCharacteristic service]`](https://developer.apple.com/documentation/corebluetooth/cbcharacteristic/service).
    pub fn service(&self) -> Option<Service> {
        unsafe { self.characteristic.service() }.map(Service::new)
    }

    /// The most recent value of the characteristic.
    ///
    /// See [`-[CBCharacteristic value]`](https://developer.apple.com/documentation/corebluetooth/cbcharacteristic/value).
    pub fn value(&self) -> Option<Vec<u8>> {
        unsafe { self.characteristic.value() }.map(|x| x.to_vec())
    }

    /// The descriptors for this characteristic.
    ///
    /// See [`-[CBCharacteristic descriptors]`](https://developer.apple.com/documentation/corebluetooth/cbcharacteristic/descriptors).
    pub fn descriptors(&self) -> Option<Vec<Descriptor>> {
        let descriptors = unsafe { self.characteristic.descriptors() };
        descriptors.map(|x| x.iter().map(Descriptor::new).collect())
    }

    /// The properties of the characteristic.
    ///
    /// See [`-[CBCharacteristic properties]`](https://developer.apple.com/documentation/corebluetooth/cbcharacteristic/properties).
    pub fn properties(&self) -> CBCharacteristicProperties {
        unsafe { self.characteristic.properties() }
    }

    /// Whether the characteristic is currently notifying.
    ///
    /// See [`-[CBCharacteristic isNotifying]`](https://developer.apple.com/documentation/corebluetooth/cbcharacteristic/isnotifying).
    pub fn is_notifying(&self) -> bool {
        unsafe { self.characteristic.isNotifying() }
    }
}
