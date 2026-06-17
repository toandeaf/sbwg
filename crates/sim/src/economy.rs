//! Wealth economy (DESIGN §9/§13). Population + buildings *produce goods*; trade
//! caravans haul those goods to markets and sell them for **wealth** (that income
//! lives in `caravan`). Here we run the slow ticks: produce goods each interval
//! and pay per-building upkeep. Wealth then bounds how much you can build.

use bevy::prelude::*;
use protocol::PlayerId;

use crate::entity::{Building, Settlement};
use crate::map::{Map, Territory};

const ECONOMY_INTERVAL: f32 = 1.0;
/// Goods produced/sec ≈ sqrt(population) × this (diminishing returns).
const GOODS_FACTOR: f32 = 2.0;
/// Don't let unsold goods pile up without bound.
const GOODS_CAP: u32 = 999;
const UPKEEP_PER_BUILDING: u32 = 1;

/// Wealth to construct one building.
pub const BUILD_COST: u32 = 50;

/// Goods/second a settlement produces from its population (diminishing returns).
pub fn goods_production(population: u32) -> u32 {
    ((population as f32).sqrt() * GOODS_FACTOR) as u32
}

/// Wealth/second spent maintaining `buildings` structures.
pub fn upkeep(buildings: u32) -> u32 {
    buildings * UPKEEP_PER_BUILDING
}

/// Can `player` build on `tile`? Owned land, open ground, and affordable.
pub fn can_build(
    map: &Map,
    territory: &Territory,
    treasury: u32,
    player: PlayerId,
    tile: IVec2,
) -> bool {
    treasury >= BUILD_COST
        && map.is_walkable(tile.x, tile.y)
        && territory.owner_at(tile.x, tile.y) == Some(player)
}

/// Produce goods and pay upkeep once per interval.
/// (Single-settlement for now — upkeep counts all buildings.)
fn update_economy(
    time: Res<Time>,
    mut elapsed: Local<f32>,
    mut settlements: Query<&mut Settlement>,
    buildings: Query<&Building>,
) {
    *elapsed += time.delta_secs();
    if *elapsed < ECONOMY_INTERVAL {
        return;
    }
    *elapsed -= ECONOMY_INTERVAL;

    let spent = upkeep(buildings.iter().count() as u32);
    for mut settlement in &mut settlements {
        let produced = goods_production(settlement.population);
        settlement.goods = (settlement.goods + produced).min(GOODS_CAP);
        settlement.treasury = settlement.treasury.saturating_sub(spent);
    }
}

/// Goods production + upkeep over time.
pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, update_economy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Map, Territory};

    #[test]
    fn goods_production_has_diminishing_returns() {
        assert_eq!(goods_production(0), 0);
        assert!(goods_production(100) > goods_production(25));
        assert!(goods_production(100) < goods_production(25) * 4);
    }

    #[test]
    fn upkeep_scales_with_buildings() {
        assert_eq!(upkeep(0), 0);
        assert_eq!(upkeep(10), 10 * UPKEEP_PER_BUILDING);
    }

    #[test]
    fn can_build_requires_owned_walkable_affordable() {
        let map = Map::default();
        let mut territory = Territory::default();
        let centre = Vec2::new(map.width as f32 / 2.0, map.height as f32 / 2.0);
        let p = map.find_walkable_near(centre, 1.0);
        let tile = IVec2::new(p.x.floor() as i32, p.y.floor() as i32);

        assert!(!can_build(&map, &territory, 9_999, 0, tile)); // unowned
        territory.claim_rect(0, tile, tile);
        assert!(can_build(&map, &territory, BUILD_COST, 0, tile));
        assert!(!can_build(&map, &territory, BUILD_COST - 1, 0, tile)); // too poor
    }
}
