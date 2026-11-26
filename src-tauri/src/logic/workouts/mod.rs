pub mod commands;
pub mod helpers;

use std::collections::{HashMap, HashSet};

use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::info;

use crate::utils::weight::to_kg;
use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dao::{
        exercise::Exercise,
        workout::Workout,
        workout_device::WorkoutDevice,
        workout_planning::WorkoutPlanning,
        workout_step::{WorkoutStep, current_one_rms},
    },
    dto::{
        notifications::{NotificationDefinition, NotificationKind},
        workouts::{PercentLoadBase, WorkoutDetails, WorkoutListItem, WorkoutPlanListItem},
    },
    logic::{
        connect::helpers::plans::{delete_plan_from_connect, push_plan_to_connect},
        devices::helpers::{
            queue::{
                delete_pending_from_connected, push_pending_to_usb,
                send_enabled_workout_to_connect_devices,
            },
            schedule::send_schedule_to_connected,
        },
        events::{BackendEvent, EventSink},
        exercises::helpers::progressions::progression_suggestions,
        notifications::show_notification,
        report_error, settings_language,
        workouts::helpers::{
            queries::{workout_details, workout_list},
            sync::spawn_workout_sync,
        },
    },
    utils::{
        time::{now, today_local_midnight},
        translations::{translate, translate_and_replace},
    },
};

/// Returns activities grouped/aggregated by workout name (count, average time, latest date), sorted by name.
pub async fn get_workout_list(
    pool: &DatabasePool,
    settings: &SettingsLock,
) -> Result<Vec<WorkoutListItem>, String> {
    info!("Getting workouts list...");
    pool.run_in_connection(async move |conn| Ok(workout_list(conn).await?))
        .await
        .inspect(|l| info!("Retreived {} workouts", l.len()))
        .map_err(|e| {
            report_error(
                e,
                settings,
                "error_workout_list",
                "Error getting workouts list",
            )
        })
}

/// Returns the workout `id` with every activity following it and its per-activity volume.
pub async fn get_workout_details(
    pool: &DatabasePool,
    settings: &SettingsLock,
    id: u16,
) -> Result<WorkoutDetails, String> {
    pool.run_in_connection(async move |conn| Ok(workout_details(conn, id).await?))
        .await
        .inspect(|l| info!("Found details for workout {}", l.name))
        .map_err(|e| {
            report_error(
                e,
                settings,
                "error_workout_details",
                "Error getting workout details",
            )
        })
}

/// Enables or disables `workout`, then delivers the queued workouts to the plugged in devices
/// (and an enabled one to the Connect devices waiting for it) or deletes the disabled ones.
pub async fn set_workout_status(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    workout: u16,
    status: bool,
) -> Result<(), String> {
    let res = pool
        .run_in_transaction(async move |tx| Ok(Workout::set_enabled(tx, workout, status).await?))
        .await
        .map_err(|e| e.to_string());

    if res.is_ok() {
        if status {
            push_pending_to_usb(pool, settings, mtp).await;
            send_enabled_workout_to_connect_devices(pool, settings, connect, workout).await;
        } else {
            delete_pending_from_connected(pool, settings, mtp).await;
        }
    }

    res
}

pub async fn set_workout_progression(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    events: &EventSink,
    workout: u16,
    value: bool,
) -> Result<(), String> {
    let res = pool
        .run_in_transaction(async move |tx| {
            Workout::set_progression(tx, workout, value).await?;
            // Committed with the change, so the upload is retried if the background one never runs
            WorkoutDevice::set_connect_pending(tx, &[workout], true).await?;
            Ok(!progression_suggestions(tx).await?.is_empty())
        })
        .await;

    match res {
        Ok(any_progression) => {
            info!("Updated progression flag for workout {workout}");
            events.emit(BackendEvent::ProgressionsAvailable(any_progression));
            // The inherited exercise notes change with the flag
            spawn_workout_sync(pool, settings, connect, mtp, vec![workout], true);
            Ok(())
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_workout_save",
            "Error updating progression flag for workout",
        )),
    }
}

pub async fn save_workout(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    edit: bool,
    workout: Workout,
) -> Result<(), String> {
    let name = workout.name.clone();
    let lang = settings_language(settings);

    // Names are unique: a clear message instead of the constraint error
    let lookup = name.clone();
    let owner = pool
        .run_in_connection(async move |conn| Ok(Workout::get_id(conn, &lookup).await?))
        .await
        .map_err(|e| {
            report_error(
                e,
                settings,
                "error_workout_save",
                "Error checking workout name",
            )
        })?;
    if owner.is_some_and(|owner| !edit || owner != workout.id) {
        return Err(report_error(
            translate_and_replace("workout_name_taken", &[&name], lang),
            settings,
            "error_workout_save",
            "Workout name already taken",
        ));
    }

    let res = pool
        .run_in_transaction(async move |tx| {
            let id = Workout::store(tx, &workout, edit).await?;
            // Percentage loads are resolved here: whatever weight the step came with is replaced
            let exercises: HashSet<(u16, u16)> = workout
                .steps
                .iter()
                .filter(|s| s.one_rm_pct.is_some())
                .filter_map(|s| s.ex_cat.zip(s.ex_id))
                .collect();
            let mut workouts =
                WorkoutStep::refresh_percent_loads(tx, Some(&exercises), now()).await?;
            if !workouts.contains(&id) {
                workouts.push(id);
            }
            // Committed with the change, so the upload is retried if the background one never runs
            WorkoutDevice::set_connect_pending(tx, &workouts, true).await?;
            Ok(workouts)
        })
        .await;

    match res {
        Ok(workouts) => {
            info!("Workout '{}' saved succesfully", name);
            show_notification(NotificationDefinition {
                title: name,
                body: translate("ok_workout_save", lang),
                kind: NotificationKind::Temporal,
            });
            let push_usb = edit || workouts.len() > 1;
            spawn_workout_sync(pool, settings, connect, mtp, workouts, push_usb);
            Ok(())
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_workout_save",
            "Error saving workout",
        )),
    }
}

/// Current estimated 1RM and weight increment of every exercise with recent loaded sets.
pub async fn get_percent_load_bases(
    pool: &DatabasePool,
    settings: &SettingsLock,
) -> Result<Vec<PercentLoadBase>, String> {
    pool.run_in_connection(async move |conn| {
        let exercises: HashMap<(u16, u16), Exercise> = Exercise::get_all(conn)
            .await?
            .into_iter()
            .map(|e| ((e.category, e.id), e))
            .collect();
        let one_rms = current_one_rms(conn, None, &exercises, now()).await?;
        Ok(one_rms
            .into_iter()
            .map(|((ex_cat, ex_id), one_rm)| PercentLoadBase {
                ex_cat,
                ex_id,
                one_rm,
                increment: exercises
                    .get(&(ex_cat, ex_id))
                    .map_or(0.0, |e| to_kg(e.weight)),
            })
            .collect())
    })
    .await
    .map_err(|e| {
        report_error(
            e,
            settings,
            "error_workout_details",
            "Error getting the current 1RM estimates",
        )
    })
}

pub async fn get_planned_workouts(pool: &DatabasePool) -> Result<Vec<WorkoutPlanListItem>, String> {
    Ok(pool
        .run_in_connection(async |conn| Ok(WorkoutPlanning::get_all(conn).await?))
        .await
        .unwrap_or_default()
        .iter()
        .map(WorkoutPlanListItem::from)
        .collect())
}

/// Deletes a future plan not done yet, then in the background sends the new schedule to the
/// plugged in devices and removes the plan from Connect.
pub async fn remove_planned_workout(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    date: u32,
    workout: u16,
) -> Result<(), String> {
    let res = pool
        .run_in_connection(async move |conn| {
            let entry = WorkoutPlanning::get(conn, date, workout).await?.unwrap();
            // Kept as the record of what was planned: the UI offers no delete for these either
            if entry.activity.is_some()
                || today_local_midnight().is_some_and(|today| entry.date < today)
            {
                return Err("Past or done planned workouts can't be deleted".into());
            }
            entry.delete_by_id_in(conn).await?;
            Ok(entry.connect_id)
        })
        .await
        .map_err(|e| e.to_string());

    let connect_id = res?;

    let (pool, settings, connect, mtp) =
        (pool.clone(), settings.clone(), connect.clone(), mtp.clone());
    tauri::async_runtime::spawn(async move {
        send_schedule_to_connected(&pool, &settings, &mtp).await;
        if let Some(id) = connect_id {
            delete_plan_from_connect(&settings, &connect, id.parse().unwrap()).await;
        }
    });

    Ok(())
}

/// Plans `workout` on `date`, then in the background pushes the plan to Connect and sends the new
/// schedule to the plugged in devices.
pub async fn add_planned_workout(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    date: u32,
    workout: u16,
) -> Result<(), String> {
    let res = pool
        .run_in_connection(async move |conn| {
            WorkoutPlanning::insert(conn, date, workout).await?;
            Ok(())
        })
        .await
        .map_err(|e| e.to_string());

    if res.is_ok() {
        let (pool, settings, connect, mtp) =
            (pool.clone(), settings.clone(), connect.clone(), mtp.clone());
        tauri::async_runtime::spawn(async move {
            // Pushing it first can upload the workout, which gives it the Connect creation time
            // the schedule needs
            push_plan_to_connect(&pool, &settings, &connect, date, workout).await;
            send_schedule_to_connected(&pool, &settings, &mtp).await;
        });
    }

    res
}

pub async fn get_next_planned_workout(
    pool: &DatabasePool,
) -> Result<Option<WorkoutPlanListItem>, String> {
    pool.run_in_connection(async |conn| {
        Ok(WorkoutPlanning::get_next(conn, today_local_midnight()).await?)
    })
    .await
    .map_err(|e| e.to_string())
    .map(|w| w.map(|w| WorkoutPlanListItem::from(&w)))
}
