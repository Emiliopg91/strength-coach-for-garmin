use std::collections::{HashMap, HashSet};

use crate::{dao::activity::Activity, tests::common};

#[tokio::test]
async fn set_name_renames_only_the_given_activities() {
    let db = common::test_db().await;
    db.run_in_transaction(async move |tx| {
        for date in [1, 2, 3] {
            common::insert_activity(tx, common::activity(date)).await;
        }
        Ok(())
    })
    .await
    .unwrap();

    let updated = db
        .run_in_transaction(async move |tx| Ok(Activity::set_name(tx, &[1, 3, 4], "Priego").await?))
        .await
        .unwrap();

    // The missing activity is not counted
    assert_eq!(updated, 2);
    let names = db
        .run_in_connection(async move |conn| {
            Ok(Activity::get_names_by_dates(conn, &HashSet::from([1, 2, 3]), None).await?)
        })
        .await
        .unwrap();
    assert_eq!(
        names,
        HashMap::from([
            (1, "Priego".to_string()),
            (2, "Activity 2".to_string()),
            (3, "Priego".to_string()),
        ])
    );
}

#[tokio::test]
async fn set_name_without_activities_changes_nothing() {
    let db = common::test_db().await;

    let updated = db
        .run_in_transaction(async move |tx| Ok(Activity::set_name(tx, &[], "Priego").await?))
        .await
        .unwrap();

    assert_eq!(updated, 0);
}
