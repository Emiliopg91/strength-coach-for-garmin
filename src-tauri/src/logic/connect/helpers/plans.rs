use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::{error, info};

use crate::{
    ConnectLock, SettingsLock,
    dao::{workout::Workout, workout_planning::WorkoutPlanning},
    logic::report_error,
};

use crate::logic::connect::helpers::workout_sync::send_workout_connect;

/// Schedules in Connect the `workout` planned on `date` and stores the schedule id, uploading the
/// workout first if Connect doesn't have it. Does nothing without a logged user.
pub async fn push_plan_to_connect(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect_state: &ConnectLock,
    date: u32,
    workout: u16,
) {
    let connect = connect_state.lock().await;
    if connect.logged_user().is_some() {
        drop(connect);
        let workout_id = match pool
            .run_in_connection(async move |conn| Ok(Workout::get(conn, workout).await?))
            .await
        {
            Err(e) => {
                error!("Could not get workout entry {}: {}", workout, e);
                None
            }
            Ok(None) => None,
            Ok(Some(workout)) => {
                if workout.connect_id.is_none() {
                    send_workout_connect(pool, settings, connect_state, workout.id)
                        .await
                        .ok()
                } else {
                    workout.connect_id.clone()
                }
            }
        };

        let Some(workout_id) = workout_id else {
            error!("Could not push workout to Garmin Connect");
            return;
        };

        let mut connect = connect_state.lock().await;
        match connect
            .schedule_workout(workout_id.parse().unwrap(), date)
            .await
        {
            Ok(schedule_id) => {
                let res = pool
                    .run_in_connection(async move |conn| {
                        Ok(WorkoutPlanning::set_schedule_id(
                            conn,
                            date,
                            workout,
                            &schedule_id.to_string(),
                        )
                        .await?)
                    })
                    .await;

                if let Err(e) = res {
                    report_error(
                        e,
                        settings,
                        "error_workout_schedule_connect",
                        "Could not store Garmin Connect schedule id",
                    );
                } else {
                    info!("Planning sent to Connect")
                }
            }
            Err(e) => {
                report_error(
                    e,
                    settings,
                    "error_workout_schedule_connect",
                    "Could not schedule workout in Garmin Connect",
                );
            }
        }
    }
}

/// Removes the schedule `schedule_id` from Connect, notifying a failure.
pub async fn delete_plan_from_connect(
    settings: &SettingsLock,
    connect: &ConnectLock,
    schedule_id: u64,
) {
    let mut connect = connect.lock().await;

    if let Err(e) = connect.delete_workout_schedule(schedule_id).await {
        report_error(
            e,
            settings,
            "error_workout_unschedule_connect",
            "Could not remove scheduled workout from Garmin Connect",
        );
    }
}
