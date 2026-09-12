//! Deterministic bilateral resource-trade agreements and Trading Post range rules.

use serde::{Deserialize, Serialize};

use crate::core::identity::PlayerId;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Unit};

/// Tradable resources one completed Trading Post level can send in a turn.
pub const TRADE_RESOURCES_PER_LEVEL: usize = 500;
/// Maximum center-to-center distance of an adjacent Trading Post route, in AU.
pub const TRADING_POST_ADJACENCY_AU: f32 = 3.0;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One side of a finalized bilateral trade.
pub struct TradeParty {
    /// Player sending this side's resources.
    pub player_id: PlayerId,
    /// Owned planet whose completed Trading Post provides this player's capacity.
    pub planet_id: PlanetId,
    /// Metal, Crystal, and Deuterium reserved for the counterparty.
    pub resources: Resources,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// A current-turn bilateral exchange accepted by both players.
pub struct TradeAgreement {
    /// Stable identifier shared with the coordination invitation.
    pub id: u64,
    /// Turn during which resources are reserved and delivered.
    pub turn: u64,
    /// Exactly two distinct players, stored in ascending player-slot order.
    pub parties: [TradeParty; 2],
}

impl TradeAgreement {
    /// Returns the party belonging to one player.
    pub fn party(&self, player_id: PlayerId) -> Option<&TradeParty> {
        self.parties.iter().find(|party| party.player_id == player_id)
    }

    /// Returns the resources this agreement will deliver to one player.
    pub fn incoming(&self, player_id: PlayerId) -> Resources {
        self.parties
            .iter()
            .find(|party| party.player_id != player_id)
            .filter(|_| self.party(player_id).is_some())
            .map_or_else(Resources::default, |party| party.resources)
    }
}

/// Returns a completed post's per-turn outgoing capacity.
pub fn trading_post_capacity(planet: &Planet, player_id: PlayerId) -> usize {
    if planet.is_destroyed || planet.is_moon() || planet.owned != Some(player_id) {
        return 0;
    }
    planet
        .army
        .amount(&Unit::Building(Building::TradingPost))
        .min(Building::MAX_LEVEL)
        .saturating_mul(TRADE_RESOURCES_PER_LEVEL)
}

/// Returns whether two owned posts form a direct commerce route.
pub fn trading_posts_are_adjacent(
    map: &Map,
    first_player: PlayerId,
    first_planet: PlanetId,
    second_player: PlayerId,
    second_planet: PlanetId,
) -> bool {
    if first_player == second_player || first_planet == second_planet {
        return false;
    }
    let (Some(first), Some(second)) = (map.try_get(first_planet), map.try_get(second_planet))
    else {
        return false;
    };
    trading_post_capacity(first, first_player) > 0
        && trading_post_capacity(second, second_player) > 0
        && planets_are_adjacent(first, second)
}

/// Returns whether two worlds are close enough to be considered adjacent for commerce.
pub fn planets_are_adjacent(first: &Planet, second: &Planet) -> bool {
    first.id != second.id
        && !first.is_destroyed
        && !second.is_destroyed
        && first.position.distance(second.position) <= Planet::SIZE * TRADING_POST_ADJACENCY_AU
}

/// Returns every adjacent foreign post reachable from one owned post.
pub fn adjacent_trading_posts(
    map: &Map,
    player_id: PlayerId,
    planet_id: PlanetId,
) -> Vec<(PlayerId, PlanetId)> {
    map.planets
        .iter()
        .filter_map(|planet| {
            let other = planet.owned?;
            trading_posts_are_adjacent(map, player_id, planet_id, other, planet.id)
                .then_some((other, planet.id))
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/core/trading.rs"]
mod tests;
