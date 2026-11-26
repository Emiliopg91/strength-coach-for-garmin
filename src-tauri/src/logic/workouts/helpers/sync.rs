use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::info;

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    logic::{
        connect::helpers::workout_sync::sync_workouts_connect,
        devices::helpers::queue::push_pending_to_usb,
    },
};

/// Uploads the just modified `workouts` to Connect in the background (they stay pending of upload
/// without a logged user) and, with `push_usb`, then pushes the queues of the USB devices plugged
/// in. In the background so the caller doesn't wait on the network, and because some callers hold
/// the Connect or device lock.
pub(crate) fn spawn_workout_sync(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    workouts: Vec<u16>,
    push_usb: bool,
) {
    if workouts.is_empty() {
        return;
    }
    let (pool, settings, connect, mtp) =
        (pool.clone(), settings.clone(), connect.clone(), mtp.clone());
    tauri::async_runtime::spawn(async move {
        info!("Syncing {} modified workouts", workouts.len());
        sync_workouts_connect(&pool, &settings, &connect, &workouts).await;
        if push_usb {
            push_pending_to_usb(&pool, &settings, &mtp).await;
        }
    });
}
