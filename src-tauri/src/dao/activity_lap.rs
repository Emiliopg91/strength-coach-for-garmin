use rusqlite_orm::{
    Entity, dao::Repository, rusqlite::Connection, types::value::Value, types::where_clause::Where,
};
use serde::{Deserialize, Serialize};

#[derive(Entity, Clone, Serialize, Deserialize)]
#[entity("activity_lap")]
#[primary_key(activity, idx)]
#[index("activity", (activity))]
pub struct ActivityLap {
    pub activity: u32,
    pub idx: u16,
    pub start_latitude: Option<i32>,
    pub start_longitude: Option<i32>,
}

impl ActivityLap {
    /// Laps of every activity in `dates`, unordered.
    pub async fn get_by_activities(
        conn: &Connection,
        dates: &[Value],
    ) -> rusqlite_orm::errors::Result<Vec<ActivityLap>> {
        ActivityLapRepository::select()
            .where_(Where::In(entity::columns::ACTIVITY, dates.to_vec()))
            .fetch_in(conn)
            .await
    }
}
