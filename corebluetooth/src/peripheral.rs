use std::any::Any;
use std::os::unix::net::UnixStream;

use btuuid::BluetoothUuid;
use dispatch_executor::{SyncClone, SyncDrop};
use objc2::rc::{Retained, RetainedFromIterator};
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, DefinedClass, Message, define_class, msg_send};
use objc2_core_bluetooth::{
    CBCharacteristic, CBCharacteristicWriteType, CBDescriptor, CBL2CAPChannel, CBPeer,
    CBPeripheral, CBPeripheralDelegate, CBPeripheralState, CBService,
};
use objc2_foundation::{NSArray, NSData, NSError, NSNumber, NSObject, NSObjectProtocol};
use uuid::Uuid;

use crate::characteristic::Characteristic;
use crate::descriptor::Descriptor;
use crate::error::{Error, Result};
use crate::l2cap_channel::L2capChannel;
use crate::service::Service;
use crate::util::to_cbuuid;

#[derive(Clone)]
pub struct Peripheral {
    pub(crate) peripheral: Retained<CBPeripheral>,
    _delegate: Retained<PeripheralDelegateBridge>,
}

unsafe impl SyncDrop for Peripheral {}
unsafe impl SyncClone for Peripheral {}

impl std::fmt::Debug for Peripheral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Peripheral")
            .field("peripheral", &self.peripheral)
            .finish()
    }
}

impl PartialEq for Peripheral {
    fn eq(&self, other: &Self) -> bool {
        self.peripheral == other.peripheral
    }
}

impl Eq for Peripheral {}

impl std::hash::Hash for Peripheral {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.peripheral.hash(state);
    }
}

impl TryFrom<Retained<CBPeer>> for Peripheral {
    type Error = Retained<CBPeer>;

    fn try_from(value: Retained<CBPeer>) -> std::result::Result<Self, Self::Error> {
        Ok(Peripheral::new(value.downcast()?))
    }
}

impl Peripheral {
    pub(crate) fn init(
        peripheral: Retained<CBPeripheral>,
        delegate_factory: impl FnOnce() -> Box<dyn PeripheralDelegate>,
    ) -> Self {
        let delegate = if let Some(delegate) =
            unsafe { peripheral.delegate() }.and_then(|delegate| delegate.downcast().ok())
        {
            delegate
        } else {
            PeripheralDelegateBridge::new(delegate_factory())
        };

        unsafe { peripheral.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };

        Peripheral {
            peripheral,
            _delegate: delegate,
        }
    }

    pub(crate) fn new(peripheral: Retained<CBPeripheral>) -> Self {
        let delegate = unsafe { peripheral.delegate() }
            .and_then(|delegate| delegate.downcast().ok())
            .unwrap();

        Peripheral {
            peripheral,
            _delegate: delegate,
        }
    }

    pub fn delegate(&self) -> &dyn PeripheralDelegate {
        &*self._delegate.ivars().delegate
    }

    pub fn identifier(&self) -> Uuid {
        let uuid = unsafe { self.peripheral.identifier() };
        Uuid::from_bytes(uuid.as_bytes())
    }

    pub fn name(&self) -> Option<String> {
        let name = unsafe { self.peripheral.name() };
        name.map(|x| x.to_string())
    }

    pub fn discover_services(&self, services: Option<&[BluetoothUuid]>) {
        let services =
            services.map(|uuids| NSArray::retained_from_iter(uuids.iter().map(to_cbuuid)));

        unsafe { self.peripheral.discoverServices(services.as_deref()) };
    }

    pub fn services(&self) -> Option<Vec<Service>> {
        let services = unsafe { self.peripheral.services() };
        services.map(|x| x.iter().map(Service::new).collect())
    }

    pub fn discover_included_services(
        &self,
        service: &Service,
        services: Option<&[BluetoothUuid]>,
    ) {
        let services =
            services.map(|uuids| NSArray::retained_from_iter(uuids.iter().map(to_cbuuid)));

        unsafe {
            self.peripheral
                .discoverIncludedServices_forService(services.as_deref(), &service.service)
        };
    }

    pub fn discover_characteristics(
        &self,
        service: &Service,
        characteristics: Option<&[BluetoothUuid]>,
    ) {
        let characteristics =
            characteristics.map(|uuids| NSArray::retained_from_iter(uuids.iter().map(to_cbuuid)));

        unsafe {
            self.peripheral
                .discoverCharacteristics_forService(characteristics.as_deref(), &service.service)
        };
    }

    pub fn discover_descriptors(&self, characteristic: &Characteristic) {
        unsafe {
            self.peripheral
                .discoverDescriptorsForCharacteristic(&characteristic.characteristic)
        };
    }

    pub fn read_characteristic_value(&self, characteristic: &Characteristic) {
        unsafe {
            self.peripheral
                .readValueForCharacteristic(&characteristic.characteristic)
        };
    }

    pub fn read_descriptor_value(&self, descriptor: &Descriptor) {
        unsafe {
            self.peripheral
                .readValueForDescriptor(&descriptor.descriptor)
        };
    }

    pub fn write_characteristic_value(
        &self,
        characteristic: &Characteristic,
        data: Vec<u8>,
        write_type: CharacteristicWriteType,
    ) {
        let data = NSData::from_vec(data);
        let write_type = match write_type {
            CharacteristicWriteType::WithResponse => CBCharacteristicWriteType::WithResponse,
            CharacteristicWriteType::WithoutResponse => CBCharacteristicWriteType::WithoutResponse,
        };

        unsafe {
            self.peripheral.writeValue_forCharacteristic_type(
                &data,
                &characteristic.characteristic,
                write_type,
            );
        }
    }

    pub fn write_descriptor_value(&self, descriptor: &Descriptor, data: Vec<u8>) {
        let data = NSData::from_vec(data);

        unsafe {
            self.peripheral
                .writeValue_forDescriptor(&data, &descriptor.descriptor);
        }
    }

    pub fn max_write_value_len(&self, write_type: CharacteristicWriteType) -> usize {
        let write_type = match write_type {
            CharacteristicWriteType::WithResponse => CBCharacteristicWriteType::WithResponse,
            CharacteristicWriteType::WithoutResponse => CBCharacteristicWriteType::WithoutResponse,
        };
        unsafe { self.peripheral.maximumWriteValueLengthForType(write_type) }
    }

    pub fn set_notify(&self, characteristic: &Characteristic, notify: bool) {
        unsafe {
            self.peripheral
                .setNotifyValue_forCharacteristic(notify, &characteristic.characteristic);
        }
    }

    pub fn state(&self) -> CBPeripheralState {
        unsafe { self.peripheral.state() }
    }

    pub fn can_send_write_without_repsonse(&self) -> bool {
        unsafe { self.peripheral.canSendWriteWithoutResponse() }
    }

    pub fn read_rssi(&self) {
        unsafe { self.peripheral.readRSSI() };
    }

    pub fn open_l2cap_channel(&self, psm: u16) {
        unsafe { self.peripheral.openL2CAPChannel(psm) };
    }

    pub fn ancs_authorized(&self) -> bool {
        unsafe { self.peripheral.ancsAuthorized() }
    }
}

#[allow(unused_variables)]
pub trait PeripheralDelegate: Any {
    fn did_update_name(&self, peripheral: Peripheral) {}

    fn did_modify_services(&self, peripheral: Peripheral, invalidated_services: Vec<Service>) {}

    fn did_read_rssi(&self, peripheral: Peripheral, rssi: Result<i16>) {}

    fn did_discover_services(&self, peripheral: Peripheral, result: Result<()>) {}

    fn did_discover_included_services(
        &self,
        peripheral: Peripheral,
        service: Service,
        result: Result<()>,
    ) {
    }

    fn did_discover_characteristics(
        &self,
        peripheral: Peripheral,
        service: Service,
        result: Result<()>,
    ) {
    }

    fn did_update_value_for_characteristic(
        &self,
        peripheral: Peripheral,
        characteristic: Characteristic,
        result: Result<()>,
    ) {
    }

    fn did_write_value_for_characteristic(
        &self,
        peripheral: Peripheral,
        characteristic: Characteristic,
        result: Result<()>,
    ) {
    }

    fn did_update_notification_state_for_characteristic(
        &self,
        peripheral: Peripheral,
        characteristic: Characteristic,
        result: Result<()>,
    ) {
    }

    fn did_discover_descriptors_for_characteristic(
        &self,
        peripheral: Peripheral,
        characteristic: Characteristic,
        result: Result<()>,
    ) {
    }

    fn did_update_value_for_descriptor(
        &self,
        peripheral: Peripheral,
        descriptor: Descriptor,
        result: Result<()>,
    ) {
    }

    fn did_write_value_for_descriptor(
        &self,
        peripheral: Peripheral,
        descriptor: Descriptor,
        result: Result<()>,
    ) {
    }

    fn is_ready_to_send_write_without_response(&self, peripheral: Peripheral) {}

    fn did_open_l2cap_channel(
        &self,
        peripheral: Peripheral,
        result: Result<(L2capChannel<Peripheral>, UnixStream)>,
    ) {
    }
}

struct PeripheralDelegateIvars {
    delegate: Box<dyn PeripheralDelegate>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = PeripheralDelegateIvars]
    struct PeripheralDelegateBridge;

    unsafe impl NSObjectProtocol for PeripheralDelegateBridge {}

    #[allow(non_snake_case)]
    unsafe impl CBPeripheralDelegate for PeripheralDelegateBridge {
        #[unsafe(method(peripheralDidUpdateName:))]
        unsafe fn peripheralDidUpdateName(&self, peripheral: &CBPeripheral) {
            self.ivars()
                .delegate
                .did_update_name(Peripheral::new(peripheral.retain()));
        }

        #[unsafe(method(peripheral:didModifyServices:))]
        unsafe fn peripheral_didModifyServices(
            &self,
            peripheral: &CBPeripheral,
            invalidated_services: &NSArray<CBService>,
        ) {
            let invalidated_services = invalidated_services.iter().map(Service::new).collect();
            self.ivars()
                .delegate
                .did_modify_services(Peripheral::new(peripheral.retain()), invalidated_services);
        }

        #[unsafe(method(peripheral:didReadRSSI:error:))]
        unsafe fn peripheral_didReadRSSI_error(
            &self,
            peripheral: &CBPeripheral,
            rssi: &NSNumber,
            error: Option<&NSError>,
        ) {
            self.ivars().delegate.did_read_rssi(
                Peripheral::new(peripheral.retain()),
                or_err(rssi.shortValue(), error),
            );
        }

        #[unsafe(method(peripheral:didDiscoverServices:))]
        unsafe fn peripheral_didDiscoverServices(
            &self,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            self.ivars()
                .delegate
                .did_discover_services(Peripheral::new(peripheral.retain()), or_err((), error));
        }

        #[unsafe(method(peripheral:didDiscoverIncludedServicesForService:error:))]
        unsafe fn peripheral_didDiscoverIncludedServicesForService_error(
            &self,
            peripheral: &CBPeripheral,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            self.ivars().delegate.did_discover_included_services(
                Peripheral::new(peripheral.retain()),
                Service::new(service.retain()),
                or_err((), error),
            );
        }

        #[unsafe(method(peripheral:didDiscoverCharacteristicsForService:error:))]
        unsafe fn peripheral_didDiscoverCharacteristicsForService_error(
            &self,
            peripheral: &CBPeripheral,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            self.ivars().delegate.did_discover_characteristics(
                Peripheral::new(peripheral.retain()),
                Service::new(service.retain()),
                or_err((), error),
            );
        }

        #[unsafe(method(peripheral:didUpdateValueForCharacteristic:error:))]
        unsafe fn peripheral_didUpdateValueForCharacteristic_error(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            self.ivars().delegate.did_update_value_for_characteristic(
                Peripheral::new(peripheral.retain()),
                Characteristic::new(characteristic.retain()),
                or_err((), error),
            );
        }

        #[unsafe(method(peripheral:didWriteValueForCharacteristic:error:))]
        unsafe fn peripheral_didWriteValueForCharacteristic_error(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            self.ivars().delegate.did_write_value_for_characteristic(
                Peripheral::new(peripheral.retain()),
                Characteristic::new(characteristic.retain()),
                or_err((), error),
            );
        }

        #[unsafe(method(peripheral:didUpdateNotificationStateForCharacteristic:error:))]
        unsafe fn peripheral_didUpdateNotificationStateForCharacteristic_error(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            self.ivars()
                .delegate
                .did_update_notification_state_for_characteristic(
                    Peripheral::new(peripheral.retain()),
                    Characteristic::new(characteristic.retain()),
                    or_err((), error),
                );
        }

        #[unsafe(method(peripheral:didDiscoverDescriptorsForCharacteristic:error:))]
        unsafe fn peripheral_didDiscoverDescriptorsForCharacteristic_error(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            self.ivars()
                .delegate
                .did_discover_descriptors_for_characteristic(
                    Peripheral::new(peripheral.retain()),
                    Characteristic::new(characteristic.retain()),
                    or_err((), error),
                );
        }

        #[unsafe(method(peripheral:didUpdateValueForDescriptor:error:))]
        unsafe fn peripheral_didUpdateValueForDescriptor_error(
            &self,
            peripheral: &CBPeripheral,
            descriptor: &CBDescriptor,
            error: Option<&NSError>,
        ) {
            self.ivars().delegate.did_update_value_for_descriptor(
                Peripheral::new(peripheral.retain()),
                Descriptor::new(descriptor.retain()),
                or_err((), error),
            );
        }

        #[unsafe(method(peripheral:didWriteValueForDescriptor:error:))]
        unsafe fn peripheral_didWriteValueForDescriptor_error(
            &self,
            peripheral: &CBPeripheral,
            descriptor: &CBDescriptor,
            error: Option<&NSError>,
        ) {
            self.ivars().delegate.did_write_value_for_descriptor(
                Peripheral::new(peripheral.retain()),
                Descriptor::new(descriptor.retain()),
                or_err((), error),
            );
        }

        #[unsafe(method(peripheralIsReadyToSendWriteWithoutResponse:))]
        unsafe fn peripheralIsReadyToSendWriteWithoutResponse(&self, peripheral: &CBPeripheral) {
            self.ivars()
                .delegate
                .is_ready_to_send_write_without_response(Peripheral::new(peripheral.retain()));
        }

        #[unsafe(method(peripheral:didOpenL2CAPChannel:error:))]
        unsafe fn peripheral_didOpenL2CAPChannel_error(
            &self,
            peripheral: &CBPeripheral,
            channel: Option<&CBL2CAPChannel>,
            error: Option<&NSError>,
        ) {
            let result = match (channel, error) {
                (Some(channel), None) => Ok(L2capChannel::<Peripheral>::new(channel.retain())),
                (None, Some(error)) => Err(Error::from_nserror(error)),
                _ => unreachable!(),
            };

            self.ivars()
                .delegate
                .did_open_l2cap_channel(Peripheral::new(peripheral.retain()), result);
        }
    }
);

impl PeripheralDelegateBridge {
    pub fn new(delegate: Box<dyn PeripheralDelegate>) -> Retained<Self> {
        let ivars = PeripheralDelegateIvars { delegate };
        let this = PeripheralDelegateBridge::alloc().set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CharacteristicWriteType {
    WithResponse,
    WithoutResponse,
}

fn or_err<T>(val: T, error: Option<&NSError>) -> Result<T> {
    match error {
        None => Ok(val),
        Some(err) => Err(Error::from_nserror(err)),
    }
}
