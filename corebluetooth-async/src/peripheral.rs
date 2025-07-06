use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Deref;
use std::os::unix::net::UnixStream;

use btuuid::BluetoothUuid;
use corebluetooth::Result as CBResult;
use corebluetooth::{
    Characteristic, CharacteristicWriteType, Descriptor, L2capChannel, Peripheral,
    PeripheralDelegate, Service,
};
use dispatch_executor::{SyncClone, SyncDrop};
use futures_channel::oneshot;
use objc2::rc::Retained;
use objc2_core_bluetooth::CBPeer;

use crate::error::Result;
use crate::util::{BroadcastReceiver, BroadcastSender, broadcast, watch};

/// An asynchronous wrapper around a [`Peripheral`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeripheralAsync {
    inner: Peripheral,
}

unsafe impl SyncDrop for PeripheralAsync {}
unsafe impl SyncClone for PeripheralAsync {}

impl TryFrom<Retained<CBPeer>> for PeripheralAsync {
    type Error = Retained<CBPeer>;

    fn try_from(value: Retained<CBPeer>) -> std::result::Result<Self, Self::Error> {
        Ok(PeripheralAsync {
            inner: Peripheral::try_from(value)?,
        })
    }
}

impl From<Peripheral> for PeripheralAsync {
    fn from(inner: Peripheral) -> Self {
        Self { inner }
    }
}

impl Deref for PeripheralAsync {
    type Target = Peripheral;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl PeripheralAsync {
    /// Creates a new `PeripheralAsync` from a `Peripheral`.
    ///
    /// # Panics
    ///
    /// This will panic if the delegate of the `Peripheral` is not a `PeripheralAsyncDelegate`.
    pub fn new(inner: Peripheral) -> Self {
        let delegate: &dyn Any = inner.delegate();
        assert!(delegate.is::<PeripheralAsyncDelegate>());
        PeripheralAsync { inner }
    }

    pub(crate) fn new_unchecked(inner: Peripheral) -> Self {
        PeripheralAsync { inner }
    }

    fn delegate(&self) -> &PeripheralAsyncDelegate {
        let delegate: &dyn Any = self.inner.delegate();
        delegate.downcast_ref().unwrap()
    }

    /// Waits for the peripheral's name to change.
    pub async fn name_changed(&self) -> BroadcastReceiver<Option<String>> {
        self.delegate().name_updates()
    }

    /// Initiates service discovery on the peripheral.
    ///
    /// If `services` is provided, only services with those UUIDs will be discovered.
    pub async fn discover_services(&self, services: Option<&[BluetoothUuid]>) -> Result<()> {
        self.inner.discover_services(services);
        let mut receiver = self.delegate().service_discovery();
        receiver.recv().await?
    }

    /// Returns a stream of service change events.
    pub fn services_changed(&self) -> async_broadcast::Receiver<Vec<Service>> {
        self.delegate().services_changed()
    }

    /// Initiates discovery of the included services of a service.
    ///
    /// The `services` parameter can limit discovery to services matching the provided
    /// UUIDs.
    ///
    /// After discovery completes, the services may be retrieved by calling
    /// [`Service::included_services()`].
    pub async fn discover_included_services(
        &self,
        service: &Service,
        services: Option<&[BluetoothUuid]>,
    ) -> Result<()> {
        self.inner.discover_included_services(service, services);
        let receiver = self.delegate().included_service_discovery(service.clone());
        receiver.await?
    }

    /// Initiates discovery of the characteristics of a service.
    ///
    /// The `characteristics` parameter can limit discovery to characteristics matching
    /// the provided UUIDs.
    ///
    /// After discovery completes, the characteristics may be retrieved by calling
    /// [`Service::characteristics()`].
    pub async fn discover_characteristics(
        &self,
        service: &Service,
        characteristics: Option<&[BluetoothUuid]>,
    ) -> Result<()> {
        self.inner
            .discover_characteristics(service, characteristics);
        let receiver = self.delegate().characteristic_discovery(service.clone());
        receiver.await?
    }

    /// Initiates discovery of the descriptors of a characteristic.
    ///
    /// After discovery completes, the characteristics may be retrieved by calling
    /// [`Characteristic::descriptors()`].
    pub async fn discover_descriptors(&self, characteristic: &Characteristic) -> Result<()> {
        self.inner.discover_descriptors(characteristic);
        let receiver = self.delegate().descriptor_discovery(characteristic.clone());
        receiver.await?
    }

    /// Reads the value of a characteristic.
    pub async fn read_characteristic_value(
        &self,
        characteristic: &Characteristic,
    ) -> Result<Vec<u8>> {
        self.inner.read_characteristic_value(characteristic);
        self.delegate()
            .characteristic_value_updates(characteristic.clone())
            .recv()
            .await?
    }

    /// Reads the value of a descriptor.
    pub async fn read_descriptor_value(&self, descriptor: &Descriptor) -> Result<Vec<u8>> {
        self.inner.read_descriptor_value(descriptor);
        self.delegate()
            .descriptor_value_updates(descriptor.clone())
            .await?
    }

    /// Writes the value of a characteristic.
    pub async fn write_characteristic_value(
        &self,
        characteristic: &Characteristic,
        data: Vec<u8>,
        write_type: CharacteristicWriteType,
    ) -> Result<()> {
        self.inner
            .write_characteristic_value(characteristic, data, write_type);
        self.delegate()
            .register_characteristic_value_write(characteristic.clone())
            .await?
    }

    /// Writes the value of a descriptor.
    pub async fn write_descriptor_value(
        &self,
        descriptor: &Descriptor,
        data: Vec<u8>,
    ) -> Result<()> {
        self.inner.write_descriptor_value(descriptor, data);
        self.delegate()
            .register_descriptor_value_write(descriptor.clone())
            .await?
    }

    /// Enables or disables notifications for a characteristic.
    pub async fn set_notify(&self, characteristic: &Characteristic, notify: bool) -> Result<bool> {
        self.inner.set_notify(characteristic, notify);
        self.delegate()
            .register_notification_update(characteristic.clone())
            .await?
    }

    /// Returns a stream of value updates for a characteristic.
    ///
    /// The characteristic value may be updated either as the result of a call to
    /// [`read_characteristic_value()`][Peripheral::read_characteristic_value] or a notification or indication from the
    /// peripheral if notifications have been enabled by a call to [`set_notify()`][Self::set_notify].
    pub fn characteristic_value_updates(
        &self,
        characteristic: &Characteristic,
    ) -> async_broadcast::Receiver<Result<Vec<u8>>> {
        self.delegate()
            .characteristic_value_updates(characteristic.clone())
    }

    /// Waits until the peripheral is ready to send a write without response.
    pub async fn ready_to_send_write_without_response(&self) -> Result<()> {
        if !self.can_send_write_without_repsonse() {
            self.delegate()
                .ready_to_send_write_without_response()
                .recv()
                .await?;
        }
        Ok(())
    }

    /// Reads the RSSI of the peripheral.
    pub async fn read_rssi(&self) -> Result<i16> {
        self.inner.read_rssi();
        let mut receiver = self.delegate().rssi_updates();
        receiver.recv().await?
    }

    /// Opens an L2CAP channel to the peripheral.
    pub async fn open_l2cap_channel(&self, psm: u16) -> Result<(L2capChannel<Self>, UnixStream)> {
        self.inner.open_l2cap_channel(psm);
        let receiver = self.delegate().register_l2cap_channel_open();
        receiver.await?
    }
}

type OneshotMap<K, V> = HashMap<K, oneshot::Sender<Result<V>>>;
type L2capChannelOpenResult = Result<(L2capChannel<PeripheralAsync>, UnixStream)>;

pub(crate) struct PeripheralAsyncDelegate {
    name_updates: BroadcastSender<Option<String>>,
    services_changed: BroadcastSender<Vec<Service>>,
    rssi_updates: BroadcastSender<Result<i16>>,
    service_discovery: BroadcastSender<Result<()>>,
    included_service_discovery: RefCell<OneshotMap<Service, ()>>,
    characteristic_discovery: RefCell<OneshotMap<Service, ()>>,
    descriptor_discovery: RefCell<OneshotMap<Characteristic, ()>>,
    characteristic_value_updates:
        RefCell<HashMap<Characteristic, BroadcastSender<Result<Vec<u8>>>>>,
    notification_updates: RefCell<OneshotMap<Characteristic, bool>>,
    characteristic_writes: RefCell<OneshotMap<Characteristic, ()>>,
    descriptor_value_updates: RefCell<OneshotMap<Descriptor, Vec<u8>>>,
    descriptor_writes: RefCell<OneshotMap<Descriptor, ()>>,
    ready_to_send_write_without_response: BroadcastSender<()>,
    l2cap_channel_opened: Cell<Option<oneshot::Sender<L2capChannelOpenResult>>>,
}

impl Default for PeripheralAsyncDelegate {
    fn default() -> Self {
        let name_updates = watch();
        let services_changed = broadcast(16);
        let rssi_updates = watch();
        let service_discovery = watch();
        let ready_to_send_write_without_response = watch();

        Self {
            name_updates,
            services_changed,
            rssi_updates,
            service_discovery,
            included_service_discovery: Default::default(),
            characteristic_discovery: Default::default(),
            descriptor_discovery: Default::default(),
            notification_updates: Default::default(),
            characteristic_writes: Default::default(),
            descriptor_writes: Default::default(),
            characteristic_value_updates: Default::default(),
            descriptor_value_updates: Default::default(),
            ready_to_send_write_without_response,
            l2cap_channel_opened: Default::default(),
        }
    }
}

impl PeripheralDelegate for PeripheralAsyncDelegate {
    fn did_update_name(&self, peripheral: Peripheral) {
        let _ = self.name_updates.try_broadcast(peripheral.name());
    }

    fn did_modify_services(
        &self,
        _peripheral: Peripheral,
        invalidated_services: Vec<corebluetooth::Service>,
    ) {
        let _ = self.services_changed.try_broadcast(invalidated_services);
    }

    fn did_read_rssi(&self, _peripheral: Peripheral, rssi: CBResult<i16>) {
        let _ = self.rssi_updates.try_broadcast(rssi.map_err(Into::into));
    }

    fn did_discover_services(&self, _peripheral: Peripheral, result: CBResult<()>) {
        let _ = self
            .service_discovery
            .try_broadcast(result.map_err(Into::into));
    }

    fn did_discover_included_services(
        &self,
        _peripheral: Peripheral,
        service: corebluetooth::Service,
        result: CBResult<()>,
    ) {
        if let Some(sender) = self
            .included_service_discovery
            .borrow_mut()
            .remove(&service)
        {
            let _ = sender.send(result.map_err(Into::into));
        }
    }

    fn did_discover_characteristics(
        &self,
        _peripheral: Peripheral,
        service: corebluetooth::Service,
        result: CBResult<()>,
    ) {
        if let Some(sender) = self.characteristic_discovery.borrow_mut().remove(&service) {
            let _ = sender.send(result.map_err(Into::into));
        }
    }

    fn did_update_value_for_characteristic(
        &self,
        _peripheral: Peripheral,
        characteristic: corebluetooth::Characteristic,
        result: CBResult<()>,
    ) {
        if let Some(sender) = self
            .characteristic_value_updates
            .borrow()
            .get(&characteristic)
        {
            let update = result.map(|_| characteristic.value().unwrap());
            if sender.receiver_count() == 0 {
                self.characteristic_value_updates
                    .borrow_mut()
                    .remove(&characteristic);
            } else {
                let _ = sender.try_broadcast(update.map_err(Into::into));
            }
        }
    }

    fn did_write_value_for_characteristic(
        &self,
        _peripheral: Peripheral,
        characteristic: corebluetooth::Characteristic,
        result: CBResult<()>,
    ) {
        if let Some(sender) = self
            .characteristic_writes
            .borrow_mut()
            .remove(&characteristic)
        {
            let _ = sender.send(result.map_err(Into::into));
        }
    }

    fn did_update_notification_state_for_characteristic(
        &self,
        _peripheral: Peripheral,
        characteristic: corebluetooth::Characteristic,
        result: CBResult<()>,
    ) {
        if let Some(sender) = self
            .notification_updates
            .borrow_mut()
            .remove(&characteristic)
        {
            let result = result.map(|_| characteristic.is_notifying());
            let _ = sender.send(result.map_err(Into::into));
        }
    }

    fn did_discover_descriptors_for_characteristic(
        &self,
        _peripheral: Peripheral,
        characteristic: corebluetooth::Characteristic,
        result: CBResult<()>,
    ) {
        if let Some(sender) = self
            .descriptor_discovery
            .borrow_mut()
            .remove(&characteristic)
        {
            let _ = sender.send(result.map_err(Into::into));
        }
    }

    fn did_update_value_for_descriptor(
        &self,
        _peripheral: Peripheral,
        descriptor: corebluetooth::Descriptor,
        result: CBResult<()>,
    ) {
        if let Some(sender) = self
            .descriptor_value_updates
            .borrow_mut()
            .remove(&descriptor)
        {
            let update = result.map(|_| descriptor.value().unwrap());
            let _ = sender.send(update.map_err(Into::into));
        }
    }

    fn did_write_value_for_descriptor(
        &self,
        _peripheral: Peripheral,
        descriptor: corebluetooth::Descriptor,
        result: CBResult<()>,
    ) {
        if let Some(sender) = self.descriptor_writes.borrow_mut().remove(&descriptor) {
            let _ = sender.send(result.map_err(Into::into));
        }
    }

    fn is_ready_to_send_write_without_response(&self, _peripheral: Peripheral) {
        let _ = self.ready_to_send_write_without_response.try_broadcast(());
    }

    fn did_open_l2cap_channel(
        &self,
        _peripheral: Peripheral,
        result: CBResult<(corebluetooth::L2capChannel<Peripheral>, UnixStream)>,
    ) {
        if let Some(sender) = self.l2cap_channel_opened.take() {
            let _ = sender.send(
                result
                    .map(|(channel, stream)| (L2capChannel::map(channel), stream))
                    .map_err(Into::into),
            );
        }
    }
}

impl PeripheralAsyncDelegate {
    pub fn name_updates(&self) -> BroadcastReceiver<Option<String>> {
        self.name_updates.new_receiver()
    }

    pub fn services_changed(&self) -> BroadcastReceiver<Vec<Service>> {
        self.services_changed.new_receiver()
    }

    pub fn rssi_updates(&self) -> BroadcastReceiver<Result<i16>> {
        self.rssi_updates.new_receiver()
    }

    pub fn service_discovery(&self) -> BroadcastReceiver<Result<()>> {
        self.service_discovery.new_receiver()
    }

    pub fn included_service_discovery(&self, service: Service) -> oneshot::Receiver<Result<()>> {
        let (sender, receiver) = oneshot::channel();
        self.included_service_discovery
            .borrow_mut()
            .insert(service, sender);
        receiver
    }

    pub fn characteristic_discovery(&self, service: Service) -> oneshot::Receiver<Result<()>> {
        let (sender, receiver) = oneshot::channel();
        self.characteristic_discovery
            .borrow_mut()
            .insert(service, sender);
        receiver
    }

    pub fn descriptor_discovery(
        &self,
        characteristic: Characteristic,
    ) -> oneshot::Receiver<Result<()>> {
        let (sender, receiver) = oneshot::channel();
        self.descriptor_discovery
            .borrow_mut()
            .insert(characteristic, sender);
        receiver
    }

    pub fn characteristic_value_updates(
        &self,
        characteristic: Characteristic,
    ) -> BroadcastReceiver<Result<Vec<u8>>> {
        use std::collections::hash_map::Entry::*;

        match self
            .characteristic_value_updates
            .borrow_mut()
            .entry(characteristic)
        {
            Occupied(entry) => entry.get().new_receiver(),
            Vacant(entry) => entry.insert(broadcast(16)).new_receiver(),
        }
    }

    pub fn descriptor_value_updates(
        &self,
        descriptor: Descriptor,
    ) -> oneshot::Receiver<Result<Vec<u8>>> {
        let (sender, receiver) = oneshot::channel();
        self.descriptor_value_updates
            .borrow_mut()
            .insert(descriptor, sender);
        receiver
    }

    pub fn register_notification_update(
        &self,
        characteristic: Characteristic,
    ) -> oneshot::Receiver<Result<bool>> {
        let (sender, receiver) = oneshot::channel();
        self.notification_updates
            .borrow_mut()
            .insert(characteristic, sender);
        receiver
    }

    pub fn register_characteristic_value_write(
        &self,
        characteristic: Characteristic,
    ) -> oneshot::Receiver<Result<()>> {
        let (sender, receiver) = oneshot::channel();
        self.characteristic_writes
            .borrow_mut()
            .insert(characteristic, sender);
        receiver
    }

    pub fn register_descriptor_value_write(
        &self,
        descriptor: Descriptor,
    ) -> oneshot::Receiver<Result<()>> {
        let (sender, receiver) = oneshot::channel();
        self.descriptor_writes
            .borrow_mut()
            .insert(descriptor, sender);
        receiver
    }

    pub fn ready_to_send_write_without_response(&self) -> BroadcastReceiver<()> {
        self.ready_to_send_write_without_response.new_receiver()
    }

    pub fn register_l2cap_channel_open(&self) -> oneshot::Receiver<L2capChannelOpenResult> {
        let (sender, receiver) = oneshot::channel();
        self.l2cap_channel_opened.replace(Some(sender));
        receiver
    }
}
