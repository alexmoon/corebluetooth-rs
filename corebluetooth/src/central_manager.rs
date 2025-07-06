//! The central manager, which is the application's interface to Bluetooth LE.

use std::any::Any;

use btuuid::BluetoothUuid;
use dispatch_executor::{Executor, SyncClone, SyncDrop};
use dispatch2::DispatchQueue;
use objc2::rc::{Retained, RetainedFromIterator};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, Message, define_class, msg_send};
use objc2_core_bluetooth::{
    CBCentralManager, CBCentralManagerDelegate, CBCentralManagerFeature,
    CBCentralManagerOptionRestoreIdentifierKey, CBCentralManagerOptionShowPowerAlertKey,
    CBCentralManagerScanOptionAllowDuplicatesKey,
    CBCentralManagerScanOptionSolicitedServiceUUIDsKey,
    CBConnectPeripheralOptionEnableAutoReconnect,
    CBConnectPeripheralOptionEnableTransportBridgingKey,
    CBConnectPeripheralOptionNotifyOnConnectionKey,
    CBConnectPeripheralOptionNotifyOnDisconnectionKey,
    CBConnectPeripheralOptionNotifyOnNotificationKey, CBConnectPeripheralOptionRequiresANCS,
    CBConnectPeripheralOptionStartDelayKey, CBConnectionEvent,
    CBConnectionEventMatchingOptionPeripheralUUIDs, CBConnectionEventMatchingOptionServiceUUIDs,
    CBError, CBManager, CBManagerAuthorization, CBManagerState, CBPeripheral,
};
use objc2_core_foundation::CFAbsoluteTime;
use objc2_foundation::{
    NSArray, NSDictionary, NSError, NSMutableDictionary, NSNumber, NSObject, NSObjectProtocol,
    NSString, NSUUID,
};
use uuid::Uuid;

use crate::PeripheralDelegate;
use crate::advertisement_data::AdvertisementData;
use crate::dispatch::DispatchQoS;
use crate::error::{Error, ErrorKind};
use crate::peripheral::Peripheral;
use crate::util::to_cbuuid;

/// An object that scans for, discovers, connects to, and manages peripherals.
#[derive(Clone)]
pub struct CentralManager {
    central: Retained<CBCentralManager>,
    delegate: Retained<CentralManagerDelegateBridge>,
}

impl std::fmt::Debug for CentralManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CentralManager")
            .field("central", &self.central)
            .finish()
    }
}

impl PartialEq for CentralManager {
    fn eq(&self, other: &Self) -> bool {
        self.central == other.central
    }
}

impl Eq for CentralManager {}

impl std::hash::Hash for CentralManager {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.central.hash(state);
    }
}

unsafe impl SyncDrop for CentralManager {}
unsafe impl SyncClone for CentralManager {}

impl CentralManager {
    /// Returns the current authorization state of the central manager.
    ///
    /// See [`[CBManager authorization]`](https://developer.apple.com/documentation/corebluetooth/cbmanager/authorization-swift.type.property).
    pub fn authorization() -> CBManagerAuthorization {
        unsafe { CBManager::authorization_class() }
    }

    /// Returns whether the device supports extended scan and connect.
    ///
    /// See [`[CBCentralManager supports:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/supports(_:)).
    pub fn supports_extended_scan_and_connect() -> bool {
        unsafe {
            CBCentralManager::supportsFeatures(CBCentralManagerFeature::ExtendedScanAndConnect)
        }
    }

    /// Creates a new central manager on a background thread.
    ///
    /// This will create a new background dispatch queue with the given quality of service class.
    /// The `delegate` will be created on this queue, and all delegate methods will be called on
    /// it. One created, `entry` will be called with the new `CentralManager` on that dispatch
    /// queue.
    pub fn background<R: Send>(
        qos: DispatchQoS,
        delegate: impl FnOnce(&Executor) -> Box<dyn CentralManagerDelegate> + Send,
        show_power_alert: bool,
        restore_id: Option<&str>,
        entry: impl FnOnce(Self, &Executor) -> R + Send,
    ) -> R {
        Executor::background("bluetooth", qos.to_attr().as_deref(), move |executor| {
            let delegate = delegate(&executor);
            let central = Self::init(executor.queue(), delegate, show_power_alert, restore_id);
            entry(central, &executor)
        })
    }

    /// Creates a new central manager on the main thread.
    pub fn main_thread(
        delegate: Box<dyn CentralManagerDelegate>,
        show_power_alert: bool,
        restore_id: Option<&str>,
        _mtm: MainThreadMarker,
    ) -> Self {
        let queue = DispatchQueue::main();
        Self::init(queue, delegate, show_power_alert, restore_id)
    }

    pub(crate) fn new(central: Retained<CBCentralManager>) -> Self {
        let delegate = unsafe { central.delegate() }
            .and_then(|delegate| delegate.downcast().ok())
            .unwrap();

        CentralManager { central, delegate }
    }

    fn init(
        queue: &DispatchQueue,
        delegate: Box<dyn CentralManagerDelegate>,
        show_power_alert: bool,
        restore_id: Option<&str>,
    ) -> Self {
        let delegate = CentralManagerDelegateBridge::new(delegate);

        let options: Retained<NSMutableDictionary<NSString, AnyObject>> =
            NSMutableDictionary::from_retained_objects(
                &[unsafe { CBCentralManagerOptionShowPowerAlertKey }],
                &[NSNumber::new_bool(show_power_alert).into()],
            );

        if let Some(restore_id) = restore_id {
            unsafe {
                options.setValue_forKey(
                    Some(&NSString::from_str(restore_id)),
                    CBCentralManagerOptionRestoreIdentifierKey,
                );
            }
        };

        let central = CBCentralManager::alloc();
        let central = unsafe {
            CBCentralManager::initWithDelegate_queue_options(
                central,
                Some(ProtocolObject::from_ref(&*delegate)),
                Some(queue),
                Some(&options),
            )
        };

        Self { central, delegate }
    }

    /// Returns a reference to the delegate.
    pub fn delegate(&self) -> &dyn CentralManagerDelegate {
        &*self.delegate.ivars().delegate
    }

    /// The current state of the central manager.
    ///
    /// See [`-[CBCentralManager state]`](https://developer.apple.com/documentation/corebluetooth/cbmanager/state).
    pub fn state(&self) -> CBManagerState {
        unsafe { self.central.state() }
    }

    /// Retrieves a list of known peripherals by their identifiers.
    ///
    /// See [`-[CBCentralManager retrievePeripheralsWithIdentifiers:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/retrieveperipherals(withidentifiers:)).
    pub fn retrieve_peripherals(&self, identifiers: &[Uuid]) -> Vec<Peripheral> {
        let identifiers = NSArray::retained_from_iter(
            identifiers
                .iter()
                .map(|uuid| NSUUID::from_bytes(uuid.into_bytes())),
        );

        let peripherals = unsafe {
            self.central
                .retrievePeripheralsWithIdentifiers(&identifiers)
        };

        peripherals
            .into_iter()
            .map(|peripheral| {
                Peripheral::init(peripheral, || {
                    self.delegate.ivars().delegate.new_peripheral_delegate()
                })
            })
            .collect()
    }

    /// Retrieves a list of the peripherals currently connected to the system.
    ///
    /// See [`-[CBCentralManager retrieveConnectedPeripheralsWithServices:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/retrieveconnectedperipherals(withservices:)).
    pub fn retrieve_connected_peripherals(&self, services: &[BluetoothUuid]) -> Vec<Peripheral> {
        let services = NSArray::retained_from_iter(services.iter().map(to_cbuuid));

        let peripherals = unsafe {
            self.central
                .retrieveConnectedPeripheralsWithServices(&services)
        };

        peripherals
            .into_iter()
            .map(|peripheral| {
                Peripheral::init(peripheral, || {
                    self.delegate.ivars().delegate.new_peripheral_delegate()
                })
            })
            .collect()
    }

    /// Establishes a connection to a peripheral.
    ///
    /// See [`-[CBCentralManager connectPeripheral:options:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/connect(_:options:)).
    pub fn connect(&self, peripheral: &Peripheral) {
        self.connect_with_options(peripheral, Default::default());
    }

    /// Establishes a connection to a peripheral with the given options.
    ///
    /// See [`-[CBCentralManager connectPeripheral:options:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/connect(_:options:)).
    pub fn connect_with_options(&self, peripheral: &Peripheral, options: ConnectPeripheralOptions) {
        unsafe {
            self.central
                .connectPeripheral_options(&peripheral.peripheral, Some(&options.to_dictionary()))
        }
    }

    /// Cancels an active or pending connection to a peripheral.
    ///
    /// See [`-[CBCentralManager cancelPeripheralConnection:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/cancelperipheralconnection(_:)).
    pub fn cancel_peripheral_connection(&self, peripheral: &Peripheral) {
        unsafe {
            self.central
                .cancelPeripheralConnection(&peripheral.peripheral);
        }
    }

    /// Whether the central manager is currently scanning.
    ///
    /// See [`-[CBCentralManager isScanning]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/isscanning).
    pub fn is_scanning(&self) -> bool {
        unsafe { self.central.isScanning() }
    }

    /// Starts scanning for peripherals.
    ///
    /// The `services` parameter is a list of service UUIDs to scan for. If it is `None`, all
    /// peripherals will be discovered.
    ///
    /// See [`-[CBCentralManager scanForPeripheralsWithServices:options:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/scanforperipherals(withservices:options:)).
    pub fn scan(
        &self,
        services: Option<&[BluetoothUuid]>,
        allow_duplicates: bool,
        solicited_services: Option<&[BluetoothUuid]>,
    ) {
        let services =
            services.map(|services| NSArray::retained_from_iter(services.iter().map(to_cbuuid)));

        let options = NSMutableDictionary::<NSString, AnyObject>::new();

        if allow_duplicates {
            unsafe {
                options.setValue_forKey(
                    Some(&NSNumber::new_bool(allow_duplicates)),
                    CBCentralManagerScanOptionAllowDuplicatesKey,
                );
            }
        }

        if let Some(services) = solicited_services {
            let services = NSArray::retained_from_iter(services.iter().map(to_cbuuid));

            unsafe {
                options.setValue_forKey(
                    Some(&services),
                    CBCentralManagerScanOptionSolicitedServiceUUIDsKey,
                );
            }
        }

        unsafe {
            self.central
                .scanForPeripheralsWithServices_options(services.as_deref(), Some(&options));
        }
    }

    /// Stops scanning for peripherals.
    ///
    /// See [`-[CBCentralManager stopScan]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/stopscan()).
    pub fn stop_scan(&self) {
        unsafe {
            self.central.stopScan();
        }
    }

    /// Registers for connection events.
    ///
    /// See [`-[CBCentralManager registerForConnectionEventsWithOptions:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/registerforconnectionevents(options:)).
    pub fn register_for_connection_events(
        &self,
        peripherals: Option<&[Uuid]>,
        services: Option<&[BluetoothUuid]>,
    ) {
        let options: Retained<NSMutableDictionary<NSString, AnyObject>> =
            NSMutableDictionary::new();

        if let Some(peripherals) = peripherals {
            let identifiers = NSArray::retained_from_iter(
                peripherals
                    .iter()
                    .map(|uuid| NSUUID::from_bytes(uuid.into_bytes())),
            );

            unsafe {
                options.setValue_forKey(
                    Some(&identifiers),
                    CBConnectionEventMatchingOptionPeripheralUUIDs,
                )
            };
        }

        if let Some(services) = services {
            let services = NSArray::retained_from_iter(services.iter().map(to_cbuuid));

            unsafe {
                options
                    .setValue_forKey(Some(&services), CBConnectionEventMatchingOptionServiceUUIDs)
            };
        }

        unsafe {
            self.central
                .registerForConnectionEventsWithOptions(Some(&options))
        };
    }
}

/// A protocol that provides updates for the state of a [`CentralManager`].
#[allow(unused_variables)]
pub trait CentralManagerDelegate: Any {
    /// This method is called when a new peripheral delegate is needed.
    fn new_peripheral_delegate(&self) -> Box<dyn PeripheralDelegate>;

    /// This method is called when the central manager's state is updated.
    ///
    /// See [`-[CBCentralManagerDelegate centralManagerDidUpdateState:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanagerdelegate/centralmanagerdidupdatestate(_:)).
    fn did_update_state(&self, central: CentralManager);

    /// This method is called when the central manager is about to restore its state.
    ///
    /// See [`-[CBCentralManagerDelegate centralManager:willRestoreState:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanagerdelegate/centralmanager(_:willrestorestate:)).
    fn will_restore_state(
        &self,
        central: CentralManager,
        dict: &NSDictionary<NSString, AnyObject>,
    ) {
    }

    /// This method is called when a peripheral is discovered.
    ///
    /// See [`-[CBCentralManagerDelegate centralManager:didDiscoverPeripheral:advertisementData:RSSI:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanagerdelegate/centralmanager(_:diddiscover:advertisementdata:rssi:)).
    fn did_discover(
        &self,
        central: CentralManager,
        peripheral: Peripheral,
        advertisement_data: AdvertisementData,
        rssi: i16,
    ) {
    }

    /// This method is called when a connection to a peripheral is established.
    ///
    /// See [`-[CBCentralManagerDelegate centralManager:didConnectPeripheral:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanagerdelegate/centralmanager(_:didconnect:)).
    fn did_connect(&self, central: CentralManager, peripheral: Peripheral) {}

    /// This method is called when a connection to a peripheral fails.
    ///
    /// See [`-[CBCentralManagerDelegate centralManager:didFailToConnectPeripheral:error:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanagerdelegate/centralmanager(_:didfailtoconnect:error:)).
    fn did_fail_to_connect(&self, central: CentralManager, peripheral: Peripheral, error: Error) {}

    /// This method is called when a peripheral is disconnected.
    ///
    /// See [`-[CBCentralManagerDelegate centralManager:didDisconnectPeripheral:error:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanagerdelegate/centralmanager(_:diddisconnectperipheral:error:))
    /// and [`-[CBCentralManagerDelegate centralManager:didDisconnectPeripheral:timestamp:isReconnecting:error:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanagerdelegate/centralmanager(_:diddisconnectperipheral:timestamp:isreconnecting:error:)).
    fn did_disconnect(
        &self,
        central: CentralManager,
        peripheral: Peripheral,
        timestamp: Option<std::time::SystemTime>,
        is_reconnecting: bool,
        error: Option<Error>,
    ) {
    }

    /// This method is called when a connection event occurs.
    ///
    /// See [`-[CBCentralManagerDelegate centralManager:connectionEventDidOccur:forPeripheral:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanagerdelegate/centralmanager(_:connectioneventdidoccur:for:)).
    fn on_connection_event(
        &self,
        central: CentralManager,
        event: CBConnectionEvent,
        peripheral: Peripheral,
    ) {
    }

    /// This method is called when the ANCS authorization for a peripheral is updated.
    ///
    /// See [`-[CBCentralManagerDelegate centralManager:didUpdateANCSAuthorizationForPeripheral:]`](https://developer.apple.com/documentation/corebluetooth/cbcentralmanagerdelegate/centralmanager(_:didupdateancsauthorizationfor:)).
    fn did_update_ancs_authorization(&self, central: CentralManager, peripheral: Peripheral) {}
}

struct CentralManagerDelegateIvars {
    delegate: Box<dyn CentralManagerDelegate>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = CentralManagerDelegateIvars]
    struct CentralManagerDelegateBridge;

    unsafe impl NSObjectProtocol for CentralManagerDelegateBridge {}

    #[allow(non_snake_case)]
    unsafe impl CBCentralManagerDelegate for CentralManagerDelegateBridge {
        #[unsafe(method(centralManagerDidUpdateState:))]
        fn centralManagerDidUpdateState(&self, central: &CBCentralManager) {
            self.ivars()
                .delegate
                .did_update_state(CentralManager::new(central.retain()));
        }

        #[unsafe(method(centralManager:willRestoreState:))]
        fn centralManager_willRestoreState(
            &self,
            central: &CBCentralManager,
            dict: &NSDictionary<NSString, AnyObject>,
        ) {
            self.ivars()
                .delegate
                .will_restore_state(CentralManager::new(central.retain()), dict);
        }

        #[unsafe(method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:))]
        fn centralManager_didDiscoverPeripheral_advertisementData_RSSI(
            &self,
            central: &CBCentralManager,
            peripheral: &CBPeripheral,
            advertisement_data: &NSDictionary<NSString, AnyObject>,
            rssi: &NSNumber,
        ) {
            let peripheral = Peripheral::init(peripheral.retain(), || {
                self.ivars().delegate.new_peripheral_delegate()
            });
            let advertisement_data = AdvertisementData::from_nsdictionary(advertisement_data);
            let rssi = rssi.shortValue();

            self.ivars().delegate.did_discover(
                CentralManager::new(central.retain()),
                peripheral,
                advertisement_data,
                rssi,
            );
        }

        #[unsafe(method(centralManager:didConnectPeripheral:))]
        fn centralManager_didConnectPeripheral(
            &self,
            central: &CBCentralManager,
            peripheral: &CBPeripheral,
        ) {
            self.ivars().delegate.did_connect(
                CentralManager::new(central.retain()),
                Peripheral::new(peripheral.retain()),
            );
        }

        #[unsafe(method(centralManager:didFailToConnectPeripheral:error:))]
        fn centralManager_didFailToConnectPeripheral_error(
            &self,
            central: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            let error =
                Error::from_nserror_or_kind(error, ErrorKind::Bluetooth(CBError::ConnectionFailed));

            self.ivars().delegate.did_fail_to_connect(
                CentralManager::new(central.retain()),
                Peripheral::new(peripheral.retain()),
                error,
            );
        }

        #[unsafe(method(centralManager:didDisconnectPeripheral:error:))]
        fn centralManager_didDisconnectPeripheral_error(
            &self,
            central: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            let error = error.map(Error::from_nserror);
            self.ivars().delegate.did_disconnect(
                CentralManager::new(central.retain()),
                Peripheral::new(peripheral.retain()),
                None,
                false,
                error,
            );
        }

        #[unsafe(method(centralManager:didDisconnectPeripheral:timestamp:isReconnecting:error:))]
        fn centralManager_didDisconnectPeripheral_timestamp_isReconnecting_error(
            &self,
            central: &CBCentralManager,
            peripheral: &CBPeripheral,
            timestamp: CFAbsoluteTime,
            is_reconnecting: bool,
            error: Option<&NSError>,
        ) {
            let error = error.map(Error::from_nserror);
            self.ivars().delegate.did_disconnect(
                CentralManager::new(central.retain()),
                Peripheral::new(peripheral.retain()),
                to_system_time(timestamp),
                is_reconnecting,
                error,
            );
        }

        #[unsafe(method(centralManager:connectionEventDidOccur:forPeripheral:))]
        fn centralManager_connectionEventDidOccur_forPeripheral(
            &self,
            central: &CBCentralManager,
            event: CBConnectionEvent,
            peripheral: &CBPeripheral,
        ) {
            self.ivars().delegate.on_connection_event(
                CentralManager::new(central.retain()),
                event,
                Peripheral::new(peripheral.retain()),
            );
        }

        #[unsafe(method(centralManager:didUpdateANCSAuthorizationForPeripheral:))]
        fn centralManager_didUpdateANCSAuthorizationForPeripheral(
            &self,
            central: &CBCentralManager,
            peripheral: &CBPeripheral,
        ) {
            self.ivars().delegate.did_update_ancs_authorization(
                CentralManager::new(central.retain()),
                Peripheral::new(peripheral.retain()),
            );
        }
    }
);

impl CentralManagerDelegateBridge {
    pub fn new(delegate: Box<dyn CentralManagerDelegate>) -> Retained<Self> {
        let ivars = CentralManagerDelegateIvars { delegate };
        let this = CentralManagerDelegateBridge::alloc().set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }
}

/// Options for connecting to a peripheral.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct ConnectPeripheralOptions {
    /// Whether to automatically reconnect to the peripheral when it is available.
    pub enable_auto_reconnect: bool,
    /// Whether to enable transport bridging.
    pub enable_transport_bridging: bool,
    /// Whether to notify on connection.
    pub notify_on_connection: bool,
    /// Whether to notify on disconnection.
    pub notify_on_disconnection: bool,
    /// Whether to notify on notification.
    pub notify_on_notification: bool,
    /// Whether ANCS is required.
    pub requires_ancs: bool,
    /// The delay before starting the connection.
    pub start_delay: Option<f32>,
}

impl ConnectPeripheralOptions {
    fn to_dictionary(self) -> Retained<NSDictionary<NSString, AnyObject>> {
        let dict = NSMutableDictionary::<NSString, AnyObject>::new();

        unsafe fn set_value(
            dict: &NSMutableDictionary<NSString, AnyObject>,
            value: Option<Retained<NSNumber>>,
            key: &NSString,
        ) {
            if let Some(value) = value {
                unsafe { dict.setValue_forKey(Some(&value), key) };
            }
        }

        unsafe {
            set_value(
                &dict,
                self.enable_auto_reconnect.then(|| NSNumber::new_bool(true)),
                CBConnectPeripheralOptionEnableAutoReconnect,
            );

            set_value(
                &dict,
                self.enable_transport_bridging
                    .then(|| NSNumber::new_bool(true)),
                CBConnectPeripheralOptionEnableTransportBridgingKey,
            );

            set_value(
                &dict,
                self.notify_on_connection.then(|| NSNumber::new_bool(true)),
                CBConnectPeripheralOptionNotifyOnConnectionKey,
            );

            set_value(
                &dict,
                self.notify_on_disconnection
                    .then(|| NSNumber::new_bool(true)),
                CBConnectPeripheralOptionNotifyOnDisconnectionKey,
            );

            set_value(
                &dict,
                self.notify_on_notification
                    .then(|| NSNumber::new_bool(true)),
                CBConnectPeripheralOptionNotifyOnNotificationKey,
            );

            set_value(
                &dict,
                self.requires_ancs.then(|| NSNumber::new_bool(true)),
                CBConnectPeripheralOptionRequiresANCS,
            );

            set_value(
                &dict,
                self.start_delay.map(NSNumber::new_f32),
                CBConnectPeripheralOptionStartDelayKey,
            );
        }

        dict.into_super()
    }
}

fn to_system_time(timestamp: CFAbsoluteTime) -> Option<std::time::SystemTime> {
    let since_1970 = timestamp + unsafe { objc2_core_foundation::kCFAbsoluteTimeIntervalSince1970 };
    std::time::UNIX_EPOCH.checked_add(std::time::Duration::try_from_secs_f64(since_1970).ok()?)
}
