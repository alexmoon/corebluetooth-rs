//! A safe wrapper for Apple's [CoreBluetooth framework](https://developer.apple.com/documentation/corebluetooth).
//!
//! This crate provides a safe, idiomatic Rust API for interacting with Bluetooth Low Energy (BLE)
//! devices from macOS and iOS. It is built on top of the `objc2` and `objc2-core-bluetooth`
//! crates, which provide the low-level Objective-C bindings.
//!
//! See the `examples` directory for more complete usage examples.

pub mod advertisement_data;
mod central;
mod central_manager;
mod characteristic;
mod descriptor;
pub mod dispatch;
pub mod error;
mod l2cap_channel;
mod peripheral;
mod service;
mod util;

pub use central::*;
pub use central_manager::*;
pub use characteristic::*;
pub use descriptor::*;
pub use error::{Error, Result};
pub use l2cap_channel::*;
pub use peripheral::*;
pub use service::*;

pub use objc2_core_bluetooth::{
    CBCharacteristicProperties, CBConnectionEvent, CBManagerAuthorization, CBManagerState,
    CBPeripheralState,
};
