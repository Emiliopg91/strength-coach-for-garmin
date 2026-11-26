pub mod commands;
pub mod helpers;

use std::collections::{HashMap, HashSet};

use rusqlite_orm::{database::DatabasePool, errors::DatabaseError};
use tauri_plugin_log::log::info;

use crate::utils::weight::from_kg;
use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dao::{
        exercise::Exercise, workout::Workout, workout_device::WorkoutDevice,
        workout_step::WorkoutStep,
    },
    dto::exercises::{ExerciseDetails, ExerciseListItem, ProgressionSuggestion},
    logic::{
        exercises::helpers::{
            progressions::{
                any_progression_suggestion, dismiss_progression, fetch_progression_suggestion,
            },
            queries::{exercise_list, exercises_catalog, fetch_exercise_details},
        },
        report_error,
        workouts::helpers::sync::spawn_workout_sync,
    },
    utils::time::now,
};

/// Returns the exercises with a personal record, annotated with it, plus those planned in some
/// workout step that have none yet.
pub async fn get_exercises(
    pool: &DatabasePool,
    settings: &SettingsLock,
) -> Result<Vec<ExerciseListItem>, String> {
    info!("Getting exercises list...");
    pool.run_in_connection(async move |conn| Ok(exercise_list(conn).await?))
        .await
        .inspect(|l| info!("Retreived {} exercises", l.len()))
        .map_err(|e| {
            report_error(
                e,
                settings,
                "error_exercise_list",
                "Error getting exercises list",
            )
        })
}

/// Returns the personal record and full per-activity set history for one exercise.
pub async fn get_exercise_details(
    pool: &DatabasePool,
    settings: &SettingsLock,
    category: u16,
    id: u16,
) -> Result<ExerciseDetails, String> {
    info!(
        "Getting details for exercise with category {} and id {}...",
        category, id
    );
    pool.run_in_connection(async move |conn| {
        Ok(fetch_exercise_details(conn, category, id, None).await?)
    })
    .await
    .inspect(|_| info!("Found details for exercise {} - {}", category, id))
    .map_err(|e| {
        report_error(
            e,
            settings,
            "error_exercise_details",
            "Error getting exercise details",
        )
    })
}

pub async fn get_exercises_catalog(pool: &DatabasePool) -> Result<HashMap<u16, Vec<u16>>, String> {
    pool.run_in_connection(async move |conn| Ok(exercises_catalog(conn).await?))
        .await
        .map_err(|e: DatabaseError| e.to_string())
}

pub async fn set_exercise_unilateral(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    category: u16,
    id: u16,
    value: bool,
) -> Result<(), String> {
    pool.run_in_transaction(async move |tx| {
        Exercise::set_unilateral(tx, category, id, value).await?;
        // Unilateral sets estimate the 1RM from half their reps
        let keys: HashSet<(u16, u16)> = [(category, id)].into();
        Ok(WorkoutStep::refresh_percent_loads(tx, Some(&keys), now()).await?)
    })
    .await
    .map(|refreshed| spawn_workout_sync(pool, settings, connect, mtp, refreshed, true))
    .inspect(|_| info!("Updated unilateral flag for exercise"))
    .map_err(|e| {
        report_error(
            e,
            settings,
            "error_updating_exercise",
            "Error updating unilateral flag for exercise",
        )
    })
}

pub async fn set_exercise_progression(
    pool: &DatabasePool,
    settings: &SettingsLock,
    category: u16,
    id: u16,
    value: bool,
) -> Result<(), String> {
    pool.run_in_connection(async move |conn| {
        Ok(Exercise::set_progression(conn, category, id, value).await?)
    })
    .await
    .inspect(|_| info!("Updated progression flag for exercise"))
    .map_err(|e| {
        report_error(
            e,
            settings,
            "error_updating_exercise",
            "Error updating progression flag for exercise",
        )
    })
}

pub async fn set_exercise_progression_parameters(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    (category, id): (u16, u16),
    reps: &str,
    weight: f32,
) -> Result<(), String> {
    let reps = reps.to_string();
    pool.run_in_transaction(async move |tx| {
        Exercise::set_progression_parameters(tx, category, id, reps, from_kg(weight)).await?;
        // Percentage loads are rounded to the weight increment
        let keys: HashSet<(u16, u16)> = [(category, id)].into();
        Ok(WorkoutStep::refresh_percent_loads(tx, Some(&keys), now()).await?)
    })
    .await
    .map(|refreshed| spawn_workout_sync(pool, settings, connect, mtp, refreshed, true))
    .inspect(|_| info!("Updated rep ranges for exercise"))
    .map_err(|e| {
        report_error(
            e,
            settings,
            "error_updating_exercise",
            "Error updating rep ranges for exercise",
        )
    })
}

pub async fn get_progression_status(
    pool: &DatabasePool,
    category: u16,
    id: u16,
) -> Result<Option<ProgressionSuggestion>, String> {
    pool.run_in_connection(async move |conn| {
        let workouts = Workout::with_progression(conn).await?;
        Ok(WorkoutStep::get_highest_load(conn, category, id, &workouts).await?)
    })
    .await
    .map(|load| load.map(|(weight, reps)| ProgressionSuggestion { weight, reps }))
    .map_err(|e: DatabaseError| e.to_string())
}

pub async fn get_progression_suggestion(
    pool: &DatabasePool,
    category: u16,
    id: u16,
) -> Result<Option<ProgressionSuggestion>, String> {
    pool.run_in_connection(async move |conn| {
        Ok(fetch_progression_suggestion(conn, category, id).await?)
    })
    .await
    .map_err(|e: DatabaseError| e.to_string())
}

pub async fn apply_progression_suggestion(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    category: u16,
    id: u16,
    suggestion: ProgressionSuggestion,
) -> Result<(), String> {
    let res = pool
        .run_in_transaction(async move |tx| {
            let (modified, workouts) = WorkoutStep::apply_progression(
                tx,
                category,
                id,
                suggestion.weight,
                suggestion.reps,
            )
            .await?;
            // Committed with the change, so the upload is retried if the background one never runs
            WorkoutDevice::set_connect_pending(tx, &workouts, true).await?;
            Ok((modified, workouts))
        })
        .await
        .map_err(|e| e.to_string());

    match res {
        Ok((modified, workouts)) => {
            info!("Applied suggested progression");
            spawn_workout_sync(pool, settings, connect, mtp, workouts, modified);
            Ok(())
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_applying_progression",
            "Error applying progression",
        )),
    }
}

pub async fn dismiss_progression_suggestion(
    pool: &DatabasePool,
    settings: &SettingsLock,
    category: u16,
    id: u16,
) -> Result<(), String> {
    pool.run_in_connection(async move |conn| Ok(dismiss_progression(conn, category, id).await?))
        .await
        .inspect(|_| info!("Dismissed progression suggestion"))
        .map_err(|e| {
            report_error(
                e,
                settings,
                "error_dismissing_progression",
                "Error dismissing progression suggestion",
            )
        })
}

pub async fn save_exercise_notes(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    category: u16,
    id: u16,
    notes: &str,
) -> Result<(), String> {
    let notes = if notes.trim().is_empty() {
        None
    } else {
        Some(notes.trim().to_string())
    };

    let workouts = pool
        .run_in_transaction(async move |tx| {
            let workouts = Exercise::update_notes(tx, category, id, notes.clone()).await?;
            // Committed with the change, so the upload is retried if the background one never runs
            WorkoutDevice::set_connect_pending(tx, &workouts, true).await?;
            Ok(workouts)
        })
        .await
        .map_err(|e| e.to_string())?;

    spawn_workout_sync(pool, settings, connect, mtp, workouts, true);

    Ok(())
}

pub async fn available_any_progression_suggestions(pool: &DatabasePool) -> bool {
    pool.run_in_connection(async move |conn| Ok(any_progression_suggestion(conn).await?))
        .await
        .unwrap_or(false)
}
