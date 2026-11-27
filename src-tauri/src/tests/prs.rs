use std::collections::HashSet;

use rusqlite_orm::{database::DatabasePool, types::order_by::OrderBy};

use crate::{
    dao::{
        activity_step::{self, ActivityStep, ActivityStepRepository},
        workout_step::StepType,
    },
    tests::common,
};

const SQUAT: (u16, u16) = (28, 0);
const BENCH: (u16, u16) = (0, 0);

async fn pr_sets(db: &DatabasePool) -> Vec<(u32, u8, u16, u16)> {
    db.run_in_connection(async move |conn| {
        Ok(ActivityStepRepository::select_by_personal_records_in(
            conn,
            true,
            Some(&[OrderBy::Asc(activity_step::entity::columns::EX_CAT)]),
        )
        .await?
        .into_iter()
        .map(|s| (s.activity, s.idx, s.ex_cat.unwrap(), s.ex_id.unwrap()))
        .collect())
    })
    .await
    .unwrap()
}

async fn insert_and_update(db: &DatabasePool, activities: Vec<(u32, Vec<ActivityStep>)>) {
    db.run_in_transaction(async move |tx| {
        let mut exercises = HashSet::new();
        for (date, sets) in &activities {
            let mut activity = common::activity(*date);
            for s in sets {
                exercises.insert((s.ex_cat.unwrap(), s.ex_id.unwrap()));
            }
            activity.steps = sets.clone();
            common::insert_activity(tx, activity).await;
        }
        ActivityStep::update_prs(tx, &exercises, &[]).await?;
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn heaviest_set_wins_over_more_reps() {
    let db = common::test_db().await;
    insert_and_update(
        &db,
        vec![
            (1, vec![common::set(1, 0, SQUAT, 10, 100.0)]),
            (2, vec![common::set(2, 0, SQUAT, 1, 120.0)]),
        ],
    )
    .await;

    assert_eq!(pr_sets(&db).await, vec![(2, 0, SQUAT.0, SQUAT.1)]);
}

#[tokio::test]
async fn same_weight_more_reps_wins() {
    let db = common::test_db().await;
    insert_and_update(
        &db,
        vec![(
            1,
            vec![
                common::set(1, 0, SQUAT, 5, 100.0),
                common::set(1, 1, SQUAT, 8, 100.0),
                common::set(1, 2, SQUAT, 6, 100.0),
            ],
        )],
    )
    .await;

    assert_eq!(pr_sets(&db).await, vec![(1, 1, SQUAT.0, SQUAT.1)]);
}

#[tokio::test]
async fn tie_goes_to_the_earliest_set() {
    let db = common::test_db().await;
    insert_and_update(
        &db,
        vec![
            (2, vec![common::set(2, 0, SQUAT, 5, 100.0)]),
            (
                1,
                vec![
                    common::set(1, 0, SQUAT, 3, 80.0),
                    common::set(1, 1, SQUAT, 5, 100.0),
                    common::set(1, 2, SQUAT, 5, 100.0),
                ],
            ),
        ],
    )
    .await;

    assert_eq!(pr_sets(&db).await, vec![(1, 1, SQUAT.0, SQUAT.1)]);
}

#[tokio::test]
async fn one_pr_per_exercise() {
    let db = common::test_db().await;
    insert_and_update(
        &db,
        vec![(
            1,
            vec![
                common::set(1, 0, BENCH, 5, 80.0),
                common::set(1, 1, BENCH, 5, 85.0),
                common::set(1, 2, SQUAT, 5, 120.0),
                common::set(1, 3, SQUAT, 5, 110.0),
            ],
        )],
    )
    .await;

    assert_eq!(
        pr_sets(&db).await,
        vec![(1, 1, BENCH.0, BENCH.1), (1, 2, SQUAT.0, SQUAT.1)]
    );
}

#[tokio::test]
async fn new_record_moves_the_flag() {
    let db = common::test_db().await;
    insert_and_update(&db, vec![(1, vec![common::set(1, 0, SQUAT, 5, 100.0)])]).await;
    assert_eq!(pr_sets(&db).await, vec![(1, 0, SQUAT.0, SQUAT.1)]);

    insert_and_update(&db, vec![(2, vec![common::set(2, 0, SQUAT, 5, 105.0)])]).await;
    assert_eq!(pr_sets(&db).await, vec![(2, 0, SQUAT.0, SQUAT.1)]);
}

#[tokio::test]
async fn set_without_weight_loses_to_a_weighted_one() {
    let db = common::test_db().await;
    let mut bodyweight = common::set(1, 0, SQUAT, 30, 0.0);
    bodyweight.weight = None;
    insert_and_update(
        &db,
        vec![
            (1, vec![bodyweight]),
            (2, vec![common::set(2, 0, SQUAT, 1, 10.0)]),
        ],
    )
    .await;

    assert_eq!(pr_sets(&db).await, vec![(2, 0, SQUAT.0, SQUAT.1)]);
}

fn non_exercise_sets() -> Vec<ActivityStep> {
    [
        StepType::Warmup,
        StepType::Cooldown,
        StepType::Recovery,
        StepType::Other,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, kind)| {
        let mut set = common::set(1, i as u8, SQUAT, 10, 200.0);
        set.kind = kind;
        set
    })
    .collect()
}

#[tokio::test]
async fn non_exercise_sets_never_hold_a_record() {
    let db = common::test_db().await;
    let mut sets = non_exercise_sets();
    sets.push(common::set(1, 4, SQUAT, 5, 100.0));
    insert_and_update(&db, vec![(1, sets)]).await;

    assert_eq!(pr_sets(&db).await, vec![(1, 4, SQUAT.0, SQUAT.1)]);
}

#[tokio::test]
async fn exercise_with_only_non_exercise_sets_has_no_record() {
    let db = common::test_db().await;
    insert_and_update(&db, vec![(1, non_exercise_sets())]).await;

    assert_eq!(pr_sets(&db).await, vec![]);
}

#[tokio::test]
async fn stale_record_on_a_warmup_is_cleared() {
    let db = common::test_db().await;
    let mut warmup = common::set(1, 0, SQUAT, 10, 200.0);
    warmup.kind = StepType::Warmup;
    warmup.pr = true;
    insert_and_update(
        &db,
        vec![(1, vec![warmup, common::set(1, 1, SQUAT, 5, 100.0)])],
    )
    .await;

    assert_eq!(pr_sets(&db).await, vec![(1, 1, SQUAT.0, SQUAT.1)]);
}
