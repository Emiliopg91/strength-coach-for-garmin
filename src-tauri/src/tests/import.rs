use rusqlite_orm::{dao::Repository, database::DatabasePool};

use crate::{
    dao::{
        activity::{Activity, ActivityRepository},
        device::{Device, DeviceRepository},
        workout::{Workout, WorkoutRepository},
    },
    fit::{FitFile, parser::FitParser},
    logic::activities::helpers::import::{
        ParsedFiles, import_device_files, import_file_list, parse_files,
    },
    tests::common,
    utils::constants,
};

/// Serial of the watch that recorded the fixtures, `3000000001` in hexadecimal.
const FIXTURE_SERIAL: &str = "0000b2d05e01";

fn fixture_path(name: &str) -> String {
    format!(
        "{}/src/tests/fixtures/{name}.fit",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn parse_fixture(name: &str) -> Activity {
    FitParser::from_file(fixture_path(name))
        .and_then(Activity::try_from)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

async fn register_device(db: &DatabasePool, serial: &str) {
    let serial = serial.to_string();
    db.run_in_transaction(async move |tx| {
        DeviceRepository::insert()
            .item(&mut Device {
                serial: serial.clone(),
                model: "MODEL".to_string(),
                name: "MODEL".to_string(),
                last_activity_sync: None,
                last_body_metric_sync: None,
                usb: true,
                connect: false,
                strength_workout: true,
            })
            .execute_in(tx)
            .await?;
        Ok(())
    })
    .await
    .unwrap();
}

/// Imports the fixture and returns the device stored in its activity.
async fn import(db: &DatabasePool, fixture: &str) -> Option<String> {
    let files = [FitFile::read(fixture_path(fixture)).unwrap()];
    let imported = db
        .run_in_transaction(async move |tx| Ok(import_file_list(tx, parse_files(&files)).await?))
        .await
        .unwrap();
    assert_eq!(imported.activities.len(), 1);

    ActivityRepository::select_by_id(db, imported.activities[0])
        .await
        .unwrap()
        .unwrap()
        .device
}

#[test]
fn parser_reads_the_serial_of_the_creator_device() {
    // In these files the last device_info messages belong to sensors with serial 0
    for fixture in ["strength_1", "walk_oslo", "indoor_biking"] {
        assert_eq!(
            parse_fixture(fixture).device.as_deref(),
            Some(FIXTURE_SERIAL),
            "{fixture}"
        );
    }
}

#[tokio::test]
async fn registered_device_is_kept() {
    let db = common::test_db().await;
    register_device(&db, FIXTURE_SERIAL).await;

    assert_eq!(
        import(&db, "strength_1").await.as_deref(),
        Some(FIXTURE_SERIAL)
    );
}

#[tokio::test]
async fn unknown_device_is_cleared() {
    let db = common::test_db().await;
    register_device(&db, "another").await;

    assert_eq!(import(&db, "strength_1").await, None);
}

/// Imports the given files and returns the Connect id stored in the single activity created.
async fn import_connect_id(db: &DatabasePool, file: FitFile) -> Option<String> {
    let files = [file];
    let imported = db
        .run_in_transaction(async move |tx| Ok(import_file_list(tx, parse_files(&files)).await?))
        .await
        .unwrap();
    assert_eq!(imported.activities.len(), 1);

    ActivityRepository::select_by_id(db, imported.activities[0])
        .await
        .unwrap()
        .unwrap()
        .connect_id
}

#[tokio::test]
async fn connect_activity_id_is_stored() {
    let db = common::test_db().await;
    let file = FitFile::read(fixture_path("strength_1"))
        .unwrap()
        .with_activity_id(25017923159);

    assert_eq!(
        import_connect_id(&db, file).await.as_deref(),
        Some("25017923159")
    );
}

#[tokio::test]
async fn local_file_has_no_connect_id() {
    let db = common::test_db().await;
    let file = FitFile::read(fixture_path("strength_1")).unwrap();

    assert_eq!(import_connect_id(&db, file).await, None);
}

fn activity_of_workout(date: u32, notes: &str) -> Activity {
    let mut activity = common::activity(date);
    activity.workout_obj = Some(Workout {
        id: 0,
        name: "Legs".to_string(),
        enabled: true,
        notes: Some(notes.to_string()),
        steps: Vec::new(),
        connect_id: None,
        connect_created_time: None,
        progression: true,
    });
    activity
}

#[tokio::test]
async fn workout_keeps_the_notes_of_its_latest_activity() {
    let db = common::test_db().await;
    // Oldest first, as `parse_files` hands them
    let parsed = ParsedFiles {
        activities: vec![
            activity_of_workout(1, "Oldest"),
            activity_of_workout(2, "Middle"),
            activity_of_workout(3, "Latest"),
        ],
        failed: Vec::new(),
    };

    let imported = db
        .run_in_transaction(async move |tx| Ok(import_file_list(tx, parsed).await?))
        .await
        .unwrap();

    assert_eq!(imported.activities, [1, 2, 3]);
    let workout = WorkoutRepository::select_by_name(&db, "Legs")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(workout.notes.as_deref(), Some("Latest"));

    // Every activity links to the single workout stored
    let activities = ActivityRepository::select().fetch(&db).await.unwrap();
    assert_eq!(activities.len(), 3);
    assert!(activities.iter().all(|s| s.workout == Some(workout.id)));
}

/// Imports the fixtures as downloaded from Connect, the way `import_from_connect` does.
async fn import_from_connect(db: &DatabasePool, fixtures: &[&str]) {
    let files: Vec<_> = fixtures
        .iter()
        .map(|f| FitFile::read(fixture_path(f)).unwrap())
        .collect();
    let mut connect = DeviceRepository::select_by_id(db, constants::CONNECT_DEVICE_SERIAL)
        .await
        .unwrap()
        .unwrap();
    db.run_in_transaction(async move |tx| {
        Ok(import_device_files(tx, parse_files(&files), &mut connect, None).await?)
    })
    .await
    .unwrap();
}

async fn last_activity_sync(db: &DatabasePool, serial: &str) -> Option<u32> {
    DeviceRepository::select_by_id(db, serial)
        .await
        .unwrap()
        .unwrap()
        .last_activity_sync
}

async fn set_last_activity_sync(db: &DatabasePool, serial: &'static str, mark: u32) {
    db.run_in_transaction(async move |tx| {
        Ok(Device::advance_activity_sync(tx, serial, mark).await?)
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn connect_import_moves_the_mark_of_the_recording_device_to_its_latest_activity() {
    let db = common::test_db().await;
    register_device(&db, FIXTURE_SERIAL).await;

    import_from_connect(&db, &["strength_2", "strength_1"]).await;

    let latest = parse_fixture("strength_2").date;
    assert_eq!(last_activity_sync(&db, FIXTURE_SERIAL).await, Some(latest));
    // The pseudo-device follows what Connect lists, not a date
    assert_eq!(
        last_activity_sync(&db, constants::CONNECT_DEVICE_SERIAL).await,
        None
    );
}

#[tokio::test]
async fn connect_import_never_moves_the_mark_back() {
    let db = common::test_db().await;
    register_device(&db, FIXTURE_SERIAL).await;
    let later = parse_fixture("strength_2").date + 1;
    set_last_activity_sync(&db, FIXTURE_SERIAL, later).await;

    import_from_connect(&db, &["strength_1"]).await;

    assert_eq!(last_activity_sync(&db, FIXTURE_SERIAL).await, Some(later));
}

#[tokio::test]
async fn connect_import_leaves_other_devices_alone() {
    let db = common::test_db().await;
    register_device(&db, FIXTURE_SERIAL).await;
    register_device(&db, "another").await;

    import_from_connect(&db, &["strength_1"]).await;

    assert_eq!(last_activity_sync(&db, "another").await, None);
}
