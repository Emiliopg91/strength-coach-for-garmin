use std::path::PathBuf;

use chrono::{Local, TimeZone};
use rusqlite_orm::{database::DatabasePool, rusqlite::Transaction};

use crate::utils::weight::from_kg;
use crate::{
    ConnectLock,
    dao::{
        activity::Activity, activity_data::ActivityData, activity_step::ActivityStep,
        settings::WeightUnit, workout_step::WorkoutStep,
    },
    dto::activities::ActivityStepsUpdate,
    fit::writer::FitWriter,
    logic::exercises::helpers::progressions::{
        progression_suggestions, progression_suggestions_among,
    },
    utils::{time::now, translations::Languages},
};

/// Looks up in Connect the activity at `timestamp` and stores its id, if a user is logged in.
pub(in crate::logic::activities) async fn retrieve_activity_connect_id(
    pool: &DatabasePool,
    connect: &ConnectLock,
    timestamp: u32,
) {
    let mut connect = connect.lock().await;
    if connect.logged_user().is_some()
        && let Ok(Some(id)) = connect.find_connect_activity(timestamp).await
    {
        let _ = pool
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
pub(crate) async fn write_activity_fit(
    pool: &DatabasePool,
    timestamp: u32,
    path: PathBuf,
    lang: Languages,
    weight_unit: WeightUnit,
) -> Result<(), String> {
    let activity = pool
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
pub(in crate::logic::activities) async fn apply_activity_changes(
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
            serie.weight.map(from_kg),
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
