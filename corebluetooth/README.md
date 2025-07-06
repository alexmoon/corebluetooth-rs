# corebluetooth

A safe wrapper for Apple's [CoreBluetooth framework](https://developer.apple.com/documentation/corebluetooth).

This crate provides a safe, delegate-based API for CoreBluetooth. It aims to be a thin wrapper
around the underlying framework, while providing a more idiomatic Rust interface. All
CoreBluetooth operations are performed on a `dispatch` queue, and results are delivered via a
delegate trait that you implement.

For most applications, it is recommended to use the [`corebluetooth-async`](../corebluetooth-async)
crate, which provides a higher-level `async` API on top of this one.

## Example

This example shows how to scan for peripherals and print their advertisement data.

```rust,no_run
use corebluetooth::{
    advertisement_data::AdvertisementData,
    central_manager::{CentralManager, CentralManagerDelegate},
    error::Error,
    peripheral::{Peripheral, PeripheralDelegate},
    CBManagerState,
};
use dispatch_executor::MainThreadMarker;
use std::time::SystemTime;

fn main() {
    // Delegate-based APIs require a run loop. For this example, we don't start one,
    // so this program will start and then exit.
    let mtm = MainThreadMarker::new().unwrap();
    let _manager = CentralManager::main_thread(
        Box::new(Delegate),
        false, // show_power_alert
        None,  // restore_id
        mtm,
    );
}

struct Delegate;

impl CentralManagerDelegate for Delegate {
    fn new_peripheral_delegate(&self) -> Box<dyn PeripheralDelegate> {
        Box::new(PeripheralDelegateImpl)
    }

    fn did_update_state(&self, central: CentralManager) {
        if central.state() == CBManagerState::PoweredOn {
            println!("Bluetooth is powered on, starting scan.");
            // Scan for all peripherals.
            central.scan(None, false, None);
        }
    }

    fn did_discover(
        &self,
        _central: CentralManager,
        peripheral: Peripheral,
        advertisement_data: AdvertisementData,
        rssi: i16,
    ) {
        if let Some(name) = peripheral.name() {
            println!(
                "Discovered peripheral '{}' (RSSI: {}) with advertisement data: {:?}",
                name, rssi, advertisement_data
            );
        }
    }

    fn did_connect(&self, _central: CentralManager, peripheral: Peripheral) {
        println!("Connected to peripheral: {:?}", peripheral.name());
    }

    fn did_fail_to_connect(
        &self,
        _central: CentralManager,
        peripheral: Peripheral,
        error: Error,
    ) {
        println!(
            "Failed to connect to peripheral: {:?}, error: {}",
            peripheral.name(),
            error
        );
    }

    fn did_disconnect(
        &self,
        _central: CentralManager,
        peripheral: Peripheral,
        _timestamp: Option<SystemTime>,
        _is_reconnecting: bool,
        error: Option<Error>,
    ) {
        println!(
            "Disconnected from peripheral: {:?}, error: {:?}",
            peripheral.name(),
            error
        );
    }
}

struct PeripheralDelegateImpl;

impl PeripheralDelegate for PeripheralDelegateImpl {}
```
