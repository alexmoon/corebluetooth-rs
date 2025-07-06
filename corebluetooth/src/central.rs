//! A remote central device.

use dispatch_executor::{SyncClone, SyncDrop};
use objc2::rc::Retained;
use objc2_core_bluetooth::{CBCentral, CBPeer};
use uuid::Uuid;

/// A remote central device that is connected to the local system.
///
/// This is only used when the local device is acting as a peripheral.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Central {
    central: Retained<CBCentral>,
}

unsafe impl SyncDrop for Central {}
unsafe impl SyncClone for Central {}

impl TryFrom<Retained<CBPeer>> for Central {
    type Error = Retained<CBPeer>;

    fn try_from(value: Retained<CBPeer>) -> Result<Self, Self::Error> {
        Ok(Central::new(value.downcast()?))
    }
}

impl Central {
    #[allow(dead_code)]
    pub(crate) fn new(central: Retained<CBCentral>) -> Self {
        Central { central }
    }

    /// The unique identifier of the central.
    ///
    /// See [`-[CBPeer identifier]`](https://developer.apple.com/documentation/corebluetooth/cbpeer/identifier).
    pub fn identifier(&self) -> Uuid {
        let uuid = unsafe { self.central.identifier() };
        Uuid::from_bytes(uuid.as_bytes())
    }

    /// The maximum size, in bytes, of a notification or indication that can be sent to the central.
    ///
    /// See [`-[CBCentral maximumUpdateValueLength]`](https://developer.apple.com/documentation/corebluetooth/cbcentral/maximumupdatevaluelength).
    pub fn max_value_update_len(&self) -> usize {
        unsafe { self.central.maximumUpdateValueLength() }
    }
}
