use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{Datelike, Days, Local, Months, TimeZone, offset::LocalResult};
use rayon::{iter::Either, prelude::*};
use rusqlite_orm::{
    database::DatabasePool,
    errors::DatabaseError,
    rusqlite::{Connection, Transaction},
    types::value::Value,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_log::log::{error, info, warn};
use tokio::sync::Mutex;

use crate::{
    ConnectLock, SettingsLock,
    dao::{
        activity::Activity, activity_data::ActivityData, activity_lap::ActivityLap,
        activity_step::ActivityStep, device::Device, exercise::Exercise, settings::WeightUnit,
        workout_step::WorkoutStep,
    },
    dto::{
        activities::{ActivityDetails, ActivityListItem, ActivityLocation, ActivityStepsUpdate},
        notifications::{NotificationDefinition, NotificationKind},
    },
    fit::{FitFile, parser::FitParser, writer::FitWriter},
    logic::{
        connect::helpers::sync_workouts_connect,
        devices::helpers::push_pending_to_usb,
        exercises::helpers::{progression_suggestions, progression_suggestions_among},
        language,
        notifications::show_notification,
        pool, report_error, store_pending_files,
    },
    utils::{
        constants, http,
        time::now,
        translations::{Languages, translate, translate_and_replace},
    },
};

/// Sends again, in the background, the `workouts` whose percentage loads changed with the
/// estimated 1RM: to Connect and to the connected USB devices. In the background because the
/// callers may hold the Connect or device lock.
pub(crate) fn spawn_percent_load_sync<R: Runtime>(app: &AppHandle<R>, workouts: Vec<u16>) {
    if workouts.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        info!("Percentage loads changed in {} workouts", workouts.len());
        let connect = app.state::<ConnectLock>();
        let settings = app.state::<SettingsLock>();
        sync_workouts_connect(&app, &connect, &settings, &workouts).await;
        push_pending_to_usb(&app).await;
    });
}

pub(super) async fn retrieve_activity_connect_id<R: Runtime>(app: &AppHandle<R>, timestamp: u32) {
    let connect = app.state::<ConnectLock>();
    let mut connect = connect.lock().await;
    if connect.logged_user().is_some()
        && let Ok(Some(id)) = connect.find_connect_activity(timestamp).await
    {
        let _ = pool(app)
            .run_in_connection(async move |conn| {
                Ok(Activity::set_connect_id(conn, timestamp, id).await?)
            })
            .await;
    }
}

/// File name for an exported activity: its local date followed by its name.
pub(crate) fn activity_file_name(timestamp: u32, name: &str) -> String {
    let date = Local
        .timestamp_opt(timestamp as i64, 0)
        .unwrap()
        .format("%Y-%m-%d");
    let name = name.trim().replace('/', "-");

    if name.is_empty() {
        format!("{date}.fit")
    } else {
        format!("{date} {name}.fit")
    }
}

/// Encodes the stored activity as a FIT file at `path`.
pub(crate) async fn write_activity_fit<R: Runtime>(
    app: &AppHandle<R>,
    timestamp: u32,
    path: PathBuf,
    lang: Languages,
    weight_unit: WeightUnit,
) -> Result<(), String> {
    let activity = pool(app)
        .run_in_connection(async move |conn| Ok(Activity::get_full(conn, timestamp).await?))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Activity {timestamp} not found"))?;

    // Encoding and writing the file is blocking work too
    tokio::task::spawn_blocking(move || {
        FitWriter::from(&activity)
            .write(path, lang, weight_unit)
            .map_err(|e| e.to_string())
    })
    .await
    .expect("FIT writing task panicked")
}

pub(crate) async fn import_file_paths<R: Runtime>(
    app: &AppHandle<R>,
    files: Vec<PathBuf>,
) -> Result<usize, String> {
    let lang = language(app);
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

    let res = pool(app)
        .run_in_transaction(async move |tx| Ok(import_file_list(tx, parsed).await?))
        .await
        .map_err(|e| e.to_string());

    finish_activity_import(
        app,
        res.map(|imported| {
            notify_import(app, &imported, lang);
            spawn_percent_load_sync(app, imported.refreshed_workouts);
            imported.activities
        }),
    )
}

/// Imports as activities of `device` the `.fit` files fetched from it. Those that fail to parse, or
/// all of them if the transaction is rolled back, are stored as pending. With `sync_mark`, the
/// device's last activity sync is moved to it in the same transaction.
pub(crate) async fn import_pending_activities<R: Runtime>(
    app: &AppHandle<R>,
    files: Vec<FitFile>,
    device: &mut Device,
    lang: Languages,
    sync_mark: Option<u32>,
) -> Result<Vec<u32>, DatabaseError> {
    // Parsed before taking a pooled connection. The files are kept to store them as pending if the
    // transaction fails; their clones share the contents
    let parsed = {
        let files = files.clone();
        tokio::task::spawn_blocking(move || parse_files(&files))
            .await
            .expect("FIT parsing task panicked")
    };
    let mut tx_device = device.clone();
    let res = pool(app)
        .run_in_transaction(async move |tx| {
            let imported = import_device_files(tx, parsed, &mut tx_device, sync_mark).await?;
            Ok((imported, tx_device))
        })
        .await;

    let pending_dir = &constants::PENDING_DOWNLOAD_ACTIVITIES_FOLDER;
    match res {
        Ok((imported, updated_device)) => {
            *device = updated_device;
            notify_import(app, &imported, lang);
            spawn_percent_load_sync(app, imported.refreshed_workouts);
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
            Ok(imported.activities)
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

/// Ends an import of activities: looks up the place names of the imported ones in the background
/// and returns how many there are, or notifies the error.
pub(crate) fn finish_activity_import<R: Runtime, E: std::fmt::Display>(
    app: &AppHandle<R>,
    res: Result<Vec<u32>, E>,
) -> Result<usize, String> {
    match res {
        Ok(activities) => {
            if !activities.is_empty() {
                spawn_geolocation_update(app);
            }
            Ok(activities.len())
        }
        Err(e) => Err(report_error(
            e,
            &app.state::<SettingsLock>(),
            "error_import_activities",
            "Error importing activities",
        )),
    }
}

/// Looks up the place names of the activities pending geolocation in the background.
pub(crate) fn spawn_geolocation_update<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { update_pending_geolocation(&app).await });
}

/// Tells the user, once the import is committed, how it went: files that failed to parse,
/// activities imported, new records and progressions. Also lets the frontend know whether any
/// progression is pending.
fn notify_import<R: Runtime>(app: &AppHandle<R>, imported: &ImportedFiles, lang: Languages) {
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
        // Typed binding: the TypeScript generator reads the payload type from it
        let payload: bool = any_progression;
        let _ = app.emit("progressions_available", payload);
    }
}

/// Congratulates the user on a new personal record.
pub(super) fn notify_new_prs(lang: Languages) {
    show_notification(NotificationDefinition {
        title: translate("new_record", lang),
        body: translate("contratulations_pr", lang),
        kind: NotificationKind::Temporal,
    });
}

/// Tells the user some exercise is ready to progress.
pub(super) fn notify_progression_available(lang: Languages) {
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

/// Serializes the geolocation runs and holds the activities that cannot be named after a place (no
/// valid position, or none Nominatim knows), skipped until the app restarts instead of being
/// loaded and queried again on every import.
static GEOLOCATION_SKIPPED: Mutex<BTreeSet<u32>> = Mutex::const_new(BTreeSet::new());

/// Names the unnamed activities recorded with GPS after the place where they started: the one
/// Garmin Connect gives their activity when logged in, otherwise the one Nominatim finds at their
/// first position.
pub(crate) async fn update_pending_geolocation<R: Runtime>(app: &AppHandle<R>) {
    let mut skipped = GEOLOCATION_SKIPPED.lock().await;
    info!("Looking for activities pending geolocation...");
    let mut pending: Vec<u32> = match pool(app)
        .run_in_connection(async |conn| Ok(ActivityData::get_pending_geolocation(conn).await?))
        .await
    {
        Ok(pending) => pending
            .into_iter()
            .filter(|activity| !skipped.contains(activity))
            .collect(),
        Err(e) => {
            error!("Error while looking for pending activities: {}", e);
            return;
        }
    };
    if pending.is_empty() {
        info!("Found no pending activities");
        return;
    }
    info!("Found {} pending activities", pending.len());

    let from_connect = connect_locations(app, &pending).await;
    pending.retain(|activity| !from_connect.contains_key(activity));
    name_after_locations(app, from_connect.into_iter().collect()).await;
    if pending.is_empty() {
        return;
    }

    let pending = match pool(app)
        .run_in_connection(async move |conn| {
            Ok(ActivityData::get_coordinates(conn, &pending).await?)
        })
        .await
    {
        Ok(pending) => pending,
        Err(e) => {
            error!("Error while looking for pending activities: {}", e);
            return;
        }
    };

    info!("Geocoding {} pending activities", pending.len());
    let mut geocoded = Vec::new();
    for data in pending {
        let activity = data.activity;
        // The blob may hold only invalid positions
        let Some((lat, lon)) = data
            .get_coordinates_degrees()
            .and_then(|coords| coords.into_iter().flatten().next())
        else {
            warn!("Activity @ {activity} has no valid position to geolocate");
            skipped.insert(activity);
            continue;
        };

        match geocode(lat, lon).await {
            Ok(Some(location)) => geocoded.push((activity, location)),
            Ok(None) => {
                skipped.insert(activity);
            }
            Err(e) => error!("Error geolocating activity @ {activity}: {e}"),
        }

        // Nominatim allows one request per second
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    name_after_locations(app, geocoded).await;
}

/// Place names Garmin Connect gives the activities of `activities`, found in a single search
/// instead of a geocoding query per activity. Empty when not logged in or on failure, so they are
/// geocoded instead.
async fn connect_locations<R: Runtime>(
    app: &AppHandle<R>,
    activities: &[u32],
) -> HashMap<u32, String> {
    let connect = app.state::<ConnectLock>();
    let mut connect = connect.lock().await;
    if connect.logged_user().is_none() {
        return HashMap::new();
    }
    connect
        .find_activity_locations(activities)
        .await
        .unwrap_or_else(|e| {
            error!("Error looking for the locations in Connect: {e}");
            HashMap::new()
        })
}

/// Names each activity of `locations` after its place in a single transaction, with one update
/// per distinct place, and once committed lets the frontend know.
async fn name_after_locations<R: Runtime>(app: &AppHandle<R>, locations: Vec<(u32, String)>) {
    if locations.is_empty() {
        return;
    }
    let mut by_location: HashMap<String, Vec<u32>> = HashMap::new();
    for (activity, location) in locations {
        by_location.entry(location).or_default().push(activity);
    }

    let res = pool(app)
        .run_in_transaction(async move |tx| {
            let mut updated = 0;
            for (location, activities) in &by_location {
                updated += Activity::set_name(tx, activities, location).await?;
            }
            Ok((by_location, updated))
        })
        .await;
    match res {
        Ok((by_location, updated)) => {
            info!(
                "Updated location for {updated} activities in {} places",
                by_location.len()
            );
            for (location, activities) in by_location {
                for activity in activities {
                    let payload: ActivityLocation = ActivityLocation {
                        activity,
                        location: location.clone(),
                    };
                    let _ = app.emit("activity_location_update", payload);
                }
            }
        }
        Err(e) => error!("Error while updating activity locations: {e}"),
    }
}

/// Name of the place at `lat`, `lon` from Nominatim: the most specific of village, town, city,
/// state and country it gives, `None` if it gives none.
async fn geocode(lat: f64, lon: f64) -> Result<Option<String>, String> {
    let url =
        format!("https://nominatim.openstreetmap.org/reverse?format=json&lat={lat}&lon={lon}");
    let response = http::client()
        .get(&url)
        .header(reqwest::header::USER_AGENT, "strength-coach-for-garmin")
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| format!("Error on geocode query: {e}"))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("Invalid status code: {}", response.status()));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Error parsing response: {e}"))?;
    let Some(address) = json.get("address") else {
        return Ok(None);
    };
    Ok(["village", "town", "city", "state", "country"]
        .iter()
        .find_map(|key| address.get(key))
        .map(|value| value.to_string().replace('"', "")))
}

/// Lists activities newest first (within `[from, to]` and of the sport given, if any), flagged
/// with whether they have sets and records.
pub(super) async fn fetch_activities(
    conn: &Connection,
    from: Option<i32>,
    to: Option<i32>,
    sport: Option<u8>,
    sub_sport: Option<u8>,
) -> rusqlite_orm::errors::Result<Vec<ActivityListItem>> {
    let activities = Activity::get_activities(
        conn,
        from.map(i64::from),
        to.map(i64::from),
        sport,
        sub_sport,
    )
    .await?;
    let set_flags = ActivityStep::get_set_flags(conn, from, to).await?;

    Ok(activities
        .into_iter()
        .map(|s| {
            let mut r = ActivityListItem::from(&s);
            if let Some(has_record) = set_flags.get(&s.date) {
                r.has_sets = true;
                r.has_record = *has_record;
            }
            r
        })
        .collect())
}

/// Details of the activity.
///
/// # Panics
/// If the activity doesn't exist.
pub(crate) async fn fetch_activity_details(
    conn: &Connection,
    timestamp: u32,
    with_add_data: bool,
    with_laps: bool,
) -> rusqlite_orm::errors::Result<ActivityDetails> {
    Ok(
        fetch_activities_details(conn, &[timestamp], with_add_data, with_laps)
            .await?
            .pop()
            .unwrap(),
    )
}

/// Details of every activity in `timestamps`, in that order, with a fixed number of queries
/// whatever the number of activities: each related table is read once for all of them.
///
/// # Panics
/// If any of the activities doesn't exist.
pub(crate) async fn fetch_activities_details(
    conn: &Connection,
    timestamps: &[u32],
    with_add_data: bool,
    with_laps: bool,
) -> rusqlite_orm::errors::Result<Vec<ActivityDetails>> {
    if timestamps.is_empty() {
        return Ok(Vec::new());
    }
    let dates: Vec<Value> = timestamps.iter().map(|ts| (*ts).into()).collect();

    let mut activities: HashMap<u32, Activity> = Activity::get_by_dates(conn, &dates)
        .await?
        .into_iter()
        .map(|s| (s.date, s))
        .collect();

    for step in ActivityStep::get_by_activities(conn, &dates).await? {
        if let Some(activity) = activities.get_mut(&step.activity) {
            activity.steps.push(step);
        }
    }

    let serials: HashSet<String> = activities
        .values()
        .filter_map(|s| s.device.clone())
        .collect();
    if !serials.is_empty() {
        let devices: HashMap<String, Device> = Device::get_by_serials(conn, &serials)
            .await?
            .into_iter()
            .map(|d| (d.serial.clone(), d))
            .collect();
        for activity in activities.values_mut() {
            activity.device_obj = activity
                .device
                .as_ref()
                .and_then(|serial| devices.get(serial).cloned());
        }
    }

    if with_add_data {
        for add_data in ActivityData::get_by_activities(conn, &dates).await? {
            if let Some(activity) = activities.get_mut(&add_data.activity) {
                activity.data = Some(add_data);
            }
        }
    }

    if with_laps {
        for lap in ActivityLap::get_by_activities(conn, &dates).await? {
            if let Some(activity) = activities.get_mut(&lap.activity) {
                activity.laps.push(lap);
            }
        }
    }

    let keys: HashSet<(u16, u16)> = activities
        .values()
        .flat_map(|s| &s.steps)
        .filter_map(|step| step.ex_cat.zip(step.ex_id))
        .collect();
    let mut exercises = Exercise::get_by_keys(conn, &keys).await?;
    Exercise::fill_muscles(conn, &mut exercises).await?;
    let exercises: HashMap<(u16, u16), Exercise> = exercises
        .into_iter()
        .map(|e| ((e.category, e.id), e))
        .collect();

    for activity in activities.values_mut() {
        activity.steps.sort_by_key(|s| s.idx);
        activity.laps.sort_by_key(|l| l.idx);
    }

    Ok(timestamps
        .iter()
        .map(|ts| {
            let activity = &activities[ts];
            // Only this activity's exercises: its muscles are computed over them
            let own_keys: HashSet<(u16, u16)> = activity
                .steps
                .iter()
                .filter_map(|step| step.ex_cat.zip(step.ex_id))
                .collect();
            let own: Vec<Exercise> = own_keys
                .iter()
                .filter_map(|key| exercises.get(key).cloned())
                .collect();
            ActivityDetails::from((
                activity,
                own.as_slice(),
                activity.steps.as_slice(),
                activity.laps.as_slice(),
            ))
        })
        .collect())
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

/// Outcome of `apply_activity_changes`.
pub(crate) struct AppliedActivityChanges {
    /// Whether the edited activity now holds a new personal record.
    pub new_prs: bool,
    /// Whether some exercise of the activity got a new progression suggestion.
    pub newly_suggested: bool,
    /// Whether any exercise has a progression suggestion pending.
    pub any_progression: bool,
    /// Workouts whose percentage loads changed with the edited sets.
    pub refreshed_workouts: Vec<u16>,
}

/// Stores the edited notes and sets of an activity and recomputes the records of the exercises
/// involved.
pub(super) async fn apply_activity_changes(
    tx: &Transaction<'_>,
    details: &ActivityStepsUpdate,
) -> rusqlite_orm::errors::Result<AppliedActivityChanges> {
    ActivityData::update_notes(tx, details.timestamp, details.notes.as_deref()).await?;

    let idxs: Vec<u8> = details.sets.iter().map(|s| s.idx).collect();
    let mut exercises = ActivityStep::get_step_exercises(tx, details.timestamp, &idxs).await?;
    exercises.extend(details.sets.iter().filter_map(|s| s.ex_cat.zip(s.ex_id)));

    // Only tells whether a suggestion is new, so a failure just counts as none before
    let suggested_before = progression_suggestions_among(tx, &exercises)
        .await
        .unwrap_or_default();

    for serie in &details.sets {
        ActivityStep::update_set(
            tx,
            details.timestamp,
            serie.idx,
            (serie.ex_cat, serie.ex_id),
            serie.reps,
            serie.weight,
        )
        .await?;
    }

    let new_prs = ActivityStep::update_prs(tx, &exercises, &[details.timestamp]).await?;
    let refreshed_workouts =
        WorkoutStep::refresh_percent_loads(tx, Some(&exercises), now()).await?;

    // Evaluated once: tells both which exercises now progress and whether any does
    let suggested_after = progression_suggestions(tx).await?;
    let newly_suggested = exercises
        .iter()
        .any(|ex| !suggested_before.contains(ex) && suggested_after.contains(ex));
    Ok(AppliedActivityChanges {
        new_prs,
        newly_suggested,
        any_progression: !suggested_after.is_empty(),
        refreshed_workouts,
    })
}

/// Training load and records of every day of the last year, by month and day of month.
pub(crate) async fn heatmap_data(database: &DatabasePool) -> Result<Vec<Vec<(f32, u16)>>, String> {
    let today = Local::now().date_naive();
    let a_year_ago = today
        .checked_sub_months(Months::new(12))
        .and_then(|d| d.checked_add_days(Days::new(1)))
        .unwrap();
    let midnight = a_year_ago.and_hms_opt(0, 0, 0).unwrap();

    let ts = Local
        .from_local_datetime(&midnight)
        .earliest()
        .unwrap()
        .timestamp();

    database
        .run_in_connection(async move |conn| {
            let activities = Activity::get_activities(conn, Some(ts), None, None, None).await?;
            let pr_counts = ActivityStep::get_pr_counts_since(conn, ts).await?;

            let mut res = vec![vec![(0_f32, 0_u16); 31]; 12];
            for activity in activities {
                let dt = Local
                    .timestamp_opt(activity.date as i64, 0)
                    .single()
                    .unwrap();
                let cell = &mut res[dt.month0() as usize][dt.day0() as usize];
                cell.0 += activity.training_load;
                cell.1 += pr_counts.get(&activity.date).copied().unwrap_or(0);
            }

            Ok(res)
        })
        .await
        .map_err(|e| e.to_string())
}

/// Merges the activity started at `from_date` into the one at `to_date`: adds up their stats and
/// appends its laps, sets and additional data.
#[allow(dead_code)]
pub(super) async fn merge_activities(
    database: &DatabasePool,
    to_date: u32,
    from_date: u32,
) -> Result<(), DatabaseError> {
    let (to, from) = database
        .run_in_connection(async move |conn| {
            let to = Activity::get_full(conn, to_date).await?;
            let from = Activity::get_full(conn, from_date).await?;
            Ok((to, from))
        })
        .await
        .expect("Error on database query");
    let mut to = to.unwrap_or_else(|| panic!("Could not find activity with timestamp {}", to_date));
    let mut from =
        from.unwrap_or_else(|| panic!("Could not find activity with timestamp {}", from_date));
    // Both activities are already stored, so their workout must be left as it is
    to.workout_obj = None;

    to.total_elapsed_time += from.total_elapsed_time;
    to.active_time += from.active_time;
    to.training_load += from.training_load;
    to.active_calories += from.active_calories;

    match &mut to.data {
        Some(add_data) => {
            if let Some(add_data2) = &mut from.data {
                fn merge_vec<T>(to: &mut Option<Vec<T>>, from: &mut Option<Vec<T>>) {
                    if let Some(v2) = from.take() {
                        match to {
                            Some(v) => v.extend(v2),
                            None => *to = Some(v2),
                        }
                    }
                }

                merge_vec(&mut add_data.heart_rates, &mut add_data2.heart_rates);
                merge_vec(&mut add_data.coordinates, &mut add_data2.coordinates);
                merge_vec(&mut add_data.speeds, &mut add_data2.speeds);

                add_data.distance = match (add_data.distance, add_data2.distance) {
                    (Some(d1), Some(d2)) => Some(d1 + d2),
                    (d1, d2) => d1.or(d2),
                };

                add_data.max_hr = match (add_data.max_hr, add_data2.max_hr) {
                    (Some(hr1), Some(hr2)) => Some(hr1.max(hr2)),
                    (hr1, hr2) => hr1.or(hr2),
                };

                if let Some(notes2) = add_data2.notes.take() {
                    match &mut add_data.notes {
                        Some(notes) => {
                            notes.push(' ');
                            notes.push_str(&notes2);
                        }
                        None => add_data.notes = Some(notes2),
                    }
                }
            }
        }
        None => {
            to.data = from.data.take().map(|mut add_data| {
                add_data.activity = to.date;
                add_data
            })
        }
    }

    for mut lap in std::mem::take(&mut from.laps) {
        lap.activity = to.date;
        lap.idx = to.laps.len() as u16;
        to.laps.push(lap);
    }

    for mut set in std::mem::take(&mut from.steps) {
        set.activity = to.date;
        set.idx = to.steps.len() as u8;
        to.steps.push(set);
    }

    database
        .run_in_transaction(async move |tx| {
            to.delete_by_id_in(tx).await?;
            from.delete_by_id_in(tx).await?;

            Activity::insert_full(tx, &mut to, &mut HashMap::new()).await?;

            let handled_exercises = to
                .steps
                .iter()
                .filter_map(|s| s.ex_cat.zip(s.ex_id))
                .collect();
            ActivityStep::update_prs(tx, &handled_exercises, &[to.date]).await?;
            WorkoutStep::refresh_percent_loads(tx, Some(&handled_exercises), now()).await?;

            Ok(())
        })
        .await
}
