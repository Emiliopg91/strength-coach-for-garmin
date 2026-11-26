use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::{error, info};

use crate::{
    ConnectLock, SettingsLock,
    dao::{
        device::Device, settings::WeightUnit, workout::Workout, workout_device::WorkoutDevice,
        workout_planning::WorkoutPlanning,
    },
    dto::connect::ConnectWorkout,
    logic::report_error,
    utils::constants,
};

use crate::logic::connect::helpers::plans::push_plan_to_connect;

/// Uploads the workout `id` to Connect, notifying the user if it fails.
/// The workout is left pending of upload on failure and cleared on success.
pub async fn send_workout_connect(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    id: u16,
) -> Result<String, String> {
    let res = upload_workout_connect(pool, settings, connect, id).await;
    mark_connect_pending(pool, &[id], res.is_err()).await;
    res.map_err(|e| {
        report_error(
            e,
            settings,
            "error_workout_send_connect",
            "Error sending workout to Garmin Connect",
        )
    })
}

/// Uploads the just modified `workouts` to Connect, or leaves them pending of upload when no
/// user is logged in.
pub(crate) async fn sync_workouts_connect(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    workouts: &[u16],
) {
    if workouts.is_empty() {
        return;
    }

    if connect.lock().await.logged_user().is_some() {
        for workout in workouts {
            let _ = send_workout_connect(pool, settings, connect, *workout).await;
        }
    } else {
        mark_connect_pending(pool, workouts, true).await;
    }
}

/// Retries the upload of the workouts left pending for Connect, if a user is logged in.
/// Failures are only logged, as they stay pending for the next attempt.
pub(crate) async fn up_to_date_connect(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
) {
    if connect.lock().await.logged_user().is_none() {
        return;
    }

    align_workouts_connect(pool, connect).await;

    let (pending_workouts, pending_plans) = match pool
        .run_in_connection(async move |conn| {
            let wkts =
                WorkoutDevice::get_pending(conn, constants::CONNECT_DEVICE_SERIAL, None).await?;
            let plans = WorkoutPlanning::get_pending_connect(conn).await?;
            Ok((wkts, plans))
        })
        .await
    {
        Ok((pending_workouts, pending_plans)) => (pending_workouts, pending_plans),
        Err(e) => {
            error!("Error fetching workouts pending of upload to Connect: {e}");
            return;
        }
    };

    if !pending_workouts.is_empty() {
        info!(
            "Uploading {} pending workouts to Connect",
            pending_workouts.len()
        );
        for (id, workout) in pending_workouts {
            let res = upload_workout_connect(pool, settings, connect, id).await;
            if let Err(e) = &res {
                error!("Error uploading pending workout '{workout}' to Connect: {e}");
            }
            mark_connect_pending(pool, &[id], res.is_err()).await;
        }
    }

    if !pending_plans.is_empty() {
        info!("Uploading {} pending plans to Connect", pending_plans.len());
        for (date, workout) in pending_plans {
            push_plan_to_connect(pool, settings, connect, date, workout).await;
        }
    }
}

/// Copies into the database the Connect creation time of the workouts already uploaded.
/// Failures are only logged, as the next alignment or upload retries them.
async fn align_workouts_connect(pool: &DatabasePool, connect: &ConnectLock) {
    let listed = connect.lock().await.list_workouts().await;
    let listed = match listed {
        Ok(listed) => listed,
        Err(e) => {
            error!("Error listing Connect workouts to align them: {e}");
            return;
        }
    };

    let created_times: Vec<(String, u32)> = listed
        .into_iter()
        .filter_map(|w| w.created_date.map(|c| (w.workout_id.to_string(), c)))
        .collect();
    let res = pool
        .run_in_transaction(async move |tx| {
            Ok(Workout::set_connect_created_times(tx, &created_times).await?)
        })
        .await;

    match res {
        Ok(0) => {}
        Ok(changed) => info!("Aligned the Connect creation time of {changed} workouts"),
        Err(e) => error!("Error storing the Connect creation time of workouts: {e}"),
    }
}

pub(super) async fn mark_connect_pending(pool: &DatabasePool, workouts: &[u16], pending: bool) {
    let workouts = workouts.to_vec();
    let res = pool
        .run_in_connection(async move |conn| {
            Ok(WorkoutDevice::set_connect_pending(conn, &workouts, pending).await?)
        })
        .await;

    if let Err(e) = res {
        error!("Error updating Connect upload status: {e}");
    }
}

/// Uploads the workout `id` to Connect and stores the keys Connect gives it. Errors start with the
/// name of the workout once loaded.
pub async fn upload_workout_connect(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    id: u16,
) -> Result<String, String> {
    let weight_unit = settings.read().unwrap().weight_unit;

    let workout = pool
        .run_in_connection(async move |conn| Ok(Workout::get_with_exercises(conn, id).await?))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Workout {id} not found"))?;

    let name = workout.name.clone();
    upload_loaded_workout_connect(pool, connect, workout, weight_unit)
        .await
        .map_err(|e| format!("{name}: {e}"))
}

async fn upload_loaded_workout_connect(
    pool: &DatabasePool,
    connect: &ConnectLock,
    mut workout: Workout,
    weight_unit: WeightUnit,
) -> Result<String, String> {
    let mut client = connect.lock().await;
    let original_id = workout.connect_id.clone();
    let original_created = workout.connect_created_time;

    // The listing gives the id of a workout already in Connect under this name, and the creation
    // time of one already uploaded
    if workout.connect_id.is_none() || workout.connect_created_time.is_none() {
        let listed = client.list_workouts().await.map_err(|e| e.to_string())?;
        let found = match workout.connect_id.as_deref() {
            Some(id) => listed.into_iter().find(|w| w.workout_id.to_string() == id),
            None => listed.into_iter().find(|w| w.workout_name == workout.name),
        };
        if let Some(found) = found {
            workout.connect_id = Some(found.workout_id.to_string());
            workout.connect_created_time = found.created_date;
        }
    }

    let payload = ConnectWorkout::from(&workout, weight_unit);
    match workout.connect_id.as_deref() {
        Some(id) => {
            let id = id
                .parse::<u64>()
                .map_err(|e| format!("Invalid Connect workout id '{id}': {e}"))?;
            client
                .update_workout(id, &payload)
                .await
                .map_err(|e| e.to_string())?;
        }
        None => {
            let created = client
                .create_workout(&payload)
                .await
                .map_err(|e| e.to_string())?;
            workout.connect_id = Some(created.workout_id.to_string());
            workout.connect_created_time = created.created_date;
        }
    }
    drop(client);

    let connect_id = workout.connect_id.clone().unwrap();
    if workout.connect_id != original_id || workout.connect_created_time != original_created {
        pool.run_in_transaction(async move |tx| Ok(workout.set_connect_keys(tx).await?))
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(connect_id)
}

/// Queues the workout `id` in Connect for delivery to the device `serial`, first uploading the
/// workout to Connect if it has no Connect id yet.
pub(crate) async fn queue_workout_on_connect(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    id: u16,
    serial: &str,
) -> Result<(), String> {
    let fetch_workout = async || {
        pool.run_in_connection(async move |conn| Ok(Workout::get(conn, id).await?))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Workout {id} not found"))
    };

    let mut workout = fetch_workout().await?;
    if workout.connect_id.is_none() {
        send_workout_connect(pool, settings, connect, id).await?;
        workout = fetch_workout().await?;
    }
    let connect_id = workout
        .connect_id
        .ok_or_else(|| format!("Workout {} has no Connect id", workout.name))?;
    let workout_id = connect_id
        .parse::<u64>()
        .map_err(|e| format!("Invalid Connect workout id '{connect_id}': {e}"))?;
    let unit_id = Device::unit_id_from_serial(serial).map_err(|e| e.to_string())?;

    connect
        .lock()
        .await
        .send_workout_to_device(workout_id, &workout.name, unit_id)
        .await
        .map_err(|e| e.to_string())
}
