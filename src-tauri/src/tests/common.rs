use rusqlite_orm::{
    dao::Repository,
    database::{DatabasePool, builder::DatabaseConnectionBuilder},
    rusqlite::Transaction,
};

use crate::{
    dao::{
        activity::{Activity, ActivityRepository},
        activity_data::{ActivityData, ActivityDataRepository},
        activity_lap::{ActivityLap, ActivityLapRepository},
        activity_step::{ActivityStep, ActivityStepRepository},
        workout::Workout,
        workout_step::StepType,
    },
    schema_ddls,
};

/// Strength training sport/sub-sport pair, present in the base schema seed data.
pub const STRENGTH_SPORT: u8 = 10;
pub const STRENGTH_SUB_SPORT: u8 = 20;

/// Builds an in-memory database with the full schema applied, mirroring `initialize()` in `lib.rs`.
pub async fn test_db() -> DatabasePool {
    let database = DatabaseConnectionBuilder::default()
        .enable_foreign_keys()
        .build("test")
        .expect("in-memory database");

    database
        .create_schema(&schema_ddls())
        .await
        .expect("schema creation");

    database
}

pub fn activity(date: u32) -> Activity {
    Activity {
        date,
        name: format!("Activity {date}"),
        total_elapsed_time: 3600,
        active_time: 3000,
        active_calories: 100,
        training_load: 80,
        device: None,
        workout: None,
        sport: STRENGTH_SPORT,
        sub_sport: STRENGTH_SUB_SPORT,
        steps: Vec::new(),
        laps: Vec::new(),
        device_obj: None,
        data: None,
        sub_sport_obj: None,
        workout_obj: None,
        connect_id: None,
    }
}

pub fn set(activity: u32, idx: u8, exercise: (u16, u16), reps: u16, weight: f32) -> ActivityStep {
    ActivityStep {
        activity,
        idx,
        ex_cat: Some(exercise.0),
        ex_id: Some(exercise.1),
        reps: Some(reps),
        reps_ref: None,
        weight: Some(weight),
        weight_ref: None,
        pr: false,
        exercise: None,
        duration: Some(0),
        duration_ref: None,
        kind: StepType::Exercise,
    }
}

pub fn additional_data(activity: u32) -> ActivityData {
    ActivityData {
        activity,
        heart_rates: None,
        coordinates: None,
        speeds: None,
        distance: None,
        notes: None,
        max_hr: None,
    }
}

pub async fn insert_activity(tx: &Transaction<'_>, mut activity: Activity) {
    let mut sets = std::mem::take(&mut activity.steps);
    let mut laps = std::mem::take(&mut activity.laps);
    let additional_data = activity.data.take();

    ActivityRepository::insert()
        .item(&mut activity)
        .execute_in(tx)
        .await
        .expect("activity insert");

    if !sets.is_empty() {
        let mut insert = ActivityStepRepository::insert();
        for set in &mut sets {
            insert = insert.item(set);
        }
        insert.execute_in(tx).await.expect("sets insert");
    }

    if !laps.is_empty() {
        let mut insert = ActivityLapRepository::insert();
        for lap in &mut laps {
            insert = insert.item(lap);
        }
        insert.execute_in(tx).await.expect("laps insert");
    }

    if let Some(mut additional_data) = additional_data {
        ActivityDataRepository::insert()
            .item(&mut additional_data)
            .execute_in(tx)
            .await
            .expect("additional data insert");
    }
}

pub fn lap(activity: u32, idx: u16, position: Option<(i32, i32)>) -> ActivityLap {
    ActivityLap {
        activity,
        idx,
        start_latitude: position.map(|p| p.0),
        start_longitude: position.map(|p| p.1),
    }
}

/// Id of the stored workout called `name`; panics if there is none.
pub async fn workout_id(conn: &rusqlite_orm::rusqlite::Connection, name: &str) -> u16 {
    Workout::get_id(conn, name)
        .await
        .expect("workout lookup")
        .unwrap_or_else(|| panic!("workout {name} not stored"))
}

/// [`workout_id`] through the pool.
pub async fn workout_id_in(db: &DatabasePool, name: &str) -> u16 {
    let name = name.to_string();
    db.run_in_connection(async move |conn| Ok(workout_id(conn, &name).await))
        .await
        .unwrap()
}
