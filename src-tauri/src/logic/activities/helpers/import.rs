use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use chrono::{Local, TimeZone, offset::LocalResult};
use rayon::{iter::Either, prelude::*};
use rusqlite_orm::{database::DatabasePool, errors::DatabaseError, rusqlite::Transaction};
use tauri_plugin_log::log::{error, info, warn};

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dao::{
        activity::Activity, activity_step::ActivityStep, device::Device, workout_step::WorkoutStep,
    },
    dto::notifications::{NotificationDefinition, NotificationKind},
    fit::{FitFile, parser::FitParser},
    logic::{
        events::{BackendEvent, EventSink},
        exercises::helpers::progressions::progression_suggestions,
        notifications::show_notification,
        report_error, settings_language, store_pending_files,
        workouts::helpers::sync::spawn_workout_sync,
    },
    utils::{
        constants,
        time::now,
        translations::{Languages, translate, translate_and_replace},
    },
};

use crate::logic::activities::helpers::geolocation::spawn_geolocation_update;

/// Reads and imports the `.fit` files at `files` as activities; returns how many were imported.
pub(crate) async fn import_file_paths(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    events: &EventSink,
    files: Vec<PathBuf>,
) -> Result<usize, String> {
    let lang = settings_language(settings);
    // Reading and parsing the files is blocking work, done before taking a pooled connection
    let parsed = tokio::task::spawn_blocking(move || {
        let files: Vec<FitFile> = files
            .iter()
            .filter_map(|path| match FitFile::read(path) {
                Ok(file) => Some(file),
                Err(e) => {
                    error!("Error reading activity file {}: {}", path.display(), e);
                    notify_parse_error(&path.display().to_string(), &e.to_string(), lang);
                    None
                }
            })
            .collect();
        parse_files(&files)
    })
    .await
    .expect("FIT reading task panicked");

    let res = pool
        .run_in_transaction(async move |tx| Ok(import_file_list(tx, parsed).await?))
        .await
        .map_err(|e| e.to_string());

    finish_activity_import(pool, settings, connect, mtp, events, res)
}

/// Imports as activities of `device` the `.fit` files fetched from it. Those that fail to parse, or
/// all of them if the transaction is rolled back, are stored as pending. With `sync_mark`, the
/// device's last activity sync is moved to it in the same transaction. The outcome is left for
/// `finish_activity_import`.
pub(crate) async fn import_pending_activities(
    pool: &DatabasePool,
    files: Vec<FitFile>,
    device: &mut Device,
    sync_mark: Option<u32>,
) -> Result<ImportedFiles, DatabaseError> {
    // Parsed before taking a pooled connection. The files are kept to store them as pending if the
    // transaction fails; their clones share the contents
    let parsed = {
        let files = files.clone();
        tokio::task::spawn_blocking(move || parse_files(&files))
            .await
            .expect("FIT parsing task panicked")
    };
    let mut tx_device = device.clone();
    let res = pool
        .run_in_transaction(async move |tx| {
            let imported = import_device_files(tx, parsed, &mut tx_device, sync_mark).await?;
            Ok((imported, tx_device))
        })
        .await;

    let pending_dir = &constants::PENDING_DOWNLOAD_ACTIVITIES_FOLDER;
    match res {
        Ok((imported, updated_device)) => {
            *device = updated_device;
            store_pending_files(
                pending_dir,
                &device.serial,
                &imported
                    .failed
                    .iter()
                    .map(|(file, _)| file)
                    .collect::<Vec<_>>(),
            )
            .await;
            Ok(imported)
        }
        Err(e) => {
            store_pending_files(
                pending_dir,
                &device.serial,
                &files.iter().collect::<Vec<_>>(),
            )
            .await;
            Err(e)
        }
    }
}

/// Ends an import of activities: notifies how it went, syncs in the background the workouts whose
/// loads changed and looks up the place names of the imported activities, then returns how many
/// there are. On failure, notifies the error.
pub(crate) fn finish_activity_import<E: std::fmt::Display>(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    events: &EventSink,
    res: Result<ImportedFiles, E>,
) -> Result<usize, String> {
    match res {
        Ok(imported) => {
            notify_import(events, &imported, settings_language(settings));
            spawn_workout_sync(
                pool,
                settings,
                connect,
                mtp,
                imported.refreshed_workouts,
                true,
            );
            if !imported.activities.is_empty() {
                spawn_geolocation_update(pool, connect, events);
            }
            Ok(imported.activities.len())
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_import_activities",
            "Error importing activities",
        )),
    }
}

/// Tells the user, once the import is committed, how it went: files that failed to parse,
/// activities imported, new records and progressions. Also lets the frontend know whether any
/// progression is pending.
fn notify_import(events: &EventSink, imported: &ImportedFiles, lang: Languages) {
    for (file, e) in &imported.failed {
        notify_parse_error(&file.name, e, lang);
    }

    if !imported.activities.is_empty() {
        show_notification(NotificationDefinition {
            title: translate("ok_import_activities", lang),
            body: translate_and_replace(
                "imported_n_activities",
                &[&imported.activities.len().to_string()],
                lang,
            ),
            kind: NotificationKind::Temporal,
        });
    }
    if imported.new_prs {
        notify_new_prs(lang);
    }
    if imported.imported_progression {
        notify_progression_available(lang);
    }

    if let Some(any_progression) = imported.any_progression {
        events.emit(BackendEvent::ProgressionsAvailable(any_progression));
    }
}

/// Congratulates the user on a new personal record.
pub(in crate::logic::activities) fn notify_new_prs(lang: Languages) {
    show_notification(NotificationDefinition {
        title: translate("new_record", lang),
        body: translate("contratulations_pr", lang),
        kind: NotificationKind::Temporal,
    });
}

/// Tells the user some exercise is ready to progress.
pub(in crate::logic::activities) fn notify_progression_available(lang: Languages) {
    show_notification(NotificationDefinition {
        title: translate("progression_available", lang),
        body: translate("progression_available_msg", lang),
        kind: NotificationKind::Temporal,
    });
}

/// Shows the notification of an activity file that could not be read or parsed.
fn notify_parse_error(name: &str, error: &str, lang: Languages) {
    let title = Path::new(name)
        .file_name()
        .map_or_else(|| name.to_string(), |n| n.to_string_lossy().into_owned());
    show_notification(NotificationDefinition {
        title,
        body: translate_and_replace("error_parsing_activity", &[error], lang),
        kind: NotificationKind::Persistant,
    });
}

/// Outcome of `import_file_list`.
#[derive(Default)]
pub(crate) struct ImportedFiles {
    /// Dates of the activities inserted.
    pub activities: Vec<u32>,
    /// Files that could not be parsed, with the parse error.
    pub failed: Vec<(FitFile, String)>,
    /// Whether any of the inserted activities holds a new personal record.
    pub new_prs: bool,
    /// Whether some exercise of the inserted activities has a progression suggestion pending.
    pub imported_progression: bool,
    /// Whether any exercise has a progression suggestion pending, checked only when activities
    /// were inserted (otherwise it cannot have changed).
    pub any_progression: Option<bool>,
    /// Workouts whose percentage loads changed with the inserted sets.
    pub refreshed_workouts: Vec<u16>,
}

/// Outcome of `parse_files`.
#[derive(Default)]
pub(crate) struct ParsedFiles {
    /// Activities parsed, oldest first.
    pub activities: Vec<Activity>,
    /// Files that could not be parsed, with the parse error.
    pub failed: Vec<(FitFile, String)>,
}

/// Parses a batch of `.FIT` files in parallel. Blocking work, to run outside the async runtime and
/// before opening the transaction that imports them.
pub(crate) fn parse_files(files: &[FitFile]) -> ParsedFiles {
    let (mut activities, failed): (Vec<_>, Vec<_>) = files.par_iter().partition_map(|file| {
        info!("Parsing activity file {}", file.name);
        match Activity::try_from(FitParser::from_fit_file(file.clone())) {
            Ok(mut activity) => {
                // Known up front when downloaded from Connect, saving the later lookup
                if let Some(id) = file.activity_id {
                    activity.connect_id = Some(id.to_string());
                }
                Either::Left(activity)
            }
            Err(e) => {
                error!("Error parsing activity file {}: {}", file.name, e);
                Either::Right((file.clone(), e.to_string()))
            }
        }
    });

    activities.sort_by_key(|s| s.date);

    ParsedFiles { activities, failed }
}

/// Inserts each new activity parsed (plus its exercises/series/heart rate/GPS/speeds) in the given
/// transaction, then refreshes personal records.
pub(crate) async fn import_file_list(
    tx: &Transaction<'_>,
    parsed: ParsedFiles,
) -> Result<ImportedFiles, DatabaseError> {
    let ParsedFiles {
        mut activities,
        failed,
    } = parsed;
    let mut success = Vec::new();
    let mut handled_exercises: HashSet<(u16, u16)> = HashSet::new();

    let dates: Vec<u32> = activities.iter().map(|s| s.date).collect();
    let mut existing_activities = Activity::get_existing_dates(tx, &dates).await?;
    let known_devices = if activities.is_empty() {
        HashSet::new()
    } else {
        Device::get_serials(tx).await?
    };
    let mut handled_workouts = HashMap::new();

    // Newest first, as `insert_full` stores each workout only with its latest activity
    for activity in activities.iter_mut().rev() {
        let formatted_time = match Local.timestamp_opt(activity.date as i64, 0) {
            LocalResult::Single(date) => date.format("%H:%M:%S %d/%m/%Y").to_string(),
            _ => String::new(),
        };
        info!(
            "Importing activity {} - {}...",
            activity.name, formatted_time
        );
        if existing_activities.contains(&activity.date) {
            warn!("Activity with date {} already exists", activity.date);
            continue;
        }

        // The serial recorded in the file may belong to a device that isn't registered
        activity.device = activity.device.take().filter(|s| known_devices.contains(s));
        Activity::insert_full(tx, activity, &mut handled_workouts).await?;

        handled_exercises.extend(
            activity
                .steps
                .iter()
                .filter_map(|serie| serie.ex_cat.zip(serie.ex_id)),
        );
        existing_activities.insert(activity.date);
        success.push(activity.date);
    }
    // Back to chronological order
    success.reverse();

    let (new_prs, imported_progression, any_progression, refreshed_workouts) = if success.is_empty()
    {
        (false, false, None, Vec::new())
    } else {
        let new_prs = ActivityStep::update_prs(tx, &handled_exercises, &success).await?;
        let refreshed =
            WorkoutStep::refresh_percent_loads(tx, Some(&handled_exercises), now()).await?;

        // Evaluated once: tells both whether the imported exercises progress and whether any
        // does
        let suggested = progression_suggestions(tx).await?;
        (
            new_prs,
            handled_exercises.iter().any(|e| suggested.contains(e)),
            Some(!suggested.is_empty()),
            refreshed,
        )
    };

    Ok(ImportedFiles {
        activities: success,
        failed,
        new_prs,
        imported_progression,
        any_progression,
        refreshed_workouts,
    })
}

/// Imports the activities parsed from the `.fit` files fetched from `device` and, with `sync_mark`,
/// moves its last activity sync to it.
pub(super) async fn import_device_files(
    tx: &mut Transaction<'_>,
    parsed: ParsedFiles,
    device: &mut Device,
    sync_mark: Option<u32>,
) -> Result<ImportedFiles, DatabaseError> {
    let fetched = parsed.activities.len() + parsed.failed.len();
    let res = if fetched > 0 {
        info!("Fetched {} activity files", fetched);
        import_file_list(tx, parsed).await?
    } else {
        ImportedFiles::default()
    };

    if let Some(mark) = sync_mark {
        device.last_activity_sync = Some(mark);
        device.update_by_id_in(tx).await?;
    }

    Ok(res)
}
