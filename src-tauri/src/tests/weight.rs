use crate::utils::weight::{from_kg, to_kg};

#[test]
fn converts_between_kg_and_tenths() {
    assert_eq!(from_kg(12.5), 125);
    assert_eq!(from_kg(12.1875), 122);
    assert_eq!(from_kg(12.199_999), 122);
    assert_eq!(from_kg(-1.0), 0);
    assert_eq!(from_kg(f32::NAN), 0);
    assert_eq!(from_kg(10_000.0), u16::MAX);
    assert_eq!(to_kg(125), 12.5);
    assert_eq!(from_kg(to_kg(805)), 805);
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
struct Load {
    #[serde(with = "crate::utils::weight::kg")]
    weight: u16,
    #[serde(with = "crate::utils::weight::kg_opt")]
    target: Option<u16>,
}

#[test]
fn serializes_tenths_as_kg() {
    let load = Load {
        weight: 125,
        target: None,
    };
    let json = serde_json::to_string(&load).unwrap();
    assert_eq!(json, r#"{"weight":12.5,"target":null}"#);
    assert_eq!(serde_json::from_str::<Load>(&json).unwrap(), load);
    assert_eq!(
        serde_json::from_str::<Load>(r#"{"weight":80,"target":62.5}"#).unwrap(),
        Load {
            weight: 800,
            target: Some(625)
        }
    );
}
