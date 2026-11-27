pub mod commands;
pub mod helpers;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use chrono::{Local, TimeZone};
use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::info;

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dao::activity::Activity,
    dto::{
        activities::{ActivityDetails, ActivityListItem, ActivityUpdate, MuscleSets},
        notifications::{NotificationDefinition, NotificationKind},
    },
    logic::{
        activities::helpers::{
            edit::{
                activity_file_name, apply_activity_changes, retrieve_activity_connect_id,
                write_activity_fit,
            },
            import::{import_file_paths, notify_new_prs, notify_progression_available},
            queries::{fetch_activities, fetch_activity_details, heatmap_data, muscle_sets},
        },
        connect::helpers::activity_sync::sync_activity_sets_connect,
        events::{BackendEvent, EventSink},
        fit_settings,
        notifications::show_notification,
        report_error, settings_language,
        workouts::helpers::sync::spawn_workout_sync,
    },
    utils::{
        constants,
        translations::{translate, translate_and_replace},
    },
};

/// Returns every recorded activity, newest first.
pub async fn get_activities(
    pool: &DatabasePool,
    settings: &SettingsLock,
    limit: Option<i32>,
) -> Result<Vec<ActivityListItem>, String> {
    info!("Getting activities list...");
    pool.run_in_connection(async move |conn| {
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

/// Returns full details for one activity (series grouped by exercise, heart rate, GPS, speeds,
/// device). Without a Connect id, it is looked up in the background.
pub async fn get_activity_details(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    timestamp: u32,
) -> Result<ActivityDetails, String> {
    info!(
        "Getting details for activity {}",
        Local
            .timestamp_opt(timestamp as i64, 0)
            .unwrap()
            .to_rfc3339()
    );

    let res = pool
        .run_in_connection(async move |conn| {
            Ok(fetch_activity_details(conn, timestamp, true, true).await?)
        })
        .await;

    match res {
        Ok(details) => {
            info!("Found details for activity {}", details.timestamp);
            if details.connect_id.is_none() {
                let (pool, connect) = (pool.clone(), connect.clone());
                tauri::async_runtime::spawn(async move {
                    retrieve_activity_connect_id(&pool, &connect, details.timestamp).await;
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

/// Where the activity at `timestamp` is offered to be saved as a FIT file: `<date> <name>.fit` in
/// the home dir.
pub async fn default_activity_file(pool: &DatabasePool, timestamp: u32) -> PathBuf {
    // Without a name the file is just named after the date; a failure surfaces when writing the file
    let name = pool
        .run_in_connection(async move |conn| Ok(Activity::get_name(conn, timestamp).await?))
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    constants::HOME_DIR.join(activity_file_name(timestamp, &name))
}

/// Writes the activity at `timestamp` as a FIT file at `file` and notifies the path.
pub async fn download_activity(
    pool: &DatabasePool,
    settings: &SettingsLock,
    timestamp: u32,
    file: &Path,
) -> Result<(), String> {
    let (lang, weight_unit) = fit_settings(settings);

    match write_activity_fit(pool, timestamp, file.to_path_buf(), lang, weight_unit).await {
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
pub async fn save_activity_changes(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    events: &EventSink,
    details: ActivityUpdate,
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

    let res = pool
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
            events.emit(BackendEvent::ProgressionsAvailable(changes.any_progression));

            spawn_workout_sync(
                pool,
                settings,
                connect,
                mtp,
                changes.refreshed_workouts,
                true,
            );

            let (pool, connect) = (pool.clone(), connect.clone());
            tauri::async_runtime::spawn(async move {
                sync_activity_sets_connect(&pool, &connect, timestamp, sets).await;
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

/// Stores the perceived effort (1-10, `None` clears it) of the activity at `timestamp`.
pub async fn set_activity_rpe(
    pool: &DatabasePool,
    settings: &SettingsLock,
    timestamp: u32,
    rpe: Option<u8>,
) -> Result<(), String> {
    info!("Setting RPE {rpe:?} on activity {timestamp}");
    pool.run_in_connection(async move |conn| Ok(Activity::set_rpe(conn, timestamp, rpe).await?))
        .await
        .map(|_| ())
        .map_err(|e| {
            report_error(
                e,
                settings,
                "error_update_activity",
                "Error updating activity RPE",
            )
        })
}

/// Stores the feeling (0-4, `None` clears it) of the activity at `timestamp`.
pub async fn set_activity_feeling(
    pool: &DatabasePool,
    settings: &SettingsLock,
    timestamp: u32,
    feeling: Option<u8>,
) -> Result<(), String> {
    info!("Setting feeling {feeling:?} on activity {timestamp}");
    pool.run_in_connection(async move |conn| {
        Ok(Activity::set_feeling(conn, timestamp, feeling).await?)
    })
    .await
    .map(|_| ())
    .map_err(|e| {
        report_error(
            e,
            settings,
            "error_update_activity",
            "Error updating activity feeling",
        )
    })
}

/// Imports the `.fit` files at `files` as activities; returns the number of activities imported.
pub async fn import_from_files(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    events: &EventSink,
    files: Vec<PathBuf>,
) -> Result<usize, String> {
    if files.is_empty() {
        return Ok(0);
    }
    import_file_paths(pool, settings, connect, mtp, events, files).await
}

pub async fn get_heatmap_data(pool: &DatabasePool) -> Result<Vec<Vec<(f32, u16)>>, String> {
    heatmap_data(pool).await
}

pub async fn get_muscle_sets(
    pool: &DatabasePool,
    since: u32,
) -> Result<BTreeMap<String, MuscleSets>, String> {
    muscle_sets(pool, since).await
}
