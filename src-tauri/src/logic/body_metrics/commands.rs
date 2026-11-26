use rusqlite_orm::database::DatabasePool;
use strength_coach_for_garmin_macros::traced_command;
use tauri::State;

use crate::{SettingsLock, dto::body_metrics::BodyMetricListItem, logic::body_metrics};

/// Every logged body measurement, newest first. On failure, shows a persistent error notification
/// and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn get_body_measures(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
) -> Result<Vec<BodyMetricListItem>, String> {
    body_metrics::get_body_measures(&pool, &settings).await
}

/// Stores a new body measurement entry. On failure, shows a persistent error notification and
/// rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn add_body_measures(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    measures: BodyMetricListItem,
) -> Result<(), String> {
    body_metrics::add_body_measures(&pool, &settings, measures).await
}

/// Deletes the body measurement logged at `date` (Unix seconds), notifying on success. On failure,
/// shows a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn delete_body_metric(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    date: u32,
) -> Result<(), String> {
    body_metrics::delete_body_metric(&pool, &settings, date).await
}
