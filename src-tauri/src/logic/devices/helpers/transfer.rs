use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::{error, info};

use crate::{
    MtpLock,
    dao::{settings::WeightUnit, workout::Workout, workout_device::WorkoutDevice},
    dto::notifications::{NotificationDefinition, NotificationKind},
    fit::writer::FitWriter,
    logic::notifications::show_notification,
    utils::{
        constants,
        translations::{Languages, translate},
    },
};

/// Serial numbers of the devices plugged in by USB; none if they can't be listed, which is logged.
pub(super) async fn connected_serials(mtp: &MtpLock) -> Vec<String> {
    // Bound in its own statement so the MTP lock is released before the caller uses the devices
    let connected = mtp.lock().await.get_connected_devices().await;
    match connected {
        Ok(devices) => devices.into_iter().map(|d| d.serial).collect(),
        Err(e) => {
            error!("Error listing connected devices: {e}");
            Vec::new()
        }
    }
}

/// Tells the user the workout was sent to a device.
pub(crate) fn notify_workout_sent(workout: &str, serial: &str, lang: Languages) {
    info!("Workout '{workout}' sent to device {serial}");
    show_notification(NotificationDefinition {
        title: workout.to_string(),
        body: translate("ok_workout_send", lang),
        kind: NotificationKind::Temporal,
    });
}

/// Tells the user the workout will be sent to a device the next time it is plugged in.
pub(in crate::logic::devices) fn notify_workout_pending(
    workout: &str,
    serial: &str,
    lang: Languages,
) {
    info!("Workout '{workout}' pending to be sent to device {serial}");
    show_notification(NotificationDefinition {
        title: workout.to_string(),
        body: translate("pending_workout_send", lang),
        kind: NotificationKind::Temporal,
    });
}

pub(in crate::logic::devices) async fn send_workout_to_device(
    pool: &DatabasePool,
    mtp: &MtpLock,
    lang: Languages,
    weight_unit: WeightUnit,
    id: u16,
    serial: &str,
) -> Result<(), String> {
    send_workouts_to_device(pool, mtp, lang, weight_unit, &[id], serial)
        .await
        .pop()
        .expect("one outcome per workout")
}

/// Uploads the workouts `ids` to the device `serial`, opening it once for all of them. Returns the
/// outcome of each workout, in the order of `ids`.
pub(super) async fn send_workouts_to_device(
    pool: &DatabasePool,
    mtp: &MtpLock,
    lang: Languages,
    weight_unit: WeightUnit,
    ids: &[u16],
    serial: &str,
) -> Vec<Result<(), String>> {
    let mut outcomes: Vec<Result<(), String>> = Vec::with_capacity(ids.len());
    // Index in `outcomes` of each file to upload
    let mut slots = Vec::new();
    let mut files = Vec::new();
    for &id in ids {
        match workout_file(pool, mtp, lang, weight_unit, id, serial).await {
            Ok(file) => {
                slots.push(outcomes.len());
                files.push(file);
                outcomes.push(Ok(()));
            }
            Err(e) => outcomes.push(Err(e)),
        }
    }
    if files.is_empty() {
        return outcomes;
    }

    let uploaded = mtp
        .lock()
        .await
        .upload_files(serial, constants::MTP_GARMIN_NEW_FILES_FOLDER, files)
        .await;
    match uploaded {
        Ok(uploads) => {
            for (slot, upload) in slots.into_iter().zip(uploads) {
                outcomes[slot] = upload.map_err(|e| e.to_string());
            }
        }
        Err(e) => {
            let e = e.to_string();
            for slot in slots {
                outcomes[slot] = Err(e.clone());
            }
        }
    }
    outcomes
}

/// File name and contents of the workout `id` for the device `serial`, deleting first from the
/// device the file of its previous name, if it was renamed.
async fn workout_file(
    pool: &DatabasePool,
    mtp: &MtpLock,
    lang: Languages,
    weight_unit: WeightUnit,
    id: u16,
    serial: &str,
) -> Result<(String, Vec<u8>), String> {
    let workout = pool
        .run_in_connection(async move |conn| Ok(Workout::get_for_device(conn, id).await?))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Workout {id} not found"))?;

    // The device would ignore the renamed workout, or keep both, next to the old file
    delete_previous_file(pool, mtp, id, serial).await?;

    // File name is kept as-is since it becomes the workout file name on the device
    let file_name = workout.get_workout_file_name();
    let data = encode_fit(move || FitWriter::from(&workout).into_bytes(lang, weight_unit)).await?;
    Ok((file_name, data))
}

/// Encodes a FIT file in memory with `encode`, off the async runtime as it is CPU-bound work.
async fn encode_fit<E: ToString>(
    encode: impl FnOnce() -> Result<Vec<u8>, E> + Send + 'static,
) -> Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(move || encode().map_err(|e| e.to_string()))
        .await
        .expect("FIT encoding task panicked")
}

/// Encodes a FIT file in memory with `encode` and uploads it as `file_name` to `GARMIN/NewFiles` on
/// the device `serial`, from where the device validates and imports it once unplugged.
pub(super) async fn upload_fit<E: ToString>(
    mtp: &MtpLock,
    serial: &str,
    file_name: &str,
    encode: impl FnOnce() -> Result<Vec<u8>, E> + Send + 'static,
) -> Result<(), String> {
    let data = encode_fit(encode).await?;

    mtp.lock()
        .await
        .upload_file(
            serial,
            constants::MTP_GARMIN_NEW_FILES_FOLDER,
            file_name,
            data,
        )
        .await
        .map_err(|e| e.to_string())
}

/// Deletes from the device `serial` the file the workout `id` had there before being renamed, if
/// any, and forgets that name.
pub(super) async fn delete_previous_file(
    pool: &DatabasePool,
    mtp: &MtpLock,
    id: u16,
    serial: &str,
) -> Result<(), String> {
    let device = serial.to_string();
    let previous = pool
        .run_in_connection(async move |conn| {
            Ok(WorkoutDevice::get_previous_name(conn, id, &device).await?)
        })
        .await
        .map_err(|e| e.to_string())?;
    let Some(previous) = previous else {
        return Ok(());
    };

    if delete_workout_from_device(mtp, &previous, serial).await? {
        info!("Deleted file of previous workout name '{previous}' from device {serial}");
    }
    let device = serial.to_string();
    pool.run_in_connection(async move |conn| {
        Ok(WorkoutDevice::clear_previous_name(conn, id, &device).await?)
    })
    .await
    .map_err(|e| e.to_string())
}

pub(super) async fn delete_workout_from_device(
    mtp: &MtpLock,
    workout: &str,
    serial: &str,
) -> Result<bool, String> {
    mtp.lock()
        .await
        .delete_workout(serial, &Workout::file_name_for(workout))
        .await
        .map_err(|e| e.to_string())
}
