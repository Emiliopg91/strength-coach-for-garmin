pub mod commands;
pub mod helpers;

use chrono::{Datelike, Local, TimeZone, Timelike};
use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::{error, info};

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dao::device::Device,
    dto::devices::DeviceListItem,
    logic::{
        activities::helpers::import::{
            ImportedFiles, finish_activity_import, import_pending_activities,
        },
        devices::helpers::{
            queue::{register_workout_on_device, set_workout_on_device, workout_name},
            transfer::{notify_workout_pending, notify_workout_sent, send_workout_to_device},
        },
        events::EventSink,
        fit_settings, report_error,
    },
    utils::constants,
};

/// Sends the workout `id` over USB to the device `serial` if it's plugged in, otherwise queues it
/// for its next plug, and notifies which.
pub async fn send_to_device(
    pool: &DatabasePool,
    settings: &SettingsLock,
    mtp: &MtpLock,
    id: u16,
    serial: &str,
) -> Result<(), String> {
    let (lang, weight_unit) = fit_settings(settings);
    let workout = &workout_name(pool, id).await;

    let connected = mtp
        .lock()
        .await
        .is_device_connected(serial)
        .await
        .map_err(|e| e.to_string())?;

    let res: Result<bool, String> = if connected {
        match send_workout_to_device(pool, mtp, lang, weight_unit, id, serial).await {
            Ok(_) => {
                // Register the pair so later changes (e.g. progressions) get re-sent to this device
                if let Err(e) = register_workout_on_device(pool, id, serial).await {
                    error!("Error registering workout '{workout}' for device {serial}: {e}");
                }

                Ok(true)
            }
            Err(e) => Err(e),
        }
    } else {
        set_workout_on_device(pool, id, serial, true)
            .await
            .map_err(|e| e.to_string())?;

        Ok(false)
    };

    match res {
        Ok(true) => {
            notify_workout_sent(workout, serial, lang);
            Ok(())
        }
        Ok(false) => {
            notify_workout_pending(workout, serial, lang);
            Ok(())
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_workout_send",
            "Error sending workout to device",
        )),
    }
}

/// Downloads new activity files from the given device since its last sync and imports them.
pub async fn import_from_device(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    events: &EventSink,
    serial: &str,
) -> Result<usize, String> {
    info!("Starting import from device with S/N {}", serial);
    let mut latest_date = "1970-01-01-00-00-00".to_string();
    let serial_str = serial.to_string();
    let mut device = pool
        .run_in_connection(async move |conn| Ok(Device::get(conn, &serial_str).await?))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Device {serial} is not registered"))?;

    if let Some(latest) = device.last_activity_sync {
        let latest = Local.timestamp_opt(latest as i64, 0).unwrap();
        latest_date = format!(
            "{:04}-{:02}-{:02}-{:02}-{:02}-{:02}",
            latest.year(),
            latest.month(),
            latest.day(),
            latest.hour(),
            latest.minute(),
            latest.second(),
        );
    }

    info!(
        "Fetching from device activity files after {}...",
        latest_date
    );
    let mut res = Ok(ImportedFiles::default());
    let mtp_client = mtp.lock().await;

    if let Ok(files) = mtp_client
        .download_activities_since(serial, latest_date)
        .await
    {
        drop(mtp_client);
        let now = Local::now().timestamp() as u32;
        res = import_pending_activities(pool, files, &mut device, Some(now)).await;
    }

    finish_activity_import(pool, settings, connect, mtp, events, res)
}

pub async fn get_registered_devices(pool: &DatabasePool) -> Result<Vec<DeviceListItem>, String> {
    Ok(pool
        .run_in_connection(async move |conn| Ok(Device::get_all(conn).await?))
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|d| {
            if d.serial != constants::CONNECT_DEVICE_SERIAL {
                Some(DeviceListItem::from(d))
            } else {
                None
            }
        })
        .collect())
}

/// Renames the device `serial`; a blank name gives it back its model. Resolves to the stored name.
pub async fn set_device_name(
    pool: &DatabasePool,
    settings: &SettingsLock,
    serial: &str,
    name: &str,
) -> Result<String, String> {
    let report = |e: &dyn std::fmt::Display| {
        report_error(
            e,
            settings,
            "error_updating_device",
            "Error renaming device",
        )
    };
    let (owned_serial, name) = (serial.to_string(), name.to_string());
    let stored = pool
        .run_in_transaction(async move |tx| Ok(Device::set_name(tx, &owned_serial, &name).await?))
        .await
        .map_err(|e| report(&e))?;
    stored
        .inspect(|_| info!("Renamed device"))
        .ok_or_else(|| report(&format!("Unknown device {serial}")))
}
