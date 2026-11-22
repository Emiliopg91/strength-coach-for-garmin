use serde::Serialize;

use crate::dto::{
    activities::ActivityDetails, body_metrics::BodyMetricListItem, exercises::ExerciseDetails,
};

#[derive(Serialize)]
pub struct ReportData {
    pub activities: Vec<ActivityDetails>,
    pub exercises: Vec<ExerciseDetails>,
    pub body_metrics: Vec<BodyMetricListItem>,
}
