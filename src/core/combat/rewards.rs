//! Cosmetic cargo allocation and cues shared by both combat presentations.

use crate::core::audio::PlayAudioMsg;
use crate::core::resources::{ResourceName, Resources};

/// Splits recorded loot by carrying strength without creating or losing any resource.
/// Callers supply only the surviving cards visible in their current formation.
pub(super) fn resource_shares(resources: Resources, weights: &[usize]) -> Vec<Resources> {
    let mut remaining_weight: u128 = weights.iter().map(|weight| *weight as u128).sum();
    let mut remaining = resources.total();
    let mut space = Vec::with_capacity(weights.len());
    for weight in weights {
        let share = (remaining as u128 * *weight as u128).checked_div(remaining_weight).unwrap_or(0)
            as usize;
        space.push(share);
        remaining -= share;
        remaining_weight -= *weight as u128;
    }
    let mut shares = vec![Resources::default(); weights.len()];
    for kind in [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium] {
        let mut amount = resources.get(&kind);
        let mut available: u128 = space.iter().map(|capacity| *capacity as u128).sum();
        for (share, capacity) in shares.iter_mut().zip(&mut space) {
            let old_capacity = *capacity;
            let taken = (amount as u128 * old_capacity as u128).checked_div(available).unwrap_or(0)
                as usize;
            *share.get_mut(&kind) = taken;
            *capacity -= taken;
            amount -= taken;
            available -= old_capacity as u128;
        }
    }
    shares
}

pub(super) fn resource_pickup_cue(raiding: bool) -> PlayAudioMsg {
    if raiding {
        PlayAudioMsg::new("raid loot").rate(1.15).gain(-12.0)
    } else {
        PlayAudioMsg::new("construction").rate(1.15)
    }
}

#[cfg(test)]
#[path = "../../../tests/core/combat_rewards.rs"]
mod tests;
