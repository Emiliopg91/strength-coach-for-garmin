use rusqlite_orm::database::DatabasePool;

use crate::{
    dao::activity_data::{ActivityData, ActivityDataRepository},
    tests::common,
};

const WITH_DATA: u32 = 1;
const WITHOUT_DATA: u32 = 2;
const OTHER: u32 = 3;

/// Inserts an activity with an additional data row (`WITH_DATA`, `OTHER`) and one without it (`WITHOUT_DATA`).
async fn setup() -> DatabasePool {
    let db = common::test_db().await;
    db.run_in_transaction(async move |tx| {
        for date in [WITH_DATA, OTHER] {
            let mut activity = common::activity(date);
            let mut add_data = common::additional_data(date);
            add_data.notes = Some(format!("Notes {date}"));
            activity.data = Some(add_data);
            common::insert_activity(tx, activity).await;
        }
        common::insert_activity(tx, common::activity(WITHOUT_DATA)).await;
        Ok(())
    })
    .await
    .unwrap();
    db
}

async fn save_notes(db: &DatabasePool, activity: u32, notes: Option<&str>) {
    let notes = notes.map(str::to_string);
    db.run_in_transaction(async move |tx| {
        Ok(ActivityData::update_notes(tx, activity, notes.as_deref()).await?)
    })
    .await
    .unwrap();
}

async fn additional_data(db: &DatabasePool, activity: u32) -> Option<ActivityData> {
    ActivityDataRepository::select_by_id(db, activity)
        .await
        .unwrap()
}

async fn notes(db: &DatabasePool, activity: u32) -> Option<String> {
    additional_data(db, activity).await.and_then(|a| a.notes)
}

#[tokio::test]
async fn updating_notes_does_not_touch_other_activities() {
    let db = setup().await;

    save_notes(&db, WITH_DATA, Some("Updated")).await;

    assert_eq!(notes(&db, WITH_DATA).await.as_deref(), Some("Updated"));
    assert_eq!(notes(&db, OTHER).await.as_deref(), Some("Notes 3"));
}

#[tokio::test]
async fn clearing_notes_does_not_touch_other_activities() {
    let db = setup().await;

    save_notes(&db, WITH_DATA, None).await;

    assert_eq!(notes(&db, WITH_DATA).await, None);
    assert_eq!(notes(&db, OTHER).await.as_deref(), Some("Notes 3"));
}

#[tokio::test]
async fn notes_on_activity_without_additional_data_are_stored() {
    let db = setup().await;

    save_notes(&db, WITHOUT_DATA, Some("New notes")).await;

    assert_eq!(notes(&db, WITHOUT_DATA).await.as_deref(), Some("New notes"));
    assert_eq!(notes(&db, WITH_DATA).await.as_deref(), Some("Notes 1"));
    assert_eq!(notes(&db, OTHER).await.as_deref(), Some("Notes 3"));
}

#[tokio::test]
async fn blank_notes_are_stored_as_null() {
    let db = setup().await;

    save_notes(&db, WITH_DATA, Some("   \n\t")).await;

    assert!(additional_data(&db, WITH_DATA).await.is_some());
    assert_eq!(notes(&db, WITH_DATA).await, None);
}

#[tokio::test]
async fn blank_notes_on_activity_without_additional_data_create_no_row() {
    let db = setup().await;

    save_notes(&db, WITHOUT_DATA, Some("  ")).await;
    save_notes(&db, WITHOUT_DATA, None).await;

    assert!(additional_data(&db, WITHOUT_DATA).await.is_none());
}

#[tokio::test]
async fn updating_notes_keeps_other_additional_data() {
    let db = setup().await;
    let mut before = additional_data(&db, WITH_DATA).await.unwrap();
    before.heart_rates = Some(vec![120, 130, 140]);
    before.distance = Some(1234.5);
    let updated = before.clone();
    db.run_in_transaction(async move |tx| Ok(updated.update_by_id_in(tx).await?))
        .await
        .unwrap();

    save_notes(&db, WITH_DATA, Some("Updated")).await;

    let after = additional_data(&db, WITH_DATA).await.unwrap();
    assert_eq!(after.heart_rates, before.heart_rates);
    assert_eq!(after.distance, before.distance);
}
