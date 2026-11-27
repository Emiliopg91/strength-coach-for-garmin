mod connect;
mod dao;
#[cfg(debug_assertions)]
pub mod debug;
mod dto;
mod fit;
mod logic;
mod mtp;
mod udev;
mod utils;

#[cfg(test)]
mod tests;

use std::{
    process::exit,
    sync::{Arc, RwLock},
    time::Duration,
};

use rusqlite_orm::{
    database::{
        DatabasePool, DdlVersion,
        builder::{DatabaseConnectionBuilder, JournalMode, Synchronous},
    },
    ddls,
};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_log::{
    Target, TargetKind,
    log::{LevelFilter, debug, error, info, warn},
};
use tokio::signal::unix::{SignalKind, signal};

use crate::{
    connect::ConnectClient,
    dao::{
        activity_step::migration_42,
        body_metric::migration_21,
        settings::{self, settings_keys},
        workout_device::WorkoutDevice,
        workout_step::WorkoutStep,
    },
    dto::app::Settings,
    logic::{
        activities::commands::{
            download_activity, get_activities, get_activity_details, get_heatmap_data,
            get_muscle_sets, import_dropped_files, import_from_files, save_activity_changes,
            set_activity_feeling, set_activity_rpe,
        },
        app::commands::{
            get_environment, get_settings, get_translations, notify_frontend_ready,
            update_settings_value,
        },
        body_metrics::commands::{add_body_measures, delete_body_metric, get_body_measures},
        connect::commands::{
            connect_logged_user, connect_login, connect_logout, import_from_connect,
            import_weight_from_connect, send_to_connect,
        },
        devices::commands::{get_registered_devices, import_from_device, send_to_device},
        events::{BackendEvent, EventSink},
        exercises::commands::{
            apply_progression_suggestion, available_any_progression_suggestions,
            dismiss_progression_suggestion, get_exercise_details, get_exercises,
            get_exercises_catalog, get_progression_status, get_progression_suggestion,
            save_exercise_notes, set_exercise_progression, set_exercise_progression_parameters,
            set_exercise_unilateral,
        },
        prompts::{
            commands::answer_prompt,
            helpers::requests::{AppPrompter, Prompts},
        },
        report::commands::{get_report_data, load_report, report_ready},
        ui::event_sink,
        workouts::commands::{
            add_planned_workout, delete_planned_workout, get_next_planned_workout,
            get_percent_load_bases, get_planned_workouts, get_workout_details, get_workout_list,
            load_workout_page, save_workout, set_workout_progression, set_workout_status,
            workout_print_ready,
        },
    },
    mtp::MtpClient,
    udev::UdevManager,
    utils::{constants, single_instance::SingleInstance},
};

#[cfg(debug_assertions)]
use crate::logic::exercises::helpers::records::update_all_prs;

pub fn check_running() -> bool {
    SingleInstance::is_app_running().0
}

pub fn force_write_mtp_rules(auto_run: bool) -> udev::errors::Result<()> {
    UdevManager::write_rules_file(auto_run)
}

/// Shared handles to the app state, cloned into the background tasks the logic spawns.
pub type SettingsLock = Arc<RwLock<Settings>>;
pub type ConnectLock = Arc<tokio::sync::Mutex<ConnectClient>>;
pub type MtpLock = Arc<tokio::sync::Mutex<MtpClient>>;

/// Opens and migrates the database and loads the settings and the Connect session. Connect sends
/// its prompts and login changes to the frontend through `events`.
fn initialize(events: &EventSink, prompts: &Prompts) -> (DatabasePool, Settings, ConnectClient) {
    debug!("Initializing database...");
    let builder = DatabaseConnectionBuilder::default()
        .location(constants::DB_FILE.clone())
        .busy_timeout(Duration::from_secs(8))
        .connection_timeout(Duration::from_secs(5))
        .pool_size(5)
        .min_idle(2)
        .enable_foreign_keys()
        // Commits don't wait for the disk; a power loss may lose the latest ones, never corrupt
        .journal_mode(JournalMode::Wal)
        .synchronous(Synchronous::Normal);
    match builder.build("gtrs") {
        Ok(database) => {
            // `setup` is synchronous and runs outside the async runtime, so it can wait here
            let settings = tauri::async_runtime::block_on(async {
                if let Err(e) = database.create_schema(&schema_ddls()).await {
                    error!("Could not initialize database: {}", e);
                    exit(constants::ExitCodes::DbError.into())
                }
                // Keeps the planner statistics current, analyzing only the tables that need it
                if let Err(e) = database
                    .run_in_connection(async |conn| Ok(conn.execute_batch("PRAGMA optimize;")?))
                    .await
                {
                    warn!("Could not optimize database: {}", e);
                }

                debug!("Loading settings...");
                let settings = Settings::load(&database).await;

                settings::Settings::set(
                    &database,
                    settings_keys::VERSION,
                    &*constants::APP_SEM_VERSION,
                )
                .await
                .unwrap();

                // Sets leave the 1RM window as days go by: the queued workouts reach USB devices
                // on their next push and Connect with the pending uploads retried once logged in
                if let Err(e) = database
                    .run_in_transaction(async |tx| {
                        let changed =
                            WorkoutStep::refresh_percent_loads(tx, None, utils::time::now())
                                .await?;
                        if !changed.is_empty() {
                            WorkoutDevice::set_connect_pending(tx, &changed, true).await?;
                        }
                        Ok(())
                    })
                    .await
                {
                    warn!("Could not refresh percentage loads: {}", e);
                }

                #[cfg(debug_assertions)]
                let _ = database
                    .run_in_transaction(async |tx| Ok(update_all_prs(tx).await?))
                    .await;

                settings
            });

            let connect = {
                let prompter = AppPrompter::new(prompts.clone(), events.clone());
                let events = events.clone();
                ConnectClient::new(
                    move |user| events.emit(BackendEvent::ConnectLogged(user)),
                    prompter,
                )
            };
            (database, settings, connect)
        }
        Err(e) => {
            error!("Could not open database: {}", e);
            exit(constants::ExitCodes::DbError.into())
        }
    }
}

/// Schema DDLs sorted by version, with the data update hooks attached to their migration.
fn schema_ddls() -> Vec<DdlVersion> {
    let mut ddls = ddls!("../resources/ddl").to_vec();
    ddls.sort_by_key(|ddl| ddl.version);

    for ddl in &mut ddls {
        ddl.update_fn = match ddl.version {
            21 => Some(|tx| Box::pin(migration_21(tx))),
            42 => Some(|tx| Box::pin(migration_42(tx))),
            _ => None,
        };
    }

    ddls
}

/// Turns SIGTERM/SIGINT/SIGHUP into a regular `app.exit(0)` so they are logged instead of killing the process.
fn listen_for_signals(app: &AppHandle) {
    for (kind, name) in [
        (SignalKind::terminate(), "SIGTERM"),
        (SignalKind::interrupt(), "SIGINT"),
        (SignalKind::hangup(), "SIGHUP"),
    ] {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            match signal(kind) {
                Ok(mut stream) => {
                    stream.recv().await;
                    info!("Received {}, shutting down...", name);
                    app.exit(0);
                }
                Err(e) => error!("Could not listen for {}: {}", name, e),
            }
        });
    }
}

/// Boots the Tauri app: acquires the single-instance lock, opens/migrates the DB, loads settings, and registers commands.
pub fn run(log_level: LevelFilter) {
    SingleInstance::acquire();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(LevelFilter::Warn)
                .level_for(constants::LIB_NAME.clone(), log_level)
                .level_for("command", log_level)
                .level_for("rusqlite_orm", log_level)
                .clear_targets()
                .target(Target::new(TargetKind::Folder {
                    path: constants::LOGS_DIR.clone(),
                    file_name: None,
                }))
                .target(
                    Target::new(TargetKind::Stdout).format(|out, message, _record| {
                        let message = message.to_string();
                        let char_count = message.chars().count();
                        let message = if char_count > 1000 {
                            let truncated: String = message.chars().take(1000).collect();
                            format!("{}... and {} more", truncated, char_count - 1000)
                        } else {
                            message
                        };

                        out.finish(format_args!("{}", message))
                    }),
                )
                .max_file_size(constants::LOG_FILE_MAX_SIZE)
                .rotation_strategy(constants::LOG_FILE_ROTATION_STRATEGY.clone())
                .format(|out, message, record| {
                    let mut target = record.target();
                    target = if target.len() > 30 {
                        &target[target.len() - 30..]
                    } else {
                        target
                    };

                    out.finish(format_args!(
                        "[{}][{:<30}][{:<5.5}] {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                        target,
                        record.level().to_string(),
                        message
                    ))
                })
                .build(),
        )
        .setup(move |app| {
            info!(
                "Starting {} v{} with PID {}",
                *constants::APP_NAME,
                *constants::APP_VERSION,
                *constants::PID
            );

            let events = event_sink(app.handle());
            let prompts = Prompts::default();
            let (database, settings, connect) = initialize(&events, &prompts);

            listen_for_signals(app.handle());

            let app_handle = app.handle().clone();
            let window = app_handle.get_webview_window("main");
            if let Some(window) = window {
                window.on_window_event(move |event| {
                    if let WindowEvent::DragDrop(event) = event
                        && let tauri::DragDropEvent::Drop { paths, position: _ } = event
                    {
                        let paths: Vec<_> = paths
                            .iter()
                            .filter(|p| p.display().to_string().to_lowercase().ends_with(".fit"))
                            .cloned()
                            .collect();

                        info!("Dropped {} .fit files: {:?}", paths.len(), paths);

                        let app_handle = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = import_dropped_files(&app_handle, paths).await;
                        });
                    }
                });
            }

            app.manage(prompts);
            app.manage(database);
            app.manage(SettingsLock::new(RwLock::new(settings)));
            app.manage(ConnectLock::new(tokio::sync::Mutex::new(connect)));
            app.manage(MtpLock::new(tokio::sync::Mutex::new(MtpClient {})));

            debug!("Setup finished");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            //Prompts
            answer_prompt,
            //Activities
            get_activities,
            get_activity_details,
            download_activity,
            save_activity_changes,
            set_activity_rpe,
            set_activity_feeling,
            import_from_files,
            get_heatmap_data,
            get_muscle_sets,
            //Exercises
            get_exercises,
            get_exercise_details,
            get_exercises_catalog,
            set_exercise_unilateral,
            set_exercise_progression,
            get_progression_suggestion,
            apply_progression_suggestion,
            dismiss_progression_suggestion,
            save_exercise_notes,
            set_exercise_progression_parameters,
            get_progression_status,
            available_any_progression_suggestions,
            //Workouts
            get_workout_list,
            get_workout_details,
            set_workout_status,
            set_workout_progression,
            save_workout,
            load_workout_page,
            workout_print_ready,
            get_planned_workouts,
            get_percent_load_bases,
            delete_planned_workout,
            add_planned_workout,
            get_next_planned_workout,
            //Devices
            import_from_device,
            send_to_device,
            get_registered_devices,
            //App
            notify_frontend_ready,
            get_report_data,
            get_environment,
            get_settings,
            update_settings_value,
            get_translations,
            //Body Metrics
            get_body_measures,
            add_body_measures,
            delete_body_metric,
            //Report
            report_ready,
            load_report,
            //Connect
            import_from_connect,
            import_weight_from_connect,
            connect_logged_user,
            connect_logout,
            connect_login,
            send_to_connect,
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("Error while running tauri application {}", e);
            exit(constants::ExitCodes::TauriError.into())
        })
        .run(|app, event| {
            if let RunEvent::Exit = event {
                // Tauri ends the process without dropping the managed state: closing the pool lets
                // SQLite checkpoint the WAL and remove the -wal and -shm files
                tauri::async_runtime::block_on(app.state::<DatabasePool>().close());
                info!(
                    "Stopped {} v{}",
                    *constants::APP_NAME,
                    *constants::APP_VERSION
                )
            }
        });
}
