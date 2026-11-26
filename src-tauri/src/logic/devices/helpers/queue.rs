use rusqlite_orm::{database::DatabasePool, errors::DatabaseError};
use tauri_plugin_log::log::{error, info};

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dao::{workout::Workout, workout_device::WorkoutDevice},
    dto::notifications::{NotificationDefinition, NotificationKind},
    fit::{FitFile, parser::FitParser},
    logic::{
        connect::helpers::workout_sync::queue_workout_on_connect,
        events::{BackendEvent, EventSink},
        fit_settings,
        notifications::show_notification,
        settings_language, store_pending_files,
    },
    utils::{
        constants,
        translations::{Languages, translate},
    },
};

use crate::logic::devices::helpers::transfer::{
    connected_serials, delete_previous_file, delete_workout_from_device, notify_workout_sent,
    send_workouts_to_device,
};

/// Records that the device `serial` has the workout, so later changes (e.g. progressions) get
/// re-sent to it. An existing entry is kept as it is.
pub(crate) async fn register_workout_on_device(
    pool: &DatabasePool,
    workout: u16,
    serial: &str,
) -> Result<usize, DatabaseError> {
    let serial = serial.to_string();
    pool.run_in_connection(async move |conn| {
        Ok(WorkoutDevice::insert_or_ignore(conn, workout, &serial).await?)
    })
    .await
}

/// Records the workout for the device `serial`, `pending` of being sent or already sent,
/// replacing any existing entry.
pub(crate) async fn set_workout_on_device(
    pool: &DatabasePool,
    workout: u16,
    serial: &str,
    pending: bool,
) -> Result<usize, DatabaseError> {
    let serial = serial.to_string();
    pool.run_in_connection(async move |conn| {
        Ok(WorkoutDevice::insert_or_replace(conn, workout, &serial, pending).await?)
    })
    .await
}

/// Name of the workout `id`, for logs and notifications; its id if it can't be read.
pub(crate) async fn workout_name(pool: &DatabasePool, id: u16) -> String {
    pool.run_in_connection(async move |conn| Ok(Workout::get(conn, id).await?))
        .await
        .ok()
        .flatten()
        .map_or_else(|| id.to_string(), |workout| workout.name)
}

/// Takes the workout off the queue of the device `serial`, logging a failure.
async fn clear_workout_pending(pool: &DatabasePool, workout: &str, id: u16, serial: &str) {
    let device = serial.to_string();
    if let Err(e) = pool
        .run_in_connection(async move |conn| {
            Ok(WorkoutDevice::clear_pending(conn, id, &device).await?)
        })
        .await
    {
        error!("Error clearing the queue entry of workout '{workout}' for {serial}: {e}");
    }
}

/// Uploads the pending workouts of every device plugged in by USB. The rest keep them pending for
/// the next plug: Connect only gets the workout itself, never a delivery to a device.
pub(crate) async fn push_pending_to_usb(
    pool: &DatabasePool,
    settings: &SettingsLock,
    mtp: &MtpLock,
) {
    for serial in connected_serials(mtp).await {
        push_pending_workouts(pool, settings, mtp, &serial).await;
    }
}

/// Delivers a just enabled `workout` through Connect to the linked devices still waiting for it,
/// so call it after `push_pending_to_usb` for USB to take precedence.
pub(crate) async fn send_enabled_workout_to_connect_devices(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    id: u16,
) {
    if connect.lock().await.logged_user().is_none() {
        return;
    }

    // Held for the whole run: a waiting caller re-reads the queue after this one has cleared it
    let _guard = PUSH_LOCK.lock().await;
    let serials = match pool
        .run_in_connection(async move |conn| {
            Ok(WorkoutDevice::get_pending_connect_devices(conn, id).await?)
        })
        .await
    {
        Ok(serials) => serials,
        Err(e) => {
            error!("Error fetching Connect devices waiting for workout {id}: {e}");
            return;
        }
    };
    if serials.is_empty() {
        return;
    }

    let workout = workout_name(pool, id).await;
    let lang = settings_language(settings);
    for serial in serials {
        if let Err(e) = queue_workout_on_connect(pool, settings, connect, id, &serial).await {
            error!("Error pushing pending workout '{workout}' to {serial}: {e}");
            continue;
        }
        finish_push(pool, &workout, id, &serial, lang).await;
    }
}

/// Delete the pending workouts of every currently connected device.
pub(crate) async fn delete_pending_from_connected(
    pool: &DatabasePool,
    settings: &SettingsLock,
    mtp: &MtpLock,
) {
    for serial in connected_serials(mtp).await {
        delete_pending_workouts(pool, settings, mtp, &serial).await;
    }
}

/// Serializes pending pushes so concurrent triggers don't upload the same queue twice.
pub(super) static PUSH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Uploads by USB to `device` the workouts queued while it was unplugged, removing each queue entry once sent.
pub(super) async fn push_pending_workouts(
    pool: &DatabasePool,
    settings: &SettingsLock,
    mtp: &MtpLock,
    serial: &str,
) {
    // Held for the whole run: a waiting caller re-reads the queue after this one has cleared it
    let _guard = PUSH_LOCK.lock().await;
    let serial = serial.to_string();
    let serial_q = serial.clone();

    let pending = match pool
        .run_in_connection(async move |conn| {
            Ok(WorkoutDevice::get_pending(conn, &serial_q, Some(true)).await?)
        })
        .await
    {
        Ok(pending) => pending,
        Err(e) => {
            error!("Error fetching pending workouts for pushing to {serial}: {e}");
            return;
        }
    };
    if pending.is_empty() {
        return;
    }

    let (lang, weight_unit) = fit_settings(settings);
    let ids: Vec<u16> = pending.iter().map(|(id, _)| *id).collect();
    let outcomes = send_workouts_to_device(pool, mtp, lang, weight_unit, &ids, &serial).await;
    for ((id, workout), outcome) in pending.into_iter().zip(outcomes) {
        if let Err(e) = outcome {
            error!("Error pushing pending workout '{workout}' to {serial}: {e}");
            continue;
        }
        finish_push(pool, &workout, id, &serial, lang).await;
    }
}

/// Removes the queue entry of a workout just sent to the device `serial` and notifies it.
pub(super) async fn finish_push(
    pool: &DatabasePool,
    workout: &str,
    id: u16,
    serial: &str,
    lang: Languages,
) {
    clear_workout_pending(pool, workout, id, serial).await;
    notify_workout_sent(workout, serial, lang);
}

/// Deletes from `device` the disabled workouts queued while it was disconnected, removing each queue entry once deleted.
pub(super) async fn delete_pending_workouts(
    pool: &DatabasePool,
    settings: &SettingsLock,
    mtp: &MtpLock,
    serial: &str,
) {
    // Held for the whole run: a waiting caller re-reads the queue after this one has cleared it
    let _guard = PUSH_LOCK.lock().await;
    let serial = serial.to_string();
    let serial_q = serial.clone();
    let pending = match pool
        .run_in_connection(async move |conn| {
            Ok(WorkoutDevice::get_pending(conn, &serial_q, Some(false)).await?)
        })
        .await
    {
        Ok(pending) => pending,
        Err(e) => {
            error!("Error fetching pending workouts for deleting from {serial}: {e}");
            return;
        }
    };
    if pending.is_empty() {
        return;
    }

    let lang = settings_language(settings);
    for (id, workout) in pending {
        // A workout renamed since sent is still under its old name on the device
        if let Err(e) = delete_previous_file(pool, mtp, id, &serial).await {
            error!("Error deleting previous file of workout '{workout}' from {serial}: {e}");
            continue;
        }
        match delete_workout_from_device(mtp, &workout, &serial).await {
            Ok(deleted) => {
                clear_workout_pending(pool, &workout, id, &serial).await;
                if deleted {
                    info!("Workout '{workout}' deleted from device {serial}");
                    show_notification(NotificationDefinition {
                        title: workout.clone(),
                        body: translate("ok_workout_delete", lang),
                        kind: NotificationKind::Temporal,
                    });
                }
            }
            Err(e) => {
                error!("Error deleting workout '{workout}' from {serial}: {e}");
            }
        }
    }
}

/// Stores the steps of the `pending_workouts` found among the `.fit` files fetched from the
/// device. Those that fail to parse or to be stored are kept as pending files.
pub(super) async fn store_pending_workouts(
    pool: &DatabasePool,
    events: &EventSink,
    files: &[FitFile],
    serial: &str,
    pending_workouts: &[String],
) {
    // Every workout on the device gets downloaded; those not pending are just discarded
    let mut failed = Vec::new();
    let mut workouts_to_insert = Vec::new();
    // Parsing is CPU-bound and grows with the workouts on the device
    let to_parse = files.to_vec();
    let parsed = tokio::task::spawn_blocking(move || {
        to_parse
            .into_iter()
            .map(|file| Workout::try_from(FitParser::from_fit_file(file)))
            .collect::<Vec<_>>()
    })
    .await
    .expect("FIT parsing task panicked");
    for (file, parsed) in files.iter().zip(parsed) {
        match parsed {
            Ok(workout)
                if !workout.steps.is_empty() && pending_workouts.contains(&workout.name) =>
            {
                workouts_to_insert.push((workout, file))
            }
            Ok(_) => {}
            Err(e) => {
                error!("Error parsing workout file {}: {e}", file.name);
                failed.push(file);
            }
        }
    }

    let mut workout_ids: Vec<u16> = Vec::new();
    for (mut workout, file) in workouts_to_insert {
        let name = workout.name.clone();
        let res = pool
            .run_in_transaction(async move |tx| {
                Workout::store_steps(tx, &mut workout).await?;
                Ok(workout.id)
            })
            .await;

        match res {
            Ok(id) => workout_ids.push(id),
            Err(e) => {
                error!("Error storing workout {} from {}: {}", name, serial, e);
                failed.push(file);
            }
        }
    }

    store_pending_files(
        &constants::PENDING_DOWNLOAD_WORKOUTS_FOLDER,
        serial,
        &failed,
    )
    .await;

    if !workout_ids.is_empty() {
        events.emit(BackendEvent::AddedWorkoutSteps(workout_ids));
    }
}

/// Downloads from `device` the step definitions of workouts that have activities recorded on it but no steps stored yet.
pub(super) async fn download_pending_workouts(
    pool: &DatabasePool,
    mtp: &MtpLock,
    events: &EventSink,
    serial: &str,
) {
    let serial_q = serial.to_string();
    let pending_workouts = match pool
        .run_in_connection(async move |conn| Ok(Workout::get_without_steps(conn, &serial_q).await?))
        .await
    {
        Ok(pending) => pending,
        Err(e) => {
            error!("Error listing pending workouts for {}: {e}", serial);
            return;
        }
    };

    if pending_workouts.is_empty() {
        return;
    }

    // Locked only for the download, so a concurrent sync is not held back by DB work.
    let downloaded = mtp.lock().await.download_workouts(serial).await;

    match downloaded {
        Ok(files) => {
            store_pending_workouts(pool, events, &files, serial, &pending_workouts).await;
        }
        Err(e) => error!("Error downloading workouts from {}: {e}", serial),
    }
}
