use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Deref;

use btuuid::BluetoothUuid;
use corebluetooth::advertisement_data::AdvertisementData;
use corebluetooth::dispatch::DispatchQoS;
use corebluetooth::{CentralManager, ConnectPeripheralOptions};
use dispatch_executor::{Executor, SyncClone, SyncDrop};
use futures_channel::{mpsc, oneshot};
use objc2::MainThreadMarker;
use objc2_core_bluetooth::{CBConnectionEvent, CBManagerState, CBPeripheralState};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::peripheral::{PeripheralAsync, PeripheralAsyncDelegate};
use crate::util::{BroadcastReceiver, BroadcastSender, broadcast, defer, watch};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CentralManagerAsync {
    inner: CentralManager,
}

unsafe impl SyncDrop for CentralManagerAsync {}
unsafe impl SyncClone for CentralManagerAsync {}

impl Deref for CentralManagerAsync {
    type Target = CentralManager;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl CentralManagerAsync {
    pub fn background<F, R>(qos: DispatchQoS, show_power_alert: bool, func: F) -> R
    where
        F: FnOnce(Self, &Executor) -> R + Send,
        R: Send,
    {
        CentralManager::background(
            qos,
            |_| Box::new(CentralManagerAsyncDelegate::new()),
            show_power_alert,
            None,
            |inner, executor| {
                let central = Self { inner };
                func(central, executor)
            },
        )
    }

    pub fn main_thread(show_power_alert: bool, mtm: MainThreadMarker) -> Self {
        let inner = CentralManager::main_thread(
            Box::new(CentralManagerAsyncDelegate::new()),
            show_power_alert,
            None,
            mtm,
        );
        Self { inner }
    }

    fn delegate(&self) -> &CentralManagerAsyncDelegate {
        let delegate: &dyn Any = self.inner.delegate();
        delegate.downcast_ref().unwrap()
    }

    pub fn state_updates(&self) -> BroadcastReceiver<CBManagerState> {
        self.delegate().state_updated()
    }

    pub async fn connect(&self, peripheral: &PeripheralAsync) -> Result<()> {
        self.connect_with_options(peripheral, Default::default())
            .await
    }

    pub async fn connect_with_options(
        &self,
        peripheral: &PeripheralAsync,
        options: ConnectPeripheralOptions,
    ) -> Result<()> {
        self.inner.connect_with_options(peripheral, options);

        let guard = defer(|| {
            if peripheral.state() == CBPeripheralState::Connecting {
                self.inner.cancel_peripheral_connection(peripheral);
            }
        });

        let receiver = self.delegate().register_connecting(peripheral);
        let res = receiver.await?;
        guard.defuse();
        res
    }

    pub async fn cancel_peripheral_connection(
        &self,
        peripheral: &PeripheralAsync,
    ) -> Option<DidDisconnect> {
        let state = peripheral.state();
        if state == CBPeripheralState::Connecting || state == CBPeripheralState::Connected {
            self.inner.cancel_peripheral_connection(peripheral);
        }

        if state == CBPeripheralState::Connected {
            let mut disconnects = self.delegate().disconnects();
            while let Ok(disconnect) = disconnects.recv().await {
                if peripheral == &disconnect.peripheral {
                    return Some(disconnect);
                }
            }

            unreachable!()
        } else {
            None
        }
    }

    pub fn disconnections(&self) -> BroadcastReceiver<DidDisconnect> {
        self.delegate().disconnects()
    }

    /// # Panics
    ///
    /// Panics if a scan is already in progress (e.g. `is_scanning()` returns true).
    pub fn scan(
        &self,
        services: Option<&[BluetoothUuid]>,
        allow_duplicates: bool,
        solicited_services: Option<&[BluetoothUuid]>,
    ) -> mpsc::UnboundedReceiver<DidDiscover> {
        if self.inner.is_scanning() {
            panic!("CentralManager::scan called while already scanning")
        }

        self.inner
            .scan(services, allow_duplicates, solicited_services);

        self.delegate().discoveries()
    }

    pub fn connection_events(&self) -> BroadcastReceiver<ConnectionEvent> {
        self.delegate().connection_events()
    }

    pub fn ancs_authorization_updates(&self) -> BroadcastReceiver<PeripheralAsync> {
        self.delegate().ancs_authorization_updates()
    }
}

struct CentralManagerAsyncDelegate {
    connecting: RefCell<HashMap<Uuid, oneshot::Sender<Result<()>>>>,
    state_updated: BroadcastSender<CBManagerState>,
    disconnects: BroadcastSender<DidDisconnect>,
    discoveries: Cell<Option<mpsc::UnboundedSender<DidDiscover>>>,
    connection_events: BroadcastSender<ConnectionEvent>,
    ancs_authorization_updates: BroadcastSender<PeripheralAsync>,
}

impl Default for CentralManagerAsyncDelegate {
    fn default() -> Self {
        Self::new()
    }
}

impl corebluetooth::CentralManagerDelegate for CentralManagerAsyncDelegate {
    fn new_peripheral_delegate(&self) -> Box<dyn corebluetooth::PeripheralDelegate> {
        Box::new(PeripheralAsyncDelegate::default())
    }

    fn did_update_state(&self, central: CentralManager) {
        let _ = self.state_updated.try_broadcast(central.state());
    }

    fn did_discover(
        &self,
        central: CentralManager,
        peripheral: corebluetooth::Peripheral,
        advertisement_data: AdvertisementData,
        rssi: i16,
    ) {
        if let Some(sender) = self.discoveries.take() {
            if sender
                .unbounded_send(DidDiscover {
                    peripheral: PeripheralAsync::new_unchecked(peripheral),
                    advertisement_data,
                    rssi,
                })
                .is_ok()
            {
                self.discoveries.set(Some(sender));
            } else {
                central.stop_scan();
            }
        }
    }

    fn did_connect(&self, _central: CentralManager, peripheral: corebluetooth::Peripheral) {
        let id = peripheral.identifier();
        if let Some(sender) = self.connecting.borrow_mut().remove(&id) {
            let _ = sender.send(Ok(()));
        }
    }

    fn did_fail_to_connect(
        &self,
        _central: CentralManager,
        peripheral: corebluetooth::Peripheral,
        error: corebluetooth::Error,
    ) {
        let id = peripheral.identifier();
        if let Some(sender) = self.connecting.borrow_mut().remove(&id) {
            let _ = sender.send(Err(error.into()));
        }
    }

    fn did_disconnect(
        &self,
        _central: CentralManager,
        peripheral: corebluetooth::Peripheral,
        timestamp: Option<std::time::SystemTime>,
        is_reconnecting: bool,
        error: Option<corebluetooth::Error>,
    ) {
        let _ = self.disconnects.try_broadcast(DidDisconnect {
            peripheral: PeripheralAsync::new_unchecked(peripheral),
            timestamp,
            is_reconnecting,
            error: error.map(Error::from),
        });
    }

    fn on_connection_event(
        &self,
        _central: CentralManager,
        event: CBConnectionEvent,
        peripheral: corebluetooth::Peripheral,
    ) {
        let _ = self.connection_events.try_broadcast(ConnectionEvent {
            peripheral: PeripheralAsync::new_unchecked(peripheral),
            event,
        });
    }

    fn did_update_ancs_authorization(
        &self,
        _central: CentralManager,
        peripheral: corebluetooth::Peripheral,
    ) {
        let _ = self
            .ancs_authorization_updates
            .try_broadcast(PeripheralAsync::new_unchecked(peripheral));
    }
}

impl CentralManagerAsyncDelegate {
    pub fn new() -> Self {
        let state_updated = watch();
        let disconnects = broadcast(16);
        let connection_events = broadcast(16);
        let ancs_authorization_updates = broadcast(16);

        Self {
            connecting: Default::default(),
            state_updated,
            disconnects,
            discoveries: Cell::new(None),
            connection_events,
            ancs_authorization_updates,
        }
    }

    pub fn register_connecting(
        &self,
        peripheral: &PeripheralAsync,
    ) -> oneshot::Receiver<Result<()>> {
        let (sender, receiver) = oneshot::channel();
        self.connecting
            .borrow_mut()
            .insert(peripheral.identifier(), sender);
        receiver
    }

    pub fn state_updated(&self) -> BroadcastReceiver<CBManagerState> {
        self.state_updated.new_receiver()
    }

    pub fn disconnects(&self) -> BroadcastReceiver<DidDisconnect> {
        self.disconnects.new_receiver()
    }

    pub fn discoveries(&self) -> mpsc::UnboundedReceiver<DidDiscover> {
        let (sender, receiver) = mpsc::unbounded();
        self.discoveries.set(Some(sender));
        receiver
    }

    pub fn connection_events(&self) -> BroadcastReceiver<ConnectionEvent> {
        self.connection_events.new_receiver()
    }

    pub fn ancs_authorization_updates(&self) -> BroadcastReceiver<PeripheralAsync> {
        self.ancs_authorization_updates.new_receiver()
    }
}

#[derive(Debug, Clone)]
pub struct DidDisconnect {
    pub peripheral: PeripheralAsync,
    pub timestamp: Option<std::time::SystemTime>,
    pub is_reconnecting: bool,
    pub error: Option<Error>,
}

#[derive(Debug, Clone)]
pub struct DidDiscover {
    pub peripheral: PeripheralAsync,
    pub advertisement_data: AdvertisementData,
    pub rssi: i16,
}

#[derive(Debug, Clone)]
pub struct ConnectionEvent {
    pub peripheral: PeripheralAsync,
    pub event: CBConnectionEvent,
}
