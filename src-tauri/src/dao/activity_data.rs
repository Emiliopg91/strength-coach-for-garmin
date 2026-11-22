use rusqlite_orm::{
    Entity,
    dao::Repository,
    rusqlite::{Connection, Transaction},
    types::{order_by::OrderBy, value::Value, where_clause::Where},
};
use serde::{Deserialize, Serialize};

use crate::dao::activity::{ActivityRepository, entity::columns as activity_cols};

#[derive(Entity, Clone, Deserialize, Serialize)]
#[entity("activity_data")]
#[primary_key(activity)]
pub struct ActivityData {
    pub activity: u32,
    pub heart_rates: Option<Vec<u8>>,
    pub coordinates: Option<Vec<u8>>,
    pub speeds: Option<Vec<u8>>,
    pub distance: Option<f64>,
    pub notes: Option<String>,
    pub max_hr: Option<u8>,
}

impl ActivityData {
    pub const INVALID_HEAR_RATE: u8 = u8::MAX;
    pub const INVALID_POSITION: i32 = i32::MAX;
    pub const INVALID_SPEED: f64 = -1_f64;

    pub const SEMICIRCLE_TO_DEGREES: f64 = 180.0 / (2_i64.pow(31) as f64);

    pub fn new(activity: u32) -> Self {
        Self {
            activity,
            coordinates: None,
            distance: None,
            heart_rates: None,
            notes: None,
            speeds: None,
            max_hr: None,
        }
    }

    /// Unpacks the raw byte blob into (lat, lon) pairs in semicircles.
    pub fn get_coordinates_semicircle(&self) -> Option<Vec<Option<(i32, i32)>>> {
        self.coordinates.as_ref().map(|records| {
            records
                .as_chunks::<8>()
                .0
                .iter()
                .map(|chunk| {
                    let lat = i32::from_be_bytes(chunk[0..4].try_into().unwrap());
                    let lon = i32::from_be_bytes(chunk[4..8].try_into().unwrap());
                    if lat != Self::INVALID_POSITION && lon != Self::INVALID_POSITION {
                        Some((lat, lon))
                    } else {
                        None
                    }
                })
                .collect()
        })
    }
    /// Unpacks the raw byte blob into (lat, lon) pairs converted to degrees.
    pub fn get_coordinates_degrees(&self) -> Option<Vec<Option<(f64, f64)>>> {
        self.get_coordinates_semicircle().map(|semicircles| {
            semicircles
                .into_iter()
                .map(|p| {
                    p.map(|p| {
                        (
                            p.0 as f64 * Self::SEMICIRCLE_TO_DEGREES,
                            p.1 as f64 * Self::SEMICIRCLE_TO_DEGREES,
                        )
                    })
                })
                .collect()
        })
    }
    /// Build blob from semicircles coordinates Vec
    pub fn build_coordinates_blob(values: &[(i32, i32)]) -> Vec<u8> {
        let mut coords = Vec::new();

        for (latitude, longitude) in values {
            coords.extend_from_slice(&latitude.to_be_bytes());
            coords.extend_from_slice(&longitude.to_be_bytes());
        }

        coords
    }

    /// Unpacks the speeds Blob into Vec
    pub fn get_speeds(&self) -> Option<Vec<Option<f64>>> {
        self.speeds.as_ref().map(|records| {
            records
                .as_chunks::<8>()
                .0
                .iter()
                .map(|chunk| {
                    let val = f64::from_be_bytes(*chunk);
                    (val != Self::INVALID_SPEED).then_some(val)
                })
                .collect()
        })
    }
    /// Build blob from speeds Vec
    pub fn build_speeds_blob(value: &[f64]) -> Vec<u8> {
        value.iter().flat_map(|speed| speed.to_be_bytes()).collect()
    }

    /// Unpacks the heart rate Blob into Vec
    pub fn get_heart_rates(&self) -> Option<Vec<Option<u8>>> {
        self.heart_rates.as_ref().map(|records| {
            records
                .iter()
                .copied()
                .map(|val| {
                    if val < Self::INVALID_HEAR_RATE {
                        Some(val)
                    } else {
                        None
                    }
                })
                .collect()
        })
    }
}

impl ActivityData {
    /// Additional data of every activity in `dates`, unordered.
    pub async fn get_by_activities(
        conn: &Connection,
        dates: &[Value],
    ) -> rusqlite_orm::errors::Result<Vec<ActivityData>> {
        ActivityDataRepository::select()
            .where_(Where::In(entity::columns::ACTIVITY, dates.to_vec()))
            .fetch_in(conn)
            .await
    }

    /// Stores the notes of a single activity, creating its additional data row if needed. Blank
    /// notes are stored as `NULL`.
    pub async fn update_notes(
        tx: &Transaction<'_>,
        activity: u32,
        notes: Option<&str>,
    ) -> rusqlite_orm::errors::Result<()> {
        let notes = notes.filter(|n| !n.trim().is_empty()).map(str::to_string);

        let updated = ActivityDataRepository::update()
            .set(entity::columns::NOTES, notes.clone().into())
            .where_(Where::Eq(entity::columns::ACTIVITY, activity.into()))
            .execute_in(tx)
            .await?;

        if updated == 0 && notes.is_some() {
            let mut add_data = ActivityData::new(activity);
            add_data.notes = notes;
            ActivityDataRepository::insert()
                .item(&mut add_data)
                .execute_in(tx)
                .await?;
        }

        Ok(())
    }

    /// Unnamed activities recorded with GPS, newest first, to name them after their location.
    pub async fn get_pending_geolocation(
        conn: &Connection,
    ) -> rusqlite_orm::errors::Result<Vec<u32>> {
        let unnamed_activities = ActivityRepository::select()
            .columns(&[activity_cols::DATE])
            .where_(Where::Eq(activity_cols::NAME, "".into()))
            .to_subquery();

        Ok(ActivityDataRepository::select()
            .columns(&[entity::columns::ACTIVITY])
            .where_(Where::And(vec![
                Where::InSub(entity::columns::ACTIVITY, unnamed_activities),
                Where::NotNull(entity::columns::COORDINATES),
            ]))
            .order_by(OrderBy::Desc(entity::columns::ACTIVITY))
            .fetch_in(conn)
            .await?
            .iter()
            .map(|row| row.get_u32(entity::columns::ACTIVITY.as_ref()))
            .collect())
    }

    /// Coordinates of the activities started at any of `dates`, newest first.
    pub async fn get_coordinates(
        conn: &Connection,
        dates: &[u32],
    ) -> rusqlite_orm::errors::Result<Vec<ActivityData>> {
        if dates.is_empty() {
            return Ok(Vec::new());
        }
        ActivityDataRepository::select()
            .columns(&[entity::columns::ACTIVITY, entity::columns::COORDINATES])
            .where_(Where::In(
                entity::columns::ACTIVITY,
                dates.iter().map(|d| (*d).into()).collect(),
            ))
            .order_by(OrderBy::Desc(entity::columns::ACTIVITY))
            .fetch_in(conn)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| {
                        let mut add_data =
                            ActivityData::new(row.get_u32(entity::columns::ACTIVITY.as_ref()));
                        add_data.coordinates =
                            row.get_blob_nullable(entity::columns::COORDINATES.as_ref());
                        add_data
                    })
                    .collect()
            })
    }
}
