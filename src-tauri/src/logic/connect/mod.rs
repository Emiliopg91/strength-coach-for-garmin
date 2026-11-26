pub mod commands;
pub mod helpers;

use rusqlite_orm::{database::DatabasePool, errors::DatabaseError};
use tauri_plugin_log::log::info;

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    connect::client::CalendarItemType,
    dao::{body_metric::BodyMetric, device::Device},
    logic::{
        activities::helpers::import::{finish_activity_import, import_pending_activities},
        connect::helpers::{
            device_sync::get_devices_from_connect,
            workout_sync::{queue_workout_on_connect, up_to_date_connect},
        },
        devices::helpers::{
            queue::{set_workout_on_device, workout_name},
            transfer::notify_workout_sent,
        },
        events::EventSink,
        report_error, settings_language,
    },
};

pub async fn import_from_connect(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    events: &EventSink,
) -> Result<usize, String> {
    let res: Result<_, String> = async {
        let (mut device, existing) = pool
            .run_in_connection(async |conn| Ok(Device::get_connect_import_state(conn).await?))
            .await
            .map_err(|e| e.to_string())?;

        let mut connect = connect.lock().await;

        let calendar_items = connect
            .list_recent_calendar_items()
            .await
            .map_err(|e| e.to_string())?;

        let pending: Vec<u64> = calendar_items
            .iter()
            .filter(|e| {
                e.item_type == CalendarItemType::Activity
                    && e.start_timestamp_local
                        .is_some_and(|d| !existing.contains(&d))
            })
            .filter_map(|e| e.id)
            .collect();

        let files = connect
            .download_activities(&pending)
            .await
            .map_err(|e| e.to_string())?;
        // Released before the import, which parses and stores the files: other Connect calls,
        // like the login status, would otherwise wait for all of it
        drop(connect);

        // The mark only follows what Connect has listed, so activities uploaded late are not
        // missed. Their places are looked up once finished, with the rest of activities pending it.
        import_pending_activities(pool, files, &mut device, None)
            .await
            .map_err(|e: DatabaseError| e.to_string())
    }
    .await;

    finish_activity_import(pool, settings, connect, mtp, events, res)
}

pub async fn import_weight_from_connect(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
) -> Result<usize, String> {
    let res: Result<usize, String> = async {
        let existing = pool
            .run_in_connection(async |conn| Ok(BodyMetric::get_dates(conn).await?))
            .await
            .map_err(|e| e.to_string())?;

        let mut entries = connect
            .lock()
            .await
            .list_weight(existing.last().copied())
            .await
            .map_err(|e| e.to_string())?;

        if entries.is_empty() {
            Ok(0_usize)
        } else {
            pool.run_in_transaction(async move |tx| {
                Ok(BodyMetric::insert_new(tx, &mut entries, &existing).await?)
            })
            .await
            .map_err(|e| e.to_string())
        }
    }
    .await;

    match res {
        Ok(0) => {
            info!("No measures added");
            Ok(0)
        }
        Ok(added) => {
            info!("Measures {} added succesfully", added);
            Ok(added)
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_adding_body_measures",
            "Error adding measures",
        )),
    }
}

pub async fn connect_logged_user(connect: &ConnectLock) -> Result<Option<String>, String> {
    Ok(connect.lock().await.logged_user())
}

/// Logs into Connect through the frontend prompts; once logged in, syncs the Connect devices and
/// retries the pending uploads. Returns the logged user, `None` if cancelled.
pub async fn connect_login(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    events: &EventSink,
) -> Result<Option<String>, String> {
    let user = {
        let mut connect = connect.lock().await;
        connect.prompt_login().await.map_err(|e| e.to_string())?;
        connect.logged_user()
    };

    // Released the lock first: the discovery needs it
    if user.is_some() {
        get_devices_from_connect(pool, connect, events).await;
        up_to_date_connect(pool, settings, connect).await;
    }

    Ok(user)
}

pub async fn connect_logout(connect: &ConnectLock) -> Result<(), String> {
    connect
        .lock()
        .await
        .disconnect()
        .await
        .map_err(|e| e.to_string())
}

pub async fn send_to_connect(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    id: u16,
    serial: &str,
) -> Result<(), String> {
    let res: Result<_, Box<dyn std::error::Error>> = async {
        queue_workout_on_connect(pool, settings, connect, id, serial).await?;

        // Register the pair so later changes (e.g. progressions) get re-sent once plugged by USB
        set_workout_on_device(pool, id, serial, false)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }
    .await;

    let lang = settings_language(settings);

    // The error isn't Send: turned into a string so the name can be awaited while matching
    let res = res.map_err(|e| e.to_string());
    match res {
        Ok(()) => {
            notify_workout_sent(&workout_name(pool, id).await, serial, lang);
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
