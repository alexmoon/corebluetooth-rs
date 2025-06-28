use btuuid::BluetoothUuid;
use dispatch_executor::{SyncClone, SyncDrop};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_bluetooth::CBDescriptor;
use objc2_foundation::{NSData, NSNumber, NSString, NSUTF8StringEncoding};

use crate::characteristic::Characteristic;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Descriptor {
    pub(crate) descriptor: Retained<CBDescriptor>,
}

unsafe impl SyncDrop for Descriptor {}
unsafe impl SyncClone for Descriptor {}

impl Descriptor {
    pub(crate) fn new(descriptor: Retained<CBDescriptor>) -> Self {
        Self { descriptor }
    }

    pub fn uuid(&self) -> BluetoothUuid {
        let data = unsafe { self.descriptor.UUID().data() };
        BluetoothUuid::from_be_slice(unsafe { data.as_bytes_unchecked() }).unwrap()
    }

    pub fn characteristic(&self) -> Option<Characteristic> {
        unsafe { self.descriptor.characteristic() }.map(Characteristic::new)
    }

    pub fn value(&self) -> Option<Vec<u8>> {
        let value = unsafe { self.descriptor.value() };
        value.map(|value| value_to_slice(&value))
    }
}

fn value_to_slice(val: &AnyObject) -> Vec<u8> {
    if let Some(val) = val.downcast_ref::<NSNumber>() {
        // Characteristic EXtended Properties, Client Characteristic COnfiguration, Service Characteristic Configuration, or L2CAP PSM Value Characteristic
        let n = val.as_u16();
        n.to_le_bytes().to_vec()
    } else if let Some(val) = val.downcast_ref::<NSString>() {
        // Characteristic User Description
        let ptr = val.UTF8String() as *const u8;
        let val = if ptr.is_null() {
            &[]
        } else {
            let len = val.lengthOfBytesUsingEncoding(NSUTF8StringEncoding);
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        val.to_vec()
    } else if let Some(val) = val.downcast_ref::<NSData>() {
        // All other descriptors
        val.to_vec()
    } else {
        Vec::new()
    }
}
