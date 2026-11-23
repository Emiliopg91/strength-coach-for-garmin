//! Weights are stored and handled as integer tenths of a kg (12.5 kg -> 125); they cross the IPC
//! boundary and FIT files in kg.

/// Tenths of a kg to kg.
pub fn to_kg(tenths: u16) -> f32 {
    f32::from(tenths) / 10.0
}

/// Kg to the nearest tenth of a kg, saturating to the `u16` range.
pub fn from_kg(kg: f32) -> u16 {
    // `as` saturates: negatives and NaN go to 0, values over the range to u16::MAX
    (kg * 10.0).round() as u16
}

/// Serde adapter for a `u16` in tenths of a kg that travels as kg.
pub mod kg {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u16, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f32(super::to_kg(*value))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
        f32::deserialize(deserializer).map(super::from_kg)
    }
}

/// Serde adapter for an `Option<u16>` in tenths of a kg that travels as kg.
pub mod kg_opt {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<u16>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(v) => serializer.serialize_some(&super::to_kg(*v)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u16>, D::Error> {
        Option::<f32>::deserialize(deserializer).map(|v| v.map(super::from_kg))
    }
}
