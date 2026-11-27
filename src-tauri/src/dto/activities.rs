use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::utils::weight::to_kg;
use crate::{
    dao::{
        activity::Activity, activity_lap::ActivityLap, activity_step::ActivityStep,
        exercise::Exercise, workout_step::StepType,
    },
    dto::split_muscles,
};

#[derive(Serialize, Default)]
pub struct ActivityListItem {
    pub name: String,
    pub timestamp: u32,
    pub active_calories: u16,
    pub training_load: f32,
    pub sport: u8,
    pub sub_sport: u8,
    pub has_record: bool,
    pub timer_time: u32,
    pub has_sets: bool,
}

impl From<&Activity> for ActivityListItem {
    fn from(value: &Activity) -> Self {
        Self {
            name: value.name.clone(),
            timestamp: value.date,
            active_calories: value.active_calories,
            training_load: value.training_load,
            sport: value.sport,
            sub_sport: value.sub_sport,
            has_record: false,
            timer_time: value.timer_time,
            has_sets: false,
        }
    }
}
impl PartialEq for ActivityListItem {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp
    }
}
impl Eq for ActivityListItem {}
impl Hash for ActivityListItem {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.timestamp.hash(state);
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ActivityStepDto {
    pub ex_cat: Option<u16>,
    pub ex_id: Option<u16>,
    pub idx: u8,
    pub reps: Option<u16>,
    pub reps_ref: Option<u16>,
    /// In kg
    pub weight: Option<f32>,
    /// In kg
    pub weight_ref: Option<f32>,
    pub pr: bool,
    pub kind: StepType,
    pub duration: Option<u32>,
    pub duration_ref: Option<u32>,
}

impl From<&ActivityStep> for ActivityStepDto {
    fn from(value: &ActivityStep) -> Self {
        Self {
            ex_cat: value.ex_cat,
            ex_id: value.ex_id,
            idx: value.idx,
            reps: value.reps,
            reps_ref: value.reps_ref,
            weight: value.weight.map(to_kg),
            weight_ref: value.weight_ref.map(to_kg),
            pr: value.pr,
            kind: value.kind.clone(),
            duration: value.duration,
            duration_ref: value.duration_ref,
        }
    }
}

#[derive(Serialize)]
pub struct ActivityLapDto {
    idx: u16,
    start_latitude: Option<i32>,
    start_longitude: Option<i32>,
}

impl From<&ActivityLap> for ActivityLapDto {
    fn from(value: &ActivityLap) -> Self {
        Self {
            idx: value.idx,
            start_latitude: value.start_latitude,
            start_longitude: value.start_longitude,
        }
    }
}

#[derive(Serialize)]
pub struct ActivityDetails {
    pub name: String,

    pub timestamp: u32,

    pub timer_time: u32,
    pub active_time: u32,

    pub active_calories: u16,

    pub training_load: f32,
    pub sport: u8,
    pub sub_sport: u8,

    pub steps: Vec<ActivityStepDto>,
    pub heart_rates: Vec<Option<u8>>,
    pub max_hr: u8,
    pub coordinates: Vec<Option<(i32, i32)>>,
    pub laps: Vec<ActivityLapDto>,
    pub speeds: Vec<Option<f64>>,
    pub distance: Option<f64>,
    pub notes: String,

    pub primary_muscles: Vec<String>,
    pub secondary_muscles: Vec<String>,

    pub device: Option<String>,
    pub connect_id: Option<String>,

    pub feeling: Option<u8>,
    pub rpe: Option<u8>,
}

/// Rounds samples to `1 / scale` precision so their JSON form stays short (e.g. `scale = 1000.0` keeps 3 decimals).
fn round_samples(samples: Vec<Option<f64>>, scale: f64) -> Vec<Option<f64>> {
    samples
        .into_iter()
        .map(|v| v.map(|v| (v * scale).round() / scale))
        .collect()
}

/// Max heart rate assumed for activities that don't record one.
const DEFAULT_MAX_HR: u8 = 189;

impl From<(&Activity, &[Exercise], &[ActivityStep], &[ActivityLap])> for ActivityDetails {
    fn from(
        (activity, exercises, steps, laps): (
            &Activity,
            &[Exercise],
            &[ActivityStep],
            &[ActivityLap],
        ),
    ) -> Self {
        let data = activity.data.as_ref();
        let (primary_muscles, secondary_muscles) = split_muscles(exercises);

        Self {
            name: activity.name.clone(),
            timestamp: activity.date,
            timer_time: activity.timer_time,
            active_time: activity.active_time,
            active_calories: activity.active_calories,
            training_load: activity.training_load,
            sport: activity.sport,
            sub_sport: activity.sub_sport,
            steps: steps.iter().map(ActivityStepDto::from).collect(),
            heart_rates: data.and_then(|d| d.get_heart_rates()).unwrap_or_default(),
            max_hr: activity.max_hr.unwrap_or(DEFAULT_MAX_HR),
            device: activity.device_obj.as_ref().map(|dev| dev.name.clone()),
            laps: laps.iter().map(ActivityLapDto::from).collect(),
            coordinates: data
                .and_then(|d| d.get_coordinates_semicircle())
                .unwrap_or_default(),
            speeds: data
                .and_then(|d| d.get_speeds())
                .map(|s| round_samples(s, 1000.0))
                .unwrap_or_default(),
            distance: activity.distance.map(|d| d / 1000_f64),
            notes: activity.notes.clone().unwrap_or_default(),
            primary_muscles,
            secondary_muscles,
            connect_id: activity.connect_id.clone(),
            feeling: activity.feeling,
            rpe: activity.rpe,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct ActivityUpdate {
    pub timestamp: u32,
    pub sets: Vec<ActivityStepDto>,
    pub notes: Option<String>,
}

/// Exercise sets that worked a muscle over a period, by the role the muscle had in each exercise.
#[derive(Serialize, Debug, PartialEq, Default)]
pub struct MuscleSets {
    pub primary: u32,
    pub secondary: u32,
}

#[derive(Serialize, Clone)]
pub struct ActivityLocation {
    pub activity: u32,
    pub location: String,
}
