use std::sync::{Arc, Mutex, RwLock};

use rusqlite_orm::{dao::Repository, database::DatabasePool};
use rustyfit::profile::typedef::Intensity;

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    connect::ConnectClient,
    dao::{
        activity::Activity,
        device::{Device, DeviceRepository},
        settings::settings_keys::DISTANCE_UNIT,
        settings::{DistanceUnit, WeightUnit},
        workout::Workout,
        workout_device::WorkoutDeviceRepository,
        workout_step::{StepType, WorkoutStep},
    },
    dto::prompts::PromptKind,
    dto::{
        activities::{ActivityStepDto, ActivityStepsUpdate},
        app::Settings,
        body_metrics::BodyMetricListItem,
        exercises::ProgressionSuggestion,
    },
    fit::parser::FitParser,
    logic::{
        activities, app, body_metrics, connect, devices,
        events::{BackendEvent, EventSink},
        exercises,
        prompts::{
            self,
            helpers::requests::{AppPrompter, Prompts},
        },
        report, workouts,
    },
    mtp::MtpClient,
    tests::common,
    utils::{constants, translations::Languages, weight::from_kg},
};

const BENCH: (u16, u16) = (0, 1);

/// State of the real app over an in-memory database, without reaching the keyring nor the USB
/// devices. The events sent to the frontend are kept in `emitted`.
struct TestApp {
    pool: DatabasePool,
    settings: SettingsLock,
    connect: ConnectLock,
    mtp: MtpLock,
    prompts: Prompts,
    events: EventSink,
    emitted: Arc<Mutex<Vec<BackendEvent>>>,
}

impl TestApp {
    /// Events emitted so far, oldest first.
    fn emitted(&self) -> Vec<BackendEvent> {
        self.emitted.lock().unwrap().clone()
    }
}

async fn test_app() -> TestApp {
    let pool = common::test_db().await;
    let settings = SettingsLock::new(RwLock::new(Settings::load(&pool).await));
    let prompts = Prompts::default();
    let emitted = Arc::new(Mutex::new(Vec::new()));
    let events = {
        let emitted = emitted.clone();
        EventSink::new(move |event| {
            emitted.lock().unwrap().push(event);
            Ok(())
        })
    };
    let prompter = AppPrompter::new(prompts.clone(), events.clone());
    TestApp {
        pool,
        settings,
        connect: ConnectLock::new(tokio::sync::Mutex::new(ConnectClient::new(
            |_| {},
            prompter,
        ))),
        mtp: MtpLock::new(tokio::sync::Mutex::new(MtpClient {})),
        prompts,
        events,
        emitted,
    }
}

fn step(idx: u16, kind: Intensity, weight: f32, reps: u16) -> WorkoutStep {
    WorkoutStep::active(
        0,
        idx,
        kind,
        Some(BENCH),
        from_kg(weight),
        Some(reps),
        None,
        None,
    )
}

fn workout(name: &str, steps: Vec<WorkoutStep>) -> Workout {
    Workout {
        id: 0,
        name: name.to_string(),
        enabled: true,
        notes: None,
        connect_id: None,
        connect_created_time: None,
        steps,
        progression: true,
    }
}

async fn save_workout(app: &TestApp, edit: bool, workout: Workout) {
    workouts::save_workout(
        &app.pool,
        &app.settings,
        &app.connect,
        &app.mtp,
        edit,
        workout,
    )
    .await
    .unwrap();
}

async fn workout_id(app: &TestApp, name: &str) -> u16 {
    common::workout_id_in(&app.pool, name).await
}

async fn workout_steps(app: &TestApp, name: &str) -> Vec<WorkoutStep> {
    let id = workout_id(app, name).await;
    workouts::get_workout_details(&app.pool, &app.settings, id)
        .await
        .unwrap()
        .steps
}

async fn insert_activity(app: &TestApp, date: u32, reps: u16, weight: f32) {
    app.pool
        .run_in_transaction(async move |tx| {
            let mut activity = common::activity(date);
            activity.steps = vec![common::set(date, 0, BENCH, reps, weight)];
            common::insert_activity(tx, activity).await;
            Ok(())
        })
        .await
        .unwrap();
}

// App

#[tokio::test]
async fn settings_are_read_and_updated_in_memory_and_database() {
    let app = test_app().await;
    app::update_settings_value(
        &app.pool,
        &app.settings,
        &app.prompts,
        &app.events,
        DISTANCE_UNIT,
        "Miles",
        |_| Ok(()),
    )
    .await
    .unwrap();

    let settings = app::get_settings(&app.settings).await.unwrap();
    assert!(matches!(settings.distance_unit, DistanceUnit::Miles));
    assert!(matches!(
        Settings::load(&app.pool).await.distance_unit,
        DistanceUnit::Miles
    ));
}

#[tokio::test]
async fn translations_contain_the_step_kinds() {
    let app = test_app().await;
    let translations = app::get_translations(&app.settings).await.unwrap();

    for key in [
        "step_type_warmup",
        "step_type_cooldown",
        "step_type_recovery",
        "step_type_other",
    ] {
        assert!(translations.contains_key(key), "{key}");
    }
}

// Body metrics

#[tokio::test]
async fn body_measures_are_added_listed_and_deleted() {
    let app = test_app().await;
    let measures = BodyMetricListItem {
        date: 1_700_000_000,
        weight: 80.0,
        fat_ratio: 15.0,
        muscle_mass: 60.0,
        water_ratio: 55.0,
    };
    let settings = &app.settings;

    body_metrics::add_body_measures(&app.pool, settings, measures)
        .await
        .unwrap();
    let listed = body_metrics::get_body_measures(&app.pool, settings)
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].weight, 80.0);

    // Stored at local midnight, the listed date is the one to delete by
    body_metrics::delete_body_metric(&app.pool, settings, listed[0].date)
        .await
        .unwrap();
    assert!(
        body_metrics::get_body_measures(&app.pool, settings)
            .await
            .unwrap()
            .is_empty()
    );
}

// Connect

#[tokio::test]
async fn connect_starts_logged_out_and_logout_never_fails() {
    let app = test_app().await;
    let client = &app.connect;

    assert_eq!(connect::connect_logged_user(client).await, Ok(None));
    assert_eq!(connect::connect_logout(client).await, Ok(()));
}

// Devices

/// Registers an unplugged USB device "watch" and queues the workout `id` on it.
async fn queue_on_watch(app: &TestApp, id: u16) {
    app.pool
        .run_in_transaction(async move |tx| {
            DeviceRepository::insert()
                .item(&mut Device {
                    serial: "watch".to_string(),
                    model: "model".to_string(),
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
    devices::send_to_device(&app.pool, &app.settings, &app.mtp, id, "watch")
        .await
        .unwrap();
}

/// Previous name of the workout `id` by device, of every device entry.
async fn previous_names(app: &TestApp, id: u16) -> Vec<(String, Option<String>)> {
    let mut entries: Vec<_> = WorkoutDeviceRepository::select()
        .fetch(&app.pool)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.workout == id)
        .map(|e| (e.device, e.previous_name))
        .collect();
    entries.sort();
    entries
}

#[tokio::test]
async fn renamed_workout_keeps_its_id_and_records_the_old_name_on_usb_devices() {
    let app = test_app().await;
    save_workout(
        &app,
        false,
        workout("Push", vec![step(0, Intensity::ACTIVE, 60.0, 10)]),
    )
    .await;
    let id = workout_id(&app, "Push").await;
    queue_on_watch(&app, id).await;

    let mut renamed = workout("Push A", vec![step(0, Intensity::ACTIVE, 60.0, 10)]);
    renamed.id = id;
    save_workout(&app, true, renamed).await;

    assert_eq!(workout_id(&app, "Push A").await, id);
    assert_eq!(
        previous_names(&app, id).await,
        [
            (constants::CONNECT_DEVICE_SERIAL.to_string(), None),
            ("watch".to_string(), Some("Push".to_string())),
        ]
    );

    // Renamed again before plugging the watch: its file is still the first one
    let mut renamed = workout("Push B", vec![step(0, Intensity::ACTIVE, 60.0, 10)]);
    renamed.id = id;
    save_workout(&app, true, renamed).await;
    assert_eq!(
        previous_names(&app, id).await[1],
        ("watch".to_string(), Some("Push".to_string()))
    );

    // Queuing it again keeps the name of the file on the device
    devices::send_to_device(&app.pool, &app.settings, &app.mtp, id, "watch")
        .await
        .unwrap();
    assert_eq!(
        previous_names(&app, id).await[1],
        ("watch".to_string(), Some("Push".to_string()))
    );
}

#[tokio::test]
async fn saved_workout_is_pending_for_connect_once_the_command_returns() {
    let app = test_app().await;
    save_workout(
        &app,
        false,
        workout("Push", vec![step(0, Intensity::ACTIVE, 60.0, 10)]),
    )
    .await;
    let id = workout_id(&app, "Push").await;

    // Marked in the command's own transaction, not by the background upload
    let pending: Vec<bool> = WorkoutDeviceRepository::select()
        .fetch(&app.pool)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.workout == id && e.device == constants::CONNECT_DEVICE_SERIAL)
        .map(|e| e.pending)
        .collect();
    assert_eq!(pending, [true]);
}

#[tokio::test]
async fn workout_name_of_another_workout_is_rejected() {
    let app = test_app().await;
    save_workout(
        &app,
        false,
        workout("Push", vec![step(0, Intensity::ACTIVE, 60.0, 10)]),
    )
    .await;
    save_workout(
        &app,
        false,
        workout("Pull", vec![step(0, Intensity::ACTIVE, 60.0, 10)]),
    )
    .await;
    let pull = workout_id(&app, "Pull").await;

    let mut renamed = workout("Push", vec![step(0, Intensity::ACTIVE, 60.0, 10)]);
    renamed.id = pull;
    let edit = workouts::save_workout(
        &app.pool,
        &app.settings,
        &app.connect,
        &app.mtp,
        true,
        renamed,
    )
    .await;
    let create = workouts::save_workout(
        &app.pool,
        &app.settings,
        &app.connect,
        &app.mtp,
        false,
        workout("Push", vec![step(0, Intensity::ACTIVE, 60.0, 10)]),
    )
    .await;

    assert!(edit.is_err());
    assert!(create.is_err());
    assert_eq!(workout_id(&app, "Pull").await, pull);
}

#[tokio::test]
async fn workout_for_an_unplugged_device_is_left_pending() {
    let app = test_app().await;
    save_workout(
        &app,
        false,
        workout("Push", vec![step(0, Intensity::ACTIVE, 60.0, 10)]),
    )
    .await;
    app.pool
        .run_in_transaction(async move |tx| {
            DeviceRepository::insert()
                .item(&mut Device {
                    serial: "watch".to_string(),
                    model: "model".to_string(),
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

    let id = workout_id(&app, "Push").await;
    devices::send_to_device(&app.pool, &app.settings, &app.mtp, id, "watch")
        .await
        .unwrap();

    let entries = WorkoutDeviceRepository::select()
        .fetch(&app.pool)
        .await
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.pending));
    // Saved while logged out, so it is also left pending of upload to Connect
    assert!(entries.iter().any(|e| e.device == "watch"));
    assert!(
        entries
            .iter()
            .any(|e| e.device == constants::CONNECT_DEVICE_SERIAL)
    );

    let registered = devices::get_registered_devices(&app.pool).await.unwrap();
    assert_eq!(registered.len(), 1);
}

// Exercises

#[tokio::test]
async fn applying_a_progression_leaves_warmups_untouched() {
    let app = test_app().await;
    save_workout(
        &app,
        false,
        workout(
            "Push",
            vec![
                step(0, Intensity::WARMUP, 40.0, 15),
                step(1, Intensity::ACTIVE, 100.0, 8),
            ],
        ),
    )
    .await;

    exercises::apply_progression_suggestion(
        &app.pool,
        &app.settings,
        &app.connect,
        &app.mtp,
        BENCH.0,
        BENCH.1,
        ProgressionSuggestion {
            weight: 1020,
            reps: 8,
        },
    )
    .await
    .unwrap();

    let steps = workout_steps(&app, "Push").await;
    assert_eq!(steps[0].kind, StepType::Warmup);
    assert_eq!(steps[0].weight, Some(400));
    assert_eq!(steps[0].reps.flatten(), Some(15));
    assert_eq!(steps[1].weight, Some(1020));
}

#[tokio::test]
async fn applied_progression_skips_workouts_without_progression() {
    let app = test_app().await;
    save_workout(
        &app,
        false,
        workout("Push", vec![step(0, Intensity::ACTIVE, 100.0, 8)]),
    )
    .await;
    let mut fixed = workout("Fixed", vec![step(0, Intensity::ACTIVE, 100.0, 8)]);
    fixed.progression = false;
    save_workout(&app, false, fixed).await;

    exercises::apply_progression_suggestion(
        &app.pool,
        &app.settings,
        &app.connect,
        &app.mtp,
        BENCH.0,
        BENCH.1,
        ProgressionSuggestion {
            weight: 1020,
            reps: 8,
        },
    )
    .await
    .unwrap();

    assert_eq!(workout_steps(&app, "Push").await[0].weight, Some(1020));
    assert_eq!(workout_steps(&app, "Fixed").await[0].weight, Some(1000));
}

#[tokio::test]
async fn progression_status_ignores_workouts_without_progression() {
    let app = test_app().await;
    save_workout(
        &app,
        false,
        workout("Push", vec![step(0, Intensity::ACTIVE, 100.0, 8)]),
    )
    .await;
    let mut fixed = workout("Fixed", vec![step(0, Intensity::ACTIVE, 120.0, 10)]);
    fixed.progression = false;
    save_workout(&app, false, fixed).await;

    let status = exercises::get_progression_status(&app.pool, BENCH.0, BENCH.1)
        .await
        .unwrap();
    assert_eq!(
        status,
        Some(ProgressionSuggestion {
            weight: 1000,
            reps: 8,
        })
    );
}

#[tokio::test]
async fn workout_progression_flag_is_stored_and_kept_on_edit() {
    let app = test_app().await;
    save_workout(
        &app,
        false,
        workout("Push", vec![step(0, Intensity::ACTIVE, 100.0, 8)]),
    )
    .await;
    let id = workout_id(&app, "Push").await;
    let details = || async {
        workouts::get_workout_details(&app.pool, &app.settings, id)
            .await
            .unwrap()
    };
    assert!(details().await.progression);

    workouts::set_workout_progression(
        &app.pool,
        &app.settings,
        &app.connect,
        &app.mtp,
        &app.events,
        id,
        false,
    )
    .await
    .unwrap();
    assert!(!details().await.progression);
    // Nothing was trained, so no exercise has a progression pending
    assert!(matches!(
        app.emitted().as_slice(),
        [BackendEvent::ProgressionsAvailable(false)]
    ));

    let mut edited = workout("Push", vec![step(0, Intensity::ACTIVE, 105.0, 8)]);
    edited.id = id;
    edited.progression = false;
    save_workout(&app, true, edited).await;
    assert!(!details().await.progression);
}

#[tokio::test]
async fn progression_status_ignores_warmups() {
    let app = test_app().await;
    save_workout(
        &app,
        false,
        workout(
            "Push",
            vec![
                step(0, Intensity::WARMUP, 120.0, 15),
                step(1, Intensity::ACTIVE, 100.0, 8),
            ],
        ),
    )
    .await;

    let status = exercises::get_progression_status(&app.pool, BENCH.0, BENCH.1)
        .await
        .unwrap();
    assert_eq!(
        status,
        Some(ProgressionSuggestion {
            weight: 1000,
            reps: 8,
        })
    );
}

#[tokio::test]
async fn exercise_notes_are_trimmed_and_empty_ones_removed() {
    let app = test_app().await;
    let settings = &app.settings;
    let save = async |notes: &str| {
        exercises::save_exercise_notes(
            &app.pool,
            &app.settings,
            &app.connect,
            &app.mtp,
            BENCH.0,
            BENCH.1,
            notes,
        )
        .await
        .unwrap();
        exercises::get_exercise_details(&app.pool, settings, BENCH.0, BENCH.1)
            .await
            .unwrap()
            .notes
    };

    assert_eq!(save("  Slow  ").await.as_deref(), Some("Slow"));
    assert_eq!(save("   ").await, None);
}

#[tokio::test]
async fn catalog_lists_the_seeded_exercises() {
    let app = test_app().await;
    let catalog = exercises::get_exercises_catalog(&app.pool).await.unwrap();

    assert!(catalog[&BENCH.0].contains(&BENCH.1));
}

// Report

#[tokio::test]
async fn report_of_an_empty_range_is_empty() {
    let app = test_app().await;
    insert_activity(&app, 1_700_000_000, 10, 60.0).await;

    let data = report::get_report_data(&app.pool, &app.settings, 1, 2)
        .await
        .unwrap();

    assert!(data.activities.is_empty());
    assert!(data.exercises.is_empty());
    assert!(data.body_metrics.is_empty());
}

// Activities

#[tokio::test]
async fn activity_changes_are_saved_and_records_recomputed() {
    let app = test_app().await;
    let date = 1_700_000_000;
    insert_activity(&app, date, 10, 60.0).await;
    let settings = &app.settings;

    let listed = activities::get_activities(&app.pool, settings, None)
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);

    let details = activities::get_activity_details(&app.pool, settings, &app.connect, date)
        .await
        .unwrap();
    let mut sets: Vec<ActivityStepDto> = details.steps;
    sets[0].weight = Some(70.0);

    activities::save_activity_changes(
        &app.pool,
        settings,
        &app.connect,
        &app.mtp,
        &app.events,
        ActivityStepsUpdate {
            timestamp: date,
            sets,
            notes: Some("Felt strong".to_string()),
        },
    )
    .await
    .unwrap();

    let details = activities::get_activity_details(&app.pool, settings, &app.connect, date)
        .await
        .unwrap();
    assert_eq!(details.steps[0].weight, Some(70.0));
    assert_eq!(details.notes, "Felt strong");

    let records = exercises::get_exercises(&app.pool, settings).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].weight, 70.0);
}

#[tokio::test]
async fn heatmap_is_available_without_activities() {
    let app = test_app().await;

    assert!(activities::get_heatmap_data(&app.pool).await.is_ok());
}

// Workouts

#[tokio::test]
async fn workouts_are_saved_edited_and_toggled() {
    let app = test_app().await;
    let settings = &app.settings;
    save_workout(
        &app,
        false,
        workout("Push", vec![step(0, Intensity::ACTIVE, 60.0, 10)]),
    )
    .await;
    let id = workout_id(&app, "Push").await;
    let mut edited = workout(
        "Push",
        vec![
            step(0, Intensity::WARMUP, 20.0, 15),
            step(1, Intensity::ACTIVE, 60.0, 10),
        ],
    );
    edited.id = id;
    save_workout(&app, true, edited).await;
    assert_eq!(workout_steps(&app, "Push").await.len(), 2);
    assert_eq!(workout_id(&app, "Push").await, id);

    workouts::set_workout_status(&app.pool, &app.settings, &app.connect, &app.mtp, id, false)
        .await
        .unwrap();

    let listed = workouts::get_workout_list(&app.pool, settings)
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].enabled);
    assert!(listed[0].has_steps);
}

#[tokio::test]
async fn stored_activity_is_written_as_fit() {
    let app = test_app().await;
    let date = 1_700_000_000;
    insert_activity(&app, date, 10, 60.0).await;
    let path = std::env::temp_dir().join(format!(
        "gtrs-test-activity-download-{}.fit",
        std::process::id()
    ));

    activities::helpers::edit::write_activity_fit(
        &app.pool,
        date,
        path.clone(),
        Languages::English,
        WeightUnit::Kilograms,
    )
    .await
    .unwrap();
    let written = FitParser::from_file(&path).and_then(Activity::try_from);
    let _ = std::fs::remove_file(&path);
    let written = written.unwrap();

    assert_eq!(written.date, date);
    assert_eq!(written.steps.len(), 1);
    assert_eq!(written.steps[0].reps, Some(10));
    assert_eq!(written.steps[0].weight, Some(600));
    assert_eq!(written.steps[0].ex_cat, Some(BENCH.0));
    assert_eq!(written.steps[0].ex_id, Some(BENCH.1));
}

#[tokio::test]
async fn missing_activity_fails_to_write_as_fit() {
    let app = test_app().await;
    let path = std::env::temp_dir().join(format!(
        "gtrs-test-activity-download-missing-{}.fit",
        std::process::id()
    ));

    let res = activities::helpers::edit::write_activity_fit(
        &app.pool,
        1,
        path.clone(),
        Languages::English,
        WeightUnit::Kilograms,
    )
    .await;

    assert!(res.is_err());
    assert!(!path.exists());
}

#[test]
fn activity_file_name_is_date_then_name() {
    let date = chrono::TimeZone::with_ymd_and_hms(&chrono::Local, 2026, 11, 4, 10, 37, 58)
        .unwrap()
        .timestamp() as u32;

    assert_eq!(
        activities::helpers::edit::activity_file_name(date, "Progresion 1 - Pierna"),
        "2026-11-04 Progresion 1 - Pierna.fit"
    );
    assert_eq!(
        activities::helpers::edit::activity_file_name(date, "Push/Pull"),
        "2026-11-04 Push-Pull.fit"
    );
    assert_eq!(
        activities::helpers::edit::activity_file_name(date, "  "),
        "2026-11-04.fit"
    );
}

#[tokio::test]
async fn prompt_resolves_with_the_answer() {
    let app = test_app().await;
    let (prompts, events) = (app.prompts.clone(), app.events.clone());

    let pending = tauri::async_runtime::spawn(async move {
        prompts::helpers::requests::request_prompt(
            &prompts,
            &events,
            PromptKind::ConnectMfa,
            None,
            None,
        )
        .await
    });
    // The first prompt gets id 0, answered once the request has registered it
    while prompts::answer_prompt(&app.prompts, 0, Some(vec!["123456".into()])).is_err() {
        std::thread::yield_now();
    }

    assert_eq!(pending.await.unwrap(), Some(vec!["123456".to_string()]));
}

#[tokio::test]
async fn prompt_resolves_to_none_when_cancelled() {
    let app = test_app().await;
    let (prompts, events) = (app.prompts.clone(), app.events.clone());

    let pending = tauri::async_runtime::spawn(async move {
        prompts::helpers::requests::request_prompt(
            &prompts,
            &events,
            PromptKind::SudoPassword,
            None,
            None,
        )
        .await
    });
    while prompts::answer_prompt(&app.prompts, 0, None).is_err() {
        std::thread::yield_now();
    }

    assert_eq!(pending.await.unwrap(), None);
}

#[tokio::test]
async fn answering_an_unknown_prompt_fails() {
    let app = test_app().await;
    assert!(prompts::answer_prompt(&app.prompts, 42, None).is_err());
}
