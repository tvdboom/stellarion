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
fn orbital_production_levels_follow_shop_order_and_match_non_public_intelligence() {
    let expected = [
        (Unit::Building(Building::SolarSatellite), 1, Some(1)),
        (Unit::Building(Building::Recycler), 1, Some(1)),
        (Unit::Building(Building::CommandRelay), 2, Some(2)),
        (Unit::Building(Building::TradingPost), 3, Some(3)),
        (Unit::Building(Building::SensorPhalanx), 3, Some(3)),
        (Unit::Building(Building::JumpGate), 4, Some(4)),
        (Unit::Building(Building::OrbitalRailgun), 5, None),
        (Unit::space_dock(), 5, None),
    ];

    assert_eq!(Unit::orbitals(), expected.iter().map(|(unit, _, _)| *unit).collect::<Vec<_>>());
    for (unit, production, intelligence) in expected {
        assert_eq!(unit.production(), production, "{unit:?}");
        assert_eq!(unit.intelligence_level(), intelligence, "{unit:?}");
    }
}
