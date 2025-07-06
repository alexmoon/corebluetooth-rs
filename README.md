# CoreBluetooth for Rust

This workspace provides safe, idiomatic Rust APIs for Apple's CoreBluetooth framework,
allowing you to interact with Bluetooth Low Energy (BLE) devices from macOS and iOS.

The workspace is divided into three crates:

- [`dispatch-executor`](./dispatch-executor): An asynchronous executor for Apple's Grand Central Dispatch (GCD).
- [`corebluetooth`](./corebluetooth): A safe wrapper around the CoreBluetooth Objective-C framework.
- [`corebluetooth-async`](./corebluetooth-async): An `async`/`.await`-friendly wrapper for `corebluetooth`.

## Crates

### `dispatch-executor`

This crate provides a basic `async` executor that runs tasks on a Grand Central Dispatch queue. It is used by 
`corebluetooth` and `corebluetooth-async` to manage concurrency and delegate callbacks, but it can also be used as a 
standalone executor.

It also provides a `Handle<T>` type, which allows for sharing `!Send` data between threads by ensuring that all access 
is synchronized on a specific dispatch queue. This is useful for working with Objective-C objects that are not 
thread-safe.

### `corebluetooth`

This crate provides a safe, delegate-based API for CoreBluetooth. It aims to be a thin wrapper around the underlying 
framework, while providing a more idiomatic Rust interface. All CoreBluetooth operations are performed on a `dispatch` 
queue, and results are delivered via a delegate trait that you implement.

### `corebluetooth-async`

This crate builds on `corebluetooth` to provide a higher-level `async` API. It uses `async` functions and streams to 
make working with CoreBluetooth more ergonomic in an asynchronous Rust context. This is likely the crate you will want 
to use for most applications.

## Examples

You can find examples of how to use these crates in the `examples` directories within the `corebluetooth` and 
`corebluetooth-async` crate folders.

For example, to run the `scan-async` example:

```bash
cargo run --example scan-async
```
