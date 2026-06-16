//! Manpower-bounded claim capacity (DESIGN §8).
//!
//! Holding frontier territory ties up garrison drawn from population, scaled by
//! distance from the seat — your settled core (within [`FREE_RADIUS`]) is free,
//! land further out costs more men to hold. You can't claim past your capacity.
//! Shared by the authoritative gate (`apply_commands`) and the client's claim
//! preview, so both agree.

use bevy::prelude::*;
use protocol::PlayerId;

use crate::map::Territory;

/// Tiles within this distance of the seat are free to hold (the settled core).
const FREE_RADIUS: f32 = 9.0;
/// Garrison cost per frontier tile, per tile of distance beyond the core.
const COST_PER_DIST: f32 = 0.5;
/// Fraction of population available as garrison "hold points".
const MANPOWER_PER_POP: f32 = 0.5;

/// Garrison capacity available from a given population.
pub fn capacity(population: u32) -> f32 {
    population as f32 * MANPOWER_PER_POP
}

/// Hold cost of a single tile, by distance from the seat (free within the core).
pub fn hold_cost(tile: IVec2, seat: Vec2) -> f32 {
    let centre = Vec2::new(tile.x as f32 + 0.5, tile.y as f32 + 0.5);
    (centre.distance(seat) - FREE_RADIUS).max(0.0) * COST_PER_DIST
}

/// Total garrison currently tied up holding `owner`'s territory.
pub fn used(territory: &Territory, owner: PlayerId, seat: Vec2) -> f32 {
    let mut total = 0.0;
    for y in 0..territory.height {
        for x in 0..territory.width {
            if territory.owner_at(x, y) == Some(owner) {
                total += hold_cost(IVec2::new(x, y), seat);
            }
        }
    }
    total
}

/// Cost of newly claiming the inclusive rect (tiles not already owned by owner).
pub fn claim_cost(territory: &Territory, owner: PlayerId, seat: Vec2, min: IVec2, max: IVec2) -> f32 {
    let mut total = 0.0;
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            if territory.in_bounds(x, y) && territory.owner_at(x, y) != Some(owner) {
                total += hold_cost(IVec2::new(x, y), seat);
            }
        }
    }
    total
}

/// Can `owner` afford to claim the rect, given their current holdings + population?
pub fn can_claim(
    territory: &Territory,
    owner: PlayerId,
    seat: Vec2,
    population: u32,
    min: IVec2,
    max: IVec2,
) -> bool {
    used(territory, owner, seat) + claim_cost(territory, owner, seat, min, max) <= capacity(population)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Territory;

    #[test]
    fn capacity_scales_with_population() {
        assert_eq!(capacity(60), 30.0);
        assert_eq!(capacity(0), 0.0);
    }

    #[test]
    fn core_is_free_frontier_costs() {
        let seat = Vec2::new(16.0, 12.0);
        assert_eq!(hold_cost(IVec2::new(16, 12), seat), 0.0); // at the seat
        assert!(hold_cost(IVec2::new(0, 0), seat) > 0.0); // far corner
    }

    #[test]
    fn already_owned_tiles_are_free_to_reclaim() {
        let mut territory = Territory::default();
        let seat = Vec2::new(16.0, 12.0);
        let (min, max) = (IVec2::new(0, 0), IVec2::new(2, 2));
        assert!(claim_cost(&territory, 0, seat, min, max) > 0.0);
        territory.claim_rect(0, min, max);
        assert_eq!(claim_cost(&territory, 0, seat, min, max), 0.0);
    }

    #[test]
    fn capacity_gates_far_claims() {
        let territory = Territory::default();
        let seat = Vec2::new(16.0, 12.0);
        // A big far rect blows a small population's capacity.
        assert!(!can_claim(&territory, 0, seat, 10, IVec2::new(0, 0), IVec2::new(10, 10)));
        // A tiny claim in the free core is always fine.
        assert!(can_claim(&territory, 0, seat, 60, IVec2::new(15, 11), IVec2::new(16, 12)));
    }
}
