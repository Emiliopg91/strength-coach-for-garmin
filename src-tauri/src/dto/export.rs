use std::collections::{HashMap, HashSet};

use rusqlite_orm::{dao::Repository, database::DatabasePool};
use serde::Serialize;

use crate::{
    dao::{
        activity::{Activity, ActivityRepository},
        activity_data::{ActivityData, ActivityDataRepository},
        activity_lap::{ActivityLap, ActivityLapRepository},
        activity_step::{ActivityStep, ActivityStepRepository},
        body_metric::{BodyMetric, BodyMetricRepository},
        device::{Device, DeviceRepository},
        settings::{Settings, SettingsRepository},
        workout::{Workout, WorkoutRepository},
        workout_step::{WorkoutStep, WorkoutStepRepository},
    },
    dto::activities::ActivityStepDto,
    utils::translations::{Languages, translate},
};

#[derive(Serialize)]
pub struct Export {
    body_metrics: Vec<BodyMetric>,
    exercises: Vec<ExerciseExport>,
    devices: Vec<Device>,
    workouts: Vec<Workout>,
    activities: Vec<ActivityExport>,
    settings: Vec<Settings>,
    laps: Vec<LapExport>,
}

impl Export {
    /// Loads every table from the database and assembles a full export snapshot.
    pub async fn from_database(
        db: &DatabasePool,
        lang: Languages,
    ) -> rusqlite_orm::errors::Result<Self> {
        db.run_in_connection(async move |conn| {
            let body_metrics = BodyMetricRepository::select().fetch_in(conn).await?;
            let mut workouts = WorkoutRepository::select().fetch_in(conn).await?;
            let mut steps: HashMap<u16, Vec<WorkoutStep>> = HashMap::new();
            for step in WorkoutStepRepository::select().fetch_in(conn).await? {
                steps.entry(step.workout).or_default().push(step);
            }
            for workout in &mut workouts {
                workout.steps = steps.remove(&workout.id).unwrap_or_default();
            }
            let devices = DeviceRepository::select().fetch_in(conn).await?;
            let settings = SettingsRepository::select().fetch_in(conn).await?;
            let activities = ActivityRepository::select().fetch_in(conn).await?;
            let laps = ActivityLapRepository::select()
                .fetch_in(conn)
                .await?
                .into_iter()
                .map(LapExport::from)
                .collect();

            let mut additional_datas: HashMap<u32, ActivityData> = HashMap::new();
            ActivityDataRepository::select()
                .fetch_in(conn)
                .await?
                .into_iter()
                .for_each(|ad| {
                    additional_datas.insert(ad.activity, ad);
                });

            let mut used_exercises = HashSet::new();

            let mut series: HashMap<u32, Vec<ActivityStep>> = HashMap::new();
            ActivityStepRepository::select()
                .fetch_in(conn)
                .await?
                .into_iter()
                .for_each(|s| {
                    used_exercises.insert((s.ex_cat, s.ex_id));
                    let entry = series.entry(s.activity).or_default();
                    entry.push(s);
                });

            let activities = activities
                .into_iter()
                .map(|activity| {
                    let serie = series.get(&activity.date).map(Vec::as_slice);
                    let add_data = additional_datas.get(&activity.date);

                    ActivityExport::from((&activity, add_data, serie))
                })
                .collect();

            let exercises = used_exercises
                .into_iter()
                .filter_map(|(category, id)| category.zip(id))
                .map(|(category, id)| ExerciseExport {
                    category,
                    id,
                    name: translate(&format!("exercise_{category}_{id}"), lang),
                })
                .collect();

            Ok(Self {
                body_metrics,
                devices,
                workouts,
                activities,
                exercises,
                settings,
                laps,
            })
        })
        .await
    }

    /// Serializes the export snapshot as JSON straight into `writer`, avoiding a full in-memory copy.
    pub fn write_json<W: std::io::Write>(&self, writer: W) -> Result<(), serde_json::Error> {
        serde_json::to_writer(writer, &self)
    }
}

#[derive(Serialize)]
pub struct ActivityExport {
    pub date: u32,
    pub workout: String,
    pub total_elapsed_time: u32,
    pub active_time: u32,
    pub active_calories: u16,
    pub training_load: f32,
    pub sport: u8,
    pub sub_sport: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<Vec<Option<(f64, f64)>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heart_rates: Option<Vec<Option<u8>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sets: Option<Vec<ActivityStepDto>>,
}

impl From<(&Activity, Option<&ActivityData>, Option<&[ActivityStep]>)> for ActivityExport {
    fn from(
        (activity, data, steps): (&Activity, Option<&ActivityData>, Option<&[ActivityStep]>),
    ) -> Self {
        Self {
            date: activity.date,
            workout: activity.name.clone(),
            total_elapsed_time: activity.total_elapsed_time,
            active_time: activity.active_time,
            active_calories: activity.active_calories,
            training_load: activity.training_load,
            sport: activity.sport,
            sub_sport: activity.sub_sport,
            device: activity.device.clone(),
            sets: steps
                .filter(|s| !s.is_empty())
                .map(|s| s.iter().map(ActivityStepDto::from).collect()),
            heart_rates: data.and_then(ActivityData::get_heart_rates),
            coordinates: data.and_then(ActivityData::get_coordinates_degrees),
        }
    }
}

#[derive(Serialize)]
pub struct ExerciseExport {
    category: u16,
    id: u16,
    name: String,
}

#[derive(Serialize)]
pub struct LapExport {
    pub activity: u32,
    pub idx: u16,
    pub start_latitude: Option<f64>,
    pub start_longitude: Option<f64>,
}

impl From<ActivityLap> for LapExport {
    fn from(value: ActivityLap) -> Self {
        Self {
            activity: value.activity,
            idx: value.idx,
            start_latitude: value
                .start_latitude
                .map(|s| s as f64 * ActivityData::SEMICIRCLE_TO_DEGREES),
            start_longitude: value
                .start_longitude
                .map(|s| s as f64 * ActivityData::SEMICIRCLE_TO_DEGREES),
        }
    }
}
