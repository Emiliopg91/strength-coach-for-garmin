pub mod commands;
pub mod helpers;

use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::info;

use crate::{
    SettingsLock,
    dto::report::ReportData,
    logic::{report::helpers::data::report_data, report_error, settings_language},
    utils::translations::translate,
};

/// Gathers everything the report needs for the `[from, to]` range, reusing the commands' DB logic on one connection.
pub async fn get_report_data(
    pool: &DatabasePool,
    settings: &SettingsLock,
    from: u32,
    to: u32,
) -> Result<ReportData, String> {
    info!("Getting report data...");
    let lang = settings_language(settings);

    let res = pool
        .run_in_connection(async move |conn| Ok(report_data(conn, from, to).await?))
        .await
        .map(|mut data| {
            // Listed by name, which depends on the language
            data.exercises
                .sort_by_key(|e| translate(&format!("exercise_{}_{}", e.category, e.id), lang));
            data
        });

    res.inspect(|data| {
        info!(
            "Retrieved report data: {} activities, {} exercises, {} body metrics",
            data.activities.len(),
            data.exercises.len(),
            data.body_metrics.len()
        )
    })
    .map_err(|e| {
        report_error(
            e,
            settings,
            "error_report_data",
            "Error getting report data",
        )
    })
}
