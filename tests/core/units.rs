use super::*;

#[test]
fn flat_roster_preserves_group_order_and_contains_each_unit_once() {
    let flat = Unit::iter().collect::<Vec<_>>();
    assert_eq!(flat, Unit::all().into_iter().flatten().collect::<Vec<_>>());
    assert_eq!(flat.len(), flat.iter().collect::<std::collections::BTreeSet<_>>().len());
}

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
        (Unit::Building(Building::OrbitalRailgun), 5, Some(5)),
        (Unit::space_dock(), 5, None),
    ];

    assert_eq!(Unit::orbitals(), expected.iter().map(|(unit, _, _)| *unit).collect::<Vec<_>>());
    for (unit, production, intelligence) in expected {
        assert_eq!(unit.production(), production, "{unit:?}");
        assert_eq!(unit.intelligence_level(), intelligence, "{unit:?}");
    }
}

#[test]
fn orbital_and_lunar_stats_follow_actual_range_and_spy_rules() {
    for (building, expected) in [
        (Building::SolarSatellite, "---"),
        (Building::Recycler, "---"),
        (Building::CommandRelay, "---"),
        (Building::TradingPost, "1.5"),
        (Building::SensorPhalanx, "1"),
        (Building::JumpGate, "---"),
        (Building::OrbitalRailgun, "2"),
        (Building::OrbitalRadar, "1.2"),
    ] {
        assert_eq!(Unit::Building(building).get_stat(&CombatStats::Range), expected);
    }
    assert_eq!(Unit::space_dock().get_stat(&CombatStats::Range), "---");
    for (building, intelligence) in [
        (Building::LunarBase, 1),
        (Building::TidalGenerator, 2),
        (Building::Shipyard, 2),
        (Building::Laboratory, 3),
        (Building::OrbitalRadar, 4),
    ] {
        let unit = Unit::Building(building);
        assert_eq!(unit.intelligence_level_on_world(true), Some(intelligence));
    }
    assert_eq!(Unit::Building(Building::Shipyard).intelligence_level_on_world(false), Some(3));
    let shipyard = Unit::Building(Building::Shipyard);
    assert!(shipyard.revealed_by_probes_on_world(6, true));
    assert!(!shipyard.revealed_by_probes_on_world(6, false));
}
