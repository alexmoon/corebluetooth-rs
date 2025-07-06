//! An asynchronous wrapper for the `corebluetooth` crate.
//!
//! This crate provides `async` functions and streams for interacting with the CoreBluetooth
//! framework.
//!
//! See the `examples` directory for more complete usage examples.

mod central_manager;
pub mod error;
mod peripheral;
mod util;

pub use central_manager::*;
pub use corebluetooth::{
    Central, Characteristic, ConnectPeripheralOptions, Descriptor, L2capChannel, Service,
    advertisement_data, dispatch,
};
pub use peripheral::*;
