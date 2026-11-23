use rusqlite_orm::{dao::Repository, database::builder::DatabaseConnectionBuilder, ddls};

use crate::{
    dao::{
        activity::ActivityRepository,
        activity_data::ActivityDataRepository,
        activity_lap::ActivityLapRepository,
        activity_step::ActivityStepRepository,
        body_metric::BodyMetricRepository,
        workout::{self, Workout, WorkoutRepository},
        workout_device::WorkoutDeviceRepository,
        workout_step::StepType,
    },
    schema_ddls,
    tests::common,
    utils::time::local_midnight,
};

#[tokio::test]
async fn schema_version_matches_last_ddl() {
    let db = common::test_db().await;
    let last = ddls!("../resources/ddl")
        .iter()
        .map(|d| d.version)
        .max()
        .unwrap();

    let version: u16 = db
        .run_in_connection(async move |conn| {
            Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
        })
        .await
        .unwrap();

    assert_eq!(version, last);
}

#[test]
fn schema_ddls_are_sorted_with_hooks_on_their_migration() {
    let ddls = schema_ddls();

    let versions: Vec<_> = ddls.iter().map(|d| d.version).collect();
    assert_eq!(versions, (1..=versions.len() as u16).collect::<Vec<_>>());

    let with_hooks: Vec<_> = ddls
        .iter()
        .filter(|d| d.update_fn.is_some())
        .map(|d| d.version)
        .collect();
    assert_eq!(with_hooks, [21]);
}

#[tokio::test]
async fn migration_26_turns_rest_flag_into_kind() {
    let db = DatabaseConnectionBuilder::default()
        .enable_foreign_keys()
        .build("test")
        .unwrap();
    let before: Vec<_> = schema_ddls()
        .into_iter()
        .filter(|d| d.version < 26)
        .collect();
    db.create_schema(&before).await.unwrap();

    db.run_in_transaction(async move |tx| {
        insert_legacy_activity(tx, 1).await?;
        tx.execute_batch(
            "INSERT INTO SESSION_STEP (session, idx, exercise_category, exercise_id, reps, weight, pr, rest)
             VALUES (1, 0, 0, 1, 10, 50, 0, 0), (1, 1, NULL, NULL, 0, 0, 0, 1);",
        )?;
        Ok(())
    }).await
    .unwrap();

    db.create_schema(&schema_ddls()).await.unwrap();

    let mut steps = ActivityStepRepository::select().fetch(&db).await.unwrap();
    steps.sort_by_key(|s| s.idx);
    let kinds: Vec<_> = steps.into_iter().map(|s| s.kind).collect();
    assert_eq!(kinds, [StepType::Exercise, StepType::Rest]);

    let has_rest: bool = db
        .run_in_connection(async move |conn| {
            Ok(conn
                .prepare("SELECT 1 FROM pragma_table_info('ACTIVITY_STEP') WHERE name = 'rest'")?
                .exists([])?)
        })
        .await
        .unwrap();
    assert!(!has_rest);
}

#[tokio::test]
async fn body_metrics_migrate_from_lean_mass_to_muscle_mass() {
    let db = DatabaseConnectionBuilder::default()
        .enable_foreign_keys()
        .build("test")
        .unwrap();
    let before: Vec<_> = schema_ddls()
        .into_iter()
        .filter(|d| d.version < 21)
        .collect();
    db.create_schema(&before).await.unwrap();

    let midnight = local_midnight(1_700_000_000).unwrap();
    let (early, late) = (midnight + 3_600, midnight + 7_200);
    db.run_in_transaction(async move |tx| {
        tx.execute_batch(&format!(
            "INSERT INTO BODY_METRIC (date, weight, fat_ratio, lean_mass, water_ratio)
             VALUES ({early}, 81.0, 18.0, 60.0, 55.0), ({late}, 80.0, 17.0, 61.0, 56.0);"
        ))?;
        Ok(())
    })
    .await
    .unwrap();

    db.create_schema(&schema_ddls()).await.unwrap();

    // Both fall on the same day: only the latest survives, moved to midnight
    let metrics = BodyMetricRepository::select().fetch(&db).await.unwrap();
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].date, midnight);
    assert_eq!(metrics[0].weight, 800);
    assert_eq!(metrics[0].muscle_mass, 610);
}

#[tokio::test]
async fn schema_creation_is_idempotent() {
    let db = common::test_db().await;
    db.create_schema(&ddls!("../resources/ddl")).await.unwrap();
}

#[tokio::test]
async fn activity_roundtrip_with_relationships() {
    let db = common::test_db().await;

    let mut activity = common::activity(1_700_000_000);
    activity.steps = vec![common::set(1_700_000_000, 0, (28, 0), 5, 100.0)];
    activity.laps = vec![common::lap(1_700_000_000, 0, Some((1, 2)))];
    let mut data = common::additional_data(1_700_000_000);
    data.notes = Some("Felt strong".to_string());
    data.distance = Some(1_000.0);
    activity.data = Some(data);

    let mut tx_activity = activity.clone();
    db.run_in_transaction(async move |tx| {
        let mut workout = Workout {
            id: 0,
            name: "Leg day".to_string(),
            enabled: true,
            notes: Some(String::new()),
            steps: Vec::new(),
            connect_id: None,
            connect_created_time: None,
            progression: true,
        };
        WorkoutRepository::insert()
            .item(&mut workout)
            .execute_in(tx)
            .await?;
        tx_activity.workout = Some(workout.id);
        common::insert_activity(tx, tx_activity).await;
        Ok(())
    })
    .await
    .unwrap();

    let stored = db
        .run_in_connection(async move |conn| {
            let mut s = ActivityRepository::select_by_id_in(conn, 1_700_000_000_u32)
                .await?
                .unwrap();
            s.fetch_steps_relationship_in(conn).await?;
            s.fetch_laps_relationship_in(conn).await?;
            s.fetch_data_relationship_in(conn).await?;
            s.fetch_workout_obj_relationship_in(conn).await?;
            Ok(s)
        })
        .await
        .unwrap();

    assert_eq!(stored.name, activity.name);
    assert_eq!(stored.steps.len(), 1);
    assert_eq!(stored.steps[0].weight, Some(1000));
    assert_eq!(stored.laps.len(), 1);
    assert_eq!(stored.laps[0].start_latitude, Some(1));
    let data = stored.data.unwrap();
    assert_eq!(data.notes.as_deref(), Some("Felt strong"));
    assert_eq!(data.distance, Some(1_000.0));
    let workout = stored.workout_obj.unwrap();
    assert!(workout.enabled);
    assert_eq!(workout.name, "Leg day");
}

#[tokio::test]
async fn deleting_an_activity_cascades() {
    let db = common::test_db().await;

    let mut activity = common::activity(1);
    activity.steps = vec![common::set(1, 0, (28, 0), 5, 100.0)];
    activity.laps = vec![common::lap(1, 0, None)];
    activity.data = Some(common::additional_data(1));

    db.run_in_transaction(async move |tx| {
        common::insert_activity(tx, activity.clone()).await;
        activity.delete_by_id_in(tx).await?;
        Ok(())
    })
    .await
    .unwrap();

    let (sets, laps, data) = db
        .run_in_connection(async move |conn| {
            Ok((
                ActivityStepRepository::select().fetch_in(conn).await?.len(),
                ActivityLapRepository::select().fetch_in(conn).await?.len(),
                ActivityDataRepository::select().fetch_in(conn).await?.len(),
            ))
        })
        .await
        .unwrap();

    assert_eq!((sets, laps, data), (0, 0, 0));
}

#[tokio::test]
async fn set_requires_known_exercise() {
    let db = common::test_db().await;

    let mut activity = common::activity(1);
    activity.steps = vec![common::set(1, 0, (9999, 9999), 5, 100.0)];

    let res = db
        .run_in_transaction(async move |tx| {
            let mut sets = std::mem::take(&mut activity.steps);
            ActivityRepository::insert()
                .item(&mut activity.clone())
                .execute_in(tx)
                .await?;
            ActivityStepRepository::insert()
                .item(&mut sets[0])
                .execute_in(tx)
                .await?;
            Ok(())
        })
        .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn migration_27_nulls_reps_and_weight_only_on_rest_steps() {
    let db = DatabaseConnectionBuilder::default()
        .enable_foreign_keys()
        .build("test")
        .unwrap();
    let before: Vec<_> = schema_ddls()
        .into_iter()
        .filter(|d| d.version < 27)
        .collect();
    db.create_schema(&before).await.unwrap();

    db.run_in_transaction(async move |tx| {
        insert_legacy_activity(tx, 1).await?;
        tx.execute_batch(
            "INSERT INTO SESSION_STEP (session, idx, exercise_category, exercise_id, reps, weight, pr, duration, kind)
             VALUES (1, 0, 0, 1, 10, 50, 1, 60, 'Exercise'),
                    (1, 1, NULL, NULL, 0, 0, 0, 90, 'Rest'),
                    (1, 2, NULL, NULL, 0, 0, 0, 30, 'Warmup');",
        )?;
        Ok(())
    }).await
    .unwrap();

    db.create_schema(&schema_ddls()).await.unwrap();

    let mut steps = ActivityStepRepository::select().fetch(&db).await.unwrap();
    steps.sort_by_key(|s| s.idx);

    let values: Vec<_> = steps
        .iter()
        .map(|s| (s.reps, s.weight, s.pr, s.duration, s.kind.clone()))
        .collect();
    assert_eq!(
        values,
        [
            (Some(10), Some(500), true, Some(60), StepType::Exercise),
            (None, None, false, Some(90), StepType::Rest),
            (Some(0), Some(0), false, Some(30), StepType::Warmup),
        ]
    );

    let indexes: i64 = db
        .run_in_connection(async move |conn| {
            Ok(conn.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'ACTIVITY_STEP' AND sql IS NOT NULL",
                [],
                |r| r.get(0),
            )?)
        }).await
        .unwrap();
    assert_eq!(indexes, 3);
}

#[tokio::test]
async fn migration_27_keeps_the_children_of_rebuilt_tables() {
    let db = DatabaseConnectionBuilder::default()
        .enable_foreign_keys()
        .build("test")
        .unwrap();
    let before: Vec<_> = schema_ddls()
        .into_iter()
        .filter(|d| d.version < 27)
        .collect();
    db.create_schema(&before).await.unwrap();

    db.run_in_transaction(async move |tx| {
        insert_legacy_activity(tx, 1).await?;
        tx.execute_batch(
            "INSERT INTO LAP (session, idx) VALUES (1, 0);
             INSERT INTO ADDITIONAL_DATA (session, max_hr) VALUES (1, 190);
             INSERT INTO SESSION_STEP (session, idx, exercise_category, exercise_id, reps, weight, pr, duration, kind)
             VALUES (1, 0, 0, 1, 10, 50, 0, 60, 'Exercise');
             INSERT INTO WORKOUT (name) VALUES ('Push');
             INSERT INTO WORKOUT_STEP (workout, idx, kind, exercise_category, exercise_id) VALUES ('Push', 0, 'Exercise', 0, 1);",
        )?;
        Ok(())
    }).await
    .unwrap();
    let count = async |table: &str| -> i64 {
        let sql = format!("SELECT count(*) FROM {table}");
        db.run_in_connection(async move |conn| Ok(conn.query_row(&sql, [], |r| r.get(0))?))
            .await
            .unwrap()
    };
    let tables = [
        "SESSION",
        "LAP",
        "ADDITIONAL_DATA",
        "SESSION_STEP",
        "WORKOUT",
        "WORKOUT_STEP",
        "MUSCLE",
        "MUSCLE_EXERCISE",
        "EXERCISE",
        "SUB_SPORT",
    ];
    let mut expected = Vec::new();
    for table in tables {
        expected.push(count(table).await);
    }
    assert!(expected.iter().all(|c| *c > 0));

    // Up to 28: later migrations add catalog rows on purpose
    let up_to_28: Vec<_> = schema_ddls()
        .into_iter()
        .filter(|d| d.version <= 28)
        .collect();
    db.create_schema(&up_to_28).await.unwrap();

    // Migration 28 renames LAP and ADDITIONAL_DATA
    let renamed = |table: &'static str| match table {
        "LAP" => "SESSION_LAP",
        "ADDITIONAL_DATA" => "SESSION_DATA",
        other => other,
    };
    let mut after = Vec::new();
    for table in tables {
        after.push(count(renamed(table)).await);
    }
    assert_eq!(after, expected);
}

#[tokio::test]
async fn catalogs_in_use_cannot_be_deleted() {
    let db = common::test_db().await;
    db.run_in_transaction(async move |tx| {
        let mut activity = common::activity(1);
        activity.steps = vec![common::set(1, 0, (0, 1), 10, 50.0)];
        common::insert_activity(tx, activity).await;
        Ok(())
    })
    .await
    .unwrap();
    let run = async |sql: &str| {
        let sql = sql.to_string();
        db.run_in_transaction(async move |tx| {
            tx.execute_batch(&sql)?;
            Ok(())
        })
        .await
    };

    assert!(
        run("DELETE FROM SUB_SPORT WHERE sport = 10 AND id = 20")
            .await
            .is_err()
    );
    assert!(
        run("DELETE FROM EXERCISE WHERE category = 0 AND id = 1")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn migration_27_turns_workout_step_time_into_seconds() {
    let db = DatabaseConnectionBuilder::default()
        .enable_foreign_keys()
        .build("test")
        .unwrap();
    let before: Vec<_> = schema_ddls()
        .into_iter()
        .filter(|d| d.version < 27)
        .collect();
    db.create_schema(&before).await.unwrap();
    db.run_in_transaction(async move |tx| {
        tx.execute_batch(
            "INSERT INTO WORKOUT (name) VALUES ('Push');
             INSERT INTO WORKOUT_STEP (workout, idx, kind, time)
             VALUES ('Push', 0, 'Rest', 90000), ('Push', 1, 'Rest', NULL);",
        )?;
        Ok(())
    })
    .await
    .unwrap();

    db.create_schema(&schema_ddls()).await.unwrap();

    let mut steps = db
        .run_in_connection(async move |conn| {
            let mut stmt = conn.prepare("SELECT time FROM WORKOUT_STEP ORDER BY idx")?;
            let rows = stmt.query_map([], |r| r.get::<_, Option<u32>>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .unwrap();
    assert_eq!(steps.remove(0), Some(90));
    assert_eq!(steps.remove(0), None);
}

#[tokio::test]
async fn migration_28_drops_chart_flag_and_cascades_workout_device() {
    let db = DatabaseConnectionBuilder::default()
        .enable_foreign_keys()
        .build("test")
        .unwrap();
    let before: Vec<_> = schema_ddls()
        .into_iter()
        .filter(|d| d.version < 28)
        .collect();
    db.create_schema(&before).await.unwrap();

    db.run_in_transaction(async move |tx| {
        tx.execute_batch(
            "INSERT INTO WORKOUT (name, enabled) VALUES ('a', 1), ('b', 1);
             INSERT INTO WORKOUT_DEVICE (workout, device, pending)
             VALUES ('a', 'connect', 1), ('b', 'connect', 0);",
        )?;
        Ok(())
    })
    .await
    .unwrap();

    db.create_schema(&schema_ddls()).await.unwrap();

    let entries = async |db: &rusqlite_orm::database::DatabasePool| {
        let mut entries: Vec<_> = WorkoutDeviceRepository::select()
            .fetch(db)
            .await
            .unwrap()
            .into_iter()
            .map(|e| (e.workout, e.pending))
            .collect();
        entries.sort();
        entries
    };
    // Workouts are numbered by name when they get their id
    assert_eq!(entries(&db).await, [(1, true), (2, false)]);

    let has_chart_flag: bool = db
        .run_in_connection(async move |conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('ACTIVITY') WHERE name = 'show_exercises_chart'",
                [],
                |r| r.get(0),
            )?)
        }).await
        .unwrap();
    assert!(!has_chart_flag);

    WorkoutRepository::delete()
        .where_(rusqlite_orm::types::where_clause::Where::Eq(
            workout::entity::columns::NAME,
            "a".into(),
        ))
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(entries(&db).await, [(2, false)]);
}

#[tokio::test]
async fn migration_30_tunes_indexes() {
    let db = common::test_db().await;

    let index_sql = async |name: &'static str| {
        db.run_in_connection(async move |conn| {
            Ok(conn
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [name],
                    |r| r.get::<_, String>(0),
                )
                .ok())
        })
        .await
        .unwrap()
    };
    assert_eq!(index_sql("WORKOUT_STEP_WORKOUT").await, None);
    assert!(index_sql("MUSCLE_EXERCISE_EXERCISE").await.is_some());
    assert!(
        index_sql("ACTIVITY_STEP_EXERCISE_PR")
            .await
            .unwrap()
            .ends_with("WHERE pr = true")
    );

    // The ORM renders booleans as `true`, which must still reach the partial index
    let plan: Vec<String> = db
        .run_in_connection(async move |conn| {
            let mut stmt = conn.prepare(
                "EXPLAIN QUERY PLAN SELECT activity FROM ACTIVITY_STEP
                 WHERE pr=true AND exercise_category IS NOT NULL AND exercise_id IS NOT NULL",
            )?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(3))?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|p| p.contains("ACTIVITY_STEP_EXERCISE_PR")),
        "{plan:?}"
    );
}

#[tokio::test]
async fn migration_34_indexes_records_by_load() {
    let db = common::test_db().await;

    let plan = async |sql: &'static str| -> Vec<String> {
        db.run_in_connection(async move |conn| {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(3))?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .await
        .unwrap()
    };

    let index_exists = async |name: &'static str| {
        db.run_in_connection(async move |conn| {
            Ok(conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [name],
                    |_| Ok(()),
                )
                .is_ok())
        })
        .await
        .unwrap()
    };
    assert!(!index_exists("ACTIVITY_STEP_PR").await);
    assert!(!index_exists("ACTIVITY_STEP_EXERCISE").await);

    // The record of an exercise is read straight from the index, without sorting its sets
    let record = plan(
        "SELECT * FROM ACTIVITY_STEP WHERE exercise_category=1 AND exercise_id=2
         ORDER BY weight DESC, reps DESC, activity ASC, idx ASC LIMIT 1",
    )
    .await;
    assert!(
        record
            .iter()
            .any(|p| p.contains("ACTIVITY_STEP_EXERCISE_RECORD")),
        "{record:?}"
    );
    assert!(
        !record.iter().any(|p| p.contains("TEMP B-TREE")),
        "{record:?}"
    );

    // Every record, as `ActivityStep::get_personal_records` asks for them, scans the partial index
    let records = plan(
        "SELECT * FROM ACTIVITY_STEP
         WHERE pr=true AND exercise_category IS NOT NULL AND exercise_id IS NOT NULL",
    )
    .await;
    assert!(
        records
            .iter()
            .any(|p| p.contains("ACTIVITY_STEP_EXERCISE_PR")),
        "{records:?}"
    );
}

#[tokio::test]
async fn migration_40_turns_weights_into_tenths_of_kg() {
    let db = DatabaseConnectionBuilder::default()
        .enable_foreign_keys()
        .build("test")
        .unwrap();
    let before: Vec<_> = schema_ddls()
        .into_iter()
        .filter(|d| d.version < 40)
        .collect();
    db.create_schema(&before).await.unwrap();

    db.run_in_transaction(async move |tx| {
        tx.execute_batch(&format!(
            "UPDATE EXERCISE SET weight = 2.5 WHERE category = 0 AND id = 1;
             INSERT INTO ACTIVITY (date, name, total_elapsed_time, sport, sub_sport)
             VALUES (1, 'Push', 0, {sport}, {sub_sport});
             INSERT INTO ACTIVITY_STEP (activity, idx, kind, exercise_category, exercise_id, reps, weight, weight_ref, pr)
             VALUES (1, 0, 'Exercise', 0, 1, 10, 12.1875, 12.199999809265137, 1),
                    (1, 1, 'Rest', NULL, NULL, NULL, NULL, NULL, 0);
             INSERT INTO WORKOUT (id, name) VALUES (1, 'Push');
             INSERT INTO WORKOUT_STEP (workout, idx, kind, exercise_category, exercise_id, reps, weight)
             VALUES (1, 0, 'Exercise', 0, 1, 10, 12.5),
                    (1, 1, 'Rest', NULL, NULL, NULL, NULL);
             INSERT INTO BODY_METRIC (date, weight, fat_ratio, muscle_mass, water_ratio)
             VALUES (1, 80.46, 18.0, 32.55, 55.0);",
            sport = common::STRENGTH_SPORT,
            sub_sport = common::STRENGTH_SUB_SPORT,
        ))?;
        Ok(())
    })
    .await
    .unwrap();

    db.create_schema(&schema_ddls()).await.unwrap();

    let query = async |sql: &'static str| -> Vec<String> {
        db.run_in_connection(async move |conn| {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .await
        .unwrap()
    };

    assert_eq!(
        query("SELECT typeof(weight) || ':' || weight FROM EXERCISE WHERE category = 0 AND id = 1")
            .await,
        ["integer:25"]
    );
    assert_eq!(
        query(
            "SELECT typeof(weight) || ':' || ifnull(weight, '') || ':' || ifnull(weight_ref, '')
             FROM ACTIVITY_STEP ORDER BY idx"
        )
        .await,
        ["integer:122:122", "null::"]
    );
    assert_eq!(
        query("SELECT typeof(weight) || ':' || ifnull(weight, '') FROM WORKOUT_STEP ORDER BY idx")
            .await,
        ["integer:125", "null:"]
    );
    assert_eq!(
        query("SELECT weight || ':' || muscle_mass FROM BODY_METRIC").await,
        ["805:326"]
    );
    // New exercises keep the 2 kg default increment
    assert_eq!(
        query("SELECT dflt_value FROM pragma_table_info('EXERCISE') WHERE name = 'weight'").await,
        ["20"]
    );
    assert_eq!(
        query(
            "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'ACTIVITY_STEP_EXERCISE_RECORD'"
        )
        .await,
        ["ACTIVITY_STEP_EXERCISE_RECORD"]
    );

    // The entities read them back
    let steps = ActivityStepRepository::select().fetch(&db).await.unwrap();
    assert!(steps.iter().any(|s| s.weight == Some(122)));
    let metrics = BodyMetricRepository::select().fetch(&db).await.unwrap();
    assert_eq!((metrics[0].weight, metrics[0].muscle_mass), (805, 326));
}

/// Inserts an activity with raw SQL, using only the columns that exist before migration 28:
/// the `Activity` entity tracks the latest schema, so its inserts fail on older ones.
pub(crate) async fn insert_legacy_activity(
    tx: &rusqlite_orm::rusqlite::Transaction<'_>,
    date: u32,
) -> rusqlite_orm::rusqlite::Result<()> {
    let activity = common::activity(date);
    tx.execute(
        "INSERT INTO SESSION (date, name, total_elapsed_time, active_time, active_calories, training_load, sport, sub_sport)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite_orm::rusqlite::params![
            activity.date,
            activity.name,
            activity.total_elapsed_time,
            activity.active_time,
            activity.active_calories,
            activity.training_load,
            activity.sport,
            activity.sub_sport,
        ],
    )?;
    Ok(())
}
