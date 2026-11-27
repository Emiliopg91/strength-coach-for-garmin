use crate::dao::activity_step::ActivityStep;

/// Recomputes the personal records of every exercise with some recorded set.
pub(crate) async fn update_all_prs(
    tx: &rusqlite_orm::rusqlite::Transaction<'_>,
) -> rusqlite_orm::errors::Result<()> {
    let exercises = ActivityStep::get_performed_exercises(tx).await?;
    ActivityStep::update_prs(tx, &exercises, &[])
        .await
        .map(|_| ())
}
