//! World setup: spawn the settlement, lay out the town (designating a central
//! water store), stamp the collision + territory grids, claim nearby water, then
//! place the leader and idle caravans.

use bevy::prelude::*;
use protocol::PlayerId;

use crate::caravan::{Caravan, WaterStore};
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
/// How many of the nearest water sources the settlement claims (placeholder —
/// a real claim mechanic comes later, DESIGN §8).
const CLAIMED_WATER: usize = 4;

// Caravan spawn config (DESIGN §13: water logistics).
pub(crate) const CARAVAN_COUNT: usize = 3;
const CARAVAN_CAPACITY: u32 = 50; // units of water per trip
const CARAVAN_CAMELS: u32 = 6;
const CARAVAN_PEOPLE: u32 = 4;

fn spawn_settlement(mut commands: Commands, map: Res<Map>) {
    let pos = Vec2::new(map.width as f32 / 2.0, map.height as f32 / 2.0);
    commands.spawn(Settlement { owner: TOWN_OWNER, pos, population: SETTLEMENT_POP });
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
        WaterStore { pos: Vec2::new(cx, cy), stored: 0 },
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
    for building in &buildings {
        let x0 = building.tile.x - TERRITORY_MARGIN;
        let y0 = building.tile.y - TERRITORY_MARGIN;
        let x1 = building.tile.x + building.size.x - 1 + TERRITORY_MARGIN;
        let y1 = building.tile.y + building.size.y - 1 + TERRITORY_MARGIN;
        for y in y0..=y1 {
            for x in x0..=x1 {
                if territory.in_bounds(x, y) {
                    let idx = territory.idx(x, y);
                    territory.owner[idx] = building.owner as i32;
                }
            }
        }
    }
}

/// Fix the water store's drop-point to a walkable tile beside its building, now
/// that the collision grid is stamped.
fn init_storage(map: Res<Map>, mut stores: Query<(&Building, &mut WaterStore)>) {
    for (building, mut store) in &mut stores {
        store.pos = map.find_walkable_near(building.center(), 1.0);
    }
}

/// Claim the nearest water sources into the settlement's territory (placeholder).
fn claim_water(map: Res<Map>, mut territory: ResMut<Territory>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    let mut waters: Vec<IVec2> = map
        .water_tiles()
        .into_iter()
        .map(|w| IVec2::new(w.x.floor() as i32, w.y.floor() as i32))
        .collect();
    waters.sort_by(|a, b| {
        let da = Vec2::new(a.x as f32 + 0.5, a.y as f32 + 0.5).distance_squared(settlement.pos);
        let db = Vec2::new(b.x as f32 + 0.5, b.y as f32 + 0.5).distance_squared(settlement.pos);
        da.partial_cmp(&db).unwrap()
    });
    for tile in waters.into_iter().take(CLAIMED_WATER) {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y) = (tile.x + dx, tile.y + dy);
                if territory.in_bounds(x, y) {
                    let idx = territory.idx(x, y);
                    territory.owner[idx] = settlement.owner as i32;
                }
            }
        }
    }
}

fn spawn_leader(mut commands: Commands, map: Res<Map>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    let home = map.find_walkable_near(settlement.pos, 3.0);
    commands.spawn(Mover { home, prev: home, pos: home, vel: Vec2::ZERO });
}

/// Spawn idle caravans parked at the water store; `assign_caravans` routes them.
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

/// Spawns the world and places the initial movers.
pub struct SetupPlugin;

impl Plugin for SetupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_settlement, build_town))
            // After buildings exist: stamp grids, fix the store's drop-point,
            // claim water, then place the leader and idle caravans.
            .add_systems(
                PostStartup,
                (
                    stamp_obstacles,
                    stamp_territory,
                    init_storage,
                    claim_water,
                    spawn_leader,
                    spawn_caravans,
                )
                    .chain(),
            );
    }
}
