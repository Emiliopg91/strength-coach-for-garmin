use tauri_plugin_log::log::{info, warn};

use crate::{
    dto::prompts::PromptKind,
    logic::{
        events::EventSink,
        prompts::helpers::requests::{Prompts, request_prompt},
    },
    udev::{UdevManager, errors::UdevError},
};

/// Writes the udev rules, asking for the sudo password through the frontend unless already root.
/// A rejected password is asked again; fails if the user cancels.
pub(in crate::logic::app) async fn install_udev_rules(
    prompts: &Prompts,
    events: &EventSink,
    auto_run: bool,
) -> Result<(), String> {
    info!("Installing udev rules...");
    if UdevManager::is_root() {
        return tokio::task::spawn_blocking(move || UdevManager::write_rules_file(auto_run))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string());
    }

    let mut detail = None;
    loop {
        let Some(password) = request_prompt(
            prompts,
            events,
            PromptKind::SudoPassword,
            None,
            detail.take(),
        )
        .await
        .and_then(|values| values.into_iter().next()) else {
            return Err("Udev rules installation cancelled by the user".to_string());
        };

        let res = tokio::task::spawn_blocking(move || {
            UdevManager::write_rules_file_with_sudo(auto_run, &password)
        })
        .await
        .map_err(|e| e.to_string())?;

        match res {
            Ok(()) => {
                info!("Udev rules installed");
                return Ok(());
            }
            Err(e @ UdevError::WrongPassword) => {
                warn!("Udev rules installation failed: {e}");
                detail = Some(e.to_string());
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}
