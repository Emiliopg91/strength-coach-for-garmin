use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::error;

use crate::{
    ConnectLock,
    dao::device::Device,
    dto::devices::DeviceListItem,
    logic::events::{BackendEvent, EventSink},
};

/// Mirrors the workout-capable devices of the Connect account into the database: enrolls new
/// ones, refreshes the Connect-owned fields of known ones and clears the `connect` flag of those
/// no longer listed. Emits `device_discovered` per added or changed device
/// Does nothing if the device list cannot be fetched.
pub(crate) async fn get_devices_from_connect(
    pool: &DatabasePool,
    connect: &ConnectLock,
    events: &EventSink,
) {
    let mut client = connect.lock().await;
    if client.logged_user().is_none() {
        return;
    }

    let devices = match client.list_devices().await {
        Ok(devices) => devices,
        Err(e) => {
            error!("Error listing Connect devices: {e}");
            return;
        }
    };
    drop(client);

    let listed: Vec<Device> = devices
        .iter()
        .filter(|d| d.workout_capable)
        .map(Device::from)
        .collect();

    let res = pool
        .run_in_transaction(async move |tx| {
            Ok(Device::sync_connect(tx, &listed)
                .await?
                .iter()
                .map(DeviceListItem::from)
                .collect::<Vec<_>>())
        })
        .await;

    match res {
        Ok(discovered) => {
            for device in discovered {
                events.emit(BackendEvent::DeviceDiscovered(device));
            }
        }
        Err(e) => error!("Error syncing Connect devices: {e}"),
    }
}
