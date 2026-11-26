use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::{error, info};

use crate::{
    MtpLock, SettingsLock,
    dao::{workout_device::WorkoutDevice, workout_planning::WorkoutPlanning},
    fit::writer::{FitWriter, ToFitMessages},
    logic::fit_settings,
    utils::{
        constants,
        time::{local_midnight, today_local_midnight},
    },
};

use crate::logic::devices::helpers::{
    queue::set_workout_on_device,
    transfer::{connected_serials, send_workouts_to_device, upload_fit},
};

/// Serializes schedule uploads so an older schedule can't land on a device after a newer one.
static SCHEDULE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Sends the schedule of the next days to every device plugged in by USB, even if it has no plans
/// left, so removed plans leave the devices too.
pub(crate) async fn send_schedule_to_connected(
    pool: &DatabasePool,
    settings: &SettingsLock,
    mtp: &MtpLock,
) {
    for serial in connected_serials(mtp).await {
        send_schedule_to_device(pool, settings, mtp, &serial, true).await;
    }
}

/// Uploads by USB to the device `serial` the workouts planned for the next
/// [`constants::SCHEDULE_DAYS`] days, today included. Without plans nothing is sent unless
/// `send_empty`. Failures are only logged.
pub(super) async fn send_schedule_to_device(
    pool: &DatabasePool,
    settings: &SettingsLock,
    mtp: &MtpLock,
    serial: &str,
    send_empty: bool,
) {
    // Held from reading the plans to the upload, so the last change is the last one sent
    let _guard = SCHEDULE_LOCK.lock().await;

    let Some(from) = today_local_midnight() else {
        error!("Could not compute the start of today");
        return;
    };
    let Some(to) = local_midnight(i64::from(from) + constants::SCHEDULE_DAYS * 86_400 + 43_200)
    else {
        error!("Could not compute the end of the schedule window");
        return;
    };

    let plans = match pool
        .run_in_connection(
            async move |conn| Ok(WorkoutPlanning::get_between(conn, from, to).await?),
        )
        .await
    {
        Ok(plans) => plans,
        Err(e) => {
            error!("Error fetching the planned workouts for {serial}: {e}");
            return;
        }
    };
    if plans.is_empty() && !send_empty {
        return;
    }

    upload_missing_workouts(pool, settings, mtp, serial, &plans).await;

    let count = plans.len();
    match upload_schedule(settings, mtp, serial, plans).await {
        Ok(()) => info!("Schedule with {count} planned workouts sent to device {serial}"),
        Err(e) => error!("Error sending the schedule to device {serial}: {e}"),
    }
}

/// Uploads to the device `serial` the workouts of `plans` it lacks, so the schedule can point to
/// them, and records them in `workout_device` as sent. A workout counts as on the device when it
/// has an entry there not pending. Only the workouts the schedule will hold are checked. Failures
/// are only logged.
async fn upload_missing_workouts(
    pool: &DatabasePool,
    settings: &SettingsLock,
    mtp: &MtpLock,
    serial: &str,
    plans: &[WorkoutPlanning],
) {
    let serial_cl = serial.to_string();
    let on_device = match pool
        .run_in_connection(async move |conn| Ok(WorkoutDevice::get_sent(conn, &serial_cl).await?))
        .await
    {
        Ok(names) => names,
        Err(e) => {
            error!("Error fetching the workouts sent to {serial}: {e}");
            return;
        }
    };
    let mut missing: Vec<(u16, &str)> = plans
        .iter()
        .filter_map(|p| p.workout_obj.as_ref())
        .filter(|w| w.file_created().is_some() && !on_device.contains(&w.id))
        .map(|w| (w.id, w.name.as_str()))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    if missing.is_empty() {
        return;
    }

    let (lang, weight_unit) = fit_settings(settings);
    for (_, workout) in &missing {
        info!("Planned workout '{workout}' missing on {serial}, uploading it");
    }
    let ids: Vec<u16> = missing.iter().map(|(id, _)| *id).collect();
    let outcomes = send_workouts_to_device(pool, mtp, lang, weight_unit, &ids, serial).await;
    for ((id, workout), outcome) in missing.into_iter().zip(outcomes) {
        if let Err(e) = outcome {
            error!("Error uploading planned workout '{workout}' to {serial}: {e}");
            continue;
        }

        if let Err(e) = set_workout_on_device(pool, id, serial, false).await {
            error!("Error registering workout '{workout}' for device {serial}: {e}");
        }
    }
}

/// Writes `plans` as a schedule file and uploads it to the device `serial`.
async fn upload_schedule(
    settings: &SettingsLock,
    mtp: &MtpLock,
    serial: &str,
    plans: Vec<WorkoutPlanning>,
) -> Result<(), String> {
    let (lang, weight_unit) = fit_settings(settings);
    upload_fit(
        mtp,
        serial,
        constants::MTP_GARMIN_SCHEDULE_FILE,
        move || FitWriter::from(&plans).into_bytes(lang, weight_unit),
    )
    .await
}
