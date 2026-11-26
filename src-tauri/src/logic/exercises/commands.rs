use std::collections::HashMap;

use rusqlite_orm::database::DatabasePool;
use strength_coach_for_garmin_macros::traced_command;
use tauri::State;

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dto::exercises::{ExerciseDetails, ExerciseListItem, ProgressionSuggestion},
    logic::exercises,
};

/// Exercises with a personal record, annotated with it, plus those planned in some workout step
/// that have none yet. On failure, shows a persistent error notification and rejects with the
/// message.
#[traced_command]
#[tauri::command]
pub async fn get_exercises(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
) -> Result<Vec<ExerciseListItem>, String> {
    exercises::get_exercises(&pool, &settings).await
}

/// Personal record and full per-activity set history of the exercise `category`/`id`. On failure,
/// shows a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn get_exercise_details(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    category: u16,
    id: u16,
) -> Result<ExerciseDetails, String> {
    exercises::get_exercise_details(&pool, &settings, category, id).await
}

/// Exercises that can be picked, as exercise ids by category. A category's generic exercise is left
/// out when the category has a real exercise with the same name.
#[traced_command]
#[tauri::command]
pub async fn get_exercises_catalog(
    pool: State<'_, DatabasePool>,
) -> Result<HashMap<u16, Vec<u16>>, String> {
    exercises::get_exercises_catalog(&pool).await
}

/// Marks the exercise `category`/`id` as performed one side at a time, or not, and sends again the
/// workouts whose percentage loads change with its estimated 1RM. On failure, shows a persistent
/// error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn set_exercise_unilateral(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    category: u16,
    id: u16,
    value: bool,
) -> Result<(), String> {
    exercises::set_exercise_unilateral(&pool, &settings, &connect, &mtp, category, id, value).await
}

/// Enables or disables progression suggestions for the exercise `category`/`id`. On failure, shows
/// a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn set_exercise_progression(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    category: u16,
    id: u16,
    value: bool,
) -> Result<(), String> {
    exercises::set_exercise_progression(&pool, &settings, category, id, value).await
}

/// Sets the progression parameters of the exercise `category`/`id`: the rep range `reps` (e.g.
/// `"8-10-12"`) and the weight increment `weight`, and sends again the workouts whose percentage
/// loads change with the new rounding. On failure, shows a persistent error notification and
/// rejects with the message.
// Four of them are the state handed to the logic, the rest the frontend's arguments
#[allow(clippy::too_many_arguments)]
#[traced_command]
#[tauri::command]
pub async fn set_exercise_progression_parameters(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    category: u16,
    id: u16,
    reps: &str,
    weight: f32,
) -> Result<(), String> {
    exercises::set_exercise_progression_parameters(
        &pool,
        &settings,
        &connect,
        &mtp,
        (category, id),
        reps,
        weight,
    )
    .await
}

/// Highest load programmed for the exercise `category`/`id` across workouts, `null` if none.
#[traced_command]
#[tauri::command]
pub async fn get_progression_status(
    pool: State<'_, DatabasePool>,
    category: u16,
    id: u16,
) -> Result<Option<ProgressionSuggestion>, String> {
    exercises::get_progression_status(&pool, category, id).await
}

/// Pending progression suggestion for the exercise `category`/`id`, `null` if none.
#[traced_command]
#[tauri::command]
pub async fn get_progression_suggestion(
    pool: State<'_, DatabasePool>,
    category: u16,
    id: u16,
) -> Result<Option<ProgressionSuggestion>, String> {
    exercises::get_progression_suggestion(&pool, category, id).await
}

/// Whether any exercise has a progression suggestion pending. `false` if it can't be checked.
#[traced_command]
#[tauri::command]
pub async fn available_any_progression_suggestions(
    pool: State<'_, DatabasePool>,
) -> Result<bool, String> {
    Ok(exercises::available_any_progression_suggestions(&pool).await)
}

/// Raises to `suggestion` the steps programming the exercise `category`/`id` below it, then
/// re-uploads in the background the affected workouts to Connect and, if any step changed, pushes
/// them to the connected USB devices. On failure, shows a persistent error notification and
/// rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn apply_progression_suggestion(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    category: u16,
    id: u16,
    suggestion: ProgressionSuggestion,
) -> Result<(), String> {
    exercises::apply_progression_suggestion(
        &pool, &settings, &connect, &mtp, category, id, suggestion,
    )
    .await
}

/// Dismisses the progression suggestion of the exercise `category`/`id`: it stays hidden until a
/// newer activity of the exercise suggests a progression again. On failure, shows a persistent
/// error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn dismiss_progression_suggestion(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    category: u16,
    id: u16,
) -> Result<(), String> {
    exercises::dismiss_progression_suggestion(&pool, &settings, category, id).await
}

/// Stores the notes of the exercise `category`/`id` (blank clears them) and re-sends in the
/// background, to Connect and the connected USB devices, the workouts whose steps inherit them.
#[traced_command]
#[tauri::command]
pub async fn save_exercise_notes(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    category: u16,
    id: u16,
    notes: &str,
) -> Result<(), String> {
    exercises::save_exercise_notes(&pool, &settings, &connect, &mtp, category, id, notes).await
}
