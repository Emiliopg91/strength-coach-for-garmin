use std::collections::BTreeSet;

use crate::{
    dao::{activity::Activity, body_metric::BodyMetric},
    dto::{body_metrics::BodyMetricListItem, report::ReportData},
    logic::{
        activities::helpers::fetch_activities_details, exercises::helpers::fetch_exercises_details,
    },
};

const STRENGTH_SPORT: u8 = 10;

const STRENGTH_SUB_SPORT: u8 = 20;

/// Strength activities between `from` and `to`, with the history of every exercise trained in them
/// and the body measurements of the period.
pub(super) async fn report_data(
    conn: &rusqlite_orm::rusqlite::Connection,
    from: u32,
    to: u32,
) -> rusqlite_orm::errors::Result<ReportData> {
    // Oldest first
    let timestamps: Vec<u32> = Activity::get_activities(
        conn,
        Some(from.into()),
        Some(to.into()),
        Some(STRENGTH_SPORT),
        Some(STRENGTH_SUB_SPORT),
    )
    .await?
    .iter()
    .rev()
    .map(|s| s.date)
    .collect();
    let activity_details = fetch_activities_details(conn, &timestamps, false, false).await?;

    // Sorted and deduplicated; sets without exercise are skipped
    let trained: BTreeSet<(u16, u16)> = activity_details
        .iter()
        .flat_map(|d| &d.steps)
        .filter_map(|set| Some((set.ex_cat?, set.ex_id?)))
        .collect();

    let mut exercises = fetch_exercises_details(conn, &trained, Some((from, to))).await?;
    for details in &mut exercises {
        details.workouts.reverse();
    }

    let mut body_metrics = BodyMetric::get_by_range(conn, Some((from, to)))
        .await?
        .iter()
        .map(BodyMetricListItem::from)
        .collect::<Vec<_>>();
    body_metrics.reverse();

    Ok(ReportData {
        activities: activity_details,
        exercises,
        body_metrics,
    })
}
