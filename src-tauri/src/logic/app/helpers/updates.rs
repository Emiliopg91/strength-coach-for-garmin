use std::time::Duration;

use semver::Version;
use serde_json::Value;
use tauri_plugin_log::log::{debug, info};

use crate::{
    SettingsLock, constants,
    dto::notifications::{NotificationDefinition, NotificationKind},
    logic::{notifications::show_notification, settings_language},
    utils::{
        http,
        translations::{translate, translate_and_replace},
    },
};

/// Looks for a newer release every hour, until one is found and notified.
pub(in crate::logic::app) async fn check_for_update(settings: &SettingsLock) {
    tokio::time::sleep(Duration::from_secs(5)).await;

    loop {
        info!("Looking for updates...");
        match latest_version().await {
            Ok(version) if version > *constants::APP_SEM_VERSION => {
                info!("Update {} found!", version);
                let lang = settings_language(settings);
                show_notification(NotificationDefinition {
                    title: translate("new_update_title", lang),
                    body: translate_and_replace("new_update_body", &[&version.to_string()], lang),
                    kind: NotificationKind::Persistant,
                });
                break;
            }
            Ok(_) => info!("No update found"),
            Err(e) => debug!("{e}"),
        }
        tokio::time::sleep(Duration::from_hours(1)).await;
    }
}

/// Latest version published on GitHub.
async fn latest_version() -> Result<Version, String> {
    let response = http::client()
        .get(constants::URL)
        .header(reqwest::header::USER_AGENT, "strength-coach-for-garmin")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Error sending request to GitHub: {e}"))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("Invalid response status: {}", response.status()));
    }

    let json: Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid parsing response: {e}"))?;
    let tag_name = json
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or("No tag name found")?;
    Version::parse(tag_name).map_err(|e| format!("Error parsing version: {e}"))
}
