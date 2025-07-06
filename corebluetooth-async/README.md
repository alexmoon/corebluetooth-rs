# corebluetooth-async

An asynchronous wrapper for the `corebluetooth` crate.

This crate provides `async` functions and streams for interacting with the CoreBluetooth
framework, making it easy to use within `async` Rust applications. It is built on top of the
[`corebluetooth`](../corebluetooth) crate. This is likely the crate you will want to use for most
applications.

## Example

This example shows how to scan for peripherals and print their advertisement data using an
asynchronous stream.

Add `futures` and `tokio` (or another async runtime) to your `Cargo.toml` dependencies. You will
also need to depend on the `corebluetooth` crate to have access to some enums.

```rust,no_run
use corebluetooth::CBManagerState;
use corebluetooth_async::CentralManagerAsync;
use dispatch_executor::MainThreadMarker;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mtm = MainThreadMarker::new().unwrap();
    let central = CentralManagerAsync::main_thread(false, mtm);

    // Wait for the manager to power on.
    central
        .state_updates()
        .filter(|&state| state == CBManagerState::PoweredOn)
        .next()
        .await;

    println!("Bluetooth is powered on, starting scan.");

    // Start scanning for peripherals.
    let mut discoveries = central.scan(None, false, None);

    // Print discoveries as they come in.
    while let Some(discovery) = discoveries.next().await {
        if let Some(name) = discovery.peripheral.name() {
            println!(
                "Discovered peripheral '{}' (RSSI: {}) with advertisement data: {:?}",
                name, discovery.rssi, discovery.advertisement_data
            );
        }
    }

    Ok(())
}
```
