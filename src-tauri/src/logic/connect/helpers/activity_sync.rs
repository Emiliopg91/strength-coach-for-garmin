use std::collections::{BTreeSet, HashMap};

use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::error;

use crate::{
    ConnectLock,
    connect::{ConnectClient, client::ExerciseCandidate},
    dao::{activity::Activity, exercise::Exercise},
    dto::{activities::ActivityStepDto, connect::ConnectUpdateSets},
};

/// Mirrors the user edits of an activity's sets on its Connect activity, if the activity is linked to
/// one and a user is logged in. Runs after the local save, so failures are only logged.
pub(crate) async fn sync_activity_sets_connect(
    pool: &DatabasePool,
    connect: &ConnectLock,
    timestamp: u32,
    sets: Vec<ActivityStepDto>,
) {
    let res = pool
        .run_in_connection(async move |conn| {
            let Some(connect_id) = Activity::get_connect_id(conn, timestamp).await? else {
                return Ok(None);
            };

            let keys: BTreeSet<(u16, u16)> =
                sets.iter().filter_map(|s| s.ex_cat.zip(s.ex_id)).collect();
            let names: HashMap<(u16, u16), (String, String)> =
                Exercise::get_by_keys_with_category(conn, &keys)
                    .await?
                    .into_iter()
                    .filter_map(|(key, exercise)| {
                        Some((key, (exercise.exercise_category?.name, exercise.name)))
                    })
                    .collect();

            let updates: Vec<ConnectUpdateSets> = sets
                .iter()
                .filter_map(|set| {
                    let (exercise_category, exercise_id) =
                        names.get(&set.ex_cat.zip(set.ex_id)?)?.clone();
                    Some(ConnectUpdateSets {
                        idx: set.idx as u16,
                        reps: set.reps,
                        weight: set.weight,
                        exercise_category,
                        exercise_id,
                    })
                })
                .collect();
            Ok(Some((connect_id, updates)))
        })
        .await;

    let (connect_id, updates) = match res {
        Ok(Some((connect_id, updates))) if !updates.is_empty() => (connect_id, updates),
        Ok(_) => return,
        Err(e) => {
            error!(
                "Error loading sets of activity {} for Connect: {}",
                timestamp, e
            );
            return;
        }
    };
    let Ok(id) = connect_id.parse::<u64>() else {
        error!("Invalid Connect activity id '{}'", connect_id);
        return;
    };

    let mut connect = connect.lock().await;
    if connect.logged_user().is_none() {
        return;
    }
    update_activity_sets(&mut connect, id, updates).await;
}

/// Applies `sets` over the current sets of the Connect activity `id` and sends them back.
/// Sets are matched by their FIT message index, which is the local set index.
pub(super) async fn update_activity_sets(
    connect: &mut ConnectClient,
    id: u64,
    sets: Vec<ConnectUpdateSets>,
) {
    let Ok(mut original_sets) = connect.get_activity_sets(id).await else {
        return;
    };

    for set in &sets {
        let Some(target) = original_sets
            .exercise_sets
            .iter_mut()
            .find(|s| s.message_index == Some(set.idx))
        else {
            error!("Set {} not found in Connect activity {}", set.idx, id);
            continue;
        };
        // Stored in kg, Connect expects grams
        target.weight = set.weight.map(|v| (v as f64 * 1000.0).round());
        target.repetition_count = set.reps;
        target.exercises = vec![ExerciseCandidate {
            category: set.exercise_category.clone(),
            // Generic exercises go without name, just with their category
            name: Some(set.exercise_id.clone()).filter(|name| !name.is_empty()),
            probability: 100_f64,
        }];
    }

    let _ = connect.update_activity_sets(&original_sets).await;
}
