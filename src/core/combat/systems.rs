//! Bevy combat presentation, animation, report navigation, and cleanup systems.

use std::time::Duration;

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use bevy_tweening::lens::{TextColorLens, TransformPositionLens, TransformScaleLens};
use bevy_tweening::{AnimCompletedEvent, Delay, PlaybackState, Tween, TweenAnim};
use strum::IntoEnumIterator;

use crate::core::assets::WorldAssets;
use crate::core::audio::{MuteAudioMsg, PauseAudioMsg, PlayAudioMsg, StopAudioMsg};
use crate::core::camera::MainCamera;
pub use crate::core::combat::effects::{
    restore_combat_camera, run_combat_animations, shake_combat_camera,
};
use crate::core::combat::effects::{Cinematic, PendingImpact, Wreck, DEATH_RAY_DURATION};
use crate::core::combat::playback::{CombatCardHome, CombatRoundJump};
use crate::core::combat::report::{
    combat_fleet_strength, combat_strength_ranges, CombatReport, MissionReport, RoundReport, Side,
};
use crate::core::combat::resolution::ShotReport;
use crate::core::constants::{
    BG2_COLOR, COMBAT_BACKGROUND_Z, COMBAT_SHIP_Z, HEALTH_COLOR, PS_WIDTH, SETUP_TIME,
    SHIELD_COLOR, UNIT_SIZE,
};
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::utils::{
    spawn_main_button, UiTransformScaleLens, MAIN_BUTTON_BOTTOM, MAIN_BUTTON_HEIGHT,
};
use crate::core::menu::systems::MenuBackground;
use crate::core::menu::utils::{add_root_node, add_text};
use crate::core::missions::BombingRaid;
use crate::core::player::Player;
use crate::core::resources::{ResourceName, Resources};
use crate::core::settings::Settings;
use crate::core::states::{CombatState, GameState};
use crate::core::turns::StartTurnMsg;
use crate::core::ui::systems::{UiCmp, UiState};
use crate::core::units::fauna::{FaunaAttack, SpaceFauna};
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Combat, Unit};
use crate::multiplayer::client::MultiplayerSession;
use crate::utils::NameFromEnum;

const COMBAT_IDENTITY_EDGE_INSET: f32 = 18.0;
const SPACE_FAUNA_BACKGROUND_TINT: Color = Color::srgb(0.72, 0.72, 0.72);
const COMBAT_SHIELD_DEFENSE_GAP: f32 = 12.0;
const COMBAT_STATUS_FONT_SIZE: f32 = 36.0;
const COMBAT_STATUS_OFFSET: f32 = -120.0;
// Use the displayed size directly so Bevy does not allocate a huge glyph atlas and scale it down.
const COMBAT_COUNT_FONT_SIZE: f32 = 30.0;
const COMBAT_COUNT_SEPARATOR: &str = "  ";
const ROUND_BANNER_ENTER_MS: u64 = 250;
const ROUND_BANNER_HOLD_MS: u64 = 650;
const ROUND_BANNER_EXIT_MS: u64 = 300;
const PLANETARY_SHIELD_HEIGHT_FACTOR: f32 = 0.3;
const COMBAT_CARD_LOWER_EXTENT_FACTOR: f32 = 0.76;
const COMBAT_DEFENDER_Y_OFFSET_FACTOR: f32 = -0.12;
const COMBAT_BUILDING_SIZE_FACTOR: f32 = 0.65;
const COMBAT_BUILDING_SPACING_FACTOR: f32 = 1.1;
const COMBAT_BUILDING_CENTER_FACTOR: f32 = 8.25;
const SALVAGE_HIGHLIGHT_TIME_MS: u64 = 500;
const SALVAGE_RETURN_TIME_MS: u64 = 900;
const SALVAGE_PICKUP_REVEAL_TIME_MS: u64 = 1_200;
const SALVAGE_PICKUP_DRIFT_TIME_MS: u64 = 1_500;
const SALVAGE_PICKUP_TIME_MS: u64 =
    SALVAGE_PICKUP_REVEAL_TIME_MS * 2 + SALVAGE_PICKUP_DRIFT_TIME_MS;
const FLEET_RETREAT_TIME_MS: u64 = 900;
const VOLLEY_RESOLUTION_PAUSE_MS: u64 = 1_000;
const COMBAT_FORMATION_TRANSITION_SECS: f32 = 0.7;
const COMBAT_FORMATION_GROUP_FADE_PORTION: f32 = 0.28;
const INDIVIDUAL_CARD_MAX_FACTOR: f32 = 0.62;
const INDIVIDUAL_SAME_TYPE_GAP_FACTOR: f32 = 0.06;
const INDIVIDUAL_TYPE_GAP_FACTOR: f32 = 0.48;
const INDIVIDUAL_ROW_GAP_FACTOR: f32 = 0.42;
const INDIVIDUAL_CARD_UPPER_EXTENT: f32 = 0.5;
const INDIVIDUAL_CARD_LOWER_EXTENT: f32 = 0.75;
const INDIVIDUAL_FLEET_SEPARATION_FACTOR: f32 = 0.115;

/// Both status overlays use this viewport anchor so pausing never moves the label.
fn combat_status_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.),
        height: Val::Percent(105.),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    }
}

fn combat_status_transform() -> UiTransform {
    UiTransform::from_translation(Val2::new(Val::ZERO, Val::Percent(COMBAT_STATUS_OFFSET)))
}

/// Resolves the combat report and round selected by the shared presentation state.
fn selected_combat_round<'a>(
    state: &UiState,
    player: &'a Player,
) -> Option<(&'a MissionReport, &'a CombatReport, &'a RoundReport)> {
    let report = player.reports.iter().find(|report| Some(report.id) == state.in_combat)?;
    let combat = report.combat_report.as_ref()?;
    let round = combat.rounds.get(state.combat_round)?;
    Some((report, combat, round))
}

#[derive(Component)]
/// Bevy component marking combat menu presentation entities.
pub struct CombatMenuCmp;

#[derive(Component)]
/// Bevy component marking combat presentation entities.
pub struct CombatCmp;

#[derive(Component)]
/// One player-colored segment in a combat identity card's accent line.
struct CombatIdentityAccentSegmentCmp;

#[derive(Component)]
/// Background card naming one side of the current combat.
struct CombatIdentityCmp;

#[derive(Component)]
/// Marks cards whose firing highlight must be reversed after firing.
pub struct CombatFireHighlight;

#[derive(Component)]
/// Holds volley playback after impacts so their explosions finish before return fire.
pub struct VolleyResolutionPause;

#[derive(Component)]
/// Bevy component marking background image presentation entities.
pub struct BackgroundImageCmp;

#[derive(Component)]
/// Marker for the pause overlay at the combat round-label position.
pub struct CombatPausedCmp;

#[derive(Component)]
/// Bevy component marking display text presentation entities.
pub struct DisplayTextCmp;

#[derive(PartialEq, Default)]
/// Presentation phase for a unit firing during combat playback.
pub enum FireState {
    #[default]
    /// The idle value.
    Idle,
    /// The select value.
    Select,
    /// The pre fire value.
    PreFire,
    /// The firing value.
    Firing,
    /// The deselect value.
    Deselect,
    /// The after fire value.
    AfterFire,
    /// The volley has landed and is waiting for its outcome presentation to finish.
    VolleyResolving,
    /// The fired value.
    Fired,
}

impl FireState {
    /// Returns whether this value has fired.
    pub fn has_fired(&self) -> bool {
        matches!(
            self,
            FireState::Firing
                | FireState::Deselect
                | FireState::AfterFire
                | FireState::VolleyResolving
                | FireState::Fired
        )
    }
}

#[derive(Component)]
/// Bevy component connecting one combat sprite to its unit, side, and animation state.
pub struct CombatUnitCmp {
    /// Unit kind represented by this record or presentation component.
    pub unit: Unit,
    /// Combat side to which the rendered unit belongs.
    pub side: Side,
    /// Current firing-animation phase for the rendered unit.
    pub fire: FireState,
    /// Shield points remaining at this stage of combat.
    pub shield: usize,
    /// Full shield value used to scale the presentation bar.
    pub max_shield: usize,
    /// Hull points remaining at this stage of combat.
    pub hull: usize,
    /// Full hull value used to scale the presentation bar.
    pub max_hull: usize,
    /// Whether this round's casualties should be reflected by the count presentation.
    pub outcome_visible: bool,
}

#[derive(Component)]
/// Marks an aggregated combat card that can split into its individual combatants.
pub struct GroupedCombatUnitCmp;

#[derive(Component, Clone)]
/// Exact presentation state for one independently rendered combatant.
pub struct IndividualCombatUnitCmp {
    /// Stable report identifier. Immediate-retreat stand-ins have no recorded combat ID.
    pub id: Option<u64>,
    /// Player who supplied this combatant.
    pub owner: Option<PlayerId>,
    /// Unit kind represented by this card.
    pub unit: Unit,
    /// Combat side to which this card belongs.
    pub side: Side,
    /// Aggregate card into which this card combines.
    group: Entity,
    /// Responsive destination in the expanded formation.
    home: Vec3,
    /// Rendered image width used by impacts and destruction choreography.
    display_size: f32,
    /// Position captured when a formation transition begins.
    transition_start: Vec3,
    /// Shield points remaining on this exact combatant.
    pub shield: usize,
    /// Full shield value used to scale this card's bar.
    pub max_shield: usize,
    /// Hull points remaining on this exact combatant.
    pub hull: usize,
    /// Full hull value used to scale this card's bar.
    pub max_hull: usize,
}

impl IndividualCombatUnitCmp {
    pub(super) fn home(&self) -> Vec3 {
        self.home
    }
}

#[derive(Resource, Clone, Copy, Debug)]
/// Current grouped/individual projection and any animated transition between them.
pub struct CombatFormationState {
    individual: bool,
    transition: Option<CombatFormationTransition>,
}

#[derive(Clone, Copy, Debug)]
struct CombatFormationTransition {
    to_individual: bool,
    elapsed: f32,
}

impl CombatFormationState {
    fn new(individual: bool) -> Self {
        Self {
            individual,
            transition: None,
        }
    }

    fn is_transitioning(&self) -> bool {
        self.transition.is_some()
    }

    pub(super) fn individual(&self) -> bool {
        self.individual
    }
}

#[derive(Clone)]
struct IndividualCardSeed {
    id: Option<u64>,
    owner: Option<PlayerId>,
    unit: Unit,
    side: Side,
    group: Entity,
    group_home: Vec3,
    entry_y: f32,
    hull: usize,
    max_hull: usize,
    shield: usize,
    max_shield: usize,
}

fn individual_unit_strength(unit: Unit) -> usize {
    unit.hull().saturating_add(unit.shield()).saturating_add(unit.damage()).max(1)
}

/// Gives capital ships a materially stronger silhouette while preserving room for full fleets.
fn individual_unit_scale(unit: Unit) -> f32 {
    let war_sun_strength = individual_unit_strength(Unit::war_sun()) as f32;
    let relative = (individual_unit_strength(unit) as f32 / war_sun_strength).clamp(0.0, 1.0);
    0.53 + 0.92 * relative.powf(0.65)
}

#[derive(Clone)]
struct IndividualTypeGroup {
    unit: Unit,
    indices: Vec<usize>,
    scale: f32,
    width: f32,
}

/// Splits ordered, indivisible unit-type groups into balanced contiguous rows.
fn balanced_type_rows(groups: &[IndividualTypeGroup], rows: usize) -> Vec<(usize, usize)> {
    let count = groups.len();
    let rows = rows.clamp(1, count);
    let mut prefix = vec![0.0_f32; count + 1];
    for (index, group) in groups.iter().enumerate() {
        prefix[index + 1] = prefix[index] + group.width;
    }
    let row_width = |start: usize, end: usize| {
        prefix[end] - prefix[start]
            + INDIVIDUAL_TYPE_GAP_FACTOR * end.saturating_sub(start + 1) as f32
    };

    let mut best = vec![vec![f32::INFINITY; count + 1]; rows + 1];
    let mut split = vec![vec![0; count + 1]; rows + 1];
    best[0][0] = 0.0;
    for row in 1..=rows {
        for end in row..=count {
            for start in row - 1..end {
                let candidate = best[row - 1][start].max(row_width(start, end));
                if candidate < best[row][end] {
                    best[row][end] = candidate;
                    split[row][end] = start;
                }
            }
        }
    }

    let mut result = Vec::with_capacity(rows);
    let mut end = count;
    for row in (1..=rows).rev() {
        let start = split[row][end];
        result.push((start, end));
        end = start;
    }
    result.reverse();
    result
}

/// Packs one side into centered rows without ever splitting one unit kind between rows.
///
/// Inputs are returned in their original order. Unit kinds progress from weak to strong from left
/// to right and from the front rank to the rear, so capital ships sit behind the smaller screen.
fn individual_formation_layout(
    units: &[Unit],
    center_x: f32,
    width: f32,
    y_min: f32,
    y_max: f32,
    attacker: bool,
    grouped_size: f32,
) -> Vec<(Vec3, f32)> {
    if units.is_empty() {
        return Vec::new();
    }

    let (mut y_min, mut y_max) = if y_min <= y_max {
        (y_min, y_max)
    } else {
        let middle = (y_min + y_max) * 0.5;
        (middle - grouped_size * 0.2, middle + grouped_size * 0.2)
    };
    let width = width.max(grouped_size * 0.5);
    let minimum_height = grouped_size * 0.35;
    if y_max - y_min < minimum_height {
        let middle = (y_min + y_max) * 0.5;
        y_min = middle - minimum_height * 0.5;
        y_max = middle + minimum_height * 0.5;
    }
    let height = y_max - y_min;
    let cap = grouped_size * INDIVIDUAL_CARD_MAX_FACTOR;

    let mut groups = Vec::<IndividualTypeGroup>::new();
    for (index, unit) in units.iter().copied().enumerate() {
        if let Some(group) = groups.iter_mut().find(|group| group.unit == unit) {
            group.indices.push(index);
        } else {
            let scale = individual_unit_scale(unit);
            groups.push(IndividualTypeGroup {
                unit,
                indices: vec![index],
                scale,
                width: 0.0,
            });
        }
    }
    groups.sort_by(|left, right| {
        individual_unit_strength(left.unit)
            .cmp(&individual_unit_strength(right.unit))
            .then_with(|| left.unit.cmp(&right.unit))
    });
    for group in &mut groups {
        group.width = group.indices.len() as f32 * group.scale
            + group.indices.len().saturating_sub(1) as f32 * INDIVIDUAL_SAME_TYPE_GAP_FACTOR;
    }

    let mut best_size = 0.0_f32;
    let mut best_rows = vec![(0, groups.len())];
    for row_count in 1..=groups.len() {
        let rows = balanced_type_rows(&groups, row_count);
        let widest = rows
            .iter()
            .map(|(start, end)| {
                groups[*start..*end].iter().map(|group| group.width).sum::<f32>()
                    + INDIVIDUAL_TYPE_GAP_FACTOR * end.saturating_sub(start + 1) as f32
            })
            .fold(0.0_f32, f32::max);
        let row_heights = rows
            .iter()
            .map(|(start, end)| {
                groups[*start..*end].iter().map(|group| group.scale).fold(0.0_f32, f32::max)
                    * (INDIVIDUAL_CARD_UPPER_EXTENT + INDIVIDUAL_CARD_LOWER_EXTENT)
            })
            .sum::<f32>()
            + INDIVIDUAL_ROW_GAP_FACTOR * row_count.saturating_sub(1) as f32;
        let fitted = (width / widest).min(height / row_heights).min(cap);
        let clearer_crowded_rows = best_size < cap - f32::EPSILON
            && fitted >= best_size * 0.985
            && row_count > best_rows.len();
        if fitted > best_size + f32::EPSILON || clearer_crowded_rows {
            best_size = fitted;
            best_rows = rows;
        }
    }
    let base_size = best_size.max(f32::EPSILON);
    let mut result = vec![(Vec3::ZERO, base_size); units.len()];

    // Rows are ordered weak-to-strong. Attackers advance downward and defenders upward, so the
    // direction changes while both sides retain weak screens in front of their capital ships.
    let mut y_edge = if attacker {
        y_min
    } else {
        y_max
    };
    for (row, (start, end)) in best_rows.iter().copied().enumerate() {
        let row_scale = groups[start..end].iter().map(|group| group.scale).fold(0.0_f32, f32::max);
        let row_size = base_size * row_scale;
        let y = if attacker {
            y_edge + row_size * INDIVIDUAL_CARD_LOWER_EXTENT
        } else {
            y_edge - row_size * INDIVIDUAL_CARD_UPPER_EXTENT
        };
        let row_width = groups[start..end].iter().map(|group| group.width).sum::<f32>()
            + INDIVIDUAL_TYPE_GAP_FACTOR * end.saturating_sub(start + 1) as f32;
        let mut x_edge = center_x - row_width * base_size * 0.5;
        for (group_offset, group) in groups[start..end].iter().enumerate() {
            let display_size = base_size * group.scale;
            for input_index in &group.indices {
                let x = x_edge + display_size * 0.5;
                // Front ranks render over rear ranks when their silhouettes overlap vertically.
                let z = COMBAT_SHIP_Z + (best_rows.len() - row) as f32 * 0.01;
                result[*input_index] = (Vec3::new(x, y, z), display_size);
                x_edge += display_size + base_size * INDIVIDUAL_SAME_TYPE_GAP_FACTOR;
            }
            x_edge -= base_size * INDIVIDUAL_SAME_TYPE_GAP_FACTOR;
            if group_offset + 1 < end - start {
                x_edge += base_size * INDIVIDUAL_TYPE_GAP_FACTOR;
            }
        }
        let row_extent = row_size * (INDIVIDUAL_CARD_UPPER_EXTENT + INDIVIDUAL_CARD_LOWER_EXTENT);
        let advance = row_extent + base_size * INDIVIDUAL_ROW_GAP_FACTOR;
        if attacker {
            y_edge += advance;
        } else {
            y_edge -= advance;
        }
    }
    result
}

#[derive(Component)]
/// Bevy component marking pscombat image presentation entities.
pub struct PSCombatImageCmp;

#[derive(Component)]
/// Marks the attacker probe card once its first-round retreat has begun.
pub struct ProbeRetreatCmp;

#[derive(Component)]
/// A surviving defending ship card flying out of the battle scene.
pub struct FleetRetreatCmp;

#[derive(Component)]
/// Keeps the recorded fleet withdrawal from replaying during later combat phases.
pub struct FleetRetreatPlayback {
    complete: bool,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
/// Identifies the commander represented by one count in a combat unit card.
pub struct CountCmp {
    owner: Option<PlayerId>,
}

/// Splits an initial combat-card count into its primary commander and protection contributions.
fn combat_unit_counts(
    report: &MissionReport,
    side: &Side,
    unit: &Unit,
) -> (Option<PlayerId>, usize, Vec<(PlayerId, usize)>) {
    match side {
        Side::Attacker => {
            let support = report
                .mission
                .joint_attack
                .as_ref()
                .map(|attack| {
                    attack
                        .attackers
                        .iter()
                        .filter_map(|(player_id, army)| {
                            let count = army.amount(unit);
                            (*player_id != report.mission.owner && count > 0)
                                .then_some((*player_id, count))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let lead = report
                .mission
                .joint_attack
                .as_ref()
                .and_then(|attack| attack.attackers.get(&report.mission.owner))
                .map_or_else(|| report.mission.army.amount(unit), |army| army.amount(unit));
            (Some(report.mission.owner), lead, support)
        },
        Side::Defender => {
            let owner = report.planet.controlled.or(report.planet.owned);
            let protection = report
                .planet
                .army
                .protectors()
                .filter_map(|(player_id, army)| {
                    let count = army.amount(unit);
                    (count > 0).then_some((player_id, count))
                })
                .collect();
            (owner, report.planet.army.controller().amount(unit), protection)
        },
    }
}

/// Leaves enough room for equally sized counts from every participating player.
fn combat_count_badge_width(
    size: f32,
    owner_count: usize,
    protection: &[(PlayerId, usize)],
) -> f32 {
    let characters = combat_count_characters(owner_count, protection);
    size * (0.22 + 0.1 * characters).clamp(0.3, 0.95)
}

fn combat_count_characters(owner_count: usize, protection: &[(PlayerId, usize)]) -> f32 {
    let owner_chars = owner_count.to_string().len() as f32;
    let separator_chars = COMBAT_COUNT_SEPARATOR.chars().count();
    let protection_chars = protection
        .iter()
        .map(|(_, count)| (count.to_string().len() + separator_chars) as f32)
        .sum::<f32>();
    owner_chars + protection_chars
}

/// Fit crowded multi-player badges without making any contribution smaller than another.
fn combat_count_font_size(
    badge_width: f32,
    projection_scale: f32,
    owner_count: usize,
    protection: &[(PlayerId, usize)],
) -> f32 {
    let full_size = COMBAT_COUNT_FONT_SIZE * projection_scale;
    let estimated_width = combat_count_characters(owner_count, protection) * full_size * 0.58;
    if estimated_width <= badge_width * 0.9 {
        full_size
    } else {
        full_size * badge_width * 0.9 / estimated_width
    }
}

#[allow(clippy::too_many_arguments)]
fn displayed_combat_unit_count(
    report: &MissionReport,
    combat: &CombatReport,
    round: &RoundReport,
    combat_state: &CombatState,
    card: &CombatUnitCmp,
    fleeing: bool,
    owner: Option<PlayerId>,
    antiballistic_fired: bool,
    interplanetary_fired: bool,
) -> usize {
    if card.unit.is_building() {
        return card.hull;
    }

    let defender_owner = report.planet.controlled.or(report.planet.owned);
    if card.side == Side::Defender
        && card.unit.is_ship()
        && owner == defender_owner
        && (fleeing
            || combat
                .defender_retreat
                .as_ref()
                .is_some_and(|retreat| retreat.after_round.is_none()))
    {
        return report.escaped_defenders(&card.unit);
    }

    let mut count = round
        .units(&card.side)
        .iter()
        .filter(|combatant| {
            combatant.unit == card.unit
                && combatant.owner == owner
                && (!(card.outcome_visible || *combat_state == CombatState::EndCombat)
                    || combatant.hull > 0
                    || card.unit.is_missile())
        })
        .count();

    if card.unit == Unit::antiballistic_missile() && antiballistic_fired {
        let fired = round
            .defender
            .iter()
            .filter(|combatant| {
                combatant.unit == card.unit
                    && combatant.owner == owner
                    && !combatant.shots.is_empty()
            })
            .count();
        count = count.saturating_sub(fired);
    }
    if card.unit == Unit::interplanetary_missile() {
        if interplanetary_fired {
            count = 0;
        } else if antiballistic_fired {
            count = count.saturating_sub(round.missiles_shot());
        }
    }

    count
}

#[derive(Component)]
/// Bevy component marking hull presentation entities.
pub struct HullCmp;

#[derive(Component)]
/// Bevy component marking shield presentation entities.
pub struct ShieldCmp;

#[derive(Component)]
/// Stores the un-depleted width of the planetary-shield fill.
pub struct PlanetaryShieldFillCmp {
    full_width: f32,
}

#[derive(Component)]
/// Bevy component marking the neutral fill used when a combat unit has no shield stat.
pub struct EmptyShieldCmp;

#[derive(Component)]
/// Bevy component marking death ray presentation entities.
pub struct DeathRayCmp;

#[derive(Component)]
/// The existing surviving Crawler card while it presents recovered resources.
pub struct SalvageCrawlerCmp {
    home_scale: Vec3,
    phase: SalvageCrawlerPhase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SalvageCrawlerPhase {
    Highlighting,
    Returning,
}

#[derive(Component)]
/// Invisible animation target timing the floating Crawler resource pickups.
pub struct SalvageTimerCmp;

#[derive(Component)]
/// One non-zero resource gain floating upward from the highlighted Crawler.
pub struct SalvagePickupCmp {
    /// Resource represented by the pickup icon.
    pub resource: ResourceName,
    /// Amount recovered of this resource.
    pub amount: usize,
}

#[derive(Clone, Message)]
/// Bevy message requesting one visible projectile or beam animation.
pub struct SpawnShotMsg {
    pub(super) shot: ShotReport,
    pub(super) repair: bool,
    pub(super) side: Side,
    /// Firing card and its world-space muzzle; never part of the persisted report.
    pub(super) source: Option<(Entity, Unit, Vec3)>,
}

fn spawn_combat_identity(
    commands: &mut Commands,
    role: &str,
    participants: &[(String, Color, u128)],
    color: Color,
    top: Option<f32>,
    bottom: Option<f32>,
    assets: &WorldAssets,
    window: &Window,
) {
    let has_name = !participants.is_empty();
    let name_lines = participants.len();
    let mut node = Node {
        position_type: PositionType::Absolute,
        left: Val::Px(18.0),
        width: Val::Auto,
        min_width: Val::Px(if has_name {
            220.0
        } else {
            132.0
        }),
        height: Val::Px(if has_name {
            38.0 + 16.0 * name_lines as f32
        } else {
            38.0
        }),
        padding: UiRect::px(10.0, 12.0, 7.0, 7.0),
        column_gap: Val::Px(10.0),
        align_items: AlignItems::Center,
        overflow: Overflow::clip(),
        ..default()
    };
    if let Some(top) = top {
        node.top = Val::Px(top);
    }
    if let Some(bottom) = bottom {
        node.bottom = Val::Px(bottom);
    }

    commands
        .spawn((
            node,
            BackgroundColor(Color::srgba(0.025, 0.045, 0.07, 0.88)),
            Pickable::IGNORE,
            ZIndex(5),
            CombatCmp,
            CombatIdentityCmp,
        ))
        .with_children(|parent| {
            parent
                .spawn(Node {
                    width: Val::Px(3.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|accent| {
                    if participants.is_empty() {
                        accent.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                            BackgroundColor(color),
                            CombatIdentityAccentSegmentCmp,
                        ));
                        return;
                    }

                    let strengths =
                        participants.iter().map(|(_, _, strength)| *strength).collect::<Vec<_>>();
                    for ((_, participant_color, _), (start, end)) in
                        participants.iter().zip(combat_strength_ranges(&strengths))
                    {
                        if end > start {
                            accent.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Percent((end - start) * 100.0),
                                    ..default()
                                },
                                BackgroundColor(*participant_color),
                                CombatIdentityAccentSegmentCmp,
                            ));
                        }
                    }
                });
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(1.0),
                    overflow: Overflow::clip(),
                    ..default()
                })
                .with_children(|content| {
                    content.spawn((
                        add_text(
                            role.to_uppercase(),
                            "medium",
                            if has_name {
                                7.0
                            } else {
                                9.0
                            },
                            assets,
                            window,
                        ),
                        TextLayout::no_wrap(),
                        TextColor(if has_name {
                            Color::srgb_u8(166, 188, 211)
                        } else {
                            color
                        }),
                    ));
                    for (name, participant_color, _) in participants {
                        content.spawn((
                            add_text(name, "medium", 9.0, assets, window),
                            TextLayout::no_wrap(),
                            TextColor(*participant_color),
                        ));
                    }
                });
        });
}

/// Participant order, names, colors and strength weights shared by both replay views.
pub(super) fn combat_identity_participants(
    report: &MissionReport,
    side: &Side,
    session: &MultiplayerSession,
) -> Vec<(String, Color, u128)> {
    if *side == Side::Defender && report.is_space_fauna_encounter() {
        return vec![(
            report.planet.name.clone(),
            Color::srgb_u8(190, 198, 210),
            combat_fleet_strength(&report.planet.army.combined()),
        )];
    }
    let players = match side {
        Side::Attacker => report.attacker_players(),
        Side::Defender => report.defender_players(),
    };
    players
        .into_iter()
        .map(|id| {
            (
                session
                    .player_name(id)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Player {id}")),
                session.player_color(id).color(),
                report.participant_fleet_strength(side, id),
            )
        })
        .collect()
}

/// Fans non-zero salvage gains above the surviving Crawler card. Each pickup owns its icon,
/// amount, and motion so the recovery reads as part of combat instead of a result-screen banner.
fn spawn_salvage_pickups(
    commands: &mut Commands,
    crawler_position: Vec3,
    size: f32,
    salvage: Resources,
    assets: &WorldAssets,
) {
    let gains = ResourceName::iter()
        .filter_map(|resource| {
            let amount = salvage.get(&resource);
            (amount > 0).then_some((resource, amount))
        })
        .collect::<Vec<_>>();
    let middle = (gains.len().saturating_sub(1)) as f32 * 0.5;

    for (index, (resource, amount)) in gains.into_iter().enumerate() {
        let lane = index as f32 - middle;
        let start = Vec3::new(
            crawler_position.x + lane * size * 0.82,
            crawler_position.y + size * 0.66,
            COMBAT_SHIP_Z + 1.0,
        );
        let end = start + Vec3::new(lane * size * 0.2, size * 1.35, 0.0);
        let icon = match resource {
            ResourceName::Metal => "metal",
            ResourceName::Crystal => "crystal",
            ResourceName::Deuterium => "deuterium",
        };
        let reveal = || {
            TweenAnim::new(
                Tween::new(
                    EaseFunction::QuadraticInOut,
                    Duration::from_millis(SALVAGE_PICKUP_REVEAL_TIME_MS),
                    TransformScaleLens {
                        start: Vec3::ZERO,
                        end: Vec3::ONE,
                    },
                )
                .then(Delay::new(Duration::from_millis(SALVAGE_PICKUP_DRIFT_TIME_MS)))
                .then(Tween::new(
                    EaseFunction::QuadraticInOut,
                    Duration::from_millis(SALVAGE_PICKUP_REVEAL_TIME_MS),
                    TransformScaleLens {
                        start: Vec3::ONE,
                        end: Vec3::ZERO,
                    },
                )),
            )
        };

        commands.spawn((
            Transform::from_translation(start),
            Visibility::Inherited,
            TweenAnim::new(Tween::new(
                EaseFunction::QuadraticOut,
                Duration::from_millis(SALVAGE_PICKUP_TIME_MS),
                TransformPositionLens {
                    start,
                    end,
                },
            )),
            SalvagePickupCmp {
                resource,
                amount,
            },
            Pickable::IGNORE,
            CombatCmp,
            children![
                (
                    Sprite {
                        color: Color::BLACK.with_alpha(0.72),
                        custom_size: Some(Vec2::new(size * 0.9, size * 0.48)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 0.0),
                    reveal(),
                    CombatCmp,
                    Pickable::IGNORE,
                ),
                (
                    Sprite {
                        image: assets.image(icon),
                        custom_size: Some(Vec2::splat(size * 0.34)),
                        ..default()
                    },
                    Transform::from_xyz(-size * 0.22, 0.0, 0.1),
                    reveal(),
                    CombatCmp,
                    Pickable::IGNORE,
                ),
                (
                    Text2d::new(format!("+{amount}")),
                    TextFont {
                        font: assets.font("bold").into(),
                        font_size: (size * 0.27).into(),
                        ..default()
                    },
                    TextColor(WHITE.into()),
                    Transform::from_xyz(size * 0.18, 0.0, 0.2),
                    reveal(),
                    CombatCmp,
                    Pickable::IGNORE,
                ),
            ],
        ));
    }
}

/// Creates the combat menu entities and resources required on state entry.
pub fn setup_combat_menu(
    mut commands: Commands,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut pause_audio_msg: MessageWriter<PauseAudioMsg>,
    assets: Res<WorldAssets>,
) {
    pause_audio_msg.write(PauseAudioMsg::new("music"));
    play_audio_msg.write(PlayAudioMsg::new("drums").background());

    commands.spawn((
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            position_type: PositionType::Absolute,
            ..default()
        },
        ImageNode::new(assets.image("combat")).with_mode(NodeImageMode::Stretch),
        Pickable {
            should_block_lower: true,
            is_hoverable: false,
        },
        ZIndex(4), // On top of end turn but below audio and Continue buttons.
        MenuBackground,
        CombatMenuCmp,
        UiCmp,
    ));

    spawn_main_button(&mut commands, "Continue", &assets)
        .insert((ZIndex(6), CombatMenuCmp))
        .observe(|_: On<Pointer<Click>>, mut next_game_state: ResMut<NextState<GameState>>| {
            next_game_state.set(GameState::Playing);
        });
}

/// Cleans up combat menu state and retained entities on state exit.
pub fn exit_combat_menu(
    mut start_turn_msg: MessageWriter<StartTurnMsg>,
    mut stop_audio_msg: MessageWriter<StopAudioMsg>,
) {
    start_turn_msg.write(StartTurnMsg::new(true, false));
    stop_audio_msg.write(StopAudioMsg::new("drums"));
}

/// Creates the combat entities and resources required on state entry.
pub fn setup_combat(
    mut commands: Commands,
    settings: Res<Settings>,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    session: Res<MultiplayerSession>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    camera: Single<(&Transform, &Projection), With<MainCamera>>,
    window: Single<&Window>,
    assets: Res<WorldAssets>,
) {
    let (camera_t, projection) = camera.into_inner();

    let pos = camera_t.translation;
    let Projection::Orthographic(projection) = projection else {
        return;
    };

    let (width, height) = (projection.area.width(), projection.area.height());

    play_audio_msg.write(PlayAudioMsg::new("horn"));

    let Some(report) = state
        .in_combat
        .and_then(|report_id| player.reports.iter().find(|report| report.id == report_id))
    else {
        return;
    };
    let Some(destination) = map.try_get(report.mission.destination) else {
        return;
    };

    let background = report
        .planet
        .army
        .combined()
        .iter()
        .find_map(|(unit, count)| match unit {
            Unit::Fauna(fauna) if *count > 0 => Some(match fauna.attack() {
                FaunaAttack::SonicPulse | FaunaAttack::Lightning => "fauna combat blue",
                FaunaAttack::BioPlasma | FaunaAttack::VoidLance | FaunaAttack::GravityPulse => {
                    "fauna combat violet"
                },
                FaunaAttack::StellarFire => "fauna combat amber",
                FaunaAttack::ExtinctionRay => "fauna combat violet",
            }),
            _ => None,
        })
        .map_or_else(|| format!("{} large", destination.kind.to_lowername()), str::to_owned);

    commands.spawn((
        Sprite {
            image: assets.image(background),
            custom_size: Some(Vec2::new(width, height)),
            color: if report.is_space_fauna_encounter() {
                SPACE_FAUNA_BACKGROUND_TINT
            } else {
                Color::WHITE
            },
            ..default()
        },
        Transform::from_xyz(pos.x, pos.y, COMBAT_BACKGROUND_Z),
        Pickable {
            should_block_lower: true,
            is_hoverable: false,
        },
        BackgroundImageCmp,
        CombatCmp,
    ));

    // Spawn units =================================================== >>
    let size = UNIT_SIZE * projection.scale;
    let spacing = size * 1.2;
    let attacker_id = report.mission.owner;
    let defender_id = report.planet.controlled.or(report.planet.owned);
    let mut grouped_cards = Vec::<(Side, Unit, Entity, Vec3, f32)>::new();

    let mut spawn_row = |commands: &mut Commands,
                         units: Vec<(Unit, usize)>,
                         side: Side,
                         x_center: f32,
                         y_start: f32,
                         y_end: f32| {
        let total = units.len() as f32;
        let total_width = spacing * (total - 1.0);
        let has_multiple_players = match &side {
            Side::Attacker => report.attacker_players().len() > 1,
            Side::Defender => report.defender_players().len() > 1,
        };
        let count_color = |owner: Option<PlayerId>| {
            if owner.is_none() && report.is_independent_population_encounter() {
                Color::srgb_u8(190, 198, 210)
            } else if has_multiple_players {
                owner.map_or(WHITE.into(), |player_id| session.player_color(player_id).color())
            } else {
                WHITE.into()
            }
        };
        for (i, (u, c)) in units.iter().enumerate() {
            let x = -total_width * 0.5 + i as f32 * spacing;

            let (owner, owner_count, protection) = combat_unit_counts(report, &side, u);
            let w = combat_count_badge_width(size, owner_count, &protection);
            let count_font_size =
                combat_count_font_size(w, projection.scale, owner_count, &protection);
            let h = size * 0.3;
            let hull = c * report.unit_hull(*u, &side);
            let home = Vec3::new(x_center + x, y_end, COMBAT_SHIP_Z);

            let mut card = commands.spawn((
                Sprite {
                    image: assets.image(u.to_lowername()),
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform::from_xyz(pos.x, y_start, COMBAT_SHIP_Z),
                CombatCardHome(home),
                CombatUnitCmp {
                    unit: *u,
                    side: side.clone(),
                    fire: FireState::Idle,
                    shield: c * report.unit_shield(*u, &side),
                    max_shield: c * report.unit_shield(*u, &side),
                    hull,
                    max_hull: hull,
                    outcome_visible: false,
                },
                GroupedCombatUnitCmp,
                if settings.combat_individual_units {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                },
                TweenAnim::new(Tween::new(
                    EaseFunction::QuadraticInOut,
                    Duration::from_secs(SETUP_TIME),
                    TransformPositionLens {
                        start: Vec3::new(pos.x, y_start, COMBAT_SHIP_Z),
                        end: home,
                    },
                )),
                Pickable::IGNORE,
                CombatCmp,
            ));
            let card_entity = card.id();
            card.with_children(|parent| {
                parent
                    .spawn((
                        Sprite {
                            color: Color::BLACK.with_alpha(0.5),
                            custom_size: Some(Vec2::new(w, h)),
                            ..default()
                        },
                        Transform::from_xyz(-size * 0.5 + w * 0.5, -size * 0.5 + h * 0.5, 0.1),
                    ))
                    .with_children(|badge| {
                        badge
                            .spawn((
                                Text2d::new(owner_count.to_string()),
                                TextFont {
                                    font: assets.font("bold").into(),
                                    font_size: count_font_size.into(),
                                    ..default()
                                },
                                TextColor(count_color(owner)),
                                Transform::default(),
                                CountCmp {
                                    owner,
                                },
                            ))
                            .with_children(|text| {
                                for (player_id, count) in &protection {
                                    text.spawn((
                                        TextSpan::new(format!("{COMBAT_COUNT_SEPARATOR}{count}")),
                                        TextFont {
                                            font: assets.font("bold").into(),
                                            font_size: count_font_size.into(),
                                            ..default()
                                        },
                                        TextColor(count_color(Some(*player_id))),
                                        CountCmp {
                                            owner: Some(*player_id),
                                        },
                                    ));
                                }
                            });
                    });

                // Unshielded support units keep the same two-slot card layout as shielded
                // units, using the depleted shield-track color for the empty slot. Missile
                // cards still omit stats they do not have.
                if u.shield() > 0
                    || *u == Unit::probe()
                    || *u == Unit::crawler()
                    || *u == Unit::repair_truck()
                {
                    let has_shield = u.shield() > 0;
                    parent
                        .spawn((
                            Sprite {
                                color: BG2_COLOR,
                                custom_size: Some(Vec2::new(size, size * 0.14)),
                                ..default()
                            },
                            Transform::from_xyz(0., -size * 0.57, 0.1),
                        ))
                        .with_children(|bar| {
                            let fill = (
                                Sprite {
                                    color: if has_shield {
                                        SHIELD_COLOR
                                    } else {
                                        BG2_COLOR
                                    },
                                    custom_size: Some(Vec2::new(size * 0.96, size * 0.14 * 0.75)),
                                    ..default()
                                },
                                Transform::from_xyz(0., 0., 0.2),
                            );
                            if has_shield {
                                bar.spawn((fill, ShieldCmp));
                            } else {
                                bar.spawn((fill, EmptyShieldCmp));
                            }
                        });
                }
                if u.hull() > 0 {
                    let hull_y = if u.is_fauna() {
                        -size * 0.57
                    } else {
                        -size * 0.69
                    };
                    parent.spawn((
                        Sprite {
                            color: BG2_COLOR,
                            custom_size: Some(Vec2::new(size, size * 0.14)),
                            ..default()
                        },
                        Transform::from_xyz(0., hull_y, 0.1),
                        children![(
                            Sprite {
                                color: HEALTH_COLOR,
                                custom_size: Some(Vec2::new(size * 0.96, size * 0.14 * 0.75)),
                                ..default()
                            },
                            Transform::from_xyz(0., 0., 0.2),
                            HullCmp,
                        )],
                    ));
                }
            });
            grouped_cards.push((side.clone(), *u, card_entity, home, y_start));
        }
    };

    let attack_c = session.player_color(attacker_id).color();
    let defend_c =
        defender_id.map_or(Color::srgb_u8(150, 158, 170), |id| session.player_color(id).color());
    let attackers = combat_identity_participants(report, &Side::Attacker, &session);
    let defenders = combat_identity_participants(report, &Side::Defender, &session);

    spawn_combat_identity(
        &mut commands,
        "Attacker",
        &attackers,
        attack_c,
        Some(COMBAT_IDENTITY_EDGE_INSET),
        None,
        &assets,
        &window,
    );
    spawn_combat_identity(
        &mut commands,
        if report.is_space_fauna_encounter() {
            "Space Fauna"
        } else {
            "Defender"
        },
        &defenders,
        defend_c,
        None,
        Some(COMBAT_IDENTITY_EDGE_INSET),
        &assets,
        &window,
    );

    let attacking = Unit::iter()
        .filter_map(|u| {
            let amount = report.mission.army.amount(&u);
            (u != Unit::colony_ship() && amount > 0).then_some((u, amount))
        })
        .collect::<Vec<_>>();

    // Keep fleet cards out of the fixed UI identity cards at either screen edge.
    // Multiplying the pixel inset by the projection scale preserves the gap while zoomed.
    let attacker_row_y = pos.y + height * 0.5 - 150.0 * projection.scale;
    let defender_edge_row_y = pos.y - height * 0.5 + 180.0 * projection.scale;

    spawn_row(
        &mut commands,
        attacking,
        Side::Attacker,
        pos.x,
        pos.y + height * 0.8,
        attacker_row_y,
    );

    let defending_army = report.planet.army.combined();
    let defending_def = Unit::defenses()
        .into_iter()
        .filter_map(|u| {
            let amount = defending_army.amount(&u);
            ((!u.is_missile()
                || report.mission.objective == Icon::MissileStrike
                    && u == Unit::antiballistic_missile())
                && u != Unit::space_dock()
                && amount > 0)
                .then_some((u, amount))
        })
        .collect::<Vec<_>>();
    let has_defending_defenses = !defending_def.is_empty();

    let defending_ships = if report.mission.objective != Icon::MissileStrike {
        Unit::ships()
            .into_iter()
            .chain(vec![Unit::space_dock()])
            .chain(SpaceFauna::iter().map(Unit::Fauna))
            .filter_map(|u| {
                let amount = defending_army.amount(&u);
                (u != Unit::colony_ship() && amount > 0).then_some((u, amount))
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let buildings = Unit::resource_buildings()
        .into_iter()
        .filter(|_| report.mission.includes_bombing(&BombingRaid::Economic))
        .chain(
            Unit::industrial_buildings()
                .into_iter()
                .filter(|_| report.mission.includes_bombing(&BombingRaid::Industrial)),
        )
        .filter_map(|unit| {
            let amount = report.planet.army.amount(&unit);
            (amount > 0).then_some((unit, amount))
        })
        .collect::<Vec<_>>();

    let ps = report.planet.army.amount(&Unit::planetary_shield());
    let draw_ps = ps > 0
        && report.mission.objective != Icon::MissileStrike
        && (!defending_def.is_empty()
            || (report.mission.includes_bombing(&BombingRaid::Economic)
                || report.mission.includes_bombing(&BombingRaid::Industrial)));

    // Reserve the fixed controls' complete vertical band, including the card stat bars.
    // This works for both the Exit combat button and its adjacent speed readout.
    let control_top =
        pos.y - height * 0.5 + (MAIN_BUTTON_BOTTOM + MAIN_BUTTON_HEIGHT) * projection.scale;
    let defender_y_offset = size * COMBAT_DEFENDER_Y_OFFSET_FACTOR;
    let defense_row_y = control_top
        + size * COMBAT_CARD_LOWER_EXTENT_FACTOR
        + COMBAT_SHIELD_DEFENSE_GAP * projection.scale
        + defender_y_offset;
    let shield_height = size * PLANETARY_SHIELD_HEIGHT_FACTOR;
    // Keep the shield bar immediately above the defense images. Its icon is anchored at the
    // bar's upper-left corner and hangs below it, clear of the first defense card.
    let shield_y = defense_row_y
        + (size + shield_height) * 0.5
        + COMBAT_SHIELD_DEFENSE_GAP * 0.5 * projection.scale;

    // Bombing targets occupy the lower-right combat field. Keep a wide defense row to their
    // left when the two rows' vertical footprints intersect.
    let defense_half_width = if defending_def.is_empty() {
        0.0
    } else {
        size * 0.5 + spacing * (defending_def.len() as f32 - 1.0) * 0.5
    };
    let defense_center_x = if buildings.is_empty() || defending_def.is_empty() {
        pos.x
    } else {
        let building_size = size * COMBAT_BUILDING_SIZE_FACTOR;
        let building_spacing = building_size * COMBAT_BUILDING_SPACING_FACTOR;
        let building_half_width =
            building_size * 0.5 + building_spacing * (buildings.len() as f32 - 1.0) * 0.5;
        let building_left =
            pos.x + building_size * COMBAT_BUILDING_CENTER_FACTOR - building_half_width;
        pos.x.min(building_left - COMBAT_SHIELD_DEFENSE_GAP * projection.scale - defense_half_width)
    };
    let occupied_top = if draw_ps {
        shield_y + shield_height * 0.5
    } else {
        defense_row_y + size * 0.5
    };
    let ship_y = if defending_def.is_empty() && !draw_ps {
        defender_edge_row_y + defender_y_offset
    } else {
        let preferred_y =
            pos.y - height * 0.1 + COMBAT_SHIELD_DEFENSE_GAP * projection.scale + defender_y_offset;
        let minimum_clear_y = occupied_top
            + size * COMBAT_CARD_LOWER_EXTENT_FACTOR
            + COMBAT_SHIELD_DEFENSE_GAP * projection.scale;
        preferred_y.max(minimum_clear_y)
    };
    let first_defense_x = (!defending_def.is_empty())
        .then_some(defense_center_x - spacing * (defending_def.len() as f32 - 1.0) * 0.5);

    spawn_row(
        &mut commands,
        defending_def,
        Side::Defender,
        defense_center_x,
        pos.y - height * 0.7,
        defense_row_y,
    );
    spawn_row(&mut commands, defending_ships, Side::Defender, pos.x, pos.y - height * 0.7, ship_y);

    // Build the exact per-combatant projection once. Grouped cards remain authoritative for the
    // existing playback state machine; these cards mirror exact IDs, damage and firing origins.
    let mut individual_seeds = Vec::<IndividualCardSeed>::new();
    if let Some(combat) = report.combat_report.as_ref() {
        if let Some(round) = combat.rounds.get(state.combat_round) {
            let previous =
                state.combat_round.checked_sub(1).and_then(|index| combat.rounds.get(index));
            for (side, unit, group, group_home, entry_y) in &grouped_cards {
                for combatant in round.units(side).iter().filter(|record| record.unit == *unit) {
                    let max_hull = report.unit_hull(*unit, side);
                    let max_shield = report.unit_shield(*unit, side);
                    let hull = previous
                        .and_then(|snapshot| {
                            snapshot.units(side).iter().find(|record| record.id == combatant.id)
                        })
                        .map_or(max_hull, |record| record.hull);
                    individual_seeds.push(IndividualCardSeed {
                        id: Some(combatant.id),
                        owner: combatant.owner,
                        unit: *unit,
                        side: side.clone(),
                        group: *group,
                        group_home: *group_home,
                        entry_y: *entry_y,
                        hull,
                        max_hull,
                        shield: max_shield,
                        max_shield,
                    });
                }

                // An immediate level-five withdrawal is intentionally absent from round zero.
                // Give those ships visual stand-ins so individual mode can show their flyaway.
                if state.combat_round == 0 && *side == Side::Defender && unit.is_ship() {
                    let withdrawn = combat
                        .defender_retreat
                        .as_ref()
                        .filter(|retreat| retreat.after_round.is_none())
                        .map_or(0, |retreat| retreat.ships.amount(unit));
                    for _ in 0..withdrawn {
                        let max_hull = report.unit_hull(*unit, side);
                        let max_shield = report.unit_shield(*unit, side);
                        individual_seeds.push(IndividualCardSeed {
                            id: None,
                            owner: defender_id,
                            unit: *unit,
                            side: side.clone(),
                            group: *group,
                            group_home: *group_home,
                            entry_y: *entry_y,
                            hull: max_hull,
                            max_hull,
                            shield: max_shield,
                            max_shield,
                        });
                    }
                }
            }
        }
    }

    let attacker_indices = individual_seeds
        .iter()
        .enumerate()
        .filter_map(|(index, seed)| (seed.side == Side::Attacker).then_some(index))
        .collect::<Vec<_>>();
    let defender_ship_indices = individual_seeds
        .iter()
        .enumerate()
        .filter_map(|(index, seed)| {
            (seed.side == Side::Defender
                && (seed.unit.is_ship() || seed.unit == Unit::space_dock() || seed.unit.is_fauna()))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let defender_ground_indices = individual_seeds
        .iter()
        .enumerate()
        .filter_map(|(index, seed)| {
            (seed.side == Side::Defender
                && !seed.unit.is_ship()
                && seed.unit != Unit::space_dock()
                && !seed.unit.is_fauna())
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let mut individual_layout = vec![None; individual_seeds.len()];
    let mut place_band = |indices: &[usize],
                          center_x: f32,
                          band_width: f32,
                          y_min: f32,
                          y_max: f32,
                          attacker: bool| {
        let units = indices.iter().map(|index| individual_seeds[*index].unit).collect::<Vec<_>>();
        for (index, layout) in indices.iter().copied().zip(individual_formation_layout(
            &units, center_x, band_width, y_min, y_max, attacker, size,
        )) {
            individual_layout[index] = Some(layout);
        }
    };

    let horizontal_room = width * 0.86;
    let ground_width = if buildings.is_empty() {
        horizontal_room
    } else {
        width * 0.58
    };
    let ground_center = if buildings.is_empty() {
        pos.x
    } else {
        pos.x - width * 0.12
    };
    let shield_top = shield_y + shield_height * 0.5;
    let shield_bottom = shield_y - shield_height * 0.5;
    // Fauna and fleets with no active ground-defense layer use the old low defender row. Besides
    // matching their grouped cards, this keeps a deliberately broad empty field between them and
    // the attacker instead of spending the unused shield/defense area on a taller formation.
    let use_lower_defender_fleet =
        report.is_space_fauna_encounter() || (!has_defending_defenses && !draw_ps);
    let (defender_ship_rear, defender_front) = if use_lower_defender_fleet {
        let front = ship_y + size * INDIVIDUAL_CARD_UPPER_EXTENT;
        ((control_top + size * 0.2).min(front - size * 0.05), front)
    } else {
        let rear = if draw_ps || !defender_ground_indices.is_empty() {
            shield_top + COMBAT_SHIELD_DEFENSE_GAP * projection.scale
        } else {
            control_top + size * 0.45
        };
        let front = (pos.y - height * 0.055).max(rear + size * 0.05);
        (rear, front)
    };
    let attacker_rear = pos.y + height * 0.5 - size * 0.82;
    let attacker_front = (pos.y + height * 0.06)
        .max(defender_front + height * INDIVIDUAL_FLEET_SEPARATION_FACTOR)
        .min(attacker_rear - size * 0.05);
    place_band(&attacker_indices, pos.x, horizontal_room, attacker_front, attacker_rear, true);
    place_band(
        &defender_ship_indices,
        pos.x,
        horizontal_room,
        defender_ship_rear,
        defender_front,
        false,
    );
    let ground_top = if draw_ps {
        shield_bottom - COMBAT_SHIELD_DEFENSE_GAP * projection.scale
    } else {
        pos.y - height * 0.1
    };
    let defender_ground_rear = (control_top + size * 0.2).min(ground_top - size * 0.05);
    place_band(
        &defender_ground_indices,
        ground_center,
        ground_width,
        defender_ground_rear,
        ground_top,
        false,
    );

    for (seed, layout) in individual_seeds.into_iter().zip(individual_layout) {
        let Some((home, card_size)) = layout else {
            continue;
        };
        let initial_position = if settings.combat_individual_units {
            Vec3::new(home.x, seed.entry_y, home.z)
        } else {
            seed.group_home
        };
        let show_owner = match &seed.side {
            Side::Attacker => report.attacker_players().len() > 1,
            Side::Defender => report.defender_players().len() > 1,
        };
        let owner_color = seed
            .owner
            .map_or(Color::srgb_u8(190, 198, 210), |owner| session.player_color(owner).color());
        let mut card = commands.spawn((
            Sprite {
                image: assets.image(seed.unit.to_lowername()),
                custom_size: Some(Vec2::splat(card_size)),
                ..default()
            },
            Transform::from_translation(initial_position),
            IndividualCombatUnitCmp {
                id: seed.id,
                owner: seed.owner,
                unit: seed.unit,
                side: seed.side.clone(),
                group: seed.group,
                home,
                display_size: card_size,
                transition_start: initial_position,
                shield: seed.shield,
                max_shield: seed.max_shield,
                hull: seed.hull,
                max_hull: seed.max_hull,
            },
            if settings.combat_individual_units {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            Pickable::IGNORE,
            CombatCmp,
        ));
        if settings.combat_individual_units {
            card.insert(TweenAnim::new(Tween::new(
                EaseFunction::QuadraticInOut,
                Duration::from_secs(SETUP_TIME),
                TransformPositionLens {
                    start: initial_position,
                    end: home,
                },
            )));
        }
        card.with_children(|parent| {
            let bar_height = (card_size * 0.11).max(2.0 * projection.scale);
            let first_bar_y = -card_size * 0.5 - bar_height * 0.5;
            if show_owner {
                parent.spawn((
                    Sprite {
                        color: owner_color,
                        custom_size: Some(Vec2::new(card_size * 0.38, bar_height * 0.5)),
                        ..default()
                    },
                    Transform::from_xyz(-card_size * 0.29, card_size * 0.47, 0.2),
                ));
            }
            if seed.max_shield > 0 {
                parent.spawn((
                    Sprite {
                        color: BG2_COLOR,
                        custom_size: Some(Vec2::new(card_size, bar_height)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, first_bar_y, 0.1),
                    children![(
                        Sprite {
                            color: SHIELD_COLOR,
                            custom_size: Some(Vec2::new(card_size * 0.96, bar_height * 0.72)),
                            ..default()
                        },
                        Transform::from_xyz(0.0, 0.0, 0.2),
                        ShieldCmp,
                    )],
                ));
            }
            if seed.max_hull > 0 {
                let hull_y = if seed.max_shield > 0 {
                    first_bar_y - bar_height
                } else {
                    first_bar_y
                };
                parent.spawn((
                    Sprite {
                        color: BG2_COLOR,
                        custom_size: Some(Vec2::new(card_size, bar_height)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, hull_y, 0.1),
                    children![(
                        Sprite {
                            color: HEALTH_COLOR,
                            custom_size: Some(Vec2::new(card_size * 0.96, bar_height * 0.72)),
                            ..default()
                        },
                        Transform::from_xyz(0.0, 0.0, 0.2),
                        HullCmp,
                    )],
                ));
            }
        });
    }
    commands.insert_resource(CombatFormationState::new(settings.combat_individual_units));

    // Keep the bar's established right edge while reserving a separate slot for the shield image
    // at the left. This prevents the image from covering the first defense card.
    if draw_ps {
        let full_bar_width = size * PS_WIDTH;
        let w = size * 0.3;
        let max_shield = report.initial_planetary_shield();
        let original_bar_left = pos.x - full_bar_width * 0.5;
        let original_icon_center_x = original_bar_left + size * 0.5;
        let icon_center_x = first_defense_x
            .map_or(original_icon_center_x, |x| (x - spacing).min(original_icon_center_x));
        let icon_left = icon_center_x - size * 0.5;
        // The bar's top-left corner and the image's top-right corner are one exact anchor.
        let bar_left = icon_left + size;
        let bar_right = pos.x + full_bar_width * 0.5;
        let bar_width = (bar_right - bar_left).max(size);
        let fill_width = bar_width * 0.997;
        let shield_center_x = (bar_left + bar_right) * 0.5;
        let shield_icon_x = icon_center_x - shield_center_x;
        let shield_icon_y = (shield_height - size) * 0.5;

        commands.spawn((
            Sprite {
                color: BG2_COLOR,
                custom_size: Some(Vec2::new(bar_width, shield_height)),
                ..default()
            },
            Transform::from_xyz(shield_center_x, pos.y - height * 0.7, COMBAT_SHIP_Z),
            CombatCardHome(Vec3::new(shield_center_x, shield_y, COMBAT_SHIP_Z)),
            CombatUnitCmp {
                unit: Unit::planetary_shield(),
                side: Side::Defender,
                fire: FireState::Idle,
                shield: max_shield,
                max_shield,
                hull: ps,
                max_hull: ps,
                outcome_visible: false,
            },
            children![
                (
                    Sprite {
                        color: SHIELD_COLOR,
                        custom_size: Some(Vec2::new(fill_width, shield_height * 0.9)),
                        ..default()
                    },
                    Transform::from_xyz(0., 0., 0.1),
                    ShieldCmp,
                    PlanetaryShieldFillCmp {
                        full_width: fill_width,
                    },
                ),
                (
                    Sprite {
                        image: assets.image("planetary shield"),
                        custom_size: Some(Vec2::splat(size)),
                        ..default()
                    },
                    Transform::from_xyz(shield_icon_x, shield_icon_y, 0.),
                    PSCombatImageCmp,
                    children![(
                        Sprite {
                            color: Color::BLACK.with_alpha(0.5),
                            custom_size: Some(Vec2::splat(w)),
                            ..default()
                        },
                        Transform::from_xyz(-size * 0.5 + w * 0.5, -size * 0.5 + w * 0.5, 0.1),
                        children![(
                            Text2d::new(ps.to_string()),
                            TextFont {
                                font: assets.font("bold").into(),
                                font_size: (COMBAT_COUNT_FONT_SIZE * projection.scale).into(),
                                ..default()
                            },
                            TextColor(WHITE.into()),
                            Transform::default(),
                        )]
                    )],
                )
            ],
            TweenAnim::new(Tween::new(
                EaseFunction::QuadraticInOut,
                Duration::from_secs(SETUP_TIME),
                TransformPositionLens {
                    start: Vec3::new(shield_center_x, pos.y - height * 0.7, COMBAT_SHIP_Z),
                    end: Vec3::new(shield_center_x, shield_y, COMBAT_SHIP_Z),
                },
            )),
            Pickable::IGNORE,
            CombatCmp,
        ));
    }

    // Spawn buildings when bombing
    if !buildings.is_empty() {
        let size = size * COMBAT_BUILDING_SIZE_FACTOR;
        let spacing = size * COMBAT_BUILDING_SPACING_FACTOR;
        let total_width = spacing * (buildings.len() as f32 - 1.0);

        for (i, (u, c)) in buildings.iter().enumerate() {
            let x = -total_width * 0.5 + i as f32 * spacing;
            let w = size * 0.5;

            commands.spawn((
                Sprite {
                    image: assets.image(u.to_lowername()),
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y - height * 0.7, COMBAT_SHIP_Z),
                CombatCardHome(Vec3::new(
                    pos.x + size * COMBAT_BUILDING_CENTER_FACTOR + x,
                    pos.y - height * 0.34,
                    COMBAT_SHIP_Z,
                )),
                CombatUnitCmp {
                    unit: *u,
                    side: Side::Defender,
                    fire: FireState::Idle,
                    shield: 0,
                    max_shield: 0,
                    hull: *c,
                    max_hull: *c,
                    outcome_visible: false,
                },
                children![(
                    Sprite {
                        color: Color::BLACK.with_alpha(0.5),
                        custom_size: Some(Vec2::splat(w)),
                        ..default()
                    },
                    Transform::from_xyz(-size * 0.5 + w * 0.5, -size * 0.5 + w * 0.5, 0.1),
                    children![(
                        Text2d::new(c.to_string()),
                        TextFont {
                            font: assets.font("bold").into(),
                            font_size: (COMBAT_COUNT_FONT_SIZE * projection.scale).into(),
                            ..default()
                        },
                        TextColor(WHITE.into()),
                        Transform::default(),
                        CountCmp {
                            owner: defender_id,
                        },
                    )]
                )],
                TweenAnim::new(Tween::new(
                    EaseFunction::QuadraticInOut,
                    Duration::from_secs(SETUP_TIME),
                    TransformPositionLens {
                        start: Vec3::new(pos.x, pos.y - height * 0.7, COMBAT_SHIP_Z),
                        end: Vec3::new(
                            pos.x + size * COMBAT_BUILDING_CENTER_FACTOR + x,
                            pos.y - height * 0.34,
                            COMBAT_SHIP_Z,
                        ),
                    },
                )),
                Pickable::IGNORE,
                CombatCmp,
            ));
        }
    }

    commands
        .spawn((
            combat_status_node(),
            if settings.combat_paused {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            Pickable::IGNORE,
            ZIndex(7),
            CombatPausedCmp,
            CombatCmp,
        ))
        .with_child((
            add_text("PAUSED", "medium", COMBAT_STATUS_FONT_SIZE, &assets, &window),
            combat_status_transform(),
            TextShadow::default(),
            Pickable::IGNORE,
        ));

    spawn_main_button(&mut commands, "Exit combat", &assets)
        .insert((ZIndex(6), CombatCmp))
        .observe(|_: On<Pointer<Click>>, mut next_game_state: ResMut<NextState<GameState>>| {
            next_game_state.set(GameState::CombatMenu);
        });
}

/// Animates every exact combatant between its responsive slot and its aggregate type card.
/// Playback waits for this short presentation-only movement, so a shot can never change visual
/// targets halfway through its flight.
pub fn update_combat_formation(
    mut commands: Commands,
    settings: Res<Settings>,
    formation: Option<ResMut<CombatFormationState>>,
    time: Res<Time>,
    pending_q: Query<(), Or<(With<PendingImpact>, With<Wreck>)>>,
    grouped_state_q: Query<&CombatUnitCmp, With<GroupedCombatUnitCmp>>,
    mut grouped_q: Query<
        (Entity, &Transform, &mut Visibility, &mut Sprite),
        (
            With<GroupedCombatUnitCmp>,
            Without<IndividualCombatUnitCmp>,
            Without<FleetRetreatCmp>,
            Without<ProbeRetreatCmp>,
        ),
    >,
    mut individual_q: Query<
        (Entity, &mut Transform, &mut Visibility, &mut IndividualCombatUnitCmp),
        (Without<GroupedCombatUnitCmp>, Without<FleetRetreatCmp>, Without<ProbeRetreatCmp>),
    >,
) {
    let Some(mut formation) = formation else {
        return;
    };
    let desired = settings.combat_individual_units;
    if formation.transition.is_none() && desired != formation.individual {
        let firing = grouped_state_q
            .iter()
            .any(|card| !matches!(card.fire, FireState::Idle | FireState::Fired));
        if firing || !pending_q.is_empty() {
            return;
        }

        let group_positions = grouped_q
            .iter_mut()
            .map(|(entity, transform, _, _)| (entity, transform.translation))
            .collect::<Vec<_>>();
        for (entity, mut transform, mut visibility, mut individual) in &mut individual_q {
            individual.transition_start = if desired {
                group_positions
                    .iter()
                    .find_map(|(entity, position)| {
                        (*entity == individual.group).then_some(*position)
                    })
                    .unwrap_or(transform.translation)
            } else {
                transform.translation
            };
            if desired {
                transform.translation = individual.transition_start;
            }
            *visibility = Visibility::Inherited;
            commands.entity(entity).remove::<TweenAnim>();
        }
        // A split starts at the aggregate but dismisses that image before the exact ships have
        // spread far enough to look like duplicates. Combining keeps the aggregate hidden until
        // the exact cards have returned to it.
        for (_, _, mut visibility, mut sprite) in &mut grouped_q {
            sprite.color = sprite.color.with_alpha(1.0);
            *visibility = if desired {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        formation.transition = Some(CombatFormationTransition {
            to_individual: desired,
            elapsed: 0.0,
        });
    }

    let Some(mut transition) = formation.transition else {
        return;
    };
    transition.elapsed += time.delta_secs() * settings.speed();
    let progress = (transition.elapsed / COMBAT_FORMATION_TRANSITION_SECS).clamp(0.0, 1.0);
    let eased = progress * progress * (3.0 - 2.0 * progress);
    let group_positions = grouped_q
        .iter_mut()
        .map(|(entity, transform, _, _)| (entity, transform.translation))
        .collect::<Vec<_>>();
    for (_, mut transform, _, individual) in &mut individual_q {
        let group_position = group_positions
            .iter()
            .find_map(|(entity, position)| (*entity == individual.group).then_some(*position))
            .unwrap_or(individual.transition_start);
        let destination = if transition.to_individual {
            individual.home
        } else {
            group_position
        };
        transform.translation = individual.transition_start.lerp(destination, eased);
        let scale = if transition.to_individual {
            0.82 + 0.18 * eased
        } else {
            1.0 - 0.18 * eased
        };
        transform.scale = Vec3::splat(scale);
    }
    if transition.to_individual {
        let alpha = (1.0 - progress / COMBAT_FORMATION_GROUP_FADE_PORTION).clamp(0.0, 1.0);
        for (_, _, mut visibility, mut sprite) in &mut grouped_q {
            sprite.color = sprite.color.with_alpha(alpha);
            if alpha <= f32::EPSILON {
                *visibility = Visibility::Hidden;
            }
        }
    }

    if progress < 1.0 {
        formation.transition = Some(transition);
        return;
    }

    formation.individual = transition.to_individual;
    formation.transition = None;
    for (_, _, mut visibility, mut sprite) in &mut grouped_q {
        sprite.color = sprite.color.with_alpha(1.0);
        *visibility = if formation.individual {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    for (_, mut transform, mut visibility, individual) in &mut individual_q {
        if formation.individual {
            transform.translation = individual.home;
            transform.scale = Vec3::ONE;
            *visibility = Visibility::Inherited;
        } else {
            transform.scale = Vec3::ONE;
            *visibility = Visibility::Hidden;
        }
    }
}

/// Flies only escaped defending ships offscreen; ground units keep their positions and state.
fn start_fleet_retreat(
    commands: &mut Commands,
    units: &mut Query<(Entity, &Transform, &mut CombatUnitCmp)>,
    individuals: &mut Query<
        (Entity, &Transform, &mut IndividualCombatUnitCmp),
        Without<CombatUnitCmp>,
    >,
    ships: &crate::core::units::Army,
    owner: Option<PlayerId>,
    center: Vec3,
    width: f32,
    height: f32,
) {
    for (entity, transform, unit) in units.iter_mut() {
        if unit.side == Side::Defender
            && unit.unit != Unit::colony_ship()
            && unit.hull > 0
            && ships.amount(&unit.unit) > 0
        {
            let horizontal_direction = if transform.translation.x < center.x {
                -1.0
            } else {
                1.0
            };
            commands.entity(entity).insert((
                FleetRetreatCmp,
                TweenAnim::new(Tween::new(
                    EaseFunction::QuadraticIn,
                    Duration::from_millis(FLEET_RETREAT_TIME_MS),
                    TransformPositionLens {
                        start: transform.translation,
                        end: Vec3::new(
                            center.x + horizontal_direction * width * 0.7,
                            center.y + height * 0.8,
                            COMBAT_SHIP_Z + 0.9,
                        ),
                    },
                )),
            ));
        }
    }
    for (entity, transform, unit) in individuals.iter_mut() {
        if unit.side == Side::Defender
            && unit.unit != Unit::colony_ship()
            && unit.unit.is_ship()
            && unit.hull > 0
            && unit.owner == owner
            && ships.amount(&unit.unit) > 0
        {
            let horizontal_direction = if transform.translation.x < center.x {
                -1.0
            } else {
                1.0
            };
            commands.entity(entity).insert((
                FleetRetreatCmp,
                TweenAnim::new(Tween::new(
                    EaseFunction::QuadraticIn,
                    Duration::from_millis(FLEET_RETREAT_TIME_MS),
                    TransformPositionLens {
                        start: transform.translation,
                        end: Vec3::new(
                            center.x + horizontal_direction * width * 0.7,
                            center.y + height * 0.8,
                            COMBAT_SHIP_Z + 0.9,
                        ),
                    },
                )),
            ));
        }
    }
    commands.spawn((
        FleetRetreatPlayback {
            complete: false,
        },
        CombatCmp,
        Transform::default(),
        TweenAnim::new(Tween::new(
            EaseFunction::Linear,
            Duration::from_millis(FLEET_RETREAT_TIME_MS),
            TransformScaleLens {
                start: Vec3::ONE,
                end: Vec3::ONE,
            },
        )),
    ));
}

/// Advances the combat presentation state machine after each animation timer.
pub fn animate_combat(
    mut commands: Commands,
    bg_q: Single<&mut Sprite, With<BackgroundImageCmp>>,
    text_q: Option<Single<Entity, With<DisplayTextCmp>>>,
    combatants: (
        Query<(Entity, &Transform, &mut CombatUnitCmp)>,
        Query<(Entity, &Transform, &mut IndividualCombatUnitCmp), Without<CombatUnitCmp>>,
    ),
    phase_entities: (
        Query<(), With<ProbeRetreatCmp>>,
        Query<Entity, With<DeathRayCmp>>,
        Query<(Entity, &SalvageCrawlerCmp)>,
        Query<Entity, With<SalvageTimerCmp>>,
        Query<Entity, With<SalvagePickupCmp>>,
        Query<(Entity, &mut FleetRetreatPlayback)>,
        Query<Entity, With<FleetRetreatCmp>>,
        Query<(), With<CombatFireHighlight>>,
        Query<Entity, With<VolleyResolutionPause>>,
        Query<(), ()>,
    ),
    mut state: ResMut<UiState>,
    player: Res<Player>,
    combat_state: Res<State<CombatState>>,
    mut next_combat_state: ResMut<NextState<CombatState>>,
    mut spawn_shot_msg: MessageWriter<SpawnShotMsg>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    completion: (MessageReader<AnimCompletedEvent>, Local<Vec<Entity>>),
    camera: Single<(&Transform, &Projection), With<MainCamera>>,
    presentation: (
        Res<WorldAssets>,
        Single<&Window>,
        Option<Res<CombatRoundJump>>,
        Option<Res<CombatFormationState>>,
    ),
    pending_q: Query<(), Or<(With<PendingImpact>, With<Wreck>)>>,
    settings: Res<Settings>,
) {
    let (mut unit_q, mut individual_q) = combatants;
    let (
        retreating_probe_q,
        death_ray_q,
        salvage_crawler_q,
        salvage_timer_q,
        salvage_pickup_q,
        mut fleet_retreat_q,
        fleeing_ships_q,
        highlighted_q,
        volley_pause_q,
        all_entities,
    ) = phase_entities;
    let (assets, window, round_jump, formation) = presentation;
    let (mut anim_completed_msg, mut deferred_completions) = completion;
    deferred_completions.extend(anim_completed_msg.read().map(|message| message.anim_entity));
    deferred_completions.retain(|entity| all_entities.contains(*entity));
    if settings.combat_paused
        || formation.as_ref().is_some_and(|state| state.is_transitioning())
        || (round_jump.is_some() && !matches!(*next_combat_state, NextState::Unchanged))
    {
        return;
    }
    // Completion messages expire after two frames. Keep any that arrive on the pause frame so
    // resuming cannot strand the state machine behind an already-finished, removed tween.
    let completed = std::mem::take(&mut *deferred_completions);
    let (camera_t, projection) = camera.into_inner();

    let pos = camera_t.translation;
    let Projection::Orthographic(projection) = projection else {
        return;
    };

    let units: Vec<_> = Unit::all_firing_order();

    let Some((report, combat, round)) = selected_combat_round(&state, &player) else {
        return;
    };

    let size = UNIT_SIZE * projection.scale;
    let volley_fire = settings.combat_volley_fire && *combat_state.get() == CombatState::Fire;
    let individual_mode =
        formation.as_ref().map_or(settings.combat_individual_units, |state| state.individual);

    if let Some((timer, mut playback)) = fleet_retreat_q.iter_mut().next() {
        if !playback.complete {
            if completed.contains(&timer) {
                for ship in &fleeing_ships_q {
                    commands.entity(ship).despawn();
                }
                commands.entity(timer).remove::<TweenAnim>();
                playback.complete = true;
            }
            return;
        }
    }

    // A level-five immediate withdrawal happens before either side fires its first shot.
    if *combat_state.get() == CombatState::Fire
        && state.combat_round == 0
        && fleet_retreat_q.is_empty()
    {
        if let Some(retreat) =
            combat.defender_retreat.as_ref().filter(|retreat| retreat.after_round.is_none())
        {
            start_fleet_retreat(
                &mut commands,
                &mut unit_q,
                &mut individual_q,
                &retreat.ships,
                report.planet.controlled.or(report.planet.owned),
                pos,
                projection.area.width(),
                projection.area.height(),
            );
            return;
        }
    }

    if matches!(
        combat_state.get(),
        CombatState::AntiBallistic
            | CombatState::Fire
            | CombatState::Repair
            | CombatState::Bomb
            | CombatState::DeathRay
    ) && unit_q.iter().all(|(_, _, cu)| {
        matches!(cu.fire, FireState::Idle | FireState::VolleyResolving | FireState::Fired)
    }) {
        // Keep consuming tween completion messages while projectiles travel, but never
        // advance a round or remove simultaneous return-fire cards before they arrive.
        if !pending_q.is_empty() {
            return;
        }

        // A volley deliberately leaves a short, speed-aware beat after all projectiles have
        // resolved. This keeps hit flashes and explosions readable before the other side fires.
        if unit_q.iter().any(|(_, _, cu)| cu.fire == FireState::VolleyResolving) {
            if let Some(pause) = volley_pause_q.iter().next() {
                if completed.contains(&pause) {
                    commands.entity(pause).despawn();
                    for (_, _, mut cu) in &mut unit_q {
                        if cu.fire == FireState::VolleyResolving {
                            cu.fire = FireState::Fired;
                        }
                    }
                }
                return;
            }
            commands.spawn((
                VolleyResolutionPause,
                CombatCmp,
                Transform::default(),
                TweenAnim::new(Tween::new(
                    EaseFunction::Linear,
                    Duration::from_millis(VOLLEY_RESOLUTION_PAUSE_MS),
                    TransformScaleLens {
                        start: Vec3::ONE,
                        end: Vec3::ONE,
                    },
                )),
            ));
            return;
        }

        for side in Side::iter() {
            // Follow recorded weapon fire even after the last defender dies: shields
            // can still be hit, and simultaneous return fire can kill would-be Bombers.
            let mut selected = false;
            for unit in &units {
                if let Some((_, _, mut cu)) = unit_q.iter_mut().find(|(_, _, cu)| {
                    cu.fire == FireState::Idle
                        && cu.unit == *unit
                        && cu.side == side
                        && cu.unit.damage() > 0
                        && round.units(&side).iter().any(|shooter| {
                            shooter.unit == *unit
                                && shooter.shots.iter().any(|shot| !shot.is_bombing())
                        })
                        && (cu.unit != Unit::interplanetary_missile()
                            || round.missiles_shot() < round.n_missiles())
                }) {
                    cu.fire = FireState::Select;
                    selected = true;
                    if !volley_fire {
                        return;
                    }
                }
            }
            // Volley fire deliberately finishes one side before beginning the other.
            if selected {
                return;
            }
        }

        // No more units to fire -> explode destroyed units
        let mut destroying = false;
        for (unit_e, unit_t, cu) in &mut unit_q {
            if cu.hull == 0
                && cu.unit != Unit::planetary_shield()
                && (cu.unit != Unit::antiballistic_missile()
                    || round.antiballistic_fired >= round.n_antiballistic())
            {
                destroying = true;
                if individual_mode
                    && matches!(cu.unit, Unit::Ship(_) | Unit::Defense(_) | Unit::Fauna(_))
                {
                    commands.entity(unit_e).despawn();
                } else {
                    commands.entity(unit_e).insert(Wreck::new(unit_t.translation, size, cu.unit));
                    // The wreck sequence owns removal after its staggered secondary blasts.
                }
            }
        }
        for (unit_e, unit_t, cu) in &mut individual_q {
            let missile_consumed = cu.unit.is_missile()
                && cu.id.is_some_and(|id| {
                    round.units(&cu.side).iter().any(|record| {
                        record.id == id
                            && (record.unit == Unit::interplanetary_missile()
                                || !record.shots.is_empty())
                    })
                });
            if cu.hull == 0 && (!cu.unit.is_missile() || missile_consumed) {
                destroying = true;
                if individual_mode && !cu.unit.is_missile() {
                    commands.entity(unit_e).insert(Wreck::new(
                        unit_t.translation,
                        cu.display_size,
                        cu.unit,
                    ));
                } else {
                    commands.entity(unit_e).despawn();
                }
            }
        }
        if destroying {
            return;
        }

        // Reveal the completed exchange as one between-round beat. Counts and health capacity
        // now describe only the survivors, while ordinary shields recharge for the next round.
        // The shared planetary shield deliberately remains depleted. Doing this before selecting
        // a Repair Truck lets its healing animate alongside the other round refreshes.
        if *combat_state.get() == CombatState::Fire {
            let has_next_round = state.combat_round + 1 < combat.rounds.len();
            for (_, _, mut cu) in &mut unit_q {
                cu.outcome_visible = true;
                if cu.unit.is_building() || cu.unit == Unit::planetary_shield() {
                    continue;
                }

                let count = round
                    .units(&cu.side)
                    .iter()
                    .filter(|combatant| combatant.unit == cu.unit && combatant.hull > 0)
                    .count();
                cu.max_shield = count * report.unit_shield(cu.unit, &cu.side);
                cu.max_hull = count * report.unit_hull(cu.unit, &cu.side);
                if has_next_round {
                    cu.shield = cu.max_shield;
                }
            }
        }

        // Scout probes fly away after ordinary battles. During a fauna encounter they remain
        // committed to the fleet for the entire fight, matching deterministic resolution.
        if state.combat_round == 0
            && combat.rounds.len() > 1
            && !report.is_space_fauna_encounter()
            && retreating_probe_q.is_empty()
        {
            if let Some((unit_e, unit_t, _)) = unit_q.iter_mut().find(|(_, _, cu)| {
                cu.hull > 0 && cu.unit == Unit::probe() && cu.side == Side::Attacker
            }) {
                commands.entity(unit_e).insert((
                    TweenAnim::new(Tween::new(
                        EaseFunction::QuadraticIn,
                        Duration::from_secs(SETUP_TIME),
                        TransformPositionLens {
                            start: unit_t.translation,
                            end: Vec3::new(
                                pos.x,
                                pos.y + projection.area.height() * 0.9,
                                COMBAT_SHIP_Z + 0.9,
                            ),
                        },
                    )),
                    ProbeRetreatCmp,
                ));
                for (probe_e, probe_t, probe) in &mut individual_q {
                    if probe.hull > 0 && probe.unit == Unit::probe() && probe.side == Side::Attacker
                    {
                        commands.entity(probe_e).insert((
                            TweenAnim::new(Tween::new(
                                EaseFunction::QuadraticIn,
                                Duration::from_secs(SETUP_TIME),
                                TransformPositionLens {
                                    start: probe_t.translation,
                                    end: Vec3::new(
                                        pos.x,
                                        pos.y + projection.area.height() * 0.9,
                                        COMBAT_SHIP_Z + 0.9,
                                    ),
                                },
                            )),
                            ProbeRetreatCmp,
                        ));
                    }
                }
                play_audio_msg.write(PlayAudioMsg::new("probe retreat").rate(1.2));
            }
        }

        // Repair Trucks restore defense turrets after the recorded exchange of fire.
        if round.units(&Side::Defender).iter().any(|cu| cu.repairs.iter().any(|r| *r > 0))
            && *combat_state.get() == CombatState::Fire
        {
            if let Some((_, _, mut cu)) =
                unit_q.iter_mut().find(|(_, _, cu)| cu.hull > 0 && cu.unit == Unit::repair_truck())
            {
                cu.fire = FireState::Select;
                next_combat_state.set(CombatState::Repair);
                return;
            }
        }

        // Replay only a recorded building raid. Weapon shots at the planetary shield
        // belong to Fire and must not trigger a second Bomber animation.
        if (report.mission.includes_bombing(&BombingRaid::Economic)
            || report.mission.includes_bombing(&BombingRaid::Industrial))
            && round
                .units(&Side::Attacker)
                .iter()
                .any(|cu| cu.shots.iter().any(ShotReport::is_bombing))
            && matches!(combat_state.get(), CombatState::Fire | CombatState::Repair)
        {
            if let Some((_, _, mut cu)) = unit_q.iter_mut().find(|(_, _, cu)| {
                cu.hull > 0 && cu.unit == Unit::Ship(Ship::Bomber) && cu.side == Side::Attacker
            }) {
                cu.fire = FireState::Select;
                next_combat_state.set(CombatState::Bomb);
                return;
            }
        }

        // Depart after the recorded volley and repairs, before a War Sun can destroy the world.
        if fleet_retreat_q.is_empty() {
            if let Some(retreat) = combat
                .defender_retreat
                .as_ref()
                .filter(|retreat| retreat.after_round == Some(state.combat_round))
            {
                start_fleet_retreat(
                    &mut commands,
                    &mut unit_q,
                    &mut individual_q,
                    &retreat.ships,
                    report.planet.controlled.or(report.planet.owned),
                    pos,
                    projection.area.width(),
                    projection.area.height(),
                );
                return;
            }
        }

        // Death ray
        if report.mission.objective == Icon::Destroy
            && round.destroy_probability > 0.
            && matches!(
                combat_state.get(),
                CombatState::Fire | CombatState::Repair | CombatState::Bomb
            )
        {
            if let Some((_, _, mut cu)) = unit_q.iter_mut().find(|(_, _, cu)| {
                cu.hull > 0 && cu.unit == Unit::war_sun() && cu.side == Side::Attacker
            }) {
                cu.fire = FireState::Select;
                next_combat_state.set(CombatState::DeathRay);
                return;
            }
        }

        next_combat_state.set(if state.combat_round == combat.rounds.len() - 1 {
            if report.defender_salvage() == default() {
                CombatState::EndCombat
            } else {
                CombatState::Salvage
            }
        } else {
            state.combat_round += 1;
            CombatState::DisplayRound
        });
        return;
    }

    // The result belongs to the conclusion, not the end of the salvage flourish. Starting it in
    // either conclusion phase lets surviving Crawlers collect resources beneath the overlay.
    if matches!(combat_state.get(), CombatState::Salvage | CombatState::EndCombat)
        && text_q.is_none()
    {
        let result = report.status(&player);

        play_audio_msg.write(PlayAudioMsg::new(result));
        commands.spawn((
            add_root_node(false),
            BackgroundColor(Color::BLACK.with_alpha(0.46)),
            children![(
                Node {
                    max_width: Val::Vw(90.),
                    ..default()
                },
                ImageNode::new(assets.image(result)),
                UiTransform {
                    translation: Val2::new(Val::ZERO, Val::Percent(-10.)),
                    scale: Vec2::ZERO,
                    ..default()
                },
                TweenAnim::new(Tween::new(
                    EaseFunction::QuadraticInOut,
                    Duration::from_millis(1500),
                    UiTransformScaleLens {
                        start: Vec2::ZERO,
                        end: Vec2::splat(match result {
                            "victory" => 0.55,
                            "draw" => 0.4,
                            "defeat" => 0.6,
                            _ => 0.5,
                        }),
                    },
                )),
                DisplayTextCmp,
                CombatCmp,
            )],
            CombatCmp,
        ));
    }

    match combat_state.get() {
        CombatState::Setup => {
            if !completed.is_empty() {
                next_combat_state.set(
                    if let Some((_, _, mut cu)) = unit_q
                        .iter_mut()
                        .find(|(_, _, cu)| cu.unit == Unit::antiballistic_missile())
                    {
                        cu.fire = FireState::Select;
                        CombatState::AntiBallistic
                    } else {
                        CombatState::DisplayRound
                    },
                );
            }
        },
        CombatState::DisplayRound => {
            if let Some(round_q) = text_q {
                let entity = round_q.into_inner();
                if completed.contains(&entity) {
                    next_combat_state.set(CombatState::Fire);
                    commands.entity(entity).despawn();
                }
            } else {
                commands.remove_resource::<CombatRoundJump>();
                // Reset all stats
                unit_q.iter_mut().for_each(|(_, _, mut cu)| {
                    if cu.unit != Unit::planetary_shield() {
                        let count = if cu.side == Side::Defender
                            && cu.unit.is_ship()
                            && combat
                                .defender_retreat
                                .as_ref()
                                .is_some_and(|retreat| retreat.after_round.is_none())
                        {
                            report.planet.army.amount(&cu.unit)
                        } else {
                            round.units(&cu.side).iter().filter(|cu2| cu.unit == cu2.unit).count()
                        };

                        cu.max_shield = count * report.unit_shield(cu.unit, &cu.side);
                        cu.max_hull = count * report.unit_hull(cu.unit, &cu.side);
                        cu.shield = cu.max_shield;
                        cu.fire = FireState::Idle;
                        cu.outcome_visible = false;
                    }
                });
                for (_, _, mut individual) in &mut individual_q {
                    individual.shield = individual.max_shield;
                }

                if combat.rounds.len() == 1 {
                    next_combat_state.set(CombatState::Fire);
                    return;
                }

                commands.spawn((
                    combat_status_node(),
                    Pickable::IGNORE,
                    ZIndex(7),
                    children![(
                        Node::default(),
                        UiTransform {
                            scale: Vec2::splat(0.97),
                            ..combat_status_transform()
                        },
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::CubicOut,
                                Duration::from_millis(ROUND_BANNER_ENTER_MS),
                                UiTransformScaleLens {
                                    start: Vec2::splat(0.97),
                                    end: Vec2::ONE,
                                },
                            )
                            .then(Delay::new(Duration::from_millis(ROUND_BANNER_HOLD_MS)))
                            .then(Tween::new(
                                EaseFunction::CubicIn,
                                Duration::from_millis(ROUND_BANNER_EXIT_MS),
                                UiTransformScaleLens {
                                    start: Vec2::ONE,
                                    end: Vec2::splat(0.99),
                                },
                            ))
                        ),
                        DisplayTextCmp,
                        CombatCmp,
                        children![(
                            add_text(
                                format!("Round {}", state.combat_round + 1),
                                "medium",
                                COMBAT_STATUS_FONT_SIZE,
                                &assets,
                                &window,
                            ),
                            TextColor(Color::from(WHITE).with_alpha(0.)),
                            TweenAnim::new(
                                Tween::new(
                                    EaseFunction::CubicOut,
                                    Duration::from_millis(ROUND_BANNER_ENTER_MS),
                                    TextColorLens {
                                        start: Color::from(WHITE).with_alpha(0.),
                                        end: WHITE.into(),
                                    },
                                )
                                .then(Delay::new(Duration::from_millis(ROUND_BANNER_HOLD_MS)))
                                .then(Tween::new(
                                    EaseFunction::CubicIn,
                                    Duration::from_millis(ROUND_BANNER_EXIT_MS),
                                    TextColorLens {
                                        start: WHITE.into(),
                                        end: Color::from(WHITE).with_alpha(0.),
                                    },
                                ))
                            ),
                            CombatCmp,
                        )],
                    )],
                    CombatCmp,
                ));
            }
        },
        CombatState::AntiBallistic
        | CombatState::Fire
        | CombatState::Repair
        | CombatState::Bomb
        | CombatState::DeathRay => {
            for (unit_e, unit_t, mut cu) in &mut unit_q {
                match cu.fire {
                    FireState::Select => {
                        if volley_fire {
                            cu.fire = FireState::Firing;
                        } else if individual_mode {
                            let bombing = *combat_state.get() == CombatState::Bomb;
                            let shooter_ids = round
                                .units(&cu.side)
                                .iter()
                                .filter(|shooter| {
                                    shooter.unit == cu.unit
                                        && (matches!(
                                            *combat_state.get(),
                                            CombatState::Repair | CombatState::DeathRay
                                        ) || shooter
                                            .shots
                                            .iter()
                                            .any(|shot| shot.is_bombing() == bombing))
                                })
                                .map(|shooter| shooter.id)
                                .collect::<Vec<_>>();
                            let mut highlighted = false;
                            for (individual_e, individual_t, individual) in &mut individual_q {
                                if individual.side == cu.side
                                    && individual.unit == cu.unit
                                    && (individual.id.is_some_and(|id| shooter_ids.contains(&id))
                                        || matches!(
                                            *combat_state.get(),
                                            CombatState::Repair | CombatState::DeathRay
                                        ) && individual.hull > 0)
                                {
                                    commands.entity(individual_e).insert((
                                        TweenAnim::new(Tween::new(
                                            EaseFunction::QuadraticInOut,
                                            Duration::from_millis(500),
                                            TransformScaleLens {
                                                start: individual_t.scale,
                                                end: individual_t.scale * 1.18,
                                            },
                                        )),
                                        CombatFireHighlight,
                                    ));
                                    highlighted = true;
                                }
                            }
                            if highlighted {
                                cu.fire = FireState::PreFire;
                            } else {
                                commands.entity(unit_e).insert((
                                    TweenAnim::new(Tween::new(
                                        EaseFunction::QuadraticInOut,
                                        Duration::from_millis(500),
                                        TransformScaleLens {
                                            start: unit_t.scale,
                                            end: unit_t.scale * 1.3,
                                        },
                                    )),
                                    CombatFireHighlight,
                                ));
                                cu.fire = FireState::PreFire;
                            }
                        } else {
                            commands.entity(unit_e).insert((
                                TweenAnim::new(Tween::new(
                                    EaseFunction::QuadraticInOut,
                                    Duration::from_millis(500),
                                    TransformScaleLens {
                                        start: unit_t.scale,
                                        end: unit_t.scale * 1.3,
                                    },
                                )),
                                CombatFireHighlight,
                            ));
                            cu.fire = FireState::PreFire;
                        }
                    },
                    FireState::PreFire => {
                        let individual_highlights = if individual_mode {
                            individual_q
                                .iter()
                                .filter_map(|(entity, _, individual)| {
                                    (individual.side == cu.side
                                        && individual.unit == cu.unit
                                        && highlighted_q.contains(entity))
                                    .then_some(entity)
                                })
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };
                        if completed.contains(&unit_e)
                            || (!individual_highlights.is_empty()
                                && individual_highlights
                                    .iter()
                                    .all(|entity| completed.contains(entity)))
                        {
                            cu.fire = FireState::Firing;
                        }
                    },
                    FireState::Firing if *combat_state.get() == CombatState::Repair => {
                        let repaired = round
                            .units(&cu.side)
                            .iter()
                            .flat_map(|cu2| cu2.repairs.iter().map(move |r| (cu2.id, cu2.unit, r)))
                            .collect::<Vec<_>>();

                        let individual_source = individual_mode.then(|| {
                            individual_q.iter().find_map(|(entity, transform, individual)| {
                                (individual.side == cu.side
                                    && individual.unit == cu.unit
                                    && individual.hull > 0)
                                    .then_some((entity, individual.unit, transform.translation))
                            })
                        });

                        for (target_id, unit, repair) in repaired {
                            // Hack the repair info into the shot report for code simplicity
                            spawn_shot_msg.write(SpawnShotMsg {
                                shot: ShotReport {
                                    target_id: Some(target_id),
                                    unit: Some(unit),
                                    hull_damage: *repair,
                                    ..default()
                                },
                                repair: true,
                                side: cu.side.clone(),
                                source: individual_source.flatten().or(Some((
                                    unit_e,
                                    cu.unit,
                                    unit_t.translation,
                                ))),
                            });
                        }

                        cu.fire = FireState::Deselect;
                    },
                    FireState::Firing if *combat_state.get() == CombatState::DeathRay => {
                        if let Some(ray_e) = death_ray_q.iter().next() {
                            if completed.contains(&ray_e) {
                                commands.entity(ray_e).despawn();
                                cu.fire = FireState::Deselect;
                                if report.planet_destroyed
                                    && state.combat_round == combat.rounds.len() - 1
                                {
                                    bg_q.into_inner().image = assets.image("destroyed bg");
                                    return;
                                }
                            }
                        } else {
                            let origin = if individual_mode {
                                individual_q
                                    .iter()
                                    .find_map(|(_, transform, individual)| {
                                        (individual.side == cu.side
                                            && individual.unit == cu.unit
                                            && individual.hull > 0)
                                            .then_some(transform.translation)
                                    })
                                    .unwrap_or(unit_t.translation)
                            } else {
                                unit_t.translation
                            };
                            commands.spawn((
                                Cinematic::new(
                                    origin,
                                    pos,
                                    projection.area.size(),
                                    size,
                                    report.planet_destroyed
                                        && state.combat_round == combat.rounds.len() - 1,
                                ),
                                // TweenAnim requires a concrete target; a bare Delay panics.
                                // This stationary tween times the cinematic and emits completion.
                                Transform::default(),
                                TweenAnim::new(Tween::new(
                                    EaseFunction::Linear,
                                    Duration::from_secs_f32(DEATH_RAY_DURATION),
                                    TransformScaleLens {
                                        start: Vec3::ONE,
                                        end: Vec3::ONE,
                                    },
                                )),
                                DeathRayCmp,
                                CombatCmp,
                            ));
                            play_audio_msg.write(PlayAudioMsg::new("death ray"));
                        }
                    },
                    FireState::Firing => {
                        for shooter in
                            round.units(&cu.side).iter().filter(|shooter| cu.unit == shooter.unit)
                        {
                            let individual_source = individual_mode.then(|| {
                                individual_q.iter().find_map(|(entity, transform, individual)| {
                                    (individual.id == Some(shooter.id)).then_some((
                                        entity,
                                        individual.unit,
                                        transform.translation,
                                    ))
                                })
                            });
                            let source = individual_source.flatten().unwrap_or((
                                unit_e,
                                cu.unit,
                                unit_t.translation,
                            ));
                            for shot in shooter.shots.iter().filter(|shot| {
                                shot.is_bombing() == (*combat_state.get() == CombatState::Bomb)
                            }) {
                                spawn_shot_msg.write(SpawnShotMsg {
                                    shot: shot.clone(),
                                    repair: false,
                                    side: cu.side.opposite(),
                                    source: Some(source),
                                });
                            }
                        }

                        cu.fire = FireState::Deselect;
                    },
                    FireState::Deselect => {
                        let individual_highlights = if individual_mode {
                            individual_q
                                .iter()
                                .filter_map(|(entity, transform, individual)| {
                                    (individual.side == cu.side
                                        && individual.unit == cu.unit
                                        && highlighted_q.contains(entity))
                                    .then_some((entity, transform.scale))
                                })
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };
                        if !individual_highlights.is_empty() {
                            for (entity, scale) in individual_highlights {
                                commands.entity(entity).insert(TweenAnim::new(Tween::new(
                                    EaseFunction::QuarticIn,
                                    Duration::from_millis(900),
                                    TransformScaleLens {
                                        start: scale,
                                        end: scale / 1.18,
                                    },
                                )));
                            }
                            cu.fire = FireState::AfterFire;
                        } else if highlighted_q.contains(unit_e) {
                            commands.entity(unit_e).insert(TweenAnim::new(Tween::new(
                                EaseFunction::QuarticIn,
                                Duration::from_millis(1500),
                                TransformScaleLens {
                                    start: unit_t.scale,
                                    end: unit_t.scale / 1.3,
                                },
                            )));
                            cu.fire = FireState::AfterFire;
                        } else if volley_fire {
                            cu.fire = FireState::VolleyResolving;
                        } else {
                            cu.fire = FireState::Fired;
                        }
                    },
                    FireState::AfterFire => {
                        let individual_highlights = if individual_mode {
                            individual_q
                                .iter()
                                .filter_map(|(entity, _, individual)| {
                                    (individual.side == cu.side
                                        && individual.unit == cu.unit
                                        && highlighted_q.contains(entity))
                                    .then_some(entity)
                                })
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };
                        if completed.contains(&unit_e)
                            || (!individual_highlights.is_empty()
                                && individual_highlights
                                    .iter()
                                    .all(|entity| completed.contains(entity)))
                        {
                            commands.entity(unit_e).remove::<CombatFireHighlight>();
                            for entity in individual_highlights {
                                commands.entity(entity).remove::<CombatFireHighlight>();
                            }
                            cu.fire = FireState::Fired;
                        }
                    },
                    _ => (),
                }
            }
        },
        CombatState::Salvage => {
            let salvage = report.defender_salvage();
            if salvage == default() {
                next_combat_state.set(CombatState::EndCombat);
                return;
            }

            if let Some(timer_e) = salvage_timer_q.iter().next() {
                if completed.contains(&timer_e) {
                    for pickup_e in &salvage_pickup_q {
                        commands.entity(pickup_e).despawn();
                    }
                    for (crawler_e, _) in &salvage_crawler_q {
                        commands.entity(crawler_e).remove::<(SalvageCrawlerCmp, TweenAnim)>();
                    }
                    commands.entity(timer_e).despawn();
                    next_combat_state.set(CombatState::EndCombat);
                }
                return;
            }

            if let Some((crawler_e, crawler)) = salvage_crawler_q.iter().next() {
                if crawler.phase == SalvageCrawlerPhase::Highlighting
                    && completed.contains(&crawler_e)
                {
                    let crawler_transform = unit_q
                        .get(crawler_e)
                        .map(|(_, transform, _)| (*transform, size))
                        .or_else(|_| {
                            individual_q
                                .get(crawler_e)
                                .map(|(_, transform, card)| (*transform, card.display_size))
                        });
                    let Ok((crawler_t, crawler_size)) = crawler_transform else {
                        next_combat_state.set(CombatState::EndCombat);
                        return;
                    };
                    commands.entity(crawler_e).insert((
                        SalvageCrawlerCmp {
                            home_scale: crawler.home_scale,
                            phase: SalvageCrawlerPhase::Returning,
                        },
                        TweenAnim::new(Tween::new(
                            EaseFunction::QuarticIn,
                            Duration::from_millis(SALVAGE_RETURN_TIME_MS),
                            TransformScaleLens {
                                start: crawler_t.scale,
                                end: crawler.home_scale,
                            },
                        )),
                    ));
                    spawn_salvage_pickups(
                        &mut commands,
                        crawler_t.translation,
                        crawler_size,
                        salvage,
                        &assets,
                    );
                    commands.spawn((
                        Transform::default(),
                        TweenAnim::new(Tween::new(
                            EaseFunction::Linear,
                            Duration::from_millis(SALVAGE_PICKUP_TIME_MS),
                            TransformScaleLens {
                                start: Vec3::ONE,
                                end: Vec3::ONE,
                            },
                        )),
                        SalvageTimerCmp,
                        CombatCmp,
                    ));
                    play_audio_msg.write(PlayAudioMsg::new("construction").rate(1.15));
                }
                return;
            }

            let individual_crawler = individual_mode.then(|| {
                individual_q.iter().find_map(|(entity, transform, unit)| {
                    (unit.side == Side::Defender && unit.unit == Unit::crawler() && unit.hull > 0)
                        .then_some((entity, *transform))
                })
            });
            let grouped_crawler = unit_q.iter().find_map(|(entity, transform, unit)| {
                (unit.side == Side::Defender && unit.unit == Unit::crawler() && unit.hull > 0)
                    .then_some((entity, *transform))
            });
            let Some((crawler_e, crawler_t)) = individual_crawler.flatten().or(grouped_crawler)
            else {
                next_combat_state.set(CombatState::EndCombat);
                return;
            };
            commands.entity(crawler_e).insert((
                SalvageCrawlerCmp {
                    home_scale: crawler_t.scale,
                    phase: SalvageCrawlerPhase::Highlighting,
                },
                TweenAnim::new(Tween::new(
                    EaseFunction::QuadraticInOut,
                    Duration::from_millis(SALVAGE_HIGHLIGHT_TIME_MS),
                    TransformScaleLens {
                        start: crawler_t.scale,
                        end: crawler_t.scale * 1.3,
                    },
                )),
            ));
        },
        CombatState::EndCombat => {},
    }
}

/// Updates combat stats from the current canonical ECS projection.
pub fn update_combat_stats(
    unit_q: Query<(Entity, &CombatUnitCmp, Option<&FleetRetreatCmp>)>,
    individual_q: Query<
        (Entity, &Sprite, &IndividualCombatUnitCmp),
        (Without<CombatUnitCmp>, Without<PendingImpact>, Without<ShieldCmp>, Without<HullCmp>),
    >,
    mut anim_q: Query<&mut TweenAnim, With<CombatCmp>>,
    mut count_q: Query<(&CountCmp, Option<&mut Text2d>, Option<&mut TextSpan>)>,
    mut shield_q: Query<
        (&mut Transform, &mut Sprite, Option<&PlanetaryShieldFillCmp>),
        (With<ShieldCmp>, Without<HullCmp>),
    >,
    mut hull_q: Query<(&mut Transform, &mut Sprite), (With<HullCmp>, Without<ShieldCmp>)>,
    mut visibility_q: ParamSet<(
        Single<&mut Visibility, With<CombatPausedCmp>>,
        Query<&mut Visibility, (With<DisplayTextCmp>, Without<CombatPausedCmp>)>,
    )>,
    children_q: Query<&Children>,
    settings: Res<Settings>,
    state: Res<UiState>,
    player: Res<Player>,
    combat_state: Res<State<CombatState>>,
    camera_q: Single<&Projection, With<MainCamera>>,
    time: Res<Time>,
) {
    let Projection::Orthographic(projection) = camera_q.into_inner() else {
        return;
    };
    let paused = settings.combat_paused && *combat_state.get() != CombatState::EndCombat;

    // Apply the speed chosen from the combat settings panel (or keyboard shortcuts).
    anim_q.iter_mut().for_each(|mut t| {
        if paused {
            t.playback_state = PlaybackState::Paused;
        } else {
            t.playback_state = PlaybackState::Playing;
            t.speed = settings.combat_speed as f64;
        }
    });

    {
        let mut paused_q = visibility_q.p0();
        **paused_q = if paused {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    // Pause replaces the round/result presentation while its animation is frozen.
    for mut visibility in &mut visibility_q.p1() {
        *visibility = if paused {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }

    let Some((report, combat, round)) = selected_combat_round(&state, &player) else {
        return;
    };

    let size = UNIT_SIZE * projection.scale;
    let speed = (3. * time.delta_secs() * settings.speed()).clamp(0., 1.);
    let antiballistic_fired = unit_q
        .iter()
        .any(|(_, cu, _)| cu.unit == Unit::antiballistic_missile() && cu.fire.has_fired());
    let interplanetary_fired = unit_q
        .iter()
        .any(|(_, cu, _)| cu.unit == Unit::interplanetary_missile() && cu.fire.has_fired());

    for (unit_e, cu, fleeing) in &unit_q {
        for child in children_q.iter_descendants(unit_e) {
            if let Ok((counter, text, span)) = count_q.get_mut(child) {
                let count = displayed_combat_unit_count(
                    report,
                    combat,
                    round,
                    combat_state.get(),
                    cu,
                    fleeing.is_some(),
                    counter.owner,
                    antiballistic_fired,
                    interplanetary_fired,
                );
                if let Some(mut text) = text {
                    text.0 = count.to_string();
                }
                if let Some(mut span) = span {
                    span.0 = format!("{COMBAT_COUNT_SEPARATOR}{count}");
                }
            }

            if let Ok((mut shield_t, mut shield_s, planetary_fill)) = shield_q.get_mut(child) {
                if let Some(shield_size) = shield_s.custom_size.as_mut() {
                    let full_size = planetary_fill.map_or(size * 0.96, |fill| fill.full_width);
                    shield_size.x = shield_size
                        .x
                        .lerp(full_size * cu.shield as f32 / cu.max_shield.max(1) as f32, speed)
                        .clamp(0., full_size);
                    shield_t.translation.x = (shield_size.x - full_size) * 0.5;
                }
            }

            if let Ok((mut hull_t, mut hull_s)) = hull_q.get_mut(child) {
                if let Some(hull_size) = hull_s.custom_size.as_mut() {
                    let full_size = size * 0.96;
                    hull_size.x = hull_size
                        .x
                        .lerp(full_size * cu.hull as f32 / cu.max_hull.max(1) as f32, speed)
                        .clamp(0., full_size);
                    hull_t.translation.x = (hull_size.x - full_size) * 0.5;
                }
            }
        }
    }

    for (unit_e, sprite, cu) in &individual_q {
        let card_size = sprite.custom_size.unwrap_or(Vec2::splat(size)).x;
        for child in children_q.iter_descendants(unit_e) {
            if let Ok((mut shield_t, mut shield_s, _)) = shield_q.get_mut(child) {
                if let Some(shield_size) = shield_s.custom_size.as_mut() {
                    let full_size = card_size * 0.96;
                    shield_size.x = shield_size
                        .x
                        .lerp(full_size * cu.shield as f32 / cu.max_shield.max(1) as f32, speed)
                        .clamp(0.0, full_size);
                    shield_t.translation.x = (shield_size.x - full_size) * 0.5;
                }
            }
            if let Ok((mut hull_t, mut hull_s)) = hull_q.get_mut(child) {
                if let Some(hull_size) = hull_s.custom_size.as_mut() {
                    let full_size = card_size * 0.96;
                    hull_size.x = hull_size
                        .x
                        .lerp(full_size * cu.hull as f32 / cu.max_hull.max(1) as f32, speed)
                        .clamp(0.0, full_size);
                    hull_t.translation.x = (hull_size.x - full_size) * 0.5;
                }
            }
        }
    }
}

/// Cleans up combat state and retained entities on state exit.
pub fn exit_combat(
    mut commands: Commands,
    mut state: ResMut<UiState>,
    mut next_combat_state: ResMut<NextState<CombatState>>,
    mut mute_audio_msg: MessageWriter<MuteAudioMsg>,
) {
    commands.remove_resource::<CombatRoundJump>();
    commands.remove_resource::<CombatFormationState>();
    state.combat_round = 0;
    mute_audio_msg.write(MuteAudioMsg);
    next_combat_state.set(CombatState::default());
}

#[cfg(test)]
#[path = "../../../tests/core/combat_animation.rs"]
mod tests;
