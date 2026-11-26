use std::collections::{HashMap, HashSet};

use rusqlite_orm::rusqlite::Connection;

use crate::{
    dao::{activity_step::ActivityStep, exercise::Exercise, workout_step::WorkoutStep},
    dto::exercises::ProgressionSuggestion,
};

/// Pending suggestion for the exercise, `None` if it doesn't exist, has progression disabled or
/// has nothing to suggest.
pub(crate) async fn fetch_progression_suggestion(
    conn: &Connection,
    category: u16,
    id: u16,
) -> rusqlite_orm::errors::Result<Option<ProgressionSuggestion>> {
    let Some(exercise) = Exercise::get(conn, category, id).await? else {
        return Ok(None);
    };
    Ok(progression_suggestions_for(conn, [&exercise])
        .await?
        .remove(&(exercise.category, exercise.id)))
}

/// Exercises with progression enabled that currently have a suggestion.
pub(crate) async fn progression_suggestions(
    conn: &Connection,
) -> rusqlite_orm::errors::Result<HashSet<(u16, u16)>> {
    let candidates = Exercise::get_progression_candidates(conn).await?;
    Ok(progression_suggestions_for(conn, &candidates)
        .await?
        .into_keys()
        .collect())
}

/// Which of the exercises in `keys` currently have a suggestion, in two queries plus one to load
/// them, whatever their number.
pub(crate) async fn progression_suggestions_among(
    conn: &Connection,
    keys: &HashSet<(u16, u16)>,
) -> rusqlite_orm::errors::Result<HashSet<(u16, u16)>> {
    let exercises = Exercise::get_by_keys(conn, keys).await?;
    Ok(progression_suggestions_for(conn, &exercises)
        .await?
        .into_keys()
        .collect())
}

/// Dismisses the progression suggestion of the exercise, keeping it hidden until an activity newer
/// than its latest one.
pub(crate) async fn dismiss_progression(
    conn: &Connection,
    category: u16,
    id: u16,
) -> rusqlite_orm::errors::Result<()> {
    let keys: HashSet<(u16, u16)> = [(category, id)].into();
    let latest = ActivityStep::get_latest_exercise_sets(conn, &keys)
        .await?
        .first()
        .map(|&(_, activity, _, _)| activity);
    Exercise::set_dismissed_progression(conn, category, id, latest).await
}

/// Whether any exercise with progression enabled currently has a suggestion pending.
pub(in crate::logic::exercises) async fn any_progression_suggestion(
    conn: &Connection,
) -> rusqlite_orm::errors::Result<bool> {
    progression_suggestions(conn)
        .await
        .map(|suggested| !suggested.is_empty())
}

/// A performed set, as weight (0 if unloaded) and reps.
/// `(weight in tenths of a kg, reps)`
type Set = (u16, u16);

/// Pending suggestions for the already loaded `exercises`, by key. Exercises with progression
/// disabled, nothing to suggest or the suggestion dismissed are left out. Takes two queries
/// whatever the number of exercises.
pub(super) async fn progression_suggestions_for<'a>(
    conn: &Connection,
    exercises: impl IntoIterator<Item = &'a Exercise>,
) -> rusqlite_orm::errors::Result<HashMap<(u16, u16), ProgressionSuggestion>> {
    let exercises: HashMap<(u16, u16), &Exercise> = exercises
        .into_iter()
        .filter(|e| e.progression)
        .map(|e| ((e.category, e.id), e))
        .collect();
    if exercises.is_empty() {
        return Ok(HashMap::new());
    }

    // Latest activity of each exercise and its sets
    let keys: HashSet<(u16, u16)> = exercises.keys().copied().collect();
    let mut latest: HashMap<(u16, u16), (u32, Vec<Set>)> = HashMap::new();
    for (key, activity, weight, reps) in ActivityStep::get_latest_exercise_sets(conn, &keys).await?
    {
        latest
            .entry(key)
            .or_insert_with(|| (activity, Vec::new()))
            .1
            .push((weight, reps));
    }

    // A suggestion dismissed on the latest activity stays hidden until a newer one
    let candidates: HashMap<(u16, u16), ProgressionSuggestion> = latest
        .iter()
        .filter_map(|(key, (activity, sets))| {
            let exercise = exercises.get(key)?;
            if exercise.dismissed_progression == Some(*activity) {
                return None;
            }
            Some((*key, suggestion_from_sets(exercise, sets)?))
        })
        .collect();
    if candidates.is_empty() {
        return Ok(HashMap::new());
    }

    // Steps of disabled workouts don't count: they alone would suggest what is already programmed
    let keys: HashSet<(u16, u16)> = candidates.keys().copied().collect();
    let mut pending = HashSet::new();
    for (key, weight, reps) in WorkoutStep::get_programmed_loads(conn, &keys).await? {
        let Some(suggestion) = candidates.get(&key) else {
            continue;
        };
        // Steps programmed below the suggestion, those already at or above it never lowered
        if weight < suggestion.weight || (weight == suggestion.weight && reps < suggestion.reps) {
            pending.insert(key);
        }
    }

    Ok(candidates
        .into_iter()
        .filter(|(key, _)| pending.contains(key))
        .collect())
}

/// Load to suggest for `exercise` given the `sets` of its latest activity, `None` if they don't
/// share a weight or don't all reach its rep range.
fn suggestion_from_sets(exercise: &Exercise, sets: &[Set]) -> Option<ProgressionSuggestion> {
    let &(weight, _) = sets.first()?;
    if sets.iter().any(|&(w, _)| w != weight) {
        return None;
    }

    let range: Vec<_> = exercise
        .reps
        .split('-')
        .filter_map(|s| s.parse::<u16>().ok())
        .collect();
    if range.is_empty() {
        return None;
    }

    // Index of the highest range step reached by each set; every set must reach the range and the weakest one rules
    let bracket = sets
        .iter()
        .map(|&(_, reps)| {
            let reps = if exercise.unilateral { reps / 2 } else { reps };
            range.iter().rposition(|&r| reps >= r)
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .min()?;

    let (weight, reps) = match bracket {
        i if i == range.len() - 1 => (weight.saturating_add(exercise.weight), range[0]),
        i => (weight, range[i + 1]),
    };
    Some(ProgressionSuggestion {
        weight,
        reps: if exercise.unilateral { reps * 2 } else { reps },
    })
}
