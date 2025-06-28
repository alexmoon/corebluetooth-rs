use btuuid::BluetoothUuid;
use dispatch_executor::{SyncClone, SyncDrop};
use objc2::rc::Retained;
use objc2_core_bluetooth::{CBCharacteristic, CBCharacteristicProperties};

use crate::descriptor::Descriptor;
use crate::service::Service;

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

    pub fn uuid(&self) -> BluetoothUuid {
        let data = unsafe { self.characteristic.UUID().data() };
        BluetoothUuid::from_be_slice(unsafe { data.as_bytes_unchecked() }).unwrap()
    }

    pub fn service(&self) -> Option<Service> {
        unsafe { self.characteristic.service() }.map(Service::new)
    }

    pub fn value(&self) -> Option<Vec<u8>> {
        unsafe { self.characteristic.value() }.map(|x| x.to_vec())
    }

    pub fn descriptors(&self) -> Option<Vec<Descriptor>> {
        let descriptors = unsafe { self.characteristic.descriptors() };
        descriptors.map(|x| x.iter().map(Descriptor::new).collect())
    }

    pub fn properties(&self) -> CBCharacteristicProperties {
        unsafe { self.characteristic.properties() }
    }

    pub fn is_notifying(&self) -> bool {
        unsafe { self.characteristic.isNotifying() }
    }
}
