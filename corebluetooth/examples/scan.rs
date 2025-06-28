use std::error::Error;

use corebluetooth::CentralManager;
use corebluetooth::advertisement_data::AdvertisementData;
use objc2::MainThreadMarker;
use objc2_core_bluetooth::CBManagerState;
use objc2_foundation::NSRunLoop;
use tracing::info;
use tracing::metadata::LevelFilter;

fn main() -> Result<(), Box<dyn Error>> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{EnvFilter, fmt};

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let run_loop = unsafe { NSRunLoop::currentRunLoop() };
    let mtm = MainThreadMarker::new().unwrap();

    let _central = CentralManager::main_thread(Box::new(CentralManagerDelegate), false, None, mtm);

    unsafe { run_loop.run() };

    Ok(())
}

struct CentralManagerDelegate;

impl corebluetooth::CentralManagerDelegate for CentralManagerDelegate {
    fn new_peripheral_delegate(&self) -> Box<dyn corebluetooth::PeripheralDelegate> {
        Box::new(PeripheralDelegate)
    }

    fn did_update_state(&self, central: CentralManager) {
        if central.state() == CBManagerState::PoweredOn {
            info!("Bluetooth is now powered on, starting scan");
            central.scan(None, false, None);
        }
    }

    fn did_discover(
        &self,
        _central: CentralManager,
        peripheral: corebluetooth::Peripheral,
        advertisement_data: AdvertisementData,
        rssi: i16,
    ) {
        info!(
            "{} ({rssi}): {advertisement_data:?}",
            peripheral.name().as_deref().unwrap_or("(unknown)"),
        );
    }
}

struct PeripheralDelegate;

impl corebluetooth::PeripheralDelegate for PeripheralDelegate {}
