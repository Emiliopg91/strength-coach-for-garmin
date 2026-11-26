use rusqlite_orm::database::DatabasePool;
use strength_coach_for_garmin_macros::traced_command;
use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_log::log::info;

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dao::workout::Workout,
    dto::workouts::{PercentLoadBase, WorkoutDetails, WorkoutListItem, WorkoutPlanListItem},
    logic::{
        ui::{event_sink, open_window, print_to_pdf},
        workouts,
    },
};

/// Every workout with its activity stats (count, average time and load, latest date), by name. On
/// failure, shows a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn get_workout_list(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
) -> Result<Vec<WorkoutListItem>, String> {
    workouts::get_workout_list(&pool, &settings).await
}

/// The workout `id` with its steps, worked muscles and every activity with its volume. On failure,
/// shows a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn get_workout_details(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    id: u16,
) -> Result<WorkoutDetails, String> {
    workouts::get_workout_details(&pool, &settings, id).await
}

/// Enables or disables `workout`. Enabling pushes it to the connected USB devices and the Connect
/// devices that have it; disabling removes it from the connected USB devices.
#[traced_command]
#[tauri::command]
pub async fn set_workout_status(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    workout: u16,
    status: bool,
) -> Result<(), String> {
    workouts::set_workout_status(&pool, &settings, &connect, &mtp, workout, status).await
}

/// Enables or disables progression for `workout`: suggestions for its exercises and the exercise
/// notes inherited by its steps. Emits `progressions_available`, then re-sends the workout in the
/// background to Connect and the connected USB devices that have it. On failure, shows a
/// persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn set_workout_progression(
    app: AppHandle,
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    workout: u16,
    value: bool,
) -> Result<(), String> {
    let events = event_sink(&app);
    workouts::set_workout_progression(&pool, &settings, &connect, &mtp, &events, workout, value)
        .await
}

/// Creates `workout`, or replaces the name, notes and steps of the existing one with its id when
/// `edit`, and uploads it to Connect in the background. A renamed workout has its old file deleted
/// from each USB device before the next push there. When editing, it is also pushed to the
/// connected USB devices that have it. Notifies on success. On failure, shows a persistent error
/// notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn save_workout(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    edit: bool,
    workout: Workout,
) -> Result<(), String> {
    workouts::save_workout(&pool, &settings, &connect, &mtp, edit, workout).await
}

/// Opens the hidden workout window for `workout`. It prints itself to PDF once it calls
/// `workout_print_ready`.
#[traced_command]
#[tauri::command]
pub async fn load_workout_page(app: AppHandle, workout: u16) -> Result<(), String> {
    open_window(&app, "workout", Some(&format!("workout={workout}")))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Called by the workout window once rendered: asks where to save the PDF (defaulting to
/// `<workout>.pdf` in the downloads dir, `workout` being its name), prints it there and opens it. The window is closed when
/// done, on cancel or on error.
#[traced_command]
#[tauri::command]
pub async fn workout_print_ready(window: WebviewWindow, workout: &str) -> Result<(), String> {
    info!("Workout ready, printing");
    print_to_pdf(window, workout).await
}

/// Current estimated 1RM (best of the last weeks) and weight increment of every exercise with
/// recent loaded sets, to preview the steps loaded with a percentage of it. On failure, shows a
/// persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn get_percent_load_bases(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
) -> Result<Vec<PercentLoadBase>, String> {
    workouts::get_percent_load_bases(&pool, &settings).await
}

/// List planned workouts.
#[traced_command]
#[tauri::command]
pub async fn get_planned_workouts(
    pool: State<'_, DatabasePool>,
) -> Result<Vec<WorkoutPlanListItem>, String> {
    workouts::get_planned_workouts(&pool).await
}

/// Delete planned workout.
#[traced_command]
#[tauri::command]
pub async fn delete_planned_workout(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    date: u32,
    workout: u16,
) -> Result<(), String> {
    workouts::remove_planned_workout(&pool, &settings, &connect, &mtp, date, workout).await
}

/// Add planned workout.
#[traced_command]
#[tauri::command]
pub async fn add_planned_workout(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    date: u32,
    workout: u16,
) -> Result<(), String> {
    workouts::add_planned_workout(&pool, &settings, &connect, &mtp, date, workout).await
}

/// get next planned workout.
#[traced_command]
#[tauri::command]
pub async fn get_next_planned_workout(
    pool: State<'_, DatabasePool>,
) -> Result<Option<WorkoutPlanListItem>, String> {
    workouts::get_next_planned_workout(&pool).await
}
