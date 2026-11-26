use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite_orm::rusqlite::Connection;

use crate::utils::weight::to_kg;
use crate::{
    dao::{
        activity::Activity,
        activity_step::ActivityStep,
        body_metric::BodyMetric,
        exercise::{Exercise, GENERIC_EXERCISE_ID},
        exercise_category::ExerciseCategory,
        workout_step::WorkoutStep,
    },
    dto::{
        activities::ActivityStepDto,
        exercises::{ExerciseDetails, ExerciseListItem},
    },
};

use crate::logic::exercises::helpers::progressions::progression_suggestions_for;

/// Loads the personal record and full per-activity set history for one exercise.
pub(crate) async fn fetch_exercise_details(
    conn: &Connection,
    category: u16,
    id: u16,
    from_to: Option<(u32, u32)>,
) -> rusqlite_orm::errors::Result<ExerciseDetails> {
    let exercise = Exercise::get_with_muscles(conn, category, id).await?;
    let series = ActivityStep::get_by_exercise(conn, category, id).await?;
    if !series.iter().any(|s| s.pr) {
        return Ok(build_exercise_details(
            &exercise,
            series,
            &HashMap::new(),
            &[],
        ));
    }

    // The activities with this exercise are exactly those of its sets
    let activity_dates: HashSet<u32> = series.iter().map(|s| s.activity).collect();
    let workouts = Activity::get_names_by_dates(conn, &activity_dates, from_to).await?;
    let body_metrics = BodyMetric::get_by_range(conn, None).await?;
    Ok(build_exercise_details(
        &exercise,
        series,
        &workouts,
        &body_metrics,
    ))
}

/// Same as [`fetch_exercise_details`] for every exercise of `keys` (in their order, skipping the
/// ones not in the catalog), with a fixed number of queries however many there are.
pub(crate) async fn fetch_exercises_details(
    conn: &Connection,
    keys: &BTreeSet<(u16, u16)>,
    from_to: Option<(u32, u32)>,
) -> rusqlite_orm::errors::Result<Vec<ExerciseDetails>> {
    let mut exercises = Exercise::get_by_keys(conn, keys).await?;
    Exercise::fill_muscles(conn, &mut exercises).await?;
    let exercises: HashMap<(u16, u16), Exercise> = exercises
        .into_iter()
        .map(|e| ((e.category, e.id), e))
        .collect();

    let mut series = ActivityStep::get_by_exercises(conn, keys).await?;

    // Only exercises with a record list their activities
    let activity_dates: HashSet<u32> = series
        .values()
        .filter(|sets| sets.iter().any(|s| s.pr))
        .flatten()
        .map(|s| s.activity)
        .collect();
    let (workouts, body_metrics) = if activity_dates.is_empty() {
        (HashMap::new(), Vec::new())
    } else {
        (
            Activity::get_names_by_dates(conn, &activity_dates, from_to).await?,
            BodyMetric::get_by_range(conn, None).await?,
        )
    };

    Ok(keys
        .iter()
        .filter_map(|key| {
            let exercise = exercises.get(key)?;
            let sets = series.remove(key).unwrap_or_default();
            Some(build_exercise_details(
                exercise,
                sets,
                &workouts,
                &body_metrics,
            ))
        })
        .collect())
}

/// Details of `exercise` from its `series` (newest activity first), naming each activity after
/// `workouts` (activities not in it are skipped) and pairing it with the body weight of
/// `body_metrics` (newest first) measured before it. Without a record, only the exercise.
fn build_exercise_details(
    exercise: &Exercise,
    series: Vec<ActivityStep>,
    workouts: &HashMap<u32, String>,
    body_metrics: &[BodyMetric],
) -> ExerciseDetails {
    let mut res = ExerciseDetails::from(exercise);
    let Some(pr) = series.iter().find(|s| s.pr) else {
        return res;
    };
    res.reps = pr.reps.unwrap_or_default();
    res.weight = to_kg(pr.weight.unwrap_or_default());
    res.pr_date = pr.activity;

    let mut last_activity = None;
    let mut ex_str = String::new();
    for serie in series {
        // Activities outside the range aren't in `workouts`: their sets are skipped
        let Some(workout) = workouts.get(&serie.activity) else {
            continue;
        };
        if last_activity != Some(serie.activity) {
            ex_str = format!("{}\n{}", workout, serie.activity);
            res.workouts.push(ex_str.clone());
            last_activity = Some(serie.activity);
        }

        let entry = res.activities.entry(ex_str.clone()).or_default();
        entry.weight = body_metrics
            .iter()
            .find_map(|b| (b.date < serie.activity).then_some(to_kg(b.weight)));
        entry.series.push(ActivityStepDto::from(&serie));
    }
    res
}

/// Exercises that can be picked, by category. The generic exercise of a category is left out when
/// the category already has a real exercise with its same name (e.g. `SQUAT` within squats).
pub(crate) async fn exercises_catalog(
    conn: &Connection,
) -> rusqlite_orm::errors::Result<HashMap<u16, Vec<u16>>> {
    let category_names: HashMap<u16, String> = ExerciseCategory::get_all(conn)
        .await?
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    let exercises = Exercise::get_all(conn).await?;
    let named_as_category: HashSet<u16> = exercises
        .iter()
        .filter(|e| {
            e.id != GENERIC_EXERCISE_ID
                && category_names
                    .get(&e.category)
                    .is_some_and(|name| *name == e.name)
        })
        .map(|e| e.category)
        .collect();

    let mut catalog: HashMap<u16, Vec<u16>> = HashMap::new();
    for e in &exercises {
        if e.id != GENERIC_EXERCISE_ID || !named_as_category.contains(&e.category) {
            catalog.entry(e.category).or_default().push(e.id);
        }
    }

    Ok(catalog)
}

/// The exercises with a personal record, annotated with it, plus those planned in some workout
/// step that have none yet.
pub(in crate::logic::exercises) async fn exercise_list(
    conn: &Connection,
) -> rusqlite_orm::errors::Result<Vec<ExerciseListItem>> {
    // Only exercise sets can hold a record; the filter just keeps the unwraps below safe
    let prs: Vec<_> = ActivityStep::get_personal_records(conn)
        .await?
        .into_iter()
        .filter(|s| s.ex_cat.is_some() && s.ex_id.is_some())
        .collect();
    let pr_keys: HashSet<(u16, u16)> = prs
        .iter()
        .filter_map(|s| Some((s.ex_cat?, s.ex_id?)))
        .collect();

    let mut planned: Vec<(u16, u16)> = WorkoutStep::get_planned_exercises(conn)
        .await?
        .into_iter()
        .filter(|key| !pr_keys.contains(key))
        .collect();
    planned.sort();

    let exercises: HashMap<(u16, u16), Exercise> =
        Exercise::get_by_keys(conn, pr_keys.iter().chain(planned.iter()))
            .await?
            .into_iter()
            .map(|e| ((e.category, e.id), e))
            .collect();

    let suggested_progressions = progression_suggestions_for(
        conn,
        exercises
            .iter()
            .filter(|(key, _)| pr_keys.contains(key))
            .map(|(_, e)| e),
    )
    .await?;

    let mut items: Vec<_> = prs
        .iter()
        .map(|pr| {
            let key = (pr.ex_cat.unwrap(), pr.ex_id.unwrap());
            let exercise = exercises.get(&key);
            ExerciseListItem {
                category: key.0,
                id: key.1,
                reps: pr.reps.unwrap_or_default(),
                weight: to_kg(pr.weight.unwrap_or_default()),
                date: pr.activity,
                unilateral: exercise.is_some_and(|e| e.unilateral),
                progression: suggested_progressions.contains_key(&key),
                notes: exercise.and_then(|e| e.notes.clone()),
                used: true,
            }
        })
        .collect();

    // Planned exercises may not have been recorded yet, so they can lack a catalog row
    items.extend(planned.iter().map(|key| {
        exercises.get(key).map_or_else(
            || ExerciseListItem {
                category: key.0,
                id: key.1,
                reps: 0,
                weight: 0.0,
                date: 0,
                unilateral: false,
                progression: false,
                notes: None,
                used: false,
            },
            ExerciseListItem::from,
        )
    }));

    Ok(items)
}
