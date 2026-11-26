use rusqlite_orm::database::DatabasePool;
use strength_coach_for_garmin_macros::traced_command;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_log::log::info;

use crate::{
    SettingsLock,
    dto::report::ReportData,
    logic::{
        report, settings_language,
        ui::{open_window, print_to_pdf},
    },
    utils::translations::translate,
};

/// Opens the hidden report window for the range `from`..`to` (Unix seconds). It prints itself to
/// PDF once it calls `report_ready`.
#[traced_command]
#[tauri::command]
pub async fn load_report(app: AppHandle, from: u32, to: u32) -> Result<(), String> {
    open_window(&app, "report", Some(&format!("from={from}&to={to}")))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Called by the report window once rendered: asks where to save the PDF (defaulting to the
/// downloads dir), prints it there and opens it. The window is closed when done, on cancel or on
/// error.
#[traced_command]
#[tauri::command]
pub async fn report_ready(window: WebviewWindow) -> Result<(), String> {
    info!("Report ready, printing");
    let lang = settings_language(&window.state::<SettingsLock>());
    print_to_pdf(window, &translate("progression_report", lang)).await
}

/// Strength activities in `from`..`to` (Unix seconds), the history in that range of every exercise
/// trained in them (sorted by translated name) and the body measurements of the period. On failure,
/// shows a persistent error notification and rejects with the message.
#[traced_command(log_payload = false)]
#[tauri::command]
pub async fn get_report_data(
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    from: u32,
    to: u32,
) -> Result<ReportData, String> {
    report::get_report_data(&pool, &settings, from, to).await
}
