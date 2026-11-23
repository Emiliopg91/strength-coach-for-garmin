use chrono::{Datelike, Days, Local, TimeZone};

use crate::{
    dao::{activity_step::ActivityStep, workout_step::StepType},
    dto::activities::MuscleSets,
    logic::activities::helpers::{heatmap_data, muscle_sets},
    tests::common,
};

/// Builds a timestamp for `days_ago` days before today, anchored at local noon so the
/// resulting instant never lands on a different calendar day due to DST shifts.
fn timestamp_days_ago(days_ago: u64) -> u32 {
    let date = Local::now()
        .date_naive()
        .checked_sub_days(Days::new(days_ago))
        .unwrap();
    let noon = date.and_hms_opt(12, 0, 0).unwrap();
    Local
        .from_local_datetime(&noon)
        .earliest()
        .unwrap()
        .timestamp() as u32
}

#[tokio::test]
async fn heatmap_totals_and_record_flags_match_inserted_activities() {
    let db = common::test_db().await;

    // 20 activities spread over the last year, far enough apart that each lands on its
    // own calendar day (no two activities share a month/day cell).
    let days_ago: Vec<u64> = (0..20).map(|i| 5 + i * 17).collect();
    assert!(days_ago.iter().all(|&d| d < 365));

    // A subset of those activities also set a personal record.
    let record_indices: [usize; 6] = [1, 4, 7, 10, 13, 17];

    let tx_days_ago = days_ago.clone();
    db.run_in_transaction(async move |tx| {
        for (i, &days) in tx_days_ago.iter().enumerate() {
            let date = timestamp_days_ago(days);
            let mut activity = common::activity(date);
            activity.training_load = 40.25 + (i as f32) * 7.0;

            if record_indices.contains(&i) {
                let mut set = common::set(date, 0, (0, i as u16), 5, 100.0 + i as f32);
                set.pr = true;
                activity.steps = vec![set];
            }

            common::insert_activity(tx, activity).await;
        }
        Ok(())
    })
    .await
    .unwrap();

    let heatmap = heatmap_data(&db).await.unwrap();

    for (i, &days) in days_ago.iter().enumerate() {
        let date = Local::now()
            .date_naive()
            .checked_sub_days(Days::new(days))
            .unwrap();
        let cell = &heatmap[date.month0() as usize][date.day0() as usize];

        let expected_load = 40.25 + (i as f32) * 7.0;
        let expected_record = record_indices.contains(&i);

        assert_eq!(
            cell.0, expected_load,
            "activity {i} ({date}) training load mismatch"
        );
        assert_eq!(
            cell.1,
            if expected_record { 1 } else { 0 },
            "activity {i} ({date}) record count mismatch"
        );
    }
}

#[tokio::test]
async fn activity_set_flags_report_sets_and_records_per_activity() {
    let db = common::test_db().await;

    db.run_in_transaction(async move |tx| {
        // No sets at all.
        common::insert_activity(tx, common::activity(1_000)).await;

        // Sets, none of them a record.
        let mut plain = common::activity(2_000);
        plain.steps = vec![common::set(2_000, 0, (0, 1), 5, 50.0)];
        common::insert_activity(tx, plain).await;

        // Sets, one of them a record.
        let mut record = common::activity(3_000);
        let mut pr = common::set(3_000, 1, (0, 2), 5, 60.0);
        pr.pr = true;
        record.steps = vec![common::set(3_000, 0, (0, 1), 5, 40.0), pr];
        common::insert_activity(tx, record).await;

        Ok(())
    })
    .await
    .unwrap();

    let all = db
        .run_in_connection(async move |conn| {
            Ok(ActivityStep::get_set_flags(conn, None, None).await?)
        })
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all.get(&2_000), Some(&false));
    assert_eq!(all.get(&3_000), Some(&true));
    assert!(!all.contains_key(&1_000));

    let recent = db
        .run_in_connection(async move |conn| {
            Ok(ActivityStep::get_set_flags(conn, Some(2_500), None).await?)
        })
        .await
        .unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent.get(&3_000), Some(&true));

    let bounded = db
        .run_in_connection(async move |conn| {
            Ok(ActivityStep::get_set_flags(conn, Some(2_500), Some(3_500)).await?)
        })
        .await
        .unwrap();
    assert_eq!(bounded.len(), 1);
    assert_eq!(bounded.get(&3_000), Some(&true));

    let upto = db
        .run_in_connection(async move |conn| {
            Ok(ActivityStep::get_set_flags(conn, None, Some(2_500)).await?)
        })
        .await
        .unwrap();
    assert_eq!(upto.len(), 1);
    assert_eq!(upto.get(&2_000), Some(&false));
}

#[tokio::test]
async fn muscle_sets_count_primary_and_secondary_sets_since_the_given_date() {
    let db = common::test_db().await;

    db.run_in_transaction(async move |tx| {
        // Before `since`: left out
        let mut old = common::activity(1_000);
        old.steps = vec![common::set(1_000, 0, (0, 1), 5, 50.0)];
        common::insert_activity(tx, old).await;

        // Bench press (chest primary; shoulders and triceps secondary) twice, close-grip bench
        // press (triceps primary; chest and shoulders secondary) once and a warm-up
        let mut recent = common::activity(3_000);
        let mut warmup = common::set(3_000, 3, (0, 1), 10, 20.0);
        warmup.kind = StepType::Warmup;
        recent.steps = vec![
            common::set(3_000, 0, (0, 1), 5, 60.0),
            common::set(3_000, 1, (0, 1), 5, 60.0),
            common::set(3_000, 2, (0, 4), 8, 40.0),
            warmup,
        ];
        common::insert_activity(tx, recent).await;
        Ok(())
    })
    .await
    .unwrap();

    let sets = muscle_sets(&db, 2_000).await.unwrap();
    let expected = [("chest", 2, 1), ("shoulders", 0, 3), ("triceps", 1, 2)].map(
        |(muscle, primary, secondary)| MuscleSets {
            muscle: muscle.to_string(),
            primary,
            secondary,
        },
    );
    assert_eq!(sets, expected);

    assert!(muscle_sets(&db, 4_000).await.unwrap().is_empty());
}
