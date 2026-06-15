//! Caravan logistics — the first "action": haul water from claimed sources to
//! the central store along A* routes.
//!
//! Assignment is **event-driven**: caravans re-plan whenever territory changes
//! or a caravan is added. The planner gathers every claimed, reachable water
//! source and partitions them across the caravans — one nearest-neighbour tour
//! split into balanced, spatially-coherent slices — so all sources get covered
//! (caravan A serves one slice, caravan B the next, …). Each caravan then cycles
//! its slice, hauling each load back to the store.

use bevy::prelude::*;
use protocol::SimEvent;
use std::collections::HashMap;

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
    /// Walkable drop-point beside the storage building (where caravans unload).
    pub pos: Vec2,
    pub stored: u32,
}

/// Set when a caravan is added, so the planner re-runs. Territory changes are
/// picked up directly via change-detection in [`plan_caravans`].
#[derive(Resource, Default)]
struct ReplanCaravans(bool);

/// What a caravan is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaravanState {
    Idle,
    ToSource,
    Loading,
    ToStore,
    Unloading,
}

/// A real, authoritative goods-hauler. It cycles through an assigned `tour` of
/// water draw-points, hauling each load back to `home` (the store). Position is
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
    /// Store drop-point this caravan returns to.
    pub home: Vec2,
    /// Assigned water draw-points, in visit order.
    pub tour: Vec<Vec2>,
    /// Index of the source currently being serviced within `tour`.
    pub tour_index: usize,
    /// Current leg's A* waypoints (from where the caravan is to its target).
    pub route: Vec<Vec2>,
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
            home: pos,
            tour: Vec::new(),
            tour_index: 0,
            route: Vec::new(),
            wp: 0,
        }
    }
}

/// Trip the re-plan when a caravan is added.
fn flag_caravan_added(added: Query<(), Added<Caravan>>, mut flag: ResMut<ReplanCaravans>) {
    if !added.is_empty() {
        flag.0 = true;
    }
}

/// Re-plan all caravans when territory changes or one was added (DESIGN §13).
fn plan_caravans(
    map: Res<Map>,
    territory: Res<Territory>,
    settlements: Query<&Settlement>,
    stores: Query<&WaterStore>,
    mut flag: ResMut<ReplanCaravans>,
    mut caravans: Query<(Entity, &mut Caravan)>,
) {
    if !territory.is_changed() && !flag.0 {
        return;
    }
    flag.0 = false;
    let Ok(settlement) = settlements.single() else { return };
    let Ok(store) = stores.single() else { return };
    let owner = settlement.owner;
    let store_pos = store.pos;
    let store_tile = tile_of(store_pos);

    // Claimed, reachable water sources → their walkable draw-points.
    let mut draws: Vec<Vec2> = Vec::new();
    for water in map.water_tiles() {
        let tile = tile_of(water);
        if territory.owner_at(tile.x, tile.y) != Some(owner) {
            continue;
        }
        let draw = map.find_walkable_near(water, 1.0);
        if find_path(&map, store_tile, tile_of(draw)).is_some() {
            draws.push(draw);
        }
    }

    // Partition the sources across the caravans, then hand each its slice.
    let cars: Vec<Entity> = caravans.iter().map(|(e, _)| e).collect();
    let plan = partition(&draws, store_pos, cars.len());
    let mut tours: HashMap<Entity, Vec<Vec2>> = HashMap::new();
    for (ci, entity) in cars.iter().enumerate() {
        tours.insert(*entity, plan[ci].iter().map(|&i| draws[i]).collect());
    }

    for (entity, mut caravan) in &mut caravans {
        let tour = tours.remove(&entity).unwrap_or_default();
        caravan.home = store_pos;
        caravan.cargo = 0;
        caravan.tour = tour;
        caravan.tour_index = 0;
        if caravan.tour.is_empty() {
            caravan.state = CaravanState::Idle;
            caravan.route.clear();
            caravan.wp = 0;
        } else {
            let goal = caravan.tour[0];
            begin_leg(&mut caravan, &map, goal);
            caravan.state = CaravanState::ToSource;
        }
    }
}

/// Order the draws into one nearest-neighbour tour from the store, then split it
/// into `n` balanced, contiguous slices — one per caravan.
fn partition(draws: &[Vec2], store: Vec2, n: usize) -> Vec<Vec<usize>> {
    let mut chunks = vec![Vec::new(); n];
    if n == 0 || draws.is_empty() {
        return chunks;
    }
    let mut remaining: Vec<usize> = (0..draws.len()).collect();
    let mut order = Vec::with_capacity(draws.len());
    let mut cur = store;
    while !remaining.is_empty() {
        let k = (0..remaining.len())
            .min_by(|&a, &b| {
                draws[remaining[a]]
                    .distance_squared(cur)
                    .partial_cmp(&draws[remaining[b]].distance_squared(cur))
                    .unwrap()
            })
            .unwrap();
        let si = remaining.swap_remove(k);
        order.push(si);
        cur = draws[si];
    }
    let base = order.len() / n;
    let extra = order.len() % n;
    let mut idx = 0;
    for (ci, chunk) in chunks.iter_mut().enumerate() {
        let size = base + if ci < extra { 1 } else { 0 };
        *chunk = order[idx..idx + size].to_vec();
        idx += size;
    }
    chunks
}

/// Move each caravan along its route, switching legs at the ends (DESIGN §13).
fn drive_caravans(
    time: Res<Time>,
    map: Res<Map>,
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
                if follow_route(&mut c, max_step) {
                    c.state = CaravanState::Loading;
                    c.timer = LOAD_SECS;
                }
            }
            CaravanState::Loading => {
                c.timer -= dt;
                if c.timer <= 0.0 {
                    c.cargo = c.capacity;
                    let home = c.home;
                    begin_leg(&mut c, &map, home);
                    c.state = CaravanState::ToStore;
                }
            }
            CaravanState::ToStore => {
                if follow_route(&mut c, max_step) {
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
                    if c.tour.is_empty() {
                        c.state = CaravanState::Idle;
                    } else {
                        c.tour_index = (c.tour_index + 1) % c.tour.len();
                        let goal = c.tour[c.tour_index];
                        begin_leg(&mut c, &map, goal);
                        c.state = CaravanState::ToSource;
                    }
                }
            }
        }
    }
}

/// Compute a fresh A* leg from the caravan's current position to `goal`.
fn begin_leg(c: &mut Caravan, map: &Map, goal: Vec2) {
    let start = tile_of(c.pos);
    let goal_tile = tile_of(goal);
    let path = find_path(map, start, goal_tile).unwrap_or_else(|| vec![start, goal_tile]);
    c.route = path.iter().map(|t| centre_of(*t)).collect();
    c.wp = if c.route.len() >= 2 { 1 } else { 0 };
}

/// Step the caravan toward `route[wp]`, advancing the index when reached. Returns
/// true when the final waypoint is hit.
fn follow_route(c: &mut Caravan, max_step: f32) -> bool {
    if c.route.is_empty() {
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
    if c.wp + 1 >= c.route.len() {
        return true;
    }
    c.wp += 1;
    false
}

fn tile_of(p: Vec2) -> IVec2 {
    IVec2::new(p.x.floor() as i32, p.y.floor() as i32)
}

fn centre_of(t: IVec2) -> Vec2 {
    Vec2::new(t.x as f32 + 0.5, t.y as f32 + 0.5)
}

/// Caravan planning (event-driven) + route-following.
pub struct CaravanPlugin;

impl Plugin for CaravanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReplanCaravans>()
            .add_systems(
                FixedUpdate,
                (flag_caravan_added, plan_caravans, drive_caravans).chain(),
            );
    }
}
