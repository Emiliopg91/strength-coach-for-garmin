use std::collections::HashMap;

use rusqlite_orm::database::DatabasePool;
use strength_coach_for_garmin_macros::traced_command;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_log::log::{error, info};

use crate::{
    ConnectLock, MtpLock, SettingsLock, constants,
    dto::app::{AppEnvironment, Settings},
    logic::{
        app,
        prompts::helpers::requests::Prompts,
        ui::{event_sink, show_main_window},
    },
};

/// Current in-memory settings (units, language and startup behaviour).
#[traced_command]
#[tauri::command]
pub async fn get_settings(settings: State<'_, SettingsLock>) -> Result<Settings, String> {
    app::get_settings(&settings).await
}

/// Signals that the main window has mounted. Shows it and, in the background: installs the udev rules if missing (exits the app if that fails), syncs the
/// Connect devices, starts the USB device watcher, sends pending workouts to Connect, geocodes
/// pending activities and checks for updates.
#[traced_command]
#[tauri::command]
pub async fn notify_frontend_ready(
    app: AppHandle,
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    connect: State<'_, ConnectLock>,
    mtp: State<'_, MtpLock>,
    prompts: State<'_, Prompts>,
) -> Result<(), String> {
    info!("UI ready");
    let events = event_sink(&app);

    {
        let app = app.clone();
        let (pool, settings, connect, mtp, prompts, events) = (
            pool.inner().clone(),
            settings.inner().clone(),
            connect.inner().clone(),
            mtp.inner().clone(),
            prompts.inner().clone(),
            events.clone(),
        );
        tauri::async_runtime::spawn(async move {
            if let Err(e) =
                app::startup_sync(&pool, &settings, &connect, &mtp, &prompts, &events).await
            {
                error!("Error installing udev rules: {e}");
                app.exit(constants::ExitCodes::UdevError.into());
            }
        });
    }

    let _ = show_main_window(&app);

    app::start_background_tasks(&pool, &settings, &connect, &events);
    Ok(())
}

/// Whether this is a debug or a release build.
#[traced_command]
#[tauri::command]
pub async fn get_environment() -> AppEnvironment {
    app::get_environment().await
}

/// Stores the setting `name` (a `settings_keys` constant) as `value`, given as text (`"true"` /
/// `"false"` for flags, the enum name for units and language), and applies its side effect:
/// autostart for `START_ON_BOOT`, udev rules for `ON_DEVICE_CONNECT` (asked for before storing, so
/// a cancelled password prompt leaves the setting untouched). Notifies on success. On failure,
/// shows a persistent error notification and rejects with the message.
#[traced_command]
#[tauri::command]
pub async fn update_settings_value(
    app: AppHandle,
    pool: State<'_, DatabasePool>,
    settings: State<'_, SettingsLock>,
    prompts: State<'_, Prompts>,
    name: &str,
    value: &str,
) -> Result<(), String> {
    let set_autostart = |enable: bool| {
        if enable {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        }
        .map_err(|e| e.to_string())
    };
    app::update_settings_value(
        &pool,
        &settings,
        &prompts,
        &event_sink(&app),
        name,
        value,
        set_autostart,
    )
    .await
}

/// Every translation key resolved to the current language, to bootstrap the frontend i18n.
#[traced_command(log_payload = false)]
#[tauri::command]
pub async fn get_translations(
    settings: State<'_, SettingsLock>,
) -> Result<HashMap<String, String>, String> {
    app::get_translations(&settings).await
}
