//! Caravan logistics — the first "action": haul water from claimed sources to
//! the central store along A* routes. The assign → route → drive shape here is
//! the template the wider action system will follow.

use bevy::prelude::*;
use protocol::SimEvent;
use std::collections::HashSet;

use crate::entity::Settlement;
use crate::map::{Map, Territory};
use crate::messages::OutgoingEvent;
use crate::path::find_path;

const CARAVAN_SPEED: f32 = 2.0; // tiles/second
const LOAD_SECS: f32 = 1.5;
const UNLOAD_SECS: f32 = 1.5;

/// Water storage held by a designated central building (DESIGN §13).
#[derive(Component, Debug)]
pub struct WaterStore {
    /// Walkable drop-point beside the storage building (the route's store end).
    pub pos: Vec2,
    pub stored: u32,
}

/// What a caravan is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaravanState {
    Idle,
    ToSource,
    Loading,
    ToStore,
    Unloading,
}

/// A real, authoritative goods-hauler: camels + people shuttling water from a
/// claimed source to the store along a fixed A* route (DESIGN §13). Position is
/// interpolated by the client between sim ticks.
#[derive(Component, Debug)]
pub struct Caravan {
    pub prev: Vec2,
    pub pos: Vec2,
    pub camels: u32,
    pub people: u32,
    pub capacity: u32,
    pub cargo: u32,
    pub state: CaravanState,
    pub timer: f32,
    /// The claimed water tile this caravan serves (`None` = idle/unassigned).
    pub source: Option<IVec2>,
    /// Route waypoints, store end (index 0) → source draw-point (last).
    pub route: Vec<Vec2>,
    /// Index of the waypoint currently being travelled toward.
    pub wp: usize,
}

impl Caravan {
    /// A fresh, unassigned caravan parked at `pos`.
    pub fn idle_at(pos: Vec2, camels: u32, people: u32, capacity: u32) -> Self {
        Self {
            prev: pos,
            pos,
            camels,
            people,
            capacity,
            cargo: 0,
            state: CaravanState::Idle,
            timer: 0.0,
            source: None,
            route: Vec::new(),
            wp: 0,
        }
    }
}

/// Assign idle caravans to claimed, unserviced water sources and route them.
///
/// 1. Work out which water sources are claimed (owned tiles).
/// 2. List available (idle) caravans.
/// 3. Assign each to the nearest unserviced claimed source.
/// 4. Route it store → source with A* pathfinding.
fn assign_caravans(
    map: Res<Map>,
    territory: Res<Territory>,
    settlements: Query<&Settlement>,
    stores: Query<&WaterStore>,
    mut caravans: Query<&mut Caravan>,
) {
    if !caravans.iter().any(|c| c.state == CaravanState::Idle) {
        return; // nothing to assign
    }
    let Ok(settlement) = settlements.single() else { return };
    let Ok(store) = stores.single() else { return };
    let owner = settlement.owner;
    let store_tile = tile_of(store.pos);

    // 1. Claimed water sources = owned water tiles, nearest to the store first.
    let mut claimed: Vec<IVec2> = map
        .water_tiles()
        .into_iter()
        .map(tile_of)
        .filter(|t| territory.owner_at(t.x, t.y) == Some(owner))
        .collect();
    claimed.sort_by_key(|t| (*t - store_tile).length_squared());

    // Sources already being served by some caravan.
    let mut serviced: HashSet<IVec2> = caravans.iter().filter_map(|c| c.source).collect();

    for mut caravan in &mut caravans {
        if caravan.state != CaravanState::Idle {
            continue;
        }
        for &tile in &claimed {
            if serviced.contains(&tile) {
                continue;
            }
            let draw = map.find_walkable_near(centre_of(tile), 1.0);
            let Some(path) = find_path(&map, store_tile, tile_of(draw)) else { continue };
            caravan.route = path.iter().map(|t| centre_of(*t)).collect();
            caravan.pos = caravan.route[0];
            caravan.prev = caravan.route[0];
            caravan.source = Some(tile);
            if caravan.route.len() < 2 {
                caravan.state = CaravanState::Loading;
                caravan.timer = LOAD_SECS;
            } else {
                caravan.state = CaravanState::ToSource;
                caravan.wp = 1;
            }
            serviced.insert(tile);
            break;
        }
    }
}

/// Move each caravan along its route, switching legs at the ends (DESIGN §13).
fn drive_caravans(
    time: Res<Time>,
    mut caravans: Query<&mut Caravan>,
    mut stores: Query<&mut WaterStore>,
    mut outbox: MessageWriter<OutgoingEvent>,
) {
    let dt = time.delta_secs();
    let max_step = CARAVAN_SPEED * dt;
    for mut c in &mut caravans {
        c.prev = c.pos; // snapshot for client interpolation
        match c.state {
            CaravanState::Idle => {}
            CaravanState::ToSource => {
                if advance(&mut c, true, max_step) {
                    c.state = CaravanState::Loading;
                    c.timer = LOAD_SECS;
                }
            }
            CaravanState::Loading => {
                c.timer -= dt;
                if c.timer <= 0.0 {
                    c.cargo = c.capacity;
                    c.state = CaravanState::ToStore;
                    c.wp = c.route.len().saturating_sub(2);
                }
            }
            CaravanState::ToStore => {
                if advance(&mut c, false, max_step) {
                    c.state = CaravanState::Unloading;
                    c.timer = UNLOAD_SECS;
                }
            }
            CaravanState::Unloading => {
                c.timer -= dt;
                if c.timer <= 0.0 {
                    let amount = c.cargo;
                    c.cargo = 0;
                    if let Ok(mut store) = stores.single_mut() {
                        store.stored += amount;
                        outbox.write(OutgoingEvent(SimEvent::WaterDelivered {
                            amount,
                            stored: store.stored,
                        }));
                    }
                    c.state = CaravanState::ToSource;
                    c.wp = 1;
                }
            }
        }
    }
}

/// Step the caravan toward `route[wp]`, advancing the index when reached. Returns
/// true when the route end (in this direction) is hit. `forward` = toward source.
fn advance(c: &mut Caravan, forward: bool, max_step: f32) -> bool {
    if c.route.len() < 2 {
        return true;
    }
    let target = c.route[c.wp];
    let to = target - c.pos;
    let dist = to.length();
    if dist > max_step {
        c.pos += to / dist * max_step;
        return false;
    }
    c.pos = target;
    if forward {
        if c.wp + 1 >= c.route.len() {
            return true;
        }
        c.wp += 1;
    } else {
        if c.wp == 0 {
            return true;
        }
        c.wp -= 1;
    }
    false
}

fn tile_of(p: Vec2) -> IVec2 {
    IVec2::new(p.x.floor() as i32, p.y.floor() as i32)
}

fn centre_of(t: IVec2) -> Vec2 {
    Vec2::new(t.x as f32 + 0.5, t.y as f32 + 0.5)
}

/// Caravan assignment + route-following.
pub struct CaravanPlugin;

impl Plugin for CaravanPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (assign_caravans, drive_caravans).chain());
    }
}
