pub mod commands;

use std::{fs, io::BufWriter, path::PathBuf};

use chrono::Local;
use tauri::{AppHandle, Runtime};
use tauri_plugin_log::log::{info, warn};

use crate::{
    SettingsLock,
    dto::{
        export::Export,
        notifications::{NotificationDefinition, NotificationKind},
    },
    logic::{
        ask_save_path, notifications::show_notification, pool, report_error, settings_language,
    },
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

pub async fn upload_to_cloud<R: Runtime>(
    app: &AppHandle<R>,
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
            pool(app)
                .vacuum_into(&snapshot)
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

/// Asks where to save the whole database as a JSON file, writes it there and notifies on success/failure.
pub async fn export_database<R: Runtime>(
    app: &AppHandle<R>,
    settings: &SettingsLock,
) -> Result<(), String> {
    let default_file = constants::HOME_DIR.join(format!(
        "{}-{}.json",
        *constants::APP_NAME,
        Local::now().format("%Y-%m-%d-%H-%M-%S")
    ));
    let dialog_lang = settings_language(settings);
    let Some(path) = ask_save_path(
        app,
        &default_file,
        "JSON",
        "json",
        &translate("select_file_save_json", dialog_lang),
    )
    .await?
    else {
        return Ok(());
    };
    info!("Exporting database to {}...", path.display());

    let export_path = path.clone();
    let export_lang = settings_language(settings);
    let res: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
        let export = Export::from_database(&pool(app), export_lang).await?;
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
