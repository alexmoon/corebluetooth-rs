use std::error::Error;
use std::pin::pin;

use corebluetooth::dispatch::DispatchQoS;
use corebluetooth_async::CentralManagerAsync;
use futures_lite::StreamExt;
use objc2_core_bluetooth::CBManagerState;
use tracing::info;
use tracing::metadata::LevelFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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

    let task =
        CentralManagerAsync::background(DispatchQoS::default(), false, |central, executor| {
            let task = async move {
                if central.state() != CBManagerState::PoweredOn {
                    let mut updates = pin!(central.state_updates());
                    loop {
                        let state = updates.next().await.unwrap();
                        if state == CBManagerState::PoweredOn {
                            break;
                        }
                    }
                }

                info!("starting scan");
                let mut scan = pin!(central.scan(None, true, None));
                info!("scan started");
                while let Some(did_discover) = scan.next().await {
                    info!(
                        "{}{}: {:?}",
                        did_discover
                            .peripheral
                            .name()
                            .as_deref()
                            .unwrap_or("(unknown)"),
                        format!(" ({}dBm)", did_discover.rssi),
                        did_discover.advertisement_data
                    );
                }
            };

            executor.spawn_local(task)
        });

    task.await;
    Ok(())
}
