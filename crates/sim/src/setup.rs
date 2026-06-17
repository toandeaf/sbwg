//! World setup: spawn the settlement, lay out the town (designating a central
//! water store), stamp the collision + territory grids, claim nearby water, then
//! place the leader and idle caravans.

use bevy::prelude::*;
use protocol::PlayerId;

use crate::caravan::{Caravan, Market, WaterStore};
use crate::entity::{Building, Mover, Settlement};
use crate::map::{Map, SimRng, Territory};

/// Starting population of the scaffold settlement. The client renders this many
/// cosmetic swarm figures (DESIGN §17.4).
const SETTLEMENT_POP: u32 = 60;

/// Town generation (DESIGN §6.2: a dense cluster of ~4–6-tile buildings).
const TOWN_RADIUS: f32 = 7.0;
const BUILDING_TARGET: usize = 13;
/// The scaffold's single player; everything it builds becomes its territory.
pub(crate) const TOWN_OWNER: PlayerId = 0;
/// Tiles of claimed margin around each building (DESIGN §8 territory).
const TERRITORY_MARGIN: i32 = 2;

/// Grace buffer in the store at game start, so the town isn't instantly parched
/// before the player claims a water source (DESIGN §13).
const STARTING_WATER: u32 = 60;
/// Treasury at game start, enough to bridge to first trade income (DESIGN §9).
const STARTING_WEALTH: u32 = 400;
/// Markets to scatter around the map for trade caravans (DESIGN §9).
const MARKET_COUNT: usize = 4;

// Caravan spawn config (DESIGN §13: water logistics + §9 trade).
pub(crate) const CARAVAN_COUNT: usize = 6;
// Deliberately modest so a single source can't sustain the starting pop —
// the player must claim more to grow (tune up later for balance).
const CARAVAN_CAPACITY: u32 = 30; // units of water per trip
const CARAVAN_CAMELS: u32 = 6;
const CARAVAN_PEOPLE: u32 = 4;

fn spawn_settlement(mut commands: Commands, map: Res<Map>) {
    let pos = Vec2::new(map.width as f32 / 2.0, map.height as f32 / 2.0);
    commands.spawn(Settlement {
        owner: TOWN_OWNER,
        pos,
        population: SETTLEMENT_POP,
        treasury: STARTING_WEALTH,
        goods: 0,
    });
}

/// Lay out a clustered town around the settlement anchor, leaving 1-tile gaps as
/// streets so the crowd can move between buildings (DESIGN §6.2/§17.4). The
/// central great-house is the water store.
fn build_town(mut commands: Commands, map: Res<Map>, mut rng: ResMut<SimRng>) {
    let cx = (map.width / 2) as f32;
    let cy = (map.height / 2) as f32;
    let mut placed: Vec<(i32, i32, i32, i32)> = Vec::new();

    // Central great-house — the designated water store (pos fixed up by init_storage).
    let centre = IVec2::new(map.width / 2 - 1, map.height / 2 - 1);
    commands.spawn((
        Building { owner: TOWN_OWNER, tile: centre, size: IVec2::new(2, 2) },
        WaterStore { pos: Vec2::new(cx, cy), stored: STARTING_WATER },
    ));
    placed.push((centre.x, centre.y, 2, 2));

    let sizes = [IVec2::new(2, 2), IVec2::new(2, 3), IVec2::new(3, 2)];
    for _ in 0..BUILDING_TARGET {
        for _try in 0..20 {
            let size = sizes[(rng.next_u64() % sizes.len() as u64) as usize];
            // Uniform-ish point in the town disc.
            let r = (rng.next_u64() as f32 / u64::MAX as f32).sqrt() * TOWN_RADIUS;
            let a = rng.next_u64() as f32 / u64::MAX as f32 * std::f32::consts::TAU;
            let x = (cx + a.cos() * r).floor() as i32 - size.x / 2;
            let y = (cy + a.sin() * r).floor() as i32 - size.y / 2;
            if building_fits(&placed, &map, x, y, size.x, size.y) {
                commands.spawn(Building { owner: TOWN_OWNER, tile: IVec2::new(x, y), size });
                placed.push((x, y, size.x, size.y));
                break;
            }
        }
    }
}

/// A candidate building fits if every tile is walkable (no water/edge) and it
/// keeps a 1-tile gap from already-placed buildings.
fn building_fits(placed: &[(i32, i32, i32, i32)], map: &Map, x: i32, y: i32, w: i32, h: i32) -> bool {
    for dy in 0..h {
        for dx in 0..w {
            if !map.is_walkable(x + dx, y + dy) {
                return false;
            }
        }
    }
    for &(px, py, pw, ph) in placed {
        if x - 1 < px + pw && px < x + w + 1 && y - 1 < py + ph && py < y + h + 1 {
            return false;
        }
    }
    true
}

/// Bake building footprints into the map's collision grid.
fn stamp_obstacles(mut map: ResMut<Map>, buildings: Query<&Building>) {
    for building in &buildings {
        for dy in 0..building.size.y {
            for dx in 0..building.size.x {
                let (x, y) = (building.tile.x + dx, building.tile.y + dy);
                if map.in_bounds(x, y) {
                    let idx = map.idx(x, y);
                    map.blocked[idx] = true;
                }
            }
        }
    }
}

/// Claim each building's footprint plus a margin for its owner (DESIGN §8).
fn stamp_territory(buildings: Query<&Building>, mut territory: ResMut<Territory>) {
    let margin = IVec2::splat(TERRITORY_MARGIN);
    for building in &buildings {
        let min = building.tile - margin;
        let max = building.tile + building.size - IVec2::ONE + margin;
        territory.claim_rect(building.owner, min, max);
    }
}

/// Fix the water store's drop-point to a walkable tile beside its building, now
/// that the collision grid is stamped.
fn init_storage(map: Res<Map>, mut stores: Query<(&Building, &mut WaterStore)>) {
    for (building, mut store) in &mut stores {
        store.pos = map.find_walkable_near(building.center(), 1.0);
    }
}

/// Claim the water source(s) inside the city bounds, so the town starts with one
/// served source; the outskirts must be claimed by the player's directive.
fn claim_home_water(map: Res<Map>, mut territory: ResMut<Territory>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    for water in map.water_tiles() {
        if water.distance(settlement.pos) <= TOWN_RADIUS {
            let tile = IVec2::new(water.x.floor() as i32, water.y.floor() as i32);
            territory.claim_rect(settlement.owner, tile - IVec2::ONE, tile + IVec2::ONE);
        }
    }
}

fn spawn_leader(mut commands: Commands, map: Res<Map>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    let home = map.find_walkable_near(settlement.pos, 3.0);
    commands.spawn(Mover { home, prev: home, pos: home, vel: Vec2::ZERO });
}

/// Spawn idle caravans parked at the water store; the planner routes them.
fn spawn_caravans(mut commands: Commands, stores: Query<&WaterStore>) {
    let Ok(store) = stores.single() else { return };
    for _ in 0..CARAVAN_COUNT {
        commands.spawn(Caravan::idle_at(
            store.pos,
            CARAVAN_CAMELS,
            CARAVAN_PEOPLE,
            CARAVAN_CAPACITY,
        ));
    }
}

/// Scatter markets around the map for trade caravans (DESIGN §9).
fn spawn_markets(mut commands: Commands, map: Res<Map>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    let dist = map.width.min(map.height) as f32 * 0.4;
    for k in 0..MARKET_COUNT {
        let a = std::f32::consts::TAU * (k as f32 / MARKET_COUNT as f32) + 0.4;
        let p = settlement.pos + Vec2::new(a.cos(), a.sin()) * dist;
        commands.spawn(Market { pos: map.find_walkable_near(p, 0.0) });
    }
}

/// Spawns the world and places the initial movers.
pub struct SetupPlugin;

impl Plugin for SetupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_settlement, build_town))
            // After buildings exist: stamp grids, fix the store's drop-point,
            // then place the leader and (idle) caravans. Water is claimed at
            // runtime via the player's claim directive.
            .add_systems(
                PostStartup,
                (
                    stamp_obstacles,
                    stamp_territory,
                    claim_home_water,
                    init_storage,
                    spawn_leader,
                    spawn_caravans,
                    spawn_markets,
                )
                    .chain(),
            );
    }
}
