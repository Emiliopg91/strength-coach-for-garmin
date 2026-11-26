use std::{collections::BTreeMap, path::PathBuf};

use rusqlite_orm::database::DatabasePool;
use strength_coach_for_garmin_macros::traced_command;
use tauri::{AppHandle, Manager, State};

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dto::activities::{ActivityDetails, ActivityListItem, ActivityStepsUpdate, MuscleSets},
    logic::{
        activities, settings_language,
        ui::{ask_open_paths, ask_save_path, event_sink},
    },
    utils::translations::translate,
};

// Tauri commands: thin entry points over the logic of each module, which is what tests call
/// Every activity, newest first, flagged with whether it has sets and records. Despite its name,
/// `limit` is the earliest activity date (Unix seconds) to include; `null` lists them all. On
/// failure, shows a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn get_activities(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    limit: Option<i32>,
) -> Result<Vec<ActivityListItem>, String> {
    activities::get_activities(&pool, &settings, limit).await
}

/// Full details of the activity at `timestamp`: sets, laps, heart rate, GPS, speeds and device. If
/// it has no Connect id, looks it up in Connect in the background. On failure, shows a persistent
/// error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn get_activity_details(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    timestamp: u32,
) -> Result<ActivityDetails, String> {
    activities::get_activity_details(&pool, &settings, &connect, timestamp).await
}

/// Asks where to save the activity at `timestamp` as a FIT file (defaulting to `<date> <name>.fit`
/// in the home dir) and writes it there, notifying the path. Resolves without doing anything if the
/// dialog is cancelled. On failure, shows a persistent error notification and rejects with the
/// message.
#[traced_command]
#[tauri::command]
pub async fn download_activity(
    app: AppHandle,
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    timestamp: u32,
) -> Result<(), String> {
    let default_file = activities::default_activity_file(&pool, timestamp).await;
    let title = translate("select_file_save_fit", settings_language(&settings));
    let Some(file) = ask_save_path(&app, &default_file, "FIT", "fit", &title).await? else {
        return Ok(());
    };
    activities::download_activity(&pool, &settings, timestamp, &file).await
}

/// Stores the edited notes and sets (reps, weight, exercise) of an activity and recomputes the
/// records of the exercises involved. Notifies the update, new records and new progression
/// suggestions, emits `progressions_available`, and syncs the sets to Connect in the background. On
/// failure, shows a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn save_activity_changes(
    app: AppHandle,
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    details: ActivityStepsUpdate,
) -> Result<(), String> {
    let events = event_sink(&app);
    activities::save_activity_changes(&pool, &settings, &connect, &mtp, &events, details).await
}

/// Asks for `.fit` files and imports them as activities. Notifies the result (files that failed to
/// parse, activities imported, new records and progressions) and emits `progressions_available`; new
/// activities are geocoded in the background. Resolves to the number of activities imported, `0` if the
/// dialog is cancelled. On failure, shows a persistent error notification and rejects with the
/// message.
#[traced_command]
#[tauri::command]
pub async fn import_from_files(
    app: AppHandle,
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
) -> Result<usize, String> {
    let title = translate("select_files_import", settings_language(&settings));
    let files = ask_open_paths(&app, "FIT", "fit", &title).await?;
    let events = event_sink(&app);
    activities::import_from_files(&pool, &settings, &connect, &mtp, &events, files).await
}

/// Imports the `.fit` files dropped on the main window. Not a command: the window event handler
/// calls it.
pub(crate) async fn import_dropped_files(
    app: &AppHandle,
    files: Vec<PathBuf>,
) -> Result<usize, String> {
    activities::import_from_files(
        &app.state::<DatabasePool>(),
        &app.state::<SettingsLock>(),
        &app.state::<ConnectLock>(),
        &app.state::<MtpLock>(),
        &event_sink(app),
        files,
    )
    .await
}

/// Activity of the last 12 months as a 12x31 grid indexed by month and day (0-based): each cell
/// holds the summed training load and number of records of that day.
#[traced_command]
#[tauri::command]
pub async fn get_heatmap_data(
    pool: State<'_, DatabasePool>,
) -> Result<Vec<Vec<(f32, u16)>>, String> {
    activities::get_heatmap_data(&pool).await
}

/// Exercise sets performed in activities from `since` (Unix seconds) on, added up by muscle name:
/// how many had it as a primary muscle and how many only as a secondary one. Muscles not worked
/// are left out.
#[traced_command]
#[tauri::command]
pub async fn get_muscle_sets(
    pool: State<'_, DatabasePool>,
    since: u32,
) -> Result<BTreeMap<String, MuscleSets>, String> {
    activities::get_muscle_sets(&pool, since).await
}
