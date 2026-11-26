use rusqlite_orm::database::DatabasePool;
use strength_coach_for_garmin_macros::traced_command;
use tauri::{AppHandle, State};

use crate::{
    SettingsLock,
    logic::{export, settings_language, ui::ask_save_path},
    rclone::providers::CloudProvider,
    utils::translations::translate,
};

/// Whether uploading to the cloud is possible, i.e. the optional rclone dependency is installed.
#[traced_command]
#[tauri::command]
pub async fn is_cloud_upload_available() -> bool {
    export::is_cloud_upload_available().await
}

/// Uploads a snapshot of the database to `provider` through rclone, configuring the remote first if
/// needed (which opens the provider's login in the browser). Notifies on success. On failure, shows
/// a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn upload_to_cloud(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    provider: CloudProvider,
) -> Result<(), String> {
    export::upload_to_cloud(&pool, &settings, provider).await
}

/// Asks where to save the whole database as JSON (defaulting to a timestamped file in the home dir)
/// and writes it there, notifying the path. Resolves without doing anything if the dialog is
/// cancelled. On failure, shows a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn export_database(
    app: AppHandle,
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
) -> Result<(), String> {
    let title = translate("select_file_save_json", settings_language(&settings));
    let default_file = export::default_export_file();
    let Some(path) = ask_save_path(&app, &default_file, "JSON", "json", &title).await? else {
        return Ok(());
    };
    export::export_database(&pool, &settings, &path).await
}
