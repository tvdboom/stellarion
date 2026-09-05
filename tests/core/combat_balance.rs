//! Reproducible balance experiments using the real Rust combat resolver.
//!
//! Fast correctness checks run with `cargo test --lib --no-default-features`.
//! Run the complete statistical survey explicitly:
//! `cargo test --lib --no-default-features balance_survey -- --ignored --nocapture`
//! `STELLARION_BALANCE_SEEDS` overrides 256 trials per scenario; optional
//! `STELLARION_BALANCE_FILTER` selects scenario IDs containing that substring.
//! Results and the exact rosters/stat fingerprint go to ignored `target/combat-balance/`.
//!
//! A passed test means valid, deterministic mechanics, NOT that the roster is balanced.
//! Counters should win favorable matchups; measured outcomes are not golden assertions.
//! Raw budgets value M/C/D equally. Scarcity budgets use 1/1.5/2 as an explicit
//! sensitivity assumption, not a canonical exchange rate. Fuel budgets include a
//! 10-AU launch, with no Reactor. Production and minimum infrastructure are reported
//! separately. Support sweeps deliberately add budget; fixed-budget alternatives are
//! included too. Ordinary buildings are capital at risk, not combatants.

use std::fmt::Write as _;

use bevy::math::Vec2;
use serde::Serialize;

use super::{
    resolve_combat_with_rng, BOMBING_HIT_CHANCE, MAX_BOMBING_LEVELS_PER_BUILDING, MAX_COMBAT_ROUNDS,
};
use crate::core::combat::report::MissionReport;
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::orders::validate_mission;
use crate::core::player::Player;
use crate::core::random::DeterministicRngState;
use crate::core::resources::Resources;
use crate::core::simulation::{resolve_turn, GameModel, GameRules, TurnCommand, TurnSubmission};
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Price, Unit};

const COMBAT_SHIPS: [Ship; 8] = [
    Ship::LightFighter,
    Ship::HeavyFighter,
    Ship::Destroyer,
    Ship::Cruiser,
    Ship::Bomber,
    Ship::Battleship,
    Ship::Dreadnought,
    Ship::WarSun,
];
const TURRETS: [Defense; 6] = [
    Defense::RocketLauncher,
    Defense::LightLaser,
    Defense::HeavyLaser,
    Defense::GaussCannon,
    Defense::IonCannon,
    Defense::PlasmaTurret,
];

#[derive(Clone, Copy, Debug, Serialize)]
enum Budget {
    Raw,
    Scarcity,
    Fuel,
}

impl Budget {
    fn value(self, unit: Unit) -> f64 {
        let p = unit.price();
        match self {
            Self::Raw => total(p),
            Self::Scarcity => p.metal as f64 + 1.5 * p.crystal as f64 + 2.0 * p.deuterium as f64,
            Self::Fuel => total(p) + 10.0 * unit.fuel_consumption() as f64,
        }
    }

    fn army_value(self, army: &Army) -> f64 {
        army.iter().map(|(u, n)| self.value(*u) * *n as f64).sum()
    }
}

#[derive(Clone, Serialize)]
struct Scenario {
    id: String,
    category: String,
    attacker: Army,
    defender: Army,
    objective: Icon,
    bombing: BombingRaid,
    combat_probes: bool,
    diameter: usize,
    moon: bool,
    budget: Budget,
}

impl Scenario {
    fn new(category: &str, id: impl Into<String>, attacker: Army, defender: Army) -> Self {
        Self {
            id: format!("{category}/{}", id.into()),
            category: category.to_string(),
            attacker,
            defender,
            objective: Icon::Attack,
            bombing: BombingRaid::None,
            combat_probes: true,
            diameter: 10_000,
            moon: false,
            budget: Budget::Raw,
        }
    }

    fn fixture(&self) -> (Planet, Mission) {
        let mut rng = DeterministicRngState::from_u64(90210).next_rng();
        let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
        origin.colonize(1);
        origin.army = self.attacker.clone();
        let mut destination = Planet::new_with_rng(
            1,
            "Target".into(),
            Vec2::X * Planet::SIZE * 11.4,
            self.moon,
            1.,
            &mut rng,
        );
        destination.diameter = self.diameter;
        destination.controlled = Some(if self.objective == Icon::Deploy {
            1
        } else {
            2
        });
        destination.owned = (!self.moon).then_some(destination.controlled.unwrap());
        destination.army = self.defender.clone();
        let mission = Mission::new_with_id(
            100,
            1,
            1,
            &origin,
            &destination,
            self.objective,
            self.attacker.clone(),
            self.bombing.clone(),
            self.combat_probes,
            false,
            None,
        );
        validate_mission(&Player::new(1, 0), &origin, &destination, &mission)
            .unwrap_or_else(|e| panic!("invalid scenario {}: {e}", self.id));
        for (unit, count) in &destination.army {
            assert!(*count > 0 && unit.valid_on(self.moon), "{}: invalid defender", self.id);
            if unit.is_building() {
                assert!(*count <= Building::MAX_LEVEL);
            }
        }
        assert!(destination.army.amount(&Unit::space_dock()) <= 1);
        (destination, mission)
    }
}

fn total(p: Resources) -> f64 {
    (p.metal + p.crystal + p.deuterium) as f64
}
fn cost(army: &Army) -> Resources {
    army.iter().map(|(u, n)| u.price() * *n).sum()
}
fn army(units: &[(Unit, usize)]) -> Army {
    units.iter().copied().filter(|(_, n)| *n > 0).collect()
}
fn ships(ship: Ship, n: usize) -> Army {
    army(&[(Unit::Ship(ship), n)])
}
fn defense(unit: Defense, n: usize) -> Army {
    army(&[(Unit::Defense(unit), n)])
}
fn affordable(unit: Unit, budget: usize, valuation: Budget) -> Army {
    army(&[(unit, (budget as f64 / valuation.value(unit)).floor() as usize)])
}

// Weights are fractions of spending, not unit-count ratios. Unspent rounding is reported.
fn composition(budget: usize, shares: &[(Unit, usize)]) -> Army {
    let weight: usize = shares.iter().map(|(_, w)| w).sum();
    shares
        .iter()
        .filter_map(|(u, w)| {
            let n = (budget as f64 * *w as f64 / weight as f64 / total(u.price())).floor() as usize;
            (n > 0).then_some((*u, n))
        })
        .collect()
}

fn scenarios() -> Vec<Scenario> {
    use Building as B;
    use Defense as D;
    use Ship as S;
    let mut all = Vec::new();
    // All ordered pairs include side reversal and same-type controls at three scales.
    for (valuation, budgets) in [
        (Budget::Raw, &[1800, 6000, 18000][..]),
        (Budget::Scarcity, &[6000][..]),
        (Budget::Fuel, &[6000][..]),
    ] {
        for &budget in budgets {
            for a in COMBAT_SHIPS {
                for b in COMBAT_SHIPS {
                    let mut s = Scenario::new(
                        "fleet",
                        format!("{valuation:?}-{budget}-{a:?}-vs-{b:?}"),
                        affordable(Unit::Ship(a), budget, valuation),
                        affordable(Unit::Ship(b), budget, valuation),
                    );
                    s.budget = valuation;
                    all.push(s);
                }
            }
        }
    }
    for budget in [1800, 6000, 18000] {
        for a in COMBAT_SHIPS {
            for b in TURRETS {
                all.push(Scenario::new(
                    "defense",
                    format!("{budget}-{a:?}-vs-{b:?}"),
                    affordable(Unit::Ship(a), budget, Budget::Raw),
                    affordable(Unit::Defense(b), budget, Budget::Raw),
                ));
            }
        }
    }
    // Exact multiples of the War Sun price remove its large unspent-budget gap at 6000.
    for budget in [7000, 14000] {
        for opponent in COMBAT_SHIPS {
            let sun = affordable(Unit::war_sun(), budget, Budget::Raw);
            let other = affordable(Unit::Ship(opponent), budget, Budget::Raw);
            all.push(Scenario::new(
                "sun-matched",
                format!("{budget}-WarSun-vs-{opponent:?}"),
                sun.clone(),
                other.clone(),
            ));
            if opponent != S::WarSun {
                all.push(Scenario::new(
                    "sun-matched",
                    format!("{budget}-{opponent:?}-vs-WarSun"),
                    other,
                    sun,
                ));
            }
        }
    }
    for a in COMBAT_SHIPS {
        all.push(Scenario::new(
            "dock",
            format!("{a:?}"),
            affordable(Unit::Ship(a), 2400, Budget::Raw),
            defense(D::SpaceDock, 1),
        ));
    }
    let mixes = [
        ("fighters", vec![(Unit::Ship(S::LightFighter), 1), (Unit::Ship(S::HeavyFighter), 1)]),
        ("screened-cruisers", vec![(Unit::Ship(S::LightFighter), 1), (Unit::Ship(S::Cruiser), 3)]),
        (
            "screened-battleships",
            vec![(Unit::Ship(S::LightFighter), 1), (Unit::Ship(S::Battleship), 3)],
        ),
        ("heavy-hunters", vec![(Unit::Ship(S::Destroyer), 1), (Unit::Ship(S::Dreadnought), 3)]),
        ("siege", vec![(Unit::Ship(S::Cruiser), 1), (Unit::Ship(S::Bomber), 1)]),
        ("screened-sun", vec![(Unit::Ship(S::LightFighter), 1), (Unit::Ship(S::WarSun), 3)]),
    ];
    for (an, a) in &mixes {
        for (bn, b) in &mixes {
            all.push(Scenario::new(
                "mixed",
                format!("{an}-vs-{bn}"),
                composition(7000, a),
                composition(7000, b),
            ));
        }
        for b in [S::Cruiser, S::Battleship, S::Dreadnought, S::WarSun] {
            let sa = composition(7000, a);
            let sb = affordable(Unit::Ship(b), 7000, Budget::Raw);
            all.push(Scenario::new("mixed", format!("{an}-vs-{b:?}"), sa.clone(), sb.clone()));
            all.push(Scenario::new("mixed", format!("{b:?}-vs-{an}"), sb, sa));
        }
    }
    // Fixed-spend defensive alternatives distinguish free extra support from real choices.
    let batteries = [
        ("gauss", vec![(Unit::Defense(D::GaussCannon), 1)]),
        (
            "screened-gauss",
            vec![(Unit::Defense(D::RocketLauncher), 1), (Unit::Defense(D::GaussCannon), 3)],
        ),
        ("crawler-gauss", vec![(Unit::Defense(D::Crawler), 1), (Unit::Defense(D::GaussCannon), 3)]),
        (
            "crawler-plasma",
            vec![(Unit::Defense(D::Crawler), 1), (Unit::Defense(D::PlasmaTurret), 3)],
        ),
        (
            "layered",
            vec![
                (Unit::Defense(D::RocketLauncher), 1),
                (Unit::Defense(D::Crawler), 1),
                (Unit::Defense(D::IonCannon), 2),
                (Unit::Defense(D::PlasmaTurret), 2),
            ],
        ),
    ];
    for a in [S::Cruiser, S::Bomber, S::Battleship, S::WarSun] {
        for (name, mix) in &batteries {
            all.push(Scenario::new(
                "battery",
                format!("{a:?}-vs-{name}"),
                affordable(Unit::Ship(a), 7000, Budget::Raw),
                composition(7000, mix),
            ));
        }
        for shield in [0, 1, 3, 5] {
            let mut defenders =
                affordable(Unit::Defense(D::GaussCannon), 7000 - shield * 500, Budget::Raw);
            if shield > 0 {
                defenders.insert(Unit::planetary_shield(), shield);
            }
            all.push(Scenario::new(
                "shield-budget",
                format!("{a:?}-shield-{shield}"),
                affordable(Unit::Ship(a), 7000, Budget::Raw),
                defenders,
            ));
        }
    }
    for a in [S::Cruiser, S::Bomber, S::WarSun] {
        for crawlers in [0, 5, 15, 30] {
            let mut defenders = defense(D::GaussCannon, 20);
            if crawlers > 0 {
                defenders.insert(Unit::crawler(), crawlers);
            }
            all.push(Scenario::new(
                "crawler-added",
                format!("{a:?}-{crawlers}"),
                affordable(Unit::Ship(a), 4000, Budget::Raw),
                defenders,
            ));
        }
    }
    // Production-limited comparison: same mature shipyard time, NOT equal resources.
    for a in COMBAT_SHIPS {
        all.push(Scenario::new(
            "production",
            format!("{a:?}-vs-Cruiser"),
            ships(a, 50 / a.production()),
            ships(S::Cruiser, 50 / S::Cruiser.production()),
        ));
    }
    for raid in [BombingRaid::None, BombingRaid::Economic, BombingRaid::Industrial] {
        for shield in [0, 1, 3, 5] {
            for guarded in [false, true] {
                let mut d = army(&[
                    (Unit::Building(B::MetalMine), 5),
                    (Unit::Building(B::CrystalMine), 5),
                    (Unit::Building(B::DeuteriumSynthesizer), 5),
                    (Unit::Building(B::Shipyard), 5),
                    (Unit::Building(B::Factory), 5),
                    (Unit::Building(B::MissileSilo), 5),
                    (Unit::planetary_shield(), shield),
                ]);
                if guarded {
                    d.extend(defense(D::GaussCannon, 12));
                }
                let mut s = Scenario::new(
                    "bombing",
                    format!("{raid:?}-shield-{shield}-guarded-{guarded}"),
                    ships(S::Bomber, 12),
                    d,
                );
                s.bombing = raid.clone();
                all.push(s);
            }
        }
    }
    // Raid damage scales with surviving Bombers, not battle duration. Include large
    // fleets to measure how often the nine-level ceiling is actually reached.
    for raid in [BombingRaid::Economic, BombingRaid::Industrial] {
        for bombers in [1, 5, 10, 20, 30, 50, 90, 150] {
            for shield in [0, 5] {
                let targets = match raid {
                    BombingRaid::Economic => Unit::resource_buildings(),
                    BombingRaid::Industrial => Unit::industrial_buildings(),
                    BombingRaid::None => unreachable!(),
                };
                let mut defenders: Army = targets.into_iter().map(|unit| (unit, 5)).collect();
                if shield > 0 {
                    defenders.insert(Unit::planetary_shield(), shield);
                }
                let mut s = Scenario::new(
                    "bombing-size",
                    format!("{raid:?}-{bombers}-shield-{shield}"),
                    ships(S::Bomber, bombers),
                    defenders,
                );
                s.bombing = raid.clone();
                all.push(s);
            }
        }
    }
    for target in TURRETS.into_iter().chain([D::Crawler, D::SpaceDock]) {
        all.push({
            let mut s = Scenario::new(
                "missile",
                format!("equal-budget-{target:?}"),
                defense(D::InterplanetaryMissile, 10),
                if target == D::SpaceDock {
                    defense(target, 1)
                } else {
                    affordable(Unit::Defense(target), 2300, Budget::Raw)
                },
            );
            s.objective = Icon::MissileStrike;
            s
        });
    }
    for interceptors in [0, 5, 10, 20, 30, 40] {
        let mut s = Scenario::new(
            "missile",
            format!("interceptors-{interceptors}"),
            defense(D::InterplanetaryMissile, 10),
            army(&[
                (Unit::Defense(D::PlasmaTurret), 5),
                (Unit::antiballistic_missile(), interceptors),
                (Unit::planetary_shield(), 5),
                (Unit::Ship(S::Cruiser), 5),
                (Unit::space_dock(), 1),
            ]),
        );
        s.objective = Icon::MissileStrike;
        all.push(s);
    }
    for probes in [5, 15, 30, 60] {
        for (name, d) in [
            ("rockets", defense(D::RocketLauncher, 20)),
            ("destroyers", ships(S::Destroyer, 5)),
            ("dock", defense(D::SpaceDock, 1)),
            ("empty", Army::new()),
        ] {
            let mut s =
                Scenario::new("spy", format!("{probes}-vs-{name}"), ships(S::Probe, probes), d);
            s.objective = Icon::Spy;
            s.combat_probes = false;
            all.push(s);
        }
    }
    for stay in [false, true] {
        for probes in [0, 20, 60] {
            let mut a = ships(S::Cruiser, 15);
            if probes > 0 {
                a.insert(Unit::probe(), probes);
            }
            let mut s = Scenario::new(
                "probe-screen",
                format!("{probes}-stay-{stay}"),
                a,
                ships(S::Battleship, 10),
            );
            s.combat_probes = stay;
            all.push(s);
        }
    }
    for escort in [0, 5, 10, 20] {
        for (name, d) in [
            ("empty", Army::new()),
            ("rockets", defense(D::RocketLauncher, 20)),
            ("gauss", defense(D::GaussCannon, 10)),
        ] {
            let mut a = ships(S::Cruiser, escort);
            a.insert(Unit::colony_ship(), 1);
            let mut s = Scenario::new("colonize", format!("escort-{escort}-{name}"), a, d);
            s.objective = Icon::Colonize;
            all.push(s);
        }
    }
    for suns in [1, 3, 5] {
        for diameter in [1500, 10000, 120000] {
            for (name, d) in [
                ("empty", Army::new()),
                ("fleet", ships(S::Cruiser, 20)),
                ("dock", defense(D::SpaceDock, 1)),
                ("plasma", defense(D::PlasmaTurret, 15)),
            ] {
                if diameter == 1500 && (name == "dock" || name == "plasma") {
                    continue;
                }
                let mut s = Scenario::new(
                    "destroy",
                    format!("{suns}-diameter-{diameter}-{name}"),
                    ships(S::WarSun, suns),
                    d,
                );
                s.objective = Icon::Destroy;
                s.diameter = diameter;
                s.moon = diameter == 1500;
                all.push(s);
            }
        }
    }
    for a in [S::Cruiser, S::WarSun] {
        let mut s =
            Scenario::new("deploy", format!("{a:?}"), ships(a, 5), ships(S::LightFighter, 10));
        s.objective = Icon::Deploy;
        all.push(s);
    }
    all
}

fn losses(before: &Army, after: &Army, include: impl Fn(&Unit) -> bool) -> Resources {
    before
        .iter()
        .filter(|(u, _)| include(u))
        .map(|(u, n)| u.price() * n.saturating_sub(after.amount(u)))
        .sum()
}

fn success(s: &Scenario, r: &MissionReport) -> bool {
    match s.objective {
        Icon::Attack => r.winner() == Some(1),
        Icon::Colonize => r.planet_colonized && r.winner() == Some(1),
        Icon::Destroy => r.planet_destroyed,
        Icon::Spy => r.scout_probes > 0,
        Icon::MissileStrike => {
            total(losses(&s.defender, &r.surviving_defender, |u| {
                u.is_turret() || *u == Unit::crawler()
            })) > 0.
        },
        Icon::Deploy => r.combat_report.is_none() && r.surviving_attacker == s.attacker,
        _ => unreachable!("only mission objectives belong in the survey"),
    }
}

fn check_report(s: &Scenario, r: &MissionReport) {
    for (unit, count) in &r.surviving_attacker {
        assert!(*count <= s.attacker.amount(unit), "{}: created attacker", s.id);
    }
    for (unit, count) in &r.surviving_defender {
        assert!(*count <= s.defender.amount(unit), "{}: created defender", s.id);
    }
    let Some(c) = &r.combat_report else {
        return;
    };
    assert!(c.rounds.len() <= MAX_COMBAT_ROUNDS, "{}", s.id);
    let mut shield =
        s.defender.amount(&Unit::planetary_shield()) * crate::core::constants::PS_SHIELD_PER_LEVEL;
    let mut raid_rounds = 0;
    let mut bombing_losses = Army::new();
    for round in &c.rounds {
        assert!(round.planetary_shield <= shield, "{}: planetary shield regenerated", s.id);
        shield = round.planetary_shield;
        for unit in round.attacker.iter().chain(&round.defender) {
            assert!(unit.hull <= unit.unit.hull() && unit.shield <= unit.unit.shield());
        }
        let mut attempts = 0;
        for unit in &round.attacker {
            let shots = unit.shots.iter().filter(|shot| shot.is_bombing()).collect::<Vec<_>>();
            assert!(shots.len() <= 1, "{}: repeated Bomber attempt", s.id);
            if !shots.is_empty() {
                assert_eq!(unit.unit, Unit::Ship(Ship::Bomber));
                assert!(unit.hull > 0 && round.planetary_shield == 0);
            }
            attempts += shots.len();
            for shot in shots.into_iter().filter(|shot| shot.killed) {
                *bombing_losses.entry(shot.unit.unwrap()).or_default() += 1;
            }
        }
        raid_rounds += usize::from(attempts > 0);
        assert!(raid_rounds <= 1, "{}: bombing repeated across rounds", s.id);
        assert!(bombing_losses.values().all(|n| *n <= 3), "{}: per-building cap exceeded", s.id);
        assert!(bombing_losses.values().sum::<usize>() <= 9, "{}: raid cap exceeded", s.id);
    }
    if s.objective == Icon::MissileStrike {
        assert_eq!(c.rounds.len(), 1);
        for shot in c.rounds[0].attacker.iter().flat_map(|u| &u.shots) {
            assert!(shot.unit.is_some_and(|u| u.is_turret() || u == Unit::crawler()));
            assert_eq!(shot.planetary_shield_damage, 0);
        }
        for (u, n) in &s.defender {
            if u.is_ship() || u.is_building() || *u == Unit::space_dock() {
                assert_eq!(
                    r.surviving_defender.amount(u),
                    *n,
                    "{}: missile damaged protected unit",
                    s.id
                );
            }
        }
    }
    if s.objective == Icon::Spy {
        assert_eq!(c.rounds.len(), 1);
    }
    if !r.planet_destroyed {
        for (u, n) in &s.defender {
            if u.is_building()
                && !(s.bombing == BombingRaid::Economic && u.is_economic_building()
                    || s.bombing == BombingRaid::Industrial && u.is_industrial_building())
            {
                assert_eq!(
                    r.surviving_defender.amount(u),
                    *n,
                    "{}: damaged wrong building category",
                    s.id
                );
            }
        }
    }
}

// Minimum unlock infrastructure only; resource buildings, ownership and construction
// lead time are excluded. Existing defensive buildings are not purchased a second time.
fn infrastructure(army: &Army) -> Resources {
    let mut required = Army::new();
    for (u, n) in army {
        if *n == 0 || u.is_building() {
            continue;
        }
        let b = if u.is_ship() {
            Building::Shipyard
        } else if u.is_missile() {
            Building::MissileSilo
        } else {
            Building::Factory
        };
        let level = required.entry(Unit::Building(b)).or_default();
        *level = (*level).max(u.production());
        if u.is_missile() {
            required.entry(Unit::Building(Building::Factory)).or_insert(1);
        }
    }
    required.iter().map(|(u, n)| u.price() * n.saturating_sub(army.amount(u))).sum()
}

#[derive(Serialize)]
struct ResultRow {
    scenario: Scenario,
    seeds: usize,
    attacker_cost: Resources,
    defender_cost: Resources,
    attacker_valued_cost: f64,
    defender_valued_cost: f64,
    attacker_production: usize,
    defender_production: usize,
    attacker_minimum_infrastructure: Resources,
    defender_minimum_infrastructure: Resources,
    attacker_fuel_at_10_au: usize,
    attacker_win_pct: f64,
    defender_win_pct: f64,
    stalemate_pct: f64,
    mutual_destruction_pct: f64,
    objective_success_pct: f64,
    objective_success_95ci_pct: [f64; 2],
    average_rounds: f64,
    attacker_losses: [f64; 3],
    defender_losses: [f64; 3],
    defender_combat_losses: f64,
    building_levels_destroyed: f64,
    /// Number of battles at each raid damage total, from zero through nine levels.
    bombing_damage_distribution: [usize; 10],
    surviving_probes: f64,
    full_intelligence_pct: f64,
    repaired_hull: f64,
    intercepted_missiles: f64,
    reproduction_seed: u64,
}

fn seed(index: usize) -> u64 {
    0x57e11a_u64.wrapping_add((index as u64).wrapping_mul(104729))
}
fn wilson(successes: usize, n: usize) -> [f64; 2] {
    let n = n as f64;
    let p = successes as f64 / n;
    let z2 = 1.96_f64.powi(2);
    let center = (p + z2 / (2. * n)) / (1. + z2 / n);
    let delta = 1.96 * (p * (1. - p) / n + z2 / (4. * n * n)).sqrt() / (1. + z2 / n);
    [100. * (center - delta).max(0.), 100. * (center + delta).min(1.)]
}

fn measure(s: Scenario, seeds: usize) -> ResultRow {
    let (destination, mission) = s.fixture();
    let mut row = ResultRow {
        attacker_cost: cost(&s.attacker),
        defender_cost: cost(&s.defender),
        attacker_valued_cost: s.budget.army_value(&s.attacker),
        defender_valued_cost: s.budget.army_value(&s.defender),
        attacker_production: s.attacker.total_production(),
        defender_production: s.defender.total_production(),
        attacker_minimum_infrastructure: infrastructure(&s.attacker),
        defender_minimum_infrastructure: infrastructure(&s.defender),
        attacker_fuel_at_10_au: s.attacker.iter().map(|(u, n)| u.fuel_consumption() * n * 10).sum(),
        scenario: s,
        seeds,
        attacker_win_pct: 0.,
        defender_win_pct: 0.,
        stalemate_pct: 0.,
        mutual_destruction_pct: 0.,
        objective_success_pct: 0.,
        objective_success_95ci_pct: [0.; 2],
        average_rounds: 0.,
        attacker_losses: [0.; 3],
        defender_losses: [0.; 3],
        defender_combat_losses: 0.,
        building_levels_destroyed: 0.,
        bombing_damage_distribution: [0; 10],
        surviving_probes: 0.,
        full_intelligence_pct: 0.,
        repaired_hull: 0.,
        intercepted_missiles: 0.,
        reproduction_seed: seed(0),
    };
    let mut successes = 0;
    let s = &row.scenario;
    for i in 0..seeds {
        let r = resolve_combat_with_rng(
            1,
            &mission,
            &destination,
            &mut DeterministicRngState::from_u64(seed(i)).next_rng(),
        );
        check_report(s, &r);
        let pct = 100. / seeds as f64;
        if matches!(s.objective, Icon::Attack | Icon::Colonize | Icon::Destroy) {
            row.attacker_win_pct += if r.winner() == Some(1) {
                pct
            } else {
                0.
            };
            row.defender_win_pct += if r.winner() == Some(2) {
                pct
            } else {
                0.
            };
            row.stalemate_pct += if r.is_stalemate() {
                pct
            } else {
                0.
            };
            let fighting = |a: &Army| {
                a.iter().any(|(u, n)| {
                    *n > 0 && !u.is_building() && !u.is_missile() && *u != Unit::colony_ship()
                })
            };
            if fighting(&s.attacker)
                && fighting(&s.defender)
                && !fighting(&r.surviving_attacker)
                && !fighting(&r.surviving_defender)
            {
                row.mutual_destruction_pct += pct;
            }
        }
        successes += usize::from(success(s, &r));
        let colony_consumed = if s.objective == Icon::Colonize && success(s, &r) {
            Unit::colony_ship().price()
        } else {
            Resources::default()
        };
        for (slot, l) in [
            (
                &mut row.attacker_losses,
                losses(&s.attacker, &r.surviving_attacker, |_| true) + colony_consumed,
            ),
            (&mut row.defender_losses, losses(&s.defender, &r.surviving_defender, |_| true)),
        ] {
            for (x, value) in slot.iter_mut().zip([l.metal, l.crystal, l.deuterium]) {
                *x += value as f64 / seeds as f64;
            }
        }
        row.defender_combat_losses += total(losses(&s.defender, &r.surviving_defender, |u| {
            !u.is_building() && !u.is_missile()
        })) / seeds as f64;
        row.building_levels_destroyed += s
            .defender
            .iter()
            .filter(|(u, _)| u.is_building())
            .map(|(u, n)| n.saturating_sub(r.surviving_defender.amount(u)))
            .sum::<usize>() as f64
            / seeds as f64;
        row.surviving_probes += r.scout_probes as f64 / seeds as f64;
        row.full_intelligence_pct += if r.scout_probes >= 21 {
            pct
        } else {
            0.
        };
        let bombing_damage = r.combat_report.as_ref().map_or(0, |combat| {
            combat
                .rounds
                .iter()
                .flat_map(|round| &round.attacker)
                .flat_map(|unit| &unit.shots)
                .filter(|shot| shot.is_bombing() && shot.killed)
                .count()
        });
        row.bombing_damage_distribution[bombing_damage] += 1;
        if let Some(c) = &r.combat_report {
            row.average_rounds += c.rounds.len() as f64 / seeds as f64;
            for round in &c.rounds {
                row.repaired_hull += round.defender.iter().flat_map(|u| &u.repairs).sum::<usize>()
                    as f64
                    / seeds as f64;
                row.intercepted_missiles += round
                    .defender
                    .iter()
                    .filter(|u| u.unit == Unit::antiballistic_missile())
                    .flat_map(|u| &u.shots)
                    .filter(|shot| shot.killed)
                    .count() as f64
                    / seeds as f64;
            }
        }
    }
    row.objective_success_pct = 100. * successes as f64 / seeds as f64;
    row.objective_success_95ci_pct = wilson(successes, seeds);
    row
}

#[test]
fn scenario_catalog_is_legal_and_covers_every_unit_and_mission() {
    let catalog = scenarios();
    let mut ids = std::collections::BTreeSet::new();
    let mut units = std::collections::BTreeSet::<Unit>::new();
    for s in &catalog {
        assert!(ids.insert(&s.id), "duplicate scenario {}", s.id);
        s.fixture();
        units.extend(s.attacker.keys().chain(s.defender.keys()).copied());
    }
    for unit in Unit::ships().into_iter().chain(Unit::defenses()) {
        assert!(units.contains(&unit), "missing {unit:?}");
    }
    for objective in
        [Icon::Attack, Icon::Colonize, Icon::Spy, Icon::MissileStrike, Icon::Destroy, Icon::Deploy]
    {
        assert!(catalog.iter().any(|s| s.objective == objective));
    }
}

#[test]
fn all_battle_scenarios_obey_combat_invariants() {
    for s in scenarios() {
        measure(s, 4);
    }
}

#[test]
fn representative_missions_replay_identically() {
    for objective in
        [Icon::Attack, Icon::Colonize, Icon::Spy, Icon::MissileStrike, Icon::Destroy, Icon::Deploy]
    {
        let s = scenarios().into_iter().find(|s| s.objective == objective).unwrap();
        let (p, m) = s.fixture();
        let run = || {
            resolve_combat_with_rng(
                1,
                &m,
                &p,
                &mut DeterministicRngState::from_u64(seed(0)).next_rng(),
            )
        };
        assert_eq!(serde_json::to_value(run()).unwrap(), serde_json::to_value(run()).unwrap());
    }
}

fn campaign(seed_value: u64) -> (GameModel, usize, usize) {
    let mut game = GameModel::new([7; 32], GameRules::default()).unwrap();
    game.start().unwrap();
    game.rng = DeterministicRngState::from_u64(seed_value);
    let origin = game.players[0].home_planet;
    let target = game.map.planets.iter().find(|p| p.owned.is_none() && !p.is_moon()).unwrap().id;
    game.map.get_mut(origin).position = Vec2::ZERO;
    game.map.get_mut(target).position = Vec2::X * Planet::SIZE * 2.5;
    game.players[0].resources = Resources::new(100_000, 100_000, 100_000);
    (game, origin, target)
}

fn send(
    id: u64,
    origin: usize,
    target: usize,
    objective: Icon,
    army: Army,
    bombing: BombingRaid,
    jump: bool,
) -> TurnCommand {
    TurnCommand::SendMission {
        mission_id: id,
        origin,
        destination: target,
        objective,
        army,
        bombing,
        combat_probes: false,
        jump_gate: jump,
    }
}

fn turn(game: &mut GameModel, commands: Vec<TurnCommand>) {
    resolve_turn(
        game,
        &[TurnSubmission::new(1, game.turn, commands), TurnSubmission::new(2, game.turn, vec![])],
    )
    .unwrap();
}

#[test]
fn same_turn_missiles_resolve_before_spying_and_colonizing() {
    for i in 0..32 {
        let (mut game, origin, target) = campaign(seed(i));
        game.map.get_mut(origin).army.extend(army(&[
            (Unit::interplanetary_missile(), 10),
            (Unit::probe(), 30),
            (Unit::Ship(Ship::Cruiser), 20),
            (Unit::colony_ship(), 1),
        ]));
        game.map.get_mut(target).colonize(2);
        game.map.get_mut(target).army = defense(Defense::RocketLauncher, 20);
        turn(
            &mut game,
            vec![
                send(
                    101,
                    origin,
                    target,
                    Icon::Colonize,
                    army(&[(Unit::Ship(Ship::Cruiser), 20), (Unit::colony_ship(), 1)]),
                    BombingRaid::None,
                    false,
                ),
                send(
                    102,
                    origin,
                    target,
                    Icon::Spy,
                    ships(Ship::Probe, 30),
                    BombingRaid::None,
                    false,
                ),
                send(
                    103,
                    origin,
                    target,
                    Icon::MissileStrike,
                    defense(Defense::InterplanetaryMissile, 10),
                    BombingRaid::None,
                    false,
                ),
            ],
        );
        let reports = &game.players[0].reports;
        assert_eq!(
            reports.iter().map(|r| r.mission.objective).collect::<Vec<_>>(),
            [Icon::MissileStrike, Icon::Spy, Icon::Colonize]
        );
        let remaining =
            reports[0].surviving_defender.amount(&Unit::Defense(Defense::RocketLauncher));
        assert_eq!(
            reports[1].planet.army.amount(&Unit::Defense(Defense::RocketLauncher)),
            remaining
        );
        assert_eq!(
            reports[2].planet.army.amount(&Unit::Defense(Defense::RocketLauncher)),
            remaining
        );
        assert_eq!(game.map.get(target).owned, Some(1));
        assert_eq!(game.map.get(target).army.amount(&Unit::colony_ship()), 0);
        assert_eq!(game.map.get(origin).army.amount(&Unit::interplanetary_missile()), 0);
    }
}

#[test]
fn bombing_losses_persist_for_both_target_categories() {
    for category in [BombingRaid::Economic, BombingRaid::Industrial] {
        let mut losses_observed = 0;
        for i in 0..32 {
            let (mut game, origin, target) = campaign(seed(i));
            game.map.get_mut(origin).army.extend(ships(Ship::Bomber, 12));
            game.map.get_mut(target).colonize(2);
            game.map.get_mut(target).army = army(&[
                (Unit::Building(Building::MetalMine), 5),
                (Unit::Building(Building::Factory), 5),
                (Unit::Defense(Defense::GaussCannon), 12),
                (Unit::planetary_shield(), 1),
            ]);
            turn(
                &mut game,
                vec![send(
                    101,
                    origin,
                    target,
                    Icon::Attack,
                    ships(Ship::Bomber, 12),
                    category.clone(),
                    false,
                )],
            );
            let report = game.players[0].reports.last().unwrap();
            for unit in [Unit::Building(Building::MetalMine), Unit::Building(Building::Factory)] {
                assert_eq!(
                    game.map.get(target).army.amount(&unit),
                    report.surviving_defender.amount(&unit)
                );
                let targeted = if category == BombingRaid::Economic {
                    unit.is_economic_building()
                } else {
                    unit.is_industrial_building()
                };
                if targeted {
                    losses_observed += 5 - report.surviving_defender.amount(&unit);
                } else {
                    assert_eq!(report.surviving_defender.amount(&unit), 5);
                }
            }
        }
        assert!(losses_observed > 0, "scenario must actually exercise bombing damage");
    }
}

#[test]
fn destroy_attempts_return_survivors_and_only_success_erases_planet() {
    let mut successes = 0;
    for i in 0..128 {
        let (mut game, origin, target) = campaign(seed(i));
        game.map.get_mut(origin).army.extend(ships(Ship::WarSun, 1));
        game.map.get_mut(target).colonize(2);
        turn(
            &mut game,
            vec![send(
                101,
                origin,
                target,
                Icon::Destroy,
                ships(Ship::WarSun, 1),
                BombingRaid::None,
                false,
            )],
        );
        let report = game.players[0].reports.last().unwrap();
        successes += usize::from(report.planet_destroyed);
        assert_eq!(game.map.get(target).is_destroyed, report.planet_destroyed);
        assert_eq!(
            game.map.get(target).owned,
            if report.planet_destroyed {
                None
            } else {
                Some(2)
            }
        );
        assert_eq!(game.missions.len(), 1);
        assert_eq!(game.missions[0].objective, Icon::Deploy);
        assert_eq!(game.missions[0].destination, origin);
        assert_eq!(game.missions[0].army.amount(&Unit::war_sun()), 1);
    }
    assert!(successes > 0 && successes < 128, "exercise successful and failed destruction");
}

#[test]
fn scouts_return_without_gaining_control() {
    let (mut game, origin, target) = campaign(seed(0));
    game.map.get_mut(origin).army.extend(ships(Ship::Probe, 30));
    game.map.get_mut(target).colonize(2);
    game.map.get_mut(target).army = defense(Defense::RocketLauncher, 5);
    turn(
        &mut game,
        vec![send(
            101,
            origin,
            target,
            Icon::Spy,
            ships(Ship::Probe, 30),
            BombingRaid::None,
            false,
        )],
    );
    let report = game.players[0].reports.last().unwrap();
    assert!(report.scout_probes >= 21);
    assert_eq!(game.map.get(target).owned, Some(2));
    assert_eq!(game.map.get(target).controlled, Some(2));
    assert_eq!(game.missions.len(), 1);
    assert_eq!(game.missions[0].army.amount(&Unit::probe()), report.scout_probes);
    assert_eq!(game.missions[0].destination, origin);
}

#[test]
fn bounded_stalemate_preserves_defender_and_returns_colonization_fleet() {
    let (mut game, origin, target) = campaign(seed(0));
    let fleet = army(&[(Unit::probe(), 1), (Unit::colony_ship(), 1)]);
    game.map.get_mut(origin).army.extend(fleet.clone());
    game.map.get_mut(target).colonize(2);
    game.map.get_mut(target).army = ships(Ship::Probe, 1);
    let mut order = send(101, origin, target, Icon::Colonize, fleet, BombingRaid::None, false);
    if let TurnCommand::SendMission {
        combat_probes,
        ..
    } = &mut order
    {
        *combat_probes = true;
    }
    turn(&mut game, vec![order]);
    let report = game.players[0].reports.last().unwrap();
    assert!(report.is_stalemate());
    // The resolver omits the animation report when neither side fires any shots.
    assert!(report.combat_report.is_none());
    assert_eq!(game.map.get(target).owned, Some(2));
    assert_eq!(game.map.get(target).army.amount(&Unit::probe()), 1);
    assert_eq!(game.missions.len(), 1);
    assert_eq!(game.missions[0].destination, origin);
    assert_eq!(game.missions[0].army.amount(&Unit::colony_ship()), 1);
}

#[test]
fn taking_home_world_wins_match_without_a_colony_ship() {
    let (mut game, origin, _) = campaign(seed(0));
    let target = game.players[1].home_planet;
    game.map.get_mut(target).position = Vec2::X * Planet::SIZE * 2.5;
    game.map.get_mut(target).army.retain(|u, _| u.is_building());
    game.map.get_mut(origin).army.extend(ships(Ship::Cruiser, 1));
    turn(
        &mut game,
        vec![send(
            101,
            origin,
            target,
            Icon::Attack,
            ships(Ship::Cruiser, 1),
            BombingRaid::None,
            false,
        )],
    );
    assert_eq!(game.status, crate::core::simulation::MatchStatus::Finished);
    assert_eq!(game.map.get(target).owned, None);
    assert_eq!(game.map.get(target).controlled, Some(1));
    assert!(game.players[1].spectator);
}

#[test]
fn moon_conquest_applies_nexus_without_destroying_lunar_base() {
    for i in 0..32 {
        let (mut game, origin, _) = campaign(seed(i));
        let target = game.map.moons()[0].id;
        let moon = game.map.get_mut(target);
        moon.position = Vec2::X * Planet::SIZE * 2.5;
        moon.controlled = Some(2);
        moon.army = army(&[
            (Unit::Building(Building::LunarBase), 5),
            (Unit::Building(Building::DemolitionNexus), 3),
            (Unit::Building(Building::Laboratory), 2),
            (Unit::Building(Building::OrbitalRadar), 3),
        ]);
        game.map.get_mut(origin).army.extend(ships(Ship::Cruiser, 1));
        turn(
            &mut game,
            vec![send(
                101,
                origin,
                target,
                Icon::Attack,
                ships(Ship::Cruiser, 1),
                BombingRaid::None,
                false,
            )],
        );
        let moon = game.map.get(target);
        assert_eq!(moon.controlled, Some(1));
        assert_eq!(moon.owned, None);
        assert_eq!(moon.army.amount(&Unit::Building(Building::LunarBase)), 5);
        assert_eq!(moon.army.amount(&Unit::Building(Building::DemolitionNexus)), 3);
        assert_eq!(
            moon.army.amount(&Unit::Building(Building::Laboratory))
                + moon.army.amount(&Unit::Building(Building::OrbitalRadar)),
            2
        );
    }
}

#[test]
fn jump_deployment_moves_production_weighted_fleet_without_fuel_or_combat() {
    let (mut game, origin, target) = campaign(seed(0));
    game.map.get_mut(target).position = Vec2::X * Planet::SIZE * 100.;
    game.map.get_mut(target).colonize(1);
    for id in [origin, target] {
        game.map.get_mut(id).army.insert(Unit::Building(Building::JumpGate), 1);
    }
    game.map.get_mut(origin).army.insert(Unit::war_sun(), 1);
    let before = game.players[0].resources;
    let income = game.players[0].resource_production(&game.map.planets);
    turn(
        &mut game,
        vec![send(
            101,
            origin,
            target,
            Icon::Deploy,
            ships(Ship::WarSun, 1),
            BombingRaid::None,
            true,
        )],
    );
    assert_eq!(game.players[0].resources, before + income);
    assert_eq!(game.map.get(target).army.amount(&Unit::war_sun()), 1);
    assert_eq!(game.map.get(origin).army.amount(&Unit::war_sun()), 0);
    assert!(game.players[0].reports.last().unwrap().combat_report.is_none());
}

#[test]
fn single_missile_interception_matches_fifty_percent_probability() {
    let mut s = Scenario::new(
        "control",
        "interception",
        defense(Defense::InterplanetaryMissile, 1),
        army(&[(Unit::antiballistic_missile(), 1), (Unit::Defense(Defense::PlasmaTurret), 1)]),
    );
    s.objective = Icon::MissileStrike;
    let measured = measure(s, 1024);
    assert!((0.45..0.55).contains(&measured.intercepted_missiles));
}

#[test]
fn undefended_destroy_probability_matches_planet_size_and_sun_count() {
    for diameter in [1500, 10000, 120000] {
        for suns in [1, 3] {
            let mut s =
                Scenario::new("control", "death-ray", ships(Ship::WarSun, suns), Army::new());
            s.objective = Icon::Destroy;
            s.diameter = diameter;
            s.moon = diameter == 1500;
            let (planet, _) = s.fixture();
            let expected =
                100. * (1. - (1. - (planet.destroy_probability() as f64 - 0.01)).powi(suns as i32));
            let measured = measure(s, 1024);
            assert!(
                (measured.objective_success_pct - expected).abs() < 5.,
                "diameter {diameter}, suns {suns}: observed {}, expected {expected}",
                measured.objective_success_pct
            );
        }
    }
}

#[test]
#[ignore = "statistical balance survey; writes target/combat-balance and prints progress"]
fn balance_survey() {
    let seeds = std::env::var("STELLARION_BALANCE_SEEDS")
        .map(|s| s.parse::<usize>().expect("positive seed count"))
        .unwrap_or(256);
    assert!((1..=100_000).contains(&seeds));
    let filter = std::env::var("STELLARION_BALANCE_FILTER").unwrap_or_default();
    let catalog = scenarios().into_iter().filter(|s| s.id.contains(&filter)).collect::<Vec<_>>();
    assert!(!catalog.is_empty(), "filter selected no scenarios");
    let n = catalog.len();
    let started = std::time::Instant::now();
    let mut rows = Vec::new();
    for (i, scenario) in catalog.into_iter().enumerate() {
        rows.push(measure(scenario, seeds));
        if i % 20 == 0 || i + 1 == n {
            println!(
                "Balance: {}/{n} scenarios, {seeds} seeds each, {:.1}s",
                i + 1,
                started.elapsed().as_secs_f64()
            );
        }
    }
    let operations = if filter.is_empty() {
        combined_operations(seeds)
    } else {
        Vec::new()
    };
    let mut directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/combat-balance");
    if !filter.is_empty() {
        directory = directory.join(
            filter
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>(),
        );
    }
    std::fs::create_dir_all(&directory).unwrap();
    let fingerprint = Unit::all().into_iter().flatten().map(|u| serde_json::json!({
        "unit": u, "cost": u.price(), "hull": u.hull(), "shield": u.shield(), "damage": u.damage(),
        "production": u.production(), "speed": u.speed(), "fuel": u.fuel_consumption(), "rapid_fire": u.rapid_fire(),
    })).collect::<Vec<_>>();
    let report = serde_json::json!({"trials": n * seeds, "seeds_per_scenario": seeds,
        "seed_formula": "0x57e11a + index * 104729; DeterministicRngState -> ChaCha8",
        "planetary_shield_per_level": crate::core::constants::PS_SHIELD_PER_LEVEL,
        "bombing": {"phases_per_battle": 1, "hit_chance": BOMBING_HIT_CHANCE,
            "max_levels_per_building": MAX_BOMBING_LEVELS_PER_BUILDING},
        "unit_stats": fingerprint, "results": rows, "combined_operations": operations});
    std::fs::write(directory.join("results.json"), serde_json::to_string_pretty(&report).unwrap())
        .unwrap();
    let mut md = format!("# Combat balance measurements\n\n{n} scenarios × {seeds} seeds = {} battles using the Rust resolver.\n\nCosts are construction M+C+D; infrastructure, production and launch fuel are separate in results.json. Scarcity and fuel scenarios use their stated valuation when selecting rosters. Support sweeps may deliberately differ in cost. Win columns apply only to Attack/Colonize/Destroy. Mission success means conquest, colonization, destruction, any surviving scout, any turret/crawler loss to missiles, or safe deployment respectively. Building losses include planet destruction. These are measurements, not proof of balance.\n\n| Scenario | Cost A/D | Win A % | Success % (95% CI) | Loss A/D | Rounds | Buildings lost | Probes returned |\n|---|---:|---:|---:|---:|---:|---:|---:|\n", n * seeds);
    let mut csv = "scenario,attacker_cost,defender_cost,attacker_win_pct,objective_success_pct,ci_low,ci_high,attacker_loss,defender_loss,rounds,building_levels_lost,probes_returned\n".to_string();
    for r in &rows {
        let a = r.attacker_losses.iter().sum::<f64>();
        let d = r.defender_losses.iter().sum::<f64>();
        writeln!(md, "| {} | {:.0}/{:.0} | {:.1} | {:.1} ({:.1}–{:.1}) | {:.0}/{:.0} | {:.1} | {:.2} | {:.1} |", r.scenario.id,
            total(r.attacker_cost), total(r.defender_cost), r.attacker_win_pct, r.objective_success_pct,
            r.objective_success_95ci_pct[0], r.objective_success_95ci_pct[1], a, d, r.average_rounds, r.building_levels_destroyed, r.surviving_probes).unwrap();
        writeln!(
            csv,
            "{},{:.0},{:.0},{:.3},{:.3},{:.3},{:.3},{a:.3},{d:.3},{:.3},{:.3},{:.3}",
            r.scenario.id,
            total(r.attacker_cost),
            total(r.defender_cost),
            r.attacker_win_pct,
            r.objective_success_pct,
            r.objective_success_95ci_pct[0],
            r.objective_success_95ci_pct[1],
            r.average_rounds,
            r.building_levels_destroyed,
            r.surviving_probes
        )
        .unwrap();
    }
    std::fs::write(directory.join("results.md"), md).unwrap();
    std::fs::write(directory.join("results.csv"), csv).unwrap();
    let mut combined = format!("# Combined missile and fleet operations\n\nEach side has approximately 7,000 M+C+D in units. The attacker splits this between missiles and ships. Defenders are pure Gauss, pure Plasma, or Gauss plus 20 interceptors purchased from the same budget. These are real submitted turns, so interception, mission ordering, missile consumption and conquest are applied together. Infrastructure, fuel and economic buildings are excluded from the spending target. {seeds} seeds per operation.\n\n| Scenario | Attack/defense cost | Conquest % (95% CI) | Attacker losses | Defender losses |\n|---|---:|---:|---:|---:|\n");
    for op in &operations {
        writeln!(
            combined,
            "| {} | {:.0}/{:.0} | {:.1} ({:.1}–{:.1}) | {:.0} | {:.0} |",
            op.id,
            op.attacker_cost,
            op.defender_cost,
            op.conquest_pct,
            op.conquest_95ci_pct[0],
            op.conquest_95ci_pct[1],
            op.attacker_loss,
            op.defender_loss
        )
        .unwrap();
    }
    std::fs::write(directory.join("combined-operations.md"), combined).unwrap();
    println!(
        "Wrote {} ({} battles in {:.1}s)",
        directory.display(),
        n * seeds,
        started.elapsed().as_secs_f64()
    );
}

#[derive(Serialize)]
struct OperationRow {
    id: String,
    seeds: usize,
    attacker: Army,
    defender: Army,
    attacker_cost: f64,
    defender_cost: f64,
    conquest_pct: f64,
    conquest_95ci_pct: [f64; 2],
    attacker_loss: f64,
    defender_loss: f64,
}

fn combined_operations(seeds: usize) -> Vec<OperationRow> {
    let mut rows = Vec::new();
    for ship in [Ship::Cruiser, Ship::Bomber] {
        for (name, turret, interceptors) in [
            ("gauss", Defense::GaussCannon, 0),
            ("plasma", Defense::PlasmaTurret, 0),
            ("gauss-abm", Defense::GaussCannon, 20),
        ] {
            for missiles in [0, 5, 10, 15] {
                let missile_cost =
                    total(Unit::interplanetary_missile().price()) as usize * missiles;
                let fleet = affordable(Unit::Ship(ship), 7000 - missile_cost, Budget::Raw);
                let mut attackers = fleet.clone();
                if missiles > 0 {
                    attackers.insert(Unit::interplanetary_missile(), missiles);
                }
                let mut defenders = affordable(
                    Unit::Defense(turret),
                    7000 - interceptors * total(Unit::antiballistic_missile().price()) as usize,
                    Budget::Raw,
                );
                if interceptors > 0 {
                    defenders.insert(Unit::antiballistic_missile(), interceptors);
                }
                let (mut template, origin, target) = campaign(seed(0));
                template.map.get_mut(origin).army.extend(attackers.clone());
                template.map.get_mut(target).colonize(2);
                template.map.get_mut(target).army = defenders.clone();
                let mut row = OperationRow {
                    id: format!("{ship:?}-{missiles}-missiles-vs-{name}"),
                    seeds,
                    attacker_cost: total(cost(&attackers)),
                    defender_cost: total(cost(&defenders)),
                    attacker: attackers,
                    defender: defenders.clone(),
                    conquest_pct: 0.,
                    conquest_95ci_pct: [0.; 2],
                    attacker_loss: 0.,
                    defender_loss: 0.,
                };
                let mut victories = 0;
                for i in 0..seeds {
                    let mut game = template.clone();
                    game.rng = DeterministicRngState::from_u64(seed(i));
                    let mut orders = vec![send(
                        101,
                        origin,
                        target,
                        Icon::Attack,
                        fleet.clone(),
                        BombingRaid::None,
                        false,
                    )];
                    if missiles > 0 {
                        orders.push(send(
                            102,
                            origin,
                            target,
                            Icon::MissileStrike,
                            defense(Defense::InterplanetaryMissile, missiles),
                            BombingRaid::None,
                            false,
                        ));
                    }
                    turn(&mut game, orders);
                    let report = game.players[0].reports.last().unwrap();
                    assert_eq!(report.mission.objective, Icon::Attack);
                    assert_eq!(
                        game.players[0].reports.len(),
                        if missiles > 0 {
                            2
                        } else {
                            1
                        }
                    );
                    victories += usize::from(game.map.get(target).controlled == Some(1));
                    row.attacker_loss += (row.attacker_cost
                        - total(cost(&report.surviving_attacker)))
                        / seeds as f64;
                    row.defender_loss +=
                        total(losses(&defenders, &report.surviving_defender, |_| true))
                            / seeds as f64;
                }
                row.conquest_pct = 100. * victories as f64 / seeds as f64;
                row.conquest_95ci_pct = wilson(victories, seeds);
                rows.push(row);
            }
        }
    }
    rows
}

#[test]
fn combined_operations_apply_missile_costs_and_conquest_consistently() {
    for row in combined_operations(4) {
        assert!(row.attacker_loss >= 0. && row.attacker_loss <= row.attacker_cost);
        assert!(row.defender_loss >= 0. && row.defender_loss <= row.defender_cost);
    }
}
