use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Days, Local, Months, TimeZone};
use rusqlite_orm::{database::DatabasePool, rusqlite::Connection, types::value::Value};

use crate::{
    dao::{
        activity::Activity, activity_data::ActivityData, activity_lap::ActivityLap,
        activity_step::ActivityStep, device::Device, exercise::Exercise,
    },
    dto::activities::{ActivityDetails, ActivityListItem, MuscleSets},
};

/// Lists activities newest first (within `[from, to]` and of the sport given, if any), flagged
/// with whether they have sets and records.
pub(in crate::logic::activities) async fn fetch_activities(
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

/// Exercise sets performed from `since` on, added up by the muscles they worked. A muscle both
/// primary and secondary in an exercise counts as primary. Sorted by muscle name.
pub(crate) async fn muscle_sets(
    database: &DatabasePool,
    since: u32,
) -> Result<BTreeMap<String, MuscleSets>, String> {
    database
        .run_in_connection(async move |conn| {
            let counts = ActivityStep::get_exercise_set_counts_since(conn, since).await?;
            let mut exercises = Exercise::get_by_keys(conn, counts.keys()).await?;
            Exercise::fill_muscles(conn, &mut exercises).await?;

            let mut by_muscle: BTreeMap<String, MuscleSets> = BTreeMap::new();
            for exercise in &exercises {
                let sets = counts[&(exercise.category, exercise.id)];
                let primary: HashSet<&str> = exercise
                    .muscles
                    .iter()
                    .filter(|m| m.is_primary)
                    .map(|m| m.muscle.as_str())
                    .collect();
                for muscle in &exercise.muscles {
                    let is_primary = primary.contains(muscle.muscle.as_str());
                    if !muscle.is_primary && is_primary {
                        continue;
                    }
                    let entry = by_muscle.entry(muscle.muscle.clone()).or_default();
                    if is_primary {
                        entry.primary += sets;
                    } else {
                        entry.secondary += sets;
                    }
                }
            }

            Ok(by_muscle)
        })
        .await
        .map_err(|e| e.to_string())
}
