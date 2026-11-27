use serde::Serialize;

use crate::dao::device::Device;

#[derive(Serialize, Clone)]
pub struct DeviceListItem {
    pub model: String,
    /// Name the device is shown by; its model until the user renames it
    pub name: String,
    pub serial_number: String,
    pub strength_workout: bool,
    /// When its activities were last imported, `None` if they never were
    pub last_activity_sync: Option<u32>,
    /// Whether it has ever been plugged by USB
    pub usb_registered: bool,
    /// Whether it is linked to the Garmin Connect account
    pub connect_registered: bool,
}

impl From<&Device> for DeviceListItem {
    fn from(value: &Device) -> Self {
        Self {
            model: value.model.clone(),
            name: value.name.clone(),
            serial_number: value.serial.clone(),
            strength_workout: value.strength_workout,
            last_activity_sync: value.last_activity_sync,
            usb_registered: value.usb,
            connect_registered: value.connect,
        }
    }
}
