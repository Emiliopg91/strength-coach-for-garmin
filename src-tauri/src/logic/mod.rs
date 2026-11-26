pub mod activities;
pub mod app;
pub mod body_metrics;
pub mod connect;
pub mod devices;
pub mod events;
pub mod exercises;
pub mod export;
pub mod notifications;
pub mod prompts;
pub mod report;
pub mod ui;
pub mod workouts;

use std::path::{Path, PathBuf};

use tauri_plugin_log::log::{error, warn};

use crate::{
    SettingsLock,
    dao::settings::WeightUnit,
    dto::notifications::{NotificationDefinition, NotificationKind},
    fit::FitFile,
    logic::notifications::show_notification,
    utils::translations::{Languages, translate},
};

/// Logs `e` (prefixed by `log_msg`), fires a persistent desktop notification titled by the
/// translation key `error_key`, and returns the stringified error for the command's
/// `Result<T, String>`.
pub fn report_error<E: std::fmt::Display>(
    e: E,
    settings: &SettingsLock,
    error_key: &str,
    log_msg: &str,
) -> String {
    error!("{}: {}", log_msg, e);
    show_notification(NotificationDefinition {
        title: translate(error_key, settings_language(settings)),
        body: e.to_string(),
        kind: NotificationKind::Persistant,
    });
    e.to_string()
}

/// Language configured in `settings`.
pub(crate) fn settings_language(settings: &SettingsLock) -> Languages {
    settings.read().unwrap().language
}

/// Language and weight unit configured in `settings`, as needed to write FIT files.
pub(crate) fn fit_settings(settings: &SettingsLock) -> (Languages, WeightUnit) {
    let settings = settings.read().unwrap();
    (settings.language, settings.weight_unit)
}

/// Adds `extension` to `path` unless it already ends in it.
pub(crate) fn ensure_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if path
        .extension()
        .is_none_or(|ext| !ext.eq_ignore_ascii_case(extension))
    {
        path.as_mut_os_string().push(format!(".{extension}"));
    }
    path
}

/// Stores under `root/<serial>` the fetched `files` whose parse or import failed, so they are kept
/// for a later look instead of being lost. A file with the same name is replaced.
pub(crate) async fn store_pending_files(root: &Path, serial: &str, files: &[&FitFile]) {
    let dir = root.join(serial);
    for file in files {
        // Names come from the device or a Connect archive, so only the last component is kept
        let Some(name) = Path::new(&file.name).file_name() else {
            error!("Invalid name for pending file '{}'", file.name);
            continue;
        };
        let dst = dir.join(name);
        let res = match tokio::fs::create_dir_all(&dir).await {
            Ok(()) => tokio::fs::write(&dst, &file.data).await,
            Err(e) => Err(e),
        };
        match res {
            Ok(()) => warn!("Stored pending file {}", dst.display()),
            Err(e) => error!("Error storing pending file {}: {e}", dst.display()),
        }
    }
}
