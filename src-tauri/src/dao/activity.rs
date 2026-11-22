use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite_orm::{
    Entity,
    dao::Repository,
    rusqlite::{Connection, Transaction},
    types::{order_by::OrderBy, value::Value, where_clause::Where},
};
use serde::{Deserialize, Serialize};

use crate::{
    dao::{
        activity_data::{self, ActivityData, ActivityDataRepository},
        activity_lap::{self, ActivityLap, ActivityLapRepository},
        activity_step::{self, ActivityStep, ActivityStepRepository},
        device::{self, Device},
        sub_sport::{self, SubSport},
        workout::{self, Workout, WorkoutRepository, entity::columns as workout_cols},
        workout_planning::{WorkoutPlanningRepository, entity::columns as workout_planning_cols},
        workout_step::{WorkoutStep, WorkoutStepRepository},
    },
    utils::time::local_midnight,
};

#[derive(Entity, Clone, Serialize, Deserialize)]
#[primary_key(date)]
#[index("name", (name))]
pub struct Activity {
    pub date: u32,
    pub name: String,
    pub total_elapsed_time: u32,
    pub active_time: u32,
    pub active_calories: u16,
    pub training_load: u16,
    pub device: Option<String>,
    pub workout: Option<u16>,
    pub sport: u8,
    pub sub_sport: u8,
    pub connect_id: Option<String>,

    #[relationship((date, activity_step::entity::columns::ACTIVITY))]
    pub steps: Vec<ActivityStep>,

    #[relationship((date, activity_lap::entity::columns::ACTIVITY))]
    pub laps: Vec<ActivityLap>,

    #[relationship((device, device::entity::columns::SERIAL))]
    pub device_obj: Option<Device>,

    #[relationship((date, activity_data::entity::columns::ACTIVITY))]
    pub data: Option<ActivityData>,

    #[relationship((sport, sub_sport::entity::columns::SPORT),(sub_sport, sub_sport::entity::columns::ID))]
    pub sub_sport_obj: Option<SubSport>,

    #[relationship((workout, workout::entity::columns::ID))]
    pub workout_obj: Option<Workout>,
}

impl Activity {
    pub async fn get_dates(conn: &Connection) -> rusqlite_orm::errors::Result<BTreeSet<u32>> {
        Ok(ActivityRepository::select()
            .columns(&[entity::columns::DATE])
            .fetch_in(conn)
            .await?
            .iter()
            .map(|row| row.get_u32(entity::columns::DATE.as_ref()))
            .collect())
    }

    /// Connect activity id of the activity started at `date`, `None` if it isn't linked to one.
    pub async fn get_connect_id(
        conn: &Connection,
        date: u32,
    ) -> rusqlite_orm::errors::Result<Option<String>> {
        Ok(ActivityRepository::select()
            .columns(&[entity::columns::CONNECT_ID])
            .where_(Where::Eq(entity::columns::DATE, date.into()))
            .fetch_one_in(conn)
            .await?
            .and_then(|row| row.get_text_nullable(entity::columns::CONNECT_ID.as_ref())))
    }

    /// Activities newest first, filtered by the bounds and sport given.
    pub async fn get_activities(
        conn: &Connection,
        from: Option<i64>,
        to: Option<i64>,
        sport: Option<u8>,
        sub_sport: Option<u8>,
    ) -> rusqlite_orm::errors::Result<Vec<Activity>> {
        let mut select_builder =
            ActivityRepository::select().order_by(OrderBy::Desc(entity::columns::DATE));

        let mut conditions = Vec::new();
        if let Some(min_date) = from {
            conditions.push(Where::Gte(entity::columns::DATE, min_date.into()))
        }
        if let Some(max_date) = to {
            conditions.push(Where::Lte(entity::columns::DATE, max_date.into()))
        }
        if let Some(sport) = sport {
            conditions.push(Where::Eq(entity::columns::SPORT, sport.into()))
        }
        if let Some(sub_sport) = sub_sport {
            conditions.push(Where::Eq(entity::columns::SUB_SPORT, sub_sport.into()))
        }
        if !conditions.is_empty() {
            select_builder = select_builder.where_(Where::And(conditions))
        }

        select_builder.fetch_in(conn).await
    }

    /// Activities started at any of `dates`, unordered.
    pub async fn get_by_dates(
        conn: &Connection,
        dates: &[Value],
    ) -> rusqlite_orm::errors::Result<Vec<Activity>> {
        ActivityRepository::select()
            .where_(Where::In(entity::columns::DATE, dates.to_vec()))
            .fetch_in(conn)
            .await
    }

    /// Which of `dates` already have an activity.
    pub async fn get_existing_dates(
        conn: &Connection,
        dates: &[u32],
    ) -> rusqlite_orm::errors::Result<HashSet<u32>> {
        if dates.is_empty() {
            return Ok(HashSet::new());
        }
        Ok(ActivityRepository::select()
            .columns(&[entity::columns::DATE])
            .where_(Where::In(
                entity::columns::DATE,
                dates.iter().map(|d| (*d).into()).collect(),
            ))
            .fetch_in(conn)
            .await?
            .iter()
            .map(|row| row.get_u32(entity::columns::DATE.as_ref()))
            .collect())
    }

    /// Names of the activities started at any of `dates`, by date, within `from_to` if given.
    pub async fn get_names_by_dates(
        conn: &Connection,
        dates: &HashSet<u32>,
        from_to: Option<(u32, u32)>,
    ) -> rusqlite_orm::errors::Result<HashMap<u32, String>> {
        let mut conditions = vec![Where::In(
            entity::columns::DATE,
            dates.iter().map(|d| (*d).into()).collect(),
        )];
        if let Some((from, to)) = from_to {
            conditions.push(Where::Gte(entity::columns::DATE, from.into()));
            conditions.push(Where::Lte(entity::columns::DATE, to.into()));
        }

        Ok(ActivityRepository::select()
            .columns(&[entity::columns::DATE, entity::columns::NAME])
            .where_(Where::And(conditions))
            .fetch_in(conn)
            .await?
            .iter()
            .map(|s| {
                (
                    s.get_u32(entity::columns::DATE.as_ref()),
                    s.get_text(entity::columns::NAME.as_ref()),
                )
            })
            .collect())
    }

    /// Activities recorded following a workout, newest first.
    pub async fn get_workout_activities(
        conn: &Connection,
    ) -> rusqlite_orm::errors::Result<Vec<Activity>> {
        ActivityRepository::select()
            .where_(Where::NotNull(entity::columns::WORKOUT))
            .order_by(OrderBy::Desc(entity::columns::DATE))
            .fetch_in(conn)
            .await
    }

    /// Activities recorded following the workout `workout`, newest first.
    pub async fn get_by_workout(
        conn: &Connection,
        workout: u16,
    ) -> rusqlite_orm::errors::Result<Vec<Activity>> {
        ActivityRepository::select()
            .where_(Where::Eq(entity::columns::WORKOUT, workout.into()))
            .order_by(OrderBy::Desc(entity::columns::DATE))
            .fetch_in(conn)
            .await
    }

    /// The activity with its sets, laps, additional data and workout, `None` if it doesn't exist.
    pub async fn get_full(
        conn: &Connection,
        timestamp: u32,
    ) -> rusqlite_orm::errors::Result<Option<Activity>> {
        let Some(mut activity) = ActivityRepository::select_by_id_in(conn, timestamp).await? else {
            return Ok(None);
        };
        activity.fetch_steps_relationship_in(conn).await?;
        activity.fetch_laps_relationship_in(conn).await?;
        activity.fetch_data_relationship_in(conn).await?;
        if activity.workout.is_some() {
            activity.fetch_workout_obj_relationship_in(conn).await?;
        }
        Ok(Some(activity))
    }

    /// Name of the activity, `None` if it doesn't exist.
    pub async fn get_name(
        conn: &Connection,
        timestamp: u32,
    ) -> rusqlite_orm::errors::Result<Option<String>> {
        Ok(ActivityRepository::select_by_id_in(conn, timestamp)
            .await?
            .map(|activity| activity.name))
    }

    /// Links the activity to its Garmin Connect activity.
    pub async fn set_connect_id(
        conn: &Connection,
        timestamp: u32,
        id: u64,
    ) -> rusqlite_orm::errors::Result<usize> {
        ActivityRepository::update()
            .where_(Where::Eq(entity::columns::DATE, timestamp.into()))
            .set(entity::columns::CONNECT_ID, id.into())
            .execute_in(conn)
            .await
    }

    /// Renames every activity of `activities` to `name`; returns the number of rows changed.
    pub async fn set_name(
        conn: &Connection,
        activities: &[u32],
        name: &str,
    ) -> rusqlite_orm::errors::Result<usize> {
        if activities.is_empty() {
            return Ok(0);
        }
        ActivityRepository::update()
            .set(entity::columns::NAME, name.into())
            .where_(Where::In(
                entity::columns::DATE,
                activities.iter().map(|s| (*s).into()).collect(),
            ))
            .execute_in(conn)
            .await
    }

    /// Inserts an activity with its sets, laps and additional data, creating its workout (and
    /// steps, if missing) when it carries one in `workout_obj`, whose id it then takes, and links
    /// it to the plan it fulfils. An activity without `workout_obj` keeps its `workout` as is. The
    /// workout is only stored if not in `handled_workouts` (ids by name) yet, so a batch must be
    /// inserted newest first. Child collections are restored on the activity after insertion.
    pub async fn insert_full(
        tx: &Transaction<'_>,
        activity: &mut Activity,
        handled_workouts: &mut HashMap<String, u16>,
    ) -> rusqlite_orm::errors::Result<()> {
        let mut series = std::mem::take(&mut activity.steps);
        let mut laps = std::mem::take(&mut activity.laps);
        let mut add_data = activity.data.take();

        // Once per batch: the caller hands the activities newest first, so the notes of the latest
        // activity of each workout win without updating them again for every older one
        if let Some(workout) = &activity.workout_obj {
            let id = match handled_workouts.get(&workout.name) {
                Some(id) => *id,
                None => {
                    let id = Self::store_workout(tx, workout).await?;
                    handled_workouts.insert(workout.name.clone(), id);
                    id
                }
            };
            activity.workout = Some(id);
        }

        ActivityRepository::insert()
            .item(activity)
            .execute_in(tx)
            .await?;

        if !series.is_empty() {
            let mut insert = ActivityStepRepository::insert();
            for serie in &mut series {
                insert = insert.item(serie);
            }
            insert.execute_in(tx).await?;
        }

        if let Some(additional_data) = &mut add_data {
            ActivityDataRepository::insert()
                .item(additional_data)
                .execute_in(tx)
                .await?;
        }

        if !laps.is_empty() {
            let mut insert = ActivityLapRepository::insert();
            for lap in &mut laps {
                insert = insert.item(lap)
            }
            insert.execute_in(tx).await?;
        }

        if let Some(workout) = activity.workout
            && let Some(plan_date) = local_midnight(activity.date as i64)
        {
            WorkoutPlanningRepository::update()
                .set(workout_planning_cols::ACTIVITY, activity.date.into())
                .where_(Where::And(vec![
                    Where::Eq(workout_planning_cols::DATE, plan_date.into()),
                    Where::Eq(workout_planning_cols::WORKOUT, workout.into()),
                    Where::Null(workout_planning_cols::ACTIVITY),
                ]))
                .execute_in(tx)
                .await?;
        }

        activity.steps = series;
        activity.laps = laps;
        activity.data = add_data;

        Ok(())
    }

    /// Stores the workout recorded with an activity: updates the notes of the stored one with its
    /// name, or creates it, and stores its steps if it has none yet. Only workouts read with their
    /// steps carry notes. Returns its id.
    async fn store_workout(
        tx: &Transaction<'_>,
        workout: &Workout,
    ) -> rusqlite_orm::errors::Result<u16> {
        let id = match Workout::get_id(tx, &workout.name).await? {
            Some(id) => {
                if !workout.steps.is_empty() {
                    WorkoutRepository::update()
                        .set(workout_cols::NOTES, workout.notes.clone().into())
                        .where_(Where::Eq(workout_cols::ID, id.into()))
                        .execute_in(tx)
                        .await?;
                }
                id
            }
            None => {
                let mut item = Workout {
                    id: 0,
                    name: workout.name.clone(),
                    enabled: workout.enabled,
                    notes: workout.notes.clone(),
                    steps: Vec::new(),
                    connect_id: None,
                    connect_created_time: None,
                    progression: true,
                };
                WorkoutRepository::insert()
                    .item(&mut item)
                    .execute_in(tx)
                    .await?;
                item.id
            }
        };

        if !workout.steps.is_empty()
            && WorkoutStepRepository::count_by_workout_in(tx, id).await? == 0
        {
            let mut steps = workout.steps.clone();
            for step in &mut steps {
                step.workout = id;
            }
            WorkoutStep::insert_all(tx, &mut steps).await?;
        }

        Ok(id)
    }
}
