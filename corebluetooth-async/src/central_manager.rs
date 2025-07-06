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

/// An asynchronous wrapper around [`CentralManager`].
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
    /// Creates a new central manager on a background thread.
    ///
    /// This will create a new background dispatch queue with the given quality of service class.
    /// The `entry` function will be called on this queue as well.
    pub fn background<F, R>(qos: DispatchQoS, show_power_alert: bool, entry: F) -> R
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
                entry(central, executor)
            },
        )
    }

    /// Creates a new central manager on the main thread.
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

    /// Returns a stream of state updates for the central manager.
    pub fn state_updates(&self) -> BroadcastReceiver<CBManagerState> {
        self.delegate().state_updated()
    }

    /// Establishes a connection to a peripheral.
    pub async fn connect(&self, peripheral: &PeripheralAsync) -> Result<()> {
        self.connect_with_options(peripheral, Default::default())
            .await
    }

    /// Establishes a connection to a peripheral with the given options.
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

    /// Cancels an active or pending connection to a peripheral.
    ///
    /// If the peripheral is already connected, this will return a [`DidDisconnect`] event when the
    /// peripheral has been disconnected.
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

    /// Returns a stream of disconnection events.
    pub fn disconnections(&self) -> BroadcastReceiver<DidDisconnect> {
        self.delegate().disconnects()
    }

    /// Starts scanning for peripherals.
    ///
    /// The `services` parameter is a list of service UUIDs to scan for. If it is `None`, all
    /// peripherals will be discovered. The `solicited_services` parameter is similar, but
    /// filtering for those peripherals that are looking for a central with the given service
    /// UUIDs.
    ///
    /// [`stop_scan()`][CentralManager::stop_scan] should be called when the returned receiver
    /// is dropped. Otherwise, the scan will not be stopped until the next discovery occurs
    /// after the receiver is dropped.
    ///
    /// # Panics
    ///
    /// Panics if a scan is already in progress (e.g.
    /// [`is_scanning()`][CentralManager::is_scanning] returns true).
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

    /// Returns a stream of connection events.
    pub fn connection_events(&self) -> BroadcastReceiver<ConnectionEvent> {
        self.delegate().connection_events()
    }

    /// Returns a stream of ANCS authorization updates.
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

/// A peripheral disconnection event.
#[derive(Debug, Clone)]
pub struct DidDisconnect {
    /// The peripheral that was disconnected.
    pub peripheral: PeripheralAsync,
    /// The time at which the disconnection occurred.
    pub timestamp: Option<std::time::SystemTime>,
    /// Whether the peripheral is being reconnected.
    pub is_reconnecting: bool,
    /// The error that caused the disconnection, if any.
    pub error: Option<Error>,
}

/// A peripheral discovery event.
#[derive(Debug, Clone)]
pub struct DidDiscover {
    /// The peripheral that was discovered.
    pub peripheral: PeripheralAsync,
    /// The advertisement data of the peripheral.
    pub advertisement_data: AdvertisementData,
    /// The RSSI of the peripheral.
    pub rssi: i16,
}

/// A connection event.
#[derive(Debug, Clone)]
pub struct ConnectionEvent {
    /// The peripheral that the event is for.
    pub peripheral: PeripheralAsync,
    /// The connection event.
    pub event: CBConnectionEvent,
}
