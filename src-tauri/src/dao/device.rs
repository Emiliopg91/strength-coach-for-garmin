use std::{
    collections::{BTreeSet, HashMap, HashSet},
    num::ParseIntError,
};

use mtp_rs::DeviceInfo;
use rusqlite_orm::{
    Entity,
    dao::Repository,
    rusqlite::Connection,
    types::{value::Value, where_clause::Where},
};
use serde::{Deserialize, Serialize};

use crate::{connect::client::DeviceSummary, dao::activity::Activity, utils::constants};

#[derive(Clone, Entity, Serialize, Deserialize)]
#[primary_key(serial)]
pub struct Device {
    pub serial: String,
    pub model: String,
    /// Name the device is shown by; its model until the user renames it
    pub name: String,
    pub last_activity_sync: Option<u32>,
    pub last_body_metric_sync: Option<u32>,
    pub usb: bool,
    pub connect: bool,
    pub strength_workout: bool,
}

impl Device {
    pub fn serial_from_unit_id(unit_id: u64) -> String {
        format!("{unit_id:012x}")
    }

    pub fn unit_id_from_serial(serial: &str) -> Result<u64, ParseIntError> {
        u64::from_str_radix(serial, 16)
    }

    /// Renames the device `serial`, trimming the name; a blank one gives it back its model. Returns
    /// the stored name, `None` if the device isn't registered.
    pub async fn set_name(
        tx: &rusqlite_orm::rusqlite::Transaction<'_>,
        serial: &str,
        name: &str,
    ) -> rusqlite_orm::errors::Result<Option<String>> {
        let Some(device) = DeviceRepository::select_by_id_in(tx, serial).await? else {
            return Ok(None);
        };
        let name = match name.trim() {
            "" => device.model,
            name => name.to_string(),
        };
        DeviceRepository::update()
            .set(entity::columns::NAME, name.clone().into())
            .where_(Where::Eq(entity::columns::SERIAL, serial.into()))
            .execute_in(tx)
            .await?;
        Ok(Some(name))
    }

    /// Enrolls the workout-capable Connect devices in `listed`, refreshes the Connect-owned fields of
    /// known ones and clears the `connect` flag of those no longer listed. The name is the user's, so
    /// known devices keep theirs. Returns the listed devices, to be announced.
    pub async fn sync_connect(
        tx: &rusqlite_orm::rusqlite::Transaction<'_>,
        listed: &[Device],
    ) -> rusqlite_orm::errors::Result<Vec<Device>> {
        // A single query for both the listed devices and the ones flagged as Connect
        let mut conditions = vec![Where::Eq(entity::columns::CONNECT, true.into())];
        if !listed.is_empty() {
            conditions.push(Where::In(
                entity::columns::SERIAL,
                listed.iter().map(|d| d.serial.clone().into()).collect(),
            ));
        }
        let mut known: HashMap<String, Device> = DeviceRepository::select()
            .where_(Where::Or(conditions))
            .fetch_in(tx)
            .await?
            .into_iter()
            .map(|db| (db.serial.clone(), db))
            .collect();

        let mut discovered = listed.to_vec();
        let mut added = Vec::new();
        for dao in &mut discovered {
            let stored = known.remove(&dao.serial);
            if let Some(db) = &stored {
                dao.name = db.name.clone();
            }
            match stored {
                None => added.push(dao),
                Some(db)
                    if !db.connect
                        || db.model != dao.model
                        || db.strength_workout != dao.strength_workout =>
                {
                    DeviceRepository::update()
                        .set(entity::columns::CONNECT, true.into())
                        .set(entity::columns::MODEL, dao.model.clone().into())
                        .set(
                            entity::columns::STRENGTH_WORKOUT,
                            dao.strength_workout.into(),
                        )
                        .where_(Where::Eq(
                            entity::columns::SERIAL,
                            dao.serial.clone().into(),
                        ))
                        .execute_in(tx)
                        .await?;
                }
                Some(_) => {}
            }
        }
        if !added.is_empty() {
            let mut insert = DeviceRepository::insert();
            for dao in added {
                insert = insert.item(dao);
            }
            insert.execute_in(tx).await?;
        }

        // What is left was flagged as Connect but no longer listed
        let removed: Vec<Value> = known
            .into_values()
            .filter(|db| db.connect)
            .map(|db| db.serial.into())
            .collect();
        if !removed.is_empty() {
            DeviceRepository::update()
                .set(entity::columns::CONNECT, false.into())
                .where_(Where::In(entity::columns::SERIAL, removed))
                .execute_in(tx)
                .await?;
        }

        Ok(discovered)
    }

    /// The Connect pseudo-device and the dates of every stored activity, to import only new activities.
    pub async fn get_connect_import_state(
        conn: &Connection,
    ) -> rusqlite_orm::errors::Result<(Device, BTreeSet<u32>)> {
        let device = DeviceRepository::select()
            .where_(Where::Eq(
                entity::columns::SERIAL,
                constants::CONNECT_DEVICE_SERIAL.into(),
            ))
            .fetch_one_in(conn)
            .await?
            .unwrap();

        let existing: BTreeSet<u32> = Activity::get_dates(conn).await?;

        Ok((device, existing))
    }

    /// Registers a device plugged by USB, or flags a known one as reachable by USB and takes its
    /// stored name.
    pub async fn enrol_usb(
        tx: &rusqlite_orm::rusqlite::Transaction<'_>,
        device: &mut Device,
    ) -> rusqlite_orm::errors::Result<()> {
        let _: () =
            if let Some(stored) = DeviceRepository::select_by_id_in(tx, &device.serial).await? {
                // Over MTP the device only gives its model
                device.name = stored.name;
                if !stored.usb {
                    DeviceRepository::update()
                        .set(entity::columns::USB, true.into())
                        .where_(Where::Eq(
                            entity::columns::SERIAL,
                            stored.serial.clone().into(),
                        ))
                        .execute_in(tx)
                        .await
                        .map(|_| ())?
                }
            } else {
                DeviceRepository::insert()
                    .item(device)
                    .execute_in(tx)
                    .await
                    .map(|_| ())?
            };
        Ok(())
    }

    /// Moves the last activity sync of the device `serial` to `mark`, unless it is already past it.
    pub async fn advance_activity_sync(
        tx: &rusqlite_orm::rusqlite::Transaction<'_>,
        serial: &str,
        mark: u32,
    ) -> rusqlite_orm::errors::Result<()> {
        DeviceRepository::update()
            .set(entity::columns::LAST_ACTIVITY_SYNC, mark.into())
            .where_(Where::And(vec![
                Where::Eq(entity::columns::SERIAL, serial.into()),
                Where::Or(vec![
                    Where::Null(entity::columns::LAST_ACTIVITY_SYNC),
                    Where::Lt(entity::columns::LAST_ACTIVITY_SYNC, mark.into()),
                ]),
            ]))
            .execute_in(tx)
            .await?;
        Ok(())
    }

    /// The registered device with that serial number, `None` if it isn't registered.
    pub async fn get(
        conn: &Connection,
        serial: &str,
    ) -> rusqlite_orm::errors::Result<Option<Device>> {
        DeviceRepository::select_by_id_in(conn, serial).await
    }

    /// Every registered device, the Connect pseudo-device included.
    pub async fn get_all(conn: &Connection) -> rusqlite_orm::errors::Result<Vec<Device>> {
        DeviceRepository::select().fetch_in(conn).await
    }

    /// The registered devices among `serials`.
    pub async fn get_by_serials(
        conn: &Connection,
        serials: &HashSet<String>,
    ) -> rusqlite_orm::errors::Result<Vec<Device>> {
        if serials.is_empty() {
            return Ok(Vec::new());
        }
        DeviceRepository::select()
            .where_(Where::In(
                entity::columns::SERIAL,
                serials.iter().map(|s| s.clone().into()).collect(),
            ))
            .fetch_in(conn)
            .await
    }

    /// Serial numbers of every registered device.
    pub async fn get_serials(conn: &Connection) -> rusqlite_orm::errors::Result<HashSet<String>> {
        Ok(DeviceRepository::select()
            .columns(&[entity::columns::SERIAL])
            .fetch_in(conn)
            .await?
            .iter()
            .map(|row| row.get_text(entity::columns::SERIAL.as_ref()))
            .collect())
    }
}

impl TryFrom<&DeviceInfo> for Device {
    type Error = ();

    fn try_from(info: &DeviceInfo) -> Result<Self, Self::Error> {
        if info.manufacturer.to_uppercase() != constants::MTP_GARMIN_MANUFACTURER {
            return Err(());
        }

        Ok(Device {
            serial: info.serial_number.clone(),
            model: info.model.clone(),
            name: info.model.clone(),
            last_activity_sync: None,
            last_body_metric_sync: None,
            usb: true,
            connect: false,
            strength_workout: true,
        })
    }
}

impl From<&DeviceSummary> for Device {
    fn from(info: &DeviceSummary) -> Self {
        Device {
            serial: Device::serial_from_unit_id(info.unit_id),
            model: info.display_name.clone(),
            name: info.display_name.clone(),
            last_activity_sync: None,
            last_body_metric_sync: None,
            usb: false,
            connect: true,
            strength_workout: info.strength_workout_capable,
        }
    }
}
