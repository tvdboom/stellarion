use super::*;

#[test]
fn every_unit_keeps_its_persisted_identifier_and_round_trips_as_an_army_key() {
    let army: Army = Unit::all().into_iter().flatten().map(|unit| (unit, 1)).collect();
    for unit in army.keys() {
        let encoded = serde_json::to_string(unit).unwrap();
        assert_eq!(encoded, format!("\"{unit:?}\""));
        assert_eq!(serde_json::from_str::<Unit>(&encoded).unwrap(), *unit);
        assert_eq!(
            serde_json::from_value::<Unit>(serde_json::json!(format!("{unit:?}"))).unwrap(),
            *unit
        );
    }
    let encoded = serde_json::to_string(&army).unwrap();
    assert_eq!(serde_json::from_str::<Army>(&encoded).unwrap(), army);
}

#[test]
fn unit_decoding_rejects_wrong_categories_malformed_names_and_non_strings() {
    for value in [
        serde_json::json!("Ship(MetalMine)"),
        serde_json::json!("Building(LightFighter)"),
        serde_json::json!("Defense(Unknown)"),
        serde_json::json!("Ship(LightFighter))"),
        serde_json::json!("Ship(LightFighter"),
        serde_json::json!("LightFighter"),
        serde_json::json!("ship(LightFighter)"),
        serde_json::json!(" Ship(LightFighter)"),
        serde_json::json!(null),
        serde_json::json!(42),
        serde_json::json!({"Ship": "LightFighter"}),
    ] {
        assert!(serde_json::from_value::<Unit>(value).is_err());
    }
}

#[test]
fn orbital_intelligence_requirements_are_distinct_from_production() {
    for (unit, expected) in [
        (Unit::Building(Building::SolarSatellite), Some(1)),
        (Unit::Building(Building::Recycler), Some(2)),
        (Unit::Building(Building::SensorPhalanx), Some(2)),
        (Unit::Building(Building::CommandRelay), Some(3)),
        (Unit::Building(Building::JumpGate), Some(4)),
        (Unit::Building(Building::OrbitalRailgun), None),
        (Unit::space_dock(), None),
    ] {
        assert_eq!(unit.intelligence_level(), expected, "{unit:?}");
    }

    assert_eq!(Unit::Building(Building::CommandRelay).production(), 1);
    assert_eq!(Unit::Building(Building::JumpGate).production(), 1);
}
