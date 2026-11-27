use rusqlite_orm::database::DatabasePool;

use crate::{
    dao::{
        activity::{Activity, ActivityRepository},
        activity_data::{ActivityData, ActivityDataRepository},
    },
    tests::common,
};

const EDITED: u32 = 1;
const OTHER: u32 = 2;

/// Inserts two activities with notes and additional data.
async fn setup() -> DatabasePool {
    let db = common::test_db().await;
    db.run_in_transaction(async move |tx| {
        for date in [EDITED, OTHER] {
            let mut activity = common::activity(date);
            activity.notes = Some(format!("Notes {date}"));
            activity.data = Some(common::additional_data(date));
            common::insert_activity(tx, activity).await;
        }
        Ok(())
    })
    .await
    .unwrap();
    db
}

async fn save_notes(db: &DatabasePool, activity: u32, notes: Option<&str>) {
    let notes = notes.map(str::to_string);
    db.run_in_connection(async move |conn| {
        Ok(Activity::update_notes(conn, activity, notes.as_deref()).await?)
    })
    .await
    .unwrap();
}

async fn notes(db: &DatabasePool, activity: u32) -> Option<String> {
    ActivityRepository::select_by_id(db, activity)
        .await
        .unwrap()
        .unwrap()
        .notes
}

async fn additional_data(db: &DatabasePool, activity: u32) -> Option<ActivityData> {
    ActivityDataRepository::select_by_id(db, activity)
        .await
        .unwrap()
}

#[tokio::test]
async fn updating_notes_does_not_touch_other_activities() {
    let db = setup().await;

    save_notes(&db, EDITED, Some("Updated")).await;

    assert_eq!(notes(&db, EDITED).await.as_deref(), Some("Updated"));
    assert_eq!(notes(&db, OTHER).await.as_deref(), Some("Notes 2"));
}

#[tokio::test]
async fn clearing_notes_does_not_touch_other_activities() {
    let db = setup().await;

    save_notes(&db, EDITED, None).await;

    assert_eq!(notes(&db, EDITED).await, None);
    assert_eq!(notes(&db, OTHER).await.as_deref(), Some("Notes 2"));
}

#[tokio::test]
async fn blank_notes_are_stored_as_null() {
    let db = setup().await;

    save_notes(&db, EDITED, Some("   \n\t")).await;

    assert_eq!(notes(&db, EDITED).await, None);
}

#[tokio::test]
async fn updating_notes_keeps_the_additional_data() {
    let db = setup().await;
    let mut before = additional_data(&db, EDITED).await.unwrap();
    before.heart_rates = Some(vec![120, 130, 140]);
    let updated = before.clone();
    db.run_in_transaction(async move |tx| Ok(updated.update_by_id_in(tx).await?))
        .await
        .unwrap();

    save_notes(&db, EDITED, Some("Updated")).await;

    let after = additional_data(&db, EDITED).await.unwrap();
    assert_eq!(after.heart_rates, before.heart_rates);
}
