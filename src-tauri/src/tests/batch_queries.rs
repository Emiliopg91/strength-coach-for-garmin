use rusqlite_orm::{dao::Repository, database::DatabasePool, types::where_clause::Where};

use std::collections::{BTreeSet, HashSet};

use crate::{
    dao::{
        activity_step::ActivityStep,
        body_metric::{BodyMetric, BodyMetricRepository},
        exercise::{self, Exercise, ExerciseRepository},
    },
    logic::{
        activities::helpers::{fetch_activities_details, fetch_activity_details},
        exercises::helpers::{fetch_exercise_details, fetch_exercises_details},
    },
    tests::common,
};

const BENCH: (u16, u16) = (0, 1);
const CABLE_ROW: (u16, u16) = (23, 20);
const PULL_UP: (u16, u16) = (21, 38);

async fn exercises(db: &DatabasePool, keys: &[(u16, u16)]) -> Vec<exercise::Exercise> {
    ExerciseRepository::select()
        .where_(Where::InMultiple(
            vec![
                exercise::entity::columns::CATEGORY,
                exercise::entity::columns::ID,
            ],
            keys.iter()
                .map(|(cat, id)| vec![(*cat).into(), (*id).into()])
                .collect(),
        ))
        .fetch(db)
        .await
        .unwrap()
}

/// Muscles of an exercise as sorted `(name, is_primary)` pairs, to compare regardless of order.
fn muscles(exercise: &exercise::Exercise) -> Vec<(String, bool)> {
    let mut muscles: Vec<_> = exercise
        .muscles
        .iter()
        .map(|m| (m.muscle.clone(), m.is_primary))
        .collect();
    muscles.sort();
    muscles
}

#[tokio::test]
async fn batched_muscles_match_the_relationship_fetcher() {
    let db = common::test_db().await;
    let mut batched = exercises(&db, &[BENCH, CABLE_ROW, PULL_UP]).await;
    let mut one_by_one = batched.clone();

    let (batched, one_by_one) = db
        .run_in_connection(async move |conn| {
            Exercise::fill_muscles(conn, &mut batched).await?;
            for exercise in &mut one_by_one {
                exercise.fetch_muscles_relationship_in(conn).await?;
            }
            Ok((batched, one_by_one))
        })
        .await
        .unwrap();

    assert!(batched.iter().any(|e| !e.muscles.is_empty()));
    for (batched, single) in batched.iter().zip(&one_by_one) {
        assert_eq!(muscles(batched), muscles(single));
    }
}

/// Details as JSON with their muscle lists sorted, as they come from a `HashSet` in any order.
fn normalized(details: &[crate::dto::activities::ActivityDetails]) -> serde_json::Value {
    let mut value = serde_json::to_value(details).unwrap();
    for detail in value.as_array_mut().unwrap() {
        for key in ["primary_muscles", "secondary_muscles"] {
            detail[key]
                .as_array_mut()
                .unwrap()
                .sort_by_key(|m| m.as_str().unwrap().to_string());
        }
    }
    value
}

#[tokio::test]
async fn batched_activity_details_match_single_ones_in_the_given_order() {
    let db = common::test_db().await;
    db.run_in_transaction(async move |tx| {
        for (date, exercise) in [(1, BENCH), (2, CABLE_ROW)] {
            let mut activity = common::activity(date);
            activity.steps = vec![
                common::set(date, 0, exercise, 10, 50.0),
                common::set(date, 1, exercise, 8, 55.0),
            ];
            activity.laps = vec![common::lap(date, 0, None)];
            activity.data = Some(common::additional_data(date));
            common::insert_activity(tx, activity).await;
        }
        Ok(())
    })
    .await
    .unwrap();

    let (batched, single) = db
        .run_in_connection(async move |conn| {
            let batched = fetch_activities_details(conn, &[2, 1], true, true).await?;
            let single = vec![
                fetch_activity_details(conn, 2, true, true).await?,
                fetch_activity_details(conn, 1, true, true).await?,
            ];
            Ok((batched, single))
        })
        .await
        .unwrap();

    let batched = normalized(&batched);
    assert_eq!(batched, normalized(&single));
    assert_eq!(batched[0]["timestamp"], 2);
    // Each activity only gets the muscles of its own exercises
    assert_ne!(batched[0]["primary_muscles"], batched[1]["primary_muscles"]);
}

#[tokio::test]
async fn no_activities_give_no_details() {
    let db = common::test_db().await;

    let details = db
        .run_in_connection(async move |conn| {
            Ok(fetch_activities_details(conn, &[], true, true).await?)
        })
        .await
        .unwrap();

    assert!(details.is_empty());
}

/// Exercise details as JSON with their muscle lists sorted, as the batched and the relationship
/// fetchers may list them in different orders.
fn normalized_exercises(details: &[crate::dto::exercises::ExerciseDetails]) -> serde_json::Value {
    let mut value = serde_json::to_value(details).unwrap();
    for detail in value.as_array_mut().unwrap() {
        for key in ["primary_muscles", "secondary_muscles"] {
            detail[key]
                .as_array_mut()
                .unwrap()
                .sort_by_key(|m| m.as_str().unwrap().to_string());
        }
    }
    value
}

#[tokio::test]
async fn batched_exercise_details_match_single_ones() {
    let db = common::test_db().await;
    db.run_in_transaction(async move |tx| {
        // Activity 3 falls outside the range asked for
        for (date, exercise) in [(10, BENCH), (20, CABLE_ROW), (30, BENCH)] {
            let mut activity = common::activity(date);
            activity.steps = vec![
                common::set(date, 0, exercise, 10, 5.0),
                common::set(date, 1, exercise, 8, date as f32),
            ];
            common::insert_activity(tx, activity).await;
        }
        BodyMetricRepository::insert()
            .item(&mut BodyMetric {
                date: 5,
                weight: 80.0,
                fat_ratio: 0.0,
                muscle_mass: 0.0,
                water_ratio: 0.0,
            })
            .execute_in(tx)
            .await?;
        ActivityStep::update_prs(tx, &HashSet::from([BENCH, CABLE_ROW]), &[]).await?;
        Ok(())
    })
    .await
    .unwrap();

    // PULL_UP has no sets: only the exercise itself
    let keys = BTreeSet::from([BENCH, CABLE_ROW, PULL_UP]);
    let range = Some((0, 25));
    let (batched, single) = db
        .run_in_connection(async move |conn| {
            let batched = fetch_exercises_details(conn, &keys, range).await?;
            let mut single = Vec::new();
            for (category, id) in keys {
                single.push(fetch_exercise_details(conn, category, id, range).await?);
            }
            Ok((batched, single))
        })
        .await
        .unwrap();

    let batched = normalized_exercises(&batched);
    assert_eq!(batched, normalized_exercises(&single));
    assert_eq!(batched.as_array().unwrap().len(), 3);
    // Bench's record is in activity 30, outside the range, but its activity 10 is listed
    assert_eq!(batched[0]["pr_date"], 30);
    assert_eq!(batched[0]["workouts"].as_array().unwrap().len(), 1);
}
