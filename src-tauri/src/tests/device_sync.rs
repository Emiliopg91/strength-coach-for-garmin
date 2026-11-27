use rusqlite_orm::{dao::Repository, database::DatabasePool};

use crate::{
    dao::device::{Device, DeviceRepository},
    tests::common,
};

fn device(serial: &str, model: &str, connect: bool) -> Device {
    Device {
        serial: serial.to_string(),
        model: model.to_string(),
        name: model.to_string(),
        last_activity_sync: None,
        last_body_metric_sync: None,
        usb: false,
        connect,
        strength_workout: true,
    }
}

async fn stored(db: &DatabasePool, serial: &str) -> Option<(String, bool)> {
    DeviceRepository::select_by_id(db, serial)
        .await
        .unwrap()
        .map(|d| (d.model, d.connect))
}

#[tokio::test]
async fn sync_connect_adds_updates_and_unflags_devices() {
    let db = common::test_db().await;
    db.run_in_transaction(async |tx| {
        for mut dao in [
            device("unchanged", "Watch", true),
            device("renamed", "Old", true),
            device("usb_only", "Watch", false),
            device("gone", "Watch", true),
        ] {
            DeviceRepository::insert()
                .item(&mut dao)
                .execute_in(tx)
                .await?;
        }
        Ok(())
    })
    .await
    .unwrap();

    let listed = vec![
        device("unchanged", "Watch", true),
        device("renamed", "New", true),
        device("usb_only", "Watch", true),
        device("new", "Watch", true),
    ];
    let discovered = db
        .run_in_transaction(async move |tx| Ok(Device::sync_connect(tx, &listed).await?))
        .await
        .unwrap();

    assert_eq!(discovered.len(), 4);
    assert_eq!(stored(&db, "unchanged").await, Some(("Watch".into(), true)));
    assert_eq!(stored(&db, "renamed").await, Some(("New".into(), true)));
    assert_eq!(stored(&db, "usb_only").await, Some(("Watch".into(), true)));
    assert_eq!(stored(&db, "new").await, Some(("Watch".into(), true)));
    assert_eq!(stored(&db, "gone").await, Some(("Watch".into(), false)));
}

#[tokio::test]
async fn sync_connect_with_nothing_listed_unflags_every_device() {
    let db = common::test_db().await;
    db.run_in_transaction(async |tx| {
        let mut dao = device("gone", "Watch", true);
        DeviceRepository::insert()
            .item(&mut dao)
            .execute_in(tx)
            .await?;
        Ok(())
    })
    .await
    .unwrap();

    let discovered = db
        .run_in_transaction(async |tx| Ok(Device::sync_connect(tx, &[]).await?))
        .await
        .unwrap();

    assert!(discovered.is_empty());
    assert_eq!(stored(&db, "gone").await, Some(("Watch".into(), false)));
}

async fn stored_name(db: &DatabasePool, serial: &str) -> Option<String> {
    DeviceRepository::select_by_id(db, serial)
        .await
        .unwrap()
        .map(|d| d.name)
}

async fn insert(db: &DatabasePool, mut dao: Device) {
    db.run_in_transaction(async move |tx| {
        DeviceRepository::insert()
            .item(&mut dao)
            .execute_in(tx)
            .await?;
        Ok(())
    })
    .await
    .unwrap();
}

async fn rename(db: &DatabasePool, serial: &'static str, name: &'static str) -> Option<String> {
    db.run_in_transaction(async move |tx| Ok(Device::set_name(tx, serial, name).await?))
        .await
        .unwrap()
}

#[tokio::test]
async fn set_name_trims_the_name_and_gives_back_the_model_when_blank() {
    let db = common::test_db().await;
    insert(&db, device("watch", "Forerunner 265", false)).await;

    assert_eq!(
        rename(&db, "watch", "  Gym watch ").await.as_deref(),
        Some("Gym watch")
    );
    assert_eq!(
        stored_name(&db, "watch").await.as_deref(),
        Some("Gym watch")
    );

    assert_eq!(
        rename(&db, "watch", "   ").await.as_deref(),
        Some("Forerunner 265")
    );
    assert_eq!(
        stored_name(&db, "watch").await.as_deref(),
        Some("Forerunner 265")
    );
}

#[tokio::test]
async fn set_name_of_an_unknown_device_changes_nothing() {
    let db = common::test_db().await;

    assert_eq!(rename(&db, "missing", "Gym watch").await, None);
    assert_eq!(stored_name(&db, "missing").await, None);
}

#[tokio::test]
async fn syncing_or_plugging_a_device_keeps_its_name() {
    let db = common::test_db().await;
    let mut named = device("watch", "Forerunner 265", true);
    named.name = "Gym watch".to_string();
    insert(&db, named).await;

    // Connect still refreshes the model, but the name is the user's
    let listed = vec![device("watch", "Forerunner 265 Music", true)];
    let discovered = db
        .run_in_transaction(async move |tx| Ok(Device::sync_connect(tx, &listed).await?))
        .await
        .unwrap();
    assert_eq!(discovered[0].name, "Gym watch");
    assert_eq!(
        stored(&db, "watch").await,
        Some(("Forerunner 265 Music".into(), true))
    );
    assert_eq!(
        stored_name(&db, "watch").await.as_deref(),
        Some("Gym watch")
    );

    // Over MTP the device only gives its model: enrolling it takes the stored name
    let mut plugged = device("watch", "Forerunner 265", false);
    let enrolled = db
        .run_in_transaction(async move |tx| {
            Device::enrol_usb(tx, &mut plugged).await?;
            Ok(plugged)
        })
        .await
        .unwrap();
    assert_eq!(enrolled.name, "Gym watch");
    assert_eq!(
        stored_name(&db, "watch").await.as_deref(),
        Some("Gym watch")
    );
}
