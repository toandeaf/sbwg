//! World setup: spawn the settlement, lay out the town, stamp the collision and
//! territory grids, then place the leader and caravans on walkable tiles.

use bevy::prelude::*;
use protocol::PlayerId;

use crate::entity::{Building, Caravan, CaravanState, Mover, Settlement};
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

// Caravan spawn config (DESIGN §13: water logistics).
pub(crate) const CARAVAN_COUNT: usize = 3;
const CARAVAN_CAPACITY: u32 = 50; // units of water per trip
const CARAVAN_CAMELS: u32 = 6;
const CARAVAN_PEOPLE: u32 = 4;

fn spawn_settlement(mut commands: Commands, map: Res<Map>) {
    let pos = Vec2::new(map.width as f32 / 2.0, map.height as f32 / 2.0);
    commands.spawn(Settlement {
        owner: TOWN_OWNER,
        pos,
        population: SETTLEMENT_POP,
        water_stored: 0,
    });
}

/// Lay out a clustered town around the settlement anchor, leaving 1-tile gaps as
/// streets so the crowd can move between buildings (DESIGN §6.2/§17.4).
fn build_town(mut commands: Commands, map: Res<Map>, mut rng: ResMut<SimRng>) {
    let cx = (map.width / 2) as f32;
    let cy = (map.height / 2) as f32;
    let mut placed: Vec<(i32, i32, i32, i32)> = Vec::new();

    // Central great-house, covering the centre tile.
    let centre = IVec2::new(map.width / 2 - 1, map.height / 2 - 1);
    commands.spawn(Building { owner: TOWN_OWNER, tile: centre, size: IVec2::new(2, 2) });
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

fn spawn_leader(mut commands: Commands, map: Res<Map>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    let home = map.find_walkable_near(settlement.pos, 3.0);
    commands.spawn(Mover { home, prev: home, pos: home, vel: Vec2::ZERO });
}

/// Spawn caravans, each assigned to a water source, dropping at the settlement.
fn spawn_caravans(mut commands: Commands, map: Res<Map>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    let mut waters = map.water_tiles();
    if waters.is_empty() {
        return;
    }
    // Placeholder: target the furthest water sources first. A proper priority
    // system (need / distance / capacity) comes later.
    let anchor = settlement.pos;
    waters.sort_by(|a, b| {
        b.distance_squared(anchor)
            .partial_cmp(&a.distance_squared(anchor))
            .unwrap()
    });
    let dest = map.find_walkable_near(settlement.pos, 2.0);
    for i in 0..CARAVAN_COUNT {
        let water = waters[i % waters.len()];
        let source = map.find_walkable_near(water, 1.0);
        commands.spawn(Caravan {
            prev: dest,
            pos: dest,
            camels: CARAVAN_CAMELS,
            people: CARAVAN_PEOPLE,
            capacity: CARAVAN_CAPACITY,
            cargo: 0,
            source,
            dest,
            state: CaravanState::ToSource,
            timer: 0.0,
        });
    }
}

/// Spawns the world and places the initial movers.
pub struct SetupPlugin;

impl Plugin for SetupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_settlement, build_town))
            // After buildings exist: stamp the collision grid and the territory
            // grid, then place the leader and caravans on walkable tiles.
            .add_systems(
                PostStartup,
                (stamp_obstacles, stamp_territory, spawn_leader, spawn_caravans).chain(),
            );
    }
}
