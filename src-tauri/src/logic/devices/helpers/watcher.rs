use std::collections::HashMap;

use futures_util::StreamExt;
use mtp_rs::{MtpDevice, mtp::HotplugEvent};
use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::{error, info};

use crate::{
    ConnectLock, MtpLock, SettingsLock,
    dao::device::Device,
    dto::{
        devices::DeviceListItem,
        notifications::{NotificationDefinition, NotificationKind},
    },
    logic::{
        devices::import_from_device,
        events::{BackendEvent, EventSink},
        notifications::show_notification,
        settings_language,
    },
    utils::{
        constants,
        translations::{translate, translate_and_replace},
    },
};

use crate::logic::devices::helpers::{
    queue::{delete_pending_workouts, download_pending_workouts, push_pending_workouts},
    schedule::send_schedule_to_device,
};

/// Spawns a background task that watches USB hotplug events and keeps the device list/DB/frontend in sync.
pub(crate) fn start_device_watcher(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    events: &EventSink,
) {
    info!("Starting device monitor...");
    let (pool, settings, connect, mtp, events) = (
        pool.clone(),
        settings.clone(),
        connect.clone(),
        mtp.clone(),
        events.clone(),
    );
    tauri::async_runtime::spawn(async move {
        match mtp_rs::mtp::watch_devices() {
            Ok(w) => {
                let mut devices: HashMap<String, Device> = HashMap::new();
                let mut locations: HashMap<u64, String> = HashMap::new();

                let mut watch = w;
                while let Some(event) = watch.next().await {
                    match event {
                        HotplugEvent::Arrived(info) => {
                            if info.vendor_id != constants::MTP_GARMIN_VENDOR_ID {
                                continue;
                            }

                            if let Ok(device) = MtpDevice::open_by_location(info.location_id).await
                            {
                                let parsed_device = Device::try_from(device.device_info());
                                let _ = device.close().await;

                                if let Ok(mut device) = parsed_device {
                                    locations.insert(info.location_id, device.serial.clone());
                                    on_device_connected(
                                        &pool,
                                        &settings,
                                        &connect,
                                        &mtp,
                                        &events,
                                        &mut device,
                                        &mut devices,
                                    )
                                    .await
                                }
                            }
                        }
                        HotplugEvent::Left(info) => {
                            if let Some(serial) = locations.remove(&info.location_id) {
                                on_device_disconnected(&settings, &events, &serial, &mut devices)
                                    .await
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Could not initialize device monitor: {e}");
            }
        };
    });
}

pub(super) async fn on_device_disconnected(
    settings: &SettingsLock,
    events: &EventSink,
    serial: &str,
    devices: &mut HashMap<String, Device>,
) {
    if let Some(device) = devices.remove(serial) {
        events.emit(BackendEvent::DeviceDisconnected(DeviceListItem::from(
            &device,
        )));

        let lang = settings_language(settings);

        info!("Disconnected Garmin {} ({})", device.model, device.serial);
        show_notification(NotificationDefinition {
            title: translate("device_disconnected", lang),
            body: format!("Garmin {}", device.model),
            kind: NotificationKind::Temporal,
        });
    }
}

pub(super) async fn on_device_connected(
    pool: &DatabasePool,
    settings: &SettingsLock,
    connect: &ConnectLock,
    mtp: &MtpLock,
    events: &EventSink,
    device: &mut Device,
    devices: &mut HashMap<String, Device>,
) {
    // The initial scan and the hotplug watcher can both report the same device.
    if devices.contains_key(&device.serial) {
        return;
    }

    let lang = settings_language(settings);
    let mut enrolled = device.clone();
    let enrol_res = pool
        .run_in_transaction(async move |tx| {
            Device::enrol_usb(tx, &mut enrolled).await?;
            Ok(enrolled)
        })
        .await;
    match enrol_res {
        Ok(enrolled) => *device = enrolled,
        Err(e) => {
            error!(
                "Error enrolling Garmin {} ({}): {}",
                device.model, device.serial, e
            );
            return;
        }
    }

    info!("Connected Garmin {} ({})", device.model, device.serial);
    devices.insert(device.serial.clone(), device.clone());

    show_notification(NotificationDefinition {
        title: translate("device_connected", lang),
        body: translate_and_replace("syncing_device", &["Garmin", &device.model], lang),
        kind: NotificationKind::Temporal,
    });

    events.emit(BackendEvent::DeviceConnected(DeviceListItem::from(
        &*device,
    )));

    let serial = &device.serial;
    with_loading(events, async {
        let imported = import_from_device(pool, settings, connect, mtp, events, serial)
            .await
            .unwrap_or(0);

        download_pending_workouts(pool, mtp, events, serial).await;
        delete_pending_workouts(pool, settings, mtp, serial).await;
        push_pending_workouts(pool, settings, mtp, serial).await;
        // After the workouts, so the device already has the ones the schedule points to
        send_schedule_to_device(pool, settings, mtp, serial, false).await;

        imported
    })
    .await;
}

/// Runs a sync between `StartLoading` and `FinishLoading`, then emits `ActivitiesAdded` if it
/// imported any activity.
pub(super) async fn with_loading(events: &EventSink, sync: impl Future<Output = usize>) {
    events.emit(BackendEvent::StartLoading);
    let imported = sync.await;
    events.emit(BackendEvent::FinishLoading);
    if imported > 0 {
        events.emit(BackendEvent::ActivitiesAdded);
    }
}
