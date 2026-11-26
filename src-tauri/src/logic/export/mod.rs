pub mod commands;

use std::{
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
};

use chrono::Local;
use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::{info, warn};

use crate::{
    SettingsLock,
    dto::{
        export::Export,
        notifications::{NotificationDefinition, NotificationKind},
    },
    logic::{notifications::show_notification, report_error, settings_language},
    rclone::{self, errors::RCloneError, providers::CloudProvider},
    utils::{
        constants,
        translations::{translate, translate_and_replace},
    },
};

/// Whether uploading to the cloud is possible, i.e. the optional rclone dependency is installed.
pub async fn is_cloud_upload_available() -> bool {
    rclone::is_installed()
}

pub async fn upload_to_cloud(
    pool: &DatabasePool,
    settings: &SettingsLock,
    provider: CloudProvider,
) -> Result<(), String> {
    let lang = settings_language(settings);

    let res: Result<(), String> = async {
        if !rclone::is_installed() {
            return Err(RCloneError::NotInstalled.to_string());
        }

        let configured = provider.is_configured().await.map_err(|e| e.to_string())?;

        if !configured {
            provider.configure().await.map_err(|e| e.to_string())?;
        }

        // The database file alone may lack the commits still in the WAL, and could change while
        // being uploaded: a snapshot of it is uploaded instead, under the same file name
        let dir = snapshot_dir();
        let res = async {
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let snapshot = dir.join(constants::DB_FILE.file_name().unwrap());
            info!("Writing database snapshot to {}", snapshot.display());
            pool.vacuum_into(&snapshot)
                .await
                .map_err(|e| e.to_string())?;
            provider.upload(snapshot).await.map_err(|e| e.to_string())
        }
        .await;
        if let Err(e) = fs::remove_dir_all(&dir) {
            warn!("Could not remove database snapshot {}: {e}", dir.display());
        }
        res
    }
    .await;

    res.inspect(|_| {
        info!("File uploaded succesfully");
        show_notification(NotificationDefinition {
            title: translate("upload_succesful", lang),
            body: String::new(),
            kind: NotificationKind::Temporal,
        });
    })
    .map_err(|e| report_error(e, settings, "upload_error", "Error uploading file"))
}

/// Folder in `/tmp` for the database snapshot of a cloud upload, unique to this process and moment.
fn snapshot_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "{}-{}-{}",
        *constants::APP_NAME,
        std::process::id(),
        Local::now().format("%Y%m%d%H%M%S%3f")
    ))
}

/// Where the database export is offered to be saved: a timestamped file in the home dir.
pub fn default_export_file() -> PathBuf {
    constants::HOME_DIR.join(format!(
        "{}-{}.json",
        *constants::APP_NAME,
        Local::now().format("%Y-%m-%d-%H-%M-%S")
    ))
}

/// Writes the whole database as a JSON file at `path` and notifies on success/failure.
pub async fn export_database(
    pool: &DatabasePool,
    settings: &SettingsLock,
    path: &Path,
) -> Result<(), String> {
    info!("Exporting database to {}...", path.display());

    let export_path = path.to_path_buf();
    let export_lang = settings_language(settings);
    let res: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
        let export = Export::from_database(pool, export_lang).await?;
        tokio::task::spawn_blocking(move || {
            let file = fs::File::create(&export_path)?;
            export.write_json(BufWriter::new(file))?;
            Ok(())
        })
        .await
        .expect("export writing task panicked")
    }
    .await;

    let lang = settings_language(settings);
    match res {
        Ok(()) => {
            show_notification(NotificationDefinition {
                title: translate("ok_on_export", lang),
                body: translate_and_replace(
                    "export_file_path",
                    &[&path.display().to_string()],
                    lang,
                ),
                kind: NotificationKind::Temporal,
            });
            Ok(())
        }
        Err(e) => Err(report_error(
            e,
            settings,
            "error_on_export",
            "Error exporting database",
        )),
    }
}
