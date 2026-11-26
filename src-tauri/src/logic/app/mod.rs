pub mod commands;
pub mod helpers;

use std::collections::HashMap;

use rusqlite_orm::database::DatabasePool;

use crate::{
    ConnectLock, MtpLock, SettingsLock, constants,
    dao::settings::settings_keys::{
        DISTANCE_UNIT, LANGUAGE, ON_DEVICE_CONNECT, START_ON_BOOT, WEIGHT_UNIT,
    },
    dto::{
        app::{AppEnvironment, Settings},
        notifications::{NotificationDefinition, NotificationKind},
    },
    logic::{
        activities::helpers::geolocation::spawn_geolocation_update,
        app::helpers::{udev::install_udev_rules, updates::check_for_update},
        connect::helpers::{
            device_sync::get_devices_from_connect, workout_sync::up_to_date_connect,
        },
        devices::helpers::watcher::start_device_watcher,
        events::EventSink,
        notifications::show_notification,
        prompts::helpers::requests::Prompts,
        report_error, settings_language,
    },
    utils::translations::{Languages, TRANSLATIONS, translate},
};

/// Returns the current in-memory app settings.
pub async fn get_settings(settings: &SettingsLock) -> Result<Settings, String> {
    Ok(settings.read().unwrap().clone())
}

/// Startup work that needs the frontend: installs the udev rules if missing (the password dialog
/// lives in the frontend), syncs the Connect devices, starts the USB device watcher and retries the
/// pending Connect uploads. Fails only if the udev rules could not be installed, before anything
/// else is done.
pub async fn startup_sync(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    prompts: &Prompts,
    events: &EventSink,
) -> Result<(), String> {
    let auto_run = settings.read().unwrap().on_device_connect;
    if !std::fs::exists(constants::RULE_FILE).unwrap_or(false) {
        install_udev_rules(prompts, events, auto_run).await?;
    }

    get_devices_from_connect(pool, connect, events).await;
    start_device_watcher(pool, settings, connect, mtp, events);
    up_to_date_connect(pool, settings, connect).await;
    Ok(())
}

/// Starts the background work that doesn't need the frontend: geocoding the pending activities and
/// the hourly update check.
pub fn start_background_tasks(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    events: &EventSink,
) {
    spawn_geolocation_update(pool, connect, events);

    let settings = settings.clone();
    tauri::async_runtime::spawn(async move { check_for_update(&settings).await });
}

/// Reports whether this is a debug or release build.
pub async fn get_environment() -> AppEnvironment {
    if cfg!(debug_assertions) {
        AppEnvironment::Debug
    } else {
        AppEnvironment::Release
    }
}

/// Updates a single named setting: persists it to the DB, applies its side effect (`set_autostart`
/// with the new flag for `START_ON_BOOT`, the udev rules for `ON_DEVICE_CONNECT`) and refreshes the
/// in-memory settings.
pub async fn update_settings_value(
    pool: &DatabasePool,
    settings: &SettingsLock,
    prompts: &Prompts,
    events: &EventSink,
    name: &str,
    value: &str,
    set_autostart: impl FnOnce(bool) -> Result<(), String>,
) -> Result<(), String> {
    let name = name.to_string();
    let value = value.to_string();

    // Installed before persisting, so a cancelled password prompt leaves the setting untouched
    if name == ON_DEVICE_CONNECT
        && let Err(e) = install_udev_rules(prompts, events, value == "true").await
    {
        return Err(report_error(
            e,
            settings,
            "error_update_settings",
            "Error updating settings",
        ));
    }

    let res: Result<Languages, String> = async {
        let flag = value == "true";
        let mut updated = settings.read().unwrap().clone();
        match name.as_str() {
            DISTANCE_UNIT => updated.distance_unit = value.as_str().try_into()?,
            WEIGHT_UNIT => updated.weight_unit = value.as_str().try_into()?,
            LANGUAGE => updated.language = Languages::from_name(&value),
            START_ON_BOOT => updated.start_boot = flag,
            ON_DEVICE_CONNECT => updated.on_device_connect = flag,
            _ => unreachable!(),
        }
        updated.save(pool).await.map_err(|e| e.to_string())?;

        if name == START_ON_BOOT {
            set_autostart(flag)?;
        }

        let lang = updated.language;
        *settings.write().unwrap() = updated;
        Ok(lang)
    }
    .await;

    match res {
        Ok(lang) => {
            show_notification(NotificationDefinition {
                title: translate("ok_update_settings", lang),
                body: String::new(),
                kind: NotificationKind::Temporal,
            });
            Ok(())
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_update_settings",
            "Error updating settings",
        )),
    }
}

/// Returns every translation key resolved to the current UI language, for the frontend's i18n bootstrap.
pub async fn get_translations(settings: &SettingsLock) -> Result<HashMap<String, String>, String> {
    let lang = settings_language(settings);
    Ok(TRANSLATIONS
        .keys()
        .map(|k| (k.to_string(), translate(k, lang)))
        .collect())
}
