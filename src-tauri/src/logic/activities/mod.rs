pub mod commands;
pub mod helpers;

use chrono::{Local, TimeZone};
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_log::log::info;

use self::helpers::{
    activity_file_name, apply_activity_changes, fetch_activities, fetch_activity_details,
    heatmap_data, import_file_paths, notify_new_prs, notify_progression_available,
    retrieve_activity_connect_id, spawn_percent_load_sync, write_activity_fit,
};
use crate::{
    SettingsLock,
    dao::activity::Activity,
    dto::{
        activities::{ActivityDetails, ActivityListItem, ActivityStepsUpdate},
        notifications::{NotificationDefinition, NotificationKind},
    },
    logic::{
        ask_open_paths, ask_save_path, connect::helpers::sync_activity_sets_connect, fit_settings,
        notifications::show_notification, pool, report_error, settings_language,
    },
    utils::{
        constants,
        translations::{translate, translate_and_replace},
    },
};

/// Returns every recorded activity, newest first.
pub async fn get_activities<R: Runtime>(
    app: &AppHandle<R>,
    settings: &SettingsLock,
    limit: Option<i32>,
) -> Result<Vec<ActivityListItem>, String> {
    info!("Getting activities list...");
    pool(app)
        .run_in_connection(async move |conn| {
            Ok(fetch_activities(conn, limit, None, None, None).await?)
        })
        .await
        .inspect(|l| info!("Retreived {} activities", l.len()))
        .map_err(|e| {
            report_error(
                e,
                settings,
                "error_activity_list",
                "Error getting activities list",
            )
        })
}

/// Returns full details for one activity (series grouped by exercise, heart rate, GPS, speeds, device).
pub async fn get_activity_details<R: Runtime>(
    app: &AppHandle<R>,
    settings: &SettingsLock,
    timestamp: u32,
) -> Result<ActivityDetails, String> {
    info!(
        "Getting details for activity {}",
        Local
            .timestamp_opt(timestamp as i64, 0)
            .unwrap()
            .to_rfc3339()
    );

    let res = pool(app)
        .run_in_connection(async move |conn| {
            Ok(fetch_activity_details(conn, timestamp, true, true).await?)
        })
        .await;

    match res {
        Ok(details) => {
            info!("Found details for activity {}", details.timestamp);
            if details.connect_id.is_none() {
                let app_cl = app.clone();
                tauri::async_runtime::spawn(async move {
                    retrieve_activity_connect_id(&app_cl, details.timestamp).await;
                });
            }

            Ok(details)
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_activity_details",
            "Error getting activity details",
        )),
    }
}

/// Asks where to save the activity as a FIT file and writes it there.
pub async fn download_activity<R: Runtime>(
    app: &AppHandle<R>,
    settings: &SettingsLock,
    timestamp: u32,
) -> Result<(), String> {
    let (lang, weight_unit) = fit_settings(app);

    // Without a name the file is just named after the date; a failure surfaces when writing the file
    let name = pool(app)
        .run_in_connection(async move |conn| Ok(Activity::get_name(conn, timestamp).await?))
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let default_file = constants::HOME_DIR.join(activity_file_name(timestamp, &name));
    let Some(file) = ask_save_path(
        app,
        &default_file,
        "FIT",
        "fit",
        &translate("select_file_save_fit", lang),
    )
    .await?
    else {
        return Ok(());
    };

    match write_activity_fit(app, timestamp, file.clone(), lang, weight_unit).await {
        Ok(()) => {
            show_notification(NotificationDefinition {
                title: translate("ok_activity_download", lang),
                body: translate_and_replace(
                    "export_file_path",
                    &[&file.display().to_string()],
                    lang,
                ),
                kind: NotificationKind::Temporal,
            });
            Ok(())
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_activity_download",
            "Error downloading activity",
        )),
    }
}

/// Applies user edits (activity notes and reps/weight/exercise of its series) and recomputes personal records.
pub async fn save_activity_changes<R: Runtime>(
    app: &AppHandle<R>,
    settings: &SettingsLock,
    details: ActivityStepsUpdate,
) -> Result<(), String> {
    info!(
        "Saving changes on activity {}...",
        Local
            .timestamp_opt(details.timestamp as i64, 0)
            .unwrap()
            .to_rfc3339()
    );
    let lang = settings_language(settings);
    let (timestamp, sets) = (details.timestamp, details.sets.clone());

    let res = pool(app)
        .run_in_transaction(async move |tx| Ok(apply_activity_changes(tx, &details).await?))
        .await;

    match res {
        Ok(changes) => {
            info!("Activity updated succesfully");
            show_notification(NotificationDefinition {
                title: translate("ok_update_activity", lang),
                body: String::new(),
                kind: NotificationKind::Temporal,
            });

            if changes.new_prs {
                notify_new_prs(lang);
            }
            if changes.newly_suggested {
                notify_progression_available(lang);
            }
            // Typed binding: the TypeScript generator reads the payload type from it
            let payload: bool = changes.any_progression;
            let _ = app.emit("progressions_available", payload);

            spawn_percent_load_sync(app, changes.refreshed_workouts);

            let app_cl = app.clone();
            tauri::async_runtime::spawn(async move {
                sync_activity_sets_connect(&app_cl, timestamp, sets).await;
            });

            Ok(())
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_update_activity",
            "Error updating activity",
        )),
    }
}

/// Asks the user for `.fit` files and imports them; returns the number of activities imported.
pub async fn import_from_files<R: Runtime>(
    app: &AppHandle<R>,
    settings: &SettingsLock,
) -> Result<usize, String> {
    let lang = settings_language(settings);
    let files = ask_open_paths(app, "FIT", "fit", &translate("select_files_import", lang)).await?;

    if !files.is_empty() {
        return import_file_paths(app, files).await;
    }

    Ok(0)
}

pub async fn get_heatmap_data<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Vec<Vec<(u32, u16)>>, String> {
    heatmap_data(&pool(app)).await
}
