use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use rusqlite_orm::database::DatabasePool;
use tauri_plugin_log::log::{error, info, warn};
use tokio::sync::Mutex;

use crate::{
    ConnectLock,
    dao::{activity::Activity, activity_data::ActivityData},
    dto::activities::ActivityLocation,
    logic::events::{BackendEvent, EventSink},
    utils::http,
};

/// Looks up the place names of the activities pending geolocation in the background.
pub(crate) fn spawn_geolocation_update(
    pool: &DatabasePool,
    connect: &ConnectLock,
    events: &EventSink,
) {
    let (pool, connect, events) = (pool.clone(), connect.clone(), events.clone());
    tauri::async_runtime::spawn(async move {
        update_pending_geolocation(&pool, &connect, &events).await
    });
}

/// Serializes the geolocation runs and holds the activities that cannot be named after a place (no
/// valid position, or none Nominatim knows), skipped until the app restarts instead of being
/// loaded and queried again on every import.
static GEOLOCATION_SKIPPED: Mutex<BTreeSet<u32>> = Mutex::const_new(BTreeSet::new());

/// Names the unnamed activities recorded with GPS after the place where they started: the one
/// Garmin Connect gives their activity when logged in, otherwise the one Nominatim finds at their
/// first position.
pub(crate) async fn update_pending_geolocation(
    pool: &DatabasePool,
    connect: &ConnectLock,
    events: &EventSink,
) {
    let mut skipped = GEOLOCATION_SKIPPED.lock().await;
    info!("Looking for activities pending geolocation...");
    let mut pending: Vec<u32> = match pool
        .run_in_connection(async |conn| Ok(ActivityData::get_pending_geolocation(conn).await?))
        .await
    {
        Ok(pending) => pending
            .into_iter()
            .filter(|activity| !skipped.contains(activity))
            .collect(),
        Err(e) => {
            error!("Error while looking for pending activities: {}", e);
            return;
        }
    };
    if pending.is_empty() {
        info!("Found no pending activities");
        return;
    }
    info!("Found {} pending activities", pending.len());

    let from_connect = connect_locations(connect, &pending).await;
    pending.retain(|activity| !from_connect.contains_key(activity));
    name_after_locations(pool, events, from_connect.into_iter().collect()).await;
    if pending.is_empty() {
        return;
    }

    let pending = match pool
        .run_in_connection(async move |conn| {
            Ok(ActivityData::get_coordinates(conn, &pending).await?)
        })
        .await
    {
        Ok(pending) => pending,
        Err(e) => {
            error!("Error while looking for pending activities: {}", e);
            return;
        }
    };

    info!("Geocoding {} pending activities", pending.len());
    let mut geocoded = Vec::new();
    for data in pending {
        let activity = data.activity;
        // The blob may hold only invalid positions
        let Some((lat, lon)) = data
            .get_coordinates_degrees()
            .and_then(|coords| coords.into_iter().flatten().next())
        else {
            warn!("Activity @ {activity} has no valid position to geolocate");
            skipped.insert(activity);
            continue;
        };

        match geocode(lat, lon).await {
            Ok(Some(location)) => geocoded.push((activity, location)),
            Ok(None) => {
                skipped.insert(activity);
            }
            Err(e) => error!("Error geolocating activity @ {activity}: {e}"),
        }

        // Nominatim allows one request per second
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    name_after_locations(pool, events, geocoded).await;
}

/// Place names Garmin Connect gives the activities of `activities`, found in a single search
/// instead of a geocoding query per activity. Empty when not logged in or on failure, so they are
/// geocoded instead.
async fn connect_locations(connect: &ConnectLock, activities: &[u32]) -> HashMap<u32, String> {
    let mut connect = connect.lock().await;
    if connect.logged_user().is_none() {
        return HashMap::new();
    }
    connect
        .find_activity_locations(activities)
        .await
        .unwrap_or_else(|e| {
            error!("Error looking for the locations in Connect: {e}");
            HashMap::new()
        })
}

/// Names each activity of `locations` after its place in a single transaction, with one update
/// per distinct place, and once committed lets the frontend know.
async fn name_after_locations(
    pool: &DatabasePool,
    events: &EventSink,
    locations: Vec<(u32, String)>,
) {
    if locations.is_empty() {
        return;
    }
    let mut by_location: HashMap<String, Vec<u32>> = HashMap::new();
    for (activity, location) in locations {
        by_location.entry(location).or_default().push(activity);
    }

    let res = pool
        .run_in_transaction(async move |tx| {
            let mut updated = 0;
            for (location, activities) in &by_location {
                updated += Activity::set_name(tx, activities, location).await?;
            }
            Ok((by_location, updated))
        })
        .await;
    match res {
        Ok((by_location, updated)) => {
            info!(
                "Updated location for {updated} activities in {} places",
                by_location.len()
            );
            for (location, activities) in by_location {
                for activity in activities {
                    events.emit(BackendEvent::ActivityLocationUpdate(ActivityLocation {
                        activity,
                        location: location.clone(),
                    }));
                }
            }
        }
        Err(e) => error!("Error while updating activity locations: {e}"),
    }
}

/// Name of the place at `lat`, `lon` from Nominatim: the most specific of village, town, city,
/// state and country it gives, `None` if it gives none.
async fn geocode(lat: f64, lon: f64) -> Result<Option<String>, String> {
    let url =
        format!("https://nominatim.openstreetmap.org/reverse?format=json&lat={lat}&lon={lon}");
    let response = http::client()
        .get(&url)
        .header(reqwest::header::USER_AGENT, "strength-coach-for-garmin")
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| format!("Error on geocode query: {e}"))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("Invalid status code: {}", response.status()));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Error parsing response: {e}"))?;
    let Some(address) = json.get("address") else {
        return Ok(None);
    };
    Ok(["village", "town", "city", "state", "country"]
        .iter()
        .find_map(|key| address.get(key))
        .map(|value| value.to_string().replace('"', "")))
}
