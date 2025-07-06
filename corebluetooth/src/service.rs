//! A GATT service.

use btuuid::BluetoothUuid;
use dispatch_executor::{SyncClone, SyncDrop};
use objc2::rc::Retained;
use objc2_core_bluetooth::CBService;

use crate::characteristic::Characteristic;
use crate::peripheral::Peripheral;

/// A GATT service.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Service {
    pub(crate) service: Retained<CBService>,
}

unsafe impl SyncDrop for Service {}
unsafe impl SyncClone for Service {}

impl Service {
    pub(crate) fn new(service: Retained<CBService>) -> Self {
        Self { service }
    }

    /// The UUID of the service.
    ///
    /// See [`-[CBAttribute UUID]`](https://developer.apple.com/documentation/corebluetooth/cbattribute/uuid).
    pub fn uuid(&self) -> BluetoothUuid {
        let data = unsafe { self.service.UUID().data() };
        BluetoothUuid::from_be_slice(unsafe { data.as_bytes_unchecked() }).unwrap()
    }

    /// The peripheral that this service belongs to.
    ///
    /// See [`-[CBService peripheral]`](https://developer.apple.com/documentation/corebluetooth/cbservice/peripheral).
    pub fn peripheral(&self) -> Option<Peripheral> {
        unsafe { self.service.peripheral() }.map(Peripheral::new)
    }

    /// Whether this is a primary service.
    ///
    /// See [`-[CBService isPrimary]`](https://developer.apple.com/documentation/corebluetooth/cbservice/isprimary).
    pub fn is_primary(&self) -> bool {
        unsafe { self.service.isPrimary() }
    }

    /// The characteristics of this service.
    ///
    /// See [`-[CBService characteristics]`](https://developer.apple.com/documentation/corebluetooth/cbservice/characteristics).
    pub fn characteristics(&self) -> Option<Vec<Characteristic>> {
        let characteristics = unsafe { self.service.characteristics() };
        characteristics.map(|x| x.iter().map(Characteristic::new).collect())
    }

    /// The included services of this service.
    ///
    /// See [`-[CBService includedServices]`](https://developer.apple.com/documentation/corebluetooth/cbservice/includedservices).
    pub fn included_services(&self) -> Option<Vec<Service>> {
        let services = unsafe { self.service.includedServices() };
        services.map(|x| x.iter().map(Service::new).collect())
    }
}
