use rusqlite_orm::database::DatabasePool;
use strength_coach_for_garmin_macros::traced_command;
use tauri::{AppHandle, State};

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dto::devices::DeviceListItem,
    logic::{devices, ui::event_sink},
};

/// Sends `workout` over USB to the device `serial` if it's connected; otherwise queues it to be
/// sent the next time it's plugged in. Either way the pair is registered so later changes are
/// re-sent, and the outcome is notified. On failure, shows a persistent error notification and
/// rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn send_to_device(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    mtp: State<'_, MtpLock>,
    workout: u16,
    serial: &str,
) -> Result<(), String> {
    devices::send_to_device(&pool, &settings, &mtp, workout, serial).await
}

/// Downloads over USB the activities recorded by the device `serial` since its last sync and
/// imports them, moving the sync mark. Files that fail to parse are kept as pending; new activities
/// are geocoded in the background. Resolves to the number of activities imported. On failure, shows a
/// persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn import_from_device(
    app: AppHandle,
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    serial: &str,
) -> Result<usize, String> {
    let events = event_sink(&app);
    devices::import_from_device(&pool, &settings, &connect, &mtp, &events, serial).await
}

/// Every registered device, leaving out the Connect pseudo-device.
#[traced_command]
#[tauri::command]
pub async fn get_registered_devices(
    pool: State<'_, DatabasePool>,
) -> Result<Vec<DeviceListItem>, String> {
    devices::get_registered_devices(&pool).await
}
