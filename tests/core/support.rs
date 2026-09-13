//! Shared fixtures with neutral outcomes; tests explicitly set identities and observed results.

use crate::core::combat::report::MissionReport;
use crate::core::map::planet::Planet;
use crate::core::missions::Mission;

pub(crate) fn empty_report(mission: Mission, planet: Planet) -> MissionReport {
    MissionReport {
        id: 0,
        turn: 0,
        mission,
        planet,
        scout_probes: 0,
        surviving_attacker: Default::default(),
        surviving_defender: Default::default(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: None,
        destination_controlled: None,
        combat_report: None,
        hidden: false,
    }
}
