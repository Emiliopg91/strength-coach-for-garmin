use serde::Serialize;

use crate::dao::{
    activity::Activity, workout_planning::WorkoutPlanning, workout_step::WorkoutStep,
};

#[derive(Serialize)]
pub struct WorkoutListItem {
    pub id: u16,
    pub name: String,
    pub latest_activity: Option<u32>,
    pub activities: u32,
    pub avg_time: u32,
    pub avg_load: f32,
    pub enabled: bool,
    pub has_steps: bool,
}

#[derive(Serialize)]
pub struct WorkoutActivity {
    pub date: u32,
    pub volume: f32,
    pub time: u32,
}

impl From<&Activity> for WorkoutActivity {
    fn from(value: &Activity) -> Self {
        WorkoutActivity {
            date: value.date,
            volume: 0_f32,
            time: value.timer_time,
        }
    }
}

#[derive(Serialize)]
pub struct WorkoutDetails {
    pub id: u16,
    pub name: String,
    pub latest_activity: Option<u32>,
    pub activity_count: u32,
    pub avg_time: u32,
    pub avg_volume: f32,
    pub activities: Vec<WorkoutActivity>,
    pub enabled: bool,
    pub progression: bool,
    pub notes: Option<String>,
    pub steps: Vec<WorkoutStep>,
    pub primary_muscles: Vec<String>,
    pub secondary_muscles: Vec<String>,
}

/// What a step loaded with a percentage of the 1RM resolves against for an exercise.
#[derive(Serialize)]
pub struct PercentLoadBase {
    pub ex_cat: u16,
    pub ex_id: u16,
    /// Best estimated 1RM of the recent sets, in kg
    pub one_rm: f32,
    /// Weight increment of the exercise the load is rounded to, in kg
    pub increment: f32,
}

#[derive(Serialize)]
pub struct WorkoutPlanListItem {
    pub date: u32,
    pub workout: u16,
    pub activity: Option<u32>,
}

impl From<&WorkoutPlanning> for WorkoutPlanListItem {
    fn from(value: &WorkoutPlanning) -> Self {
        Self {
            date: value.date,
            workout: value.workout,
            activity: value.activity,
        }
    }
}
