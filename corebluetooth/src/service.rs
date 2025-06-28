use btuuid::BluetoothUuid;
use dispatch_executor::{SyncClone, SyncDrop};
use objc2::rc::Retained;
use objc2_core_bluetooth::CBService;

use crate::characteristic::Characteristic;
use crate::peripheral::Peripheral;

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

    pub fn uuid(&self) -> BluetoothUuid {
        let data = unsafe { self.service.UUID().data() };
        BluetoothUuid::from_be_slice(unsafe { data.as_bytes_unchecked() }).unwrap()
    }

    pub fn peripheral(&self) -> Option<Peripheral> {
        unsafe { self.service.peripheral() }.map(Peripheral::new)
    }

    pub fn is_primary(&self) -> bool {
        unsafe { self.service.isPrimary() }
    }

    pub fn characteristics(&self) -> Option<Vec<Characteristic>> {
        let characteristics = unsafe { self.service.characteristics() };
        characteristics.map(|x| x.iter().map(Characteristic::new).collect())
    }

    pub fn included_services(&self) -> Option<Vec<Service>> {
        let services = unsafe { self.service.includedServices() };
        services.map(|x| x.iter().map(Service::new).collect())
    }
}
