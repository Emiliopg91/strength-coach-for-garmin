use std::collections::HashMap;

use rusqlite_orm::rusqlite::Connection;
use tauri_plugin_log::log::info;

use crate::{
    dao::{
        activity::Activity, activity_step::ActivityStep, exercise::Exercise, workout::Workout,
        workout_step::WorkoutStep,
    },
    dto::{
        split_muscles,
        workouts::{WorkoutActivity, WorkoutDetails, WorkoutListItem},
    },
};

/// Activity stats of a workout, accumulated over its activities.
#[derive(Default)]
struct WorkoutStats {
    activities: u32,
    total_time: u32,
    latest_activity: Option<u32>,
    total_load: f32,
}

/// Every workout with its activity stats (count, average time and load, latest date), by name.
pub(in crate::logic::workouts) async fn workout_list(
    conn: &Connection,
) -> rusqlite_orm::errors::Result<Vec<WorkoutListItem>> {
    let workouts = Workout::get_all(conn).await?;
    let activities = Activity::get_workout_activities(conn).await?;

    let mut workout_stats: HashMap<u16, WorkoutStats> = workouts
        .iter()
        .map(|w| (w.id, WorkoutStats::default()))
        .collect();

    for s in &activities {
        let Some(stats) = s.workout.and_then(|id| workout_stats.get_mut(&id)) else {
            continue;
        };
        stats.activities += 1;
        stats.total_time += s.timer_time;
        stats.latest_activity.get_or_insert(s.date);
        stats.total_load += s.training_load;
    }

    let with_steps = WorkoutStep::get_workouts_with_steps(conn).await?;

    let mut res: Vec<_> = workouts
        .into_iter()
        .map(|workout| {
            let stats = workout_stats.remove(&workout.id).unwrap_or_default();
            WorkoutListItem {
                id: workout.id,
                has_steps: with_steps.contains(&workout.id),
                enabled: workout.enabled,
                activities: stats.activities,
                avg_time: stats.total_time.checked_div(stats.activities).unwrap_or(0),
                avg_load: if stats.activities > 0 {
                    stats.total_load / stats.activities as f32
                } else {
                    0.0
                },
                latest_activity: stats.latest_activity,
                name: workout.name,
            }
        })
        .collect();

    res.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(res)
}

/// The workout with its steps, worked muscles and every activity with its volume.
pub(in crate::logic::workouts) async fn workout_details(
    conn: &Connection,
    id: u16,
) -> rusqlite_orm::errors::Result<WorkoutDetails> {
    info!("Getting details for workout {}", id);

    let workout = Workout::get_with_steps(conn, id)
        .await?
        .unwrap_or_else(|| panic!("Workout {id} not found"));
    let activities = Activity::get_by_workout(conn, id).await?;

    let mut exercises = Exercise::get_by_workout(conn, id).await?;
    Exercise::fill_muscles(conn, &mut exercises).await?;
    let (primary_muscles, secondary_muscles) = split_muscles(&exercises);

    let latest = activities.first().map(|latest| latest.date);
    let mut count = 0_u32;
    let mut time = 0_u32;
    let mut volume = 0_f32;

    let volume_by_activity = ActivityStep::get_volume_by_activity(conn, id).await?;

    let mut activity_list = Vec::new();
    for activity in &activities {
        count += 1;
        time += activity.timer_time;

        let local_volume = volume_by_activity
            .get(&activity.date)
            .copied()
            .unwrap_or(0.0);
        volume += local_volume;
        let mut wk_sess = WorkoutActivity::from(activity);
        wk_sess.volume = local_volume;
        activity_list.push(wk_sess);
    }

    let details = WorkoutDetails {
        id,
        name: workout.name,
        avg_time: if !activities.is_empty() {
            time / (activities.len() as u32)
        } else {
            0
        },
        latest_activity: latest,
        avg_volume: volume / (activities.len() as f32),
        activity_count: count,
        activities: activity_list,
        enabled: workout.enabled,
        progression: workout.progression,
        notes: workout.notes,
        steps: workout.steps,
        primary_muscles,
        secondary_muscles,
    };

    Ok(details)
}
