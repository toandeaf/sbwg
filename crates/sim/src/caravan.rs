//! Caravan logistics (DESIGN §13/§9). A caravan hauls a commodity between its
//! **home** (the town hub) and a **target**, branching by [`CaravanJob`]:
//!
//! * **Water** — empty out to a well (target), load, haul back, deposit to the store.
//! * **Trade** — load goods at home, haul out to a market (target), sell for wealth.
//!
//! One state machine serves both; only the load/unload effects differ. Assignment
//! is event-driven (territory change or caravan added): it covers claimed water
//! sources first, then sends leftover caravans to markets — so claiming more water
//! costs you trade capacity (opportunity cost).

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
/// Wealth earned per unit of goods sold at a market.
const GOODS_PRICE: u32 = 2;

/// Water storage held by a designated central building (DESIGN §13).
#[derive(Component, Debug)]
pub struct WaterStore {
    /// Walkable drop-point beside the storage building (the town's hub).
    pub pos: Vec2,
    pub stored: u32,
}

/// A market a trade caravan can sell goods at (DESIGN §9). A walkable spot.
#[derive(Component, Debug)]
pub struct Market {
    pub pos: Vec2,
}

/// What a caravan hauls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaravanJob {
    Water,
    Trade,
}

/// Set when a caravan is added, so the planner re-runs. Territory changes are
/// picked up directly via change-detection in [`plan_caravans`].
#[derive(Resource, Default)]
struct ReplanCaravans(bool);

/// Where a caravan is in its haul cycle. (Names predate the job split: "Source"
/// = the target end, "Store" = home; the *effect* at each end depends on job.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaravanState {
    Idle,
    ToSource,
    Loading,
    ToStore,
    Unloading,
}

/// A real, authoritative goods-hauler. Interpolated by the client between ticks.
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
    pub job: CaravanJob,
    /// Town hub: store drop-point for water, goods pickup for trade.
    pub home: Vec2,
    /// Water draw-point (Water job) or market (Trade job).
    pub target: Vec2,
    /// Current leg's A* waypoints.
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
            job: CaravanJob::Water,
            home: pos,
            target: pos,
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

/// Assign caravans: cover claimed water sources first, then send the rest to
/// markets (DESIGN §13/§9). Re-runs on territory change or a caravan being added.
fn plan_caravans(
    map: Res<Map>,
    territory: Res<Territory>,
    settlements: Query<&Settlement>,
    stores: Query<&WaterStore>,
    markets: Query<&Market>,
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
    let home = store.pos;
    let store_tile = tile_of(home);
    let reachable = |draw: Vec2| find_path(&map, store_tile, tile_of(draw)).is_some();

    // Claimed, reachable water draw-points (nearest first).
    let mut water: Vec<Vec2> = map
        .water_tiles()
        .into_iter()
        .filter(|w| territory.owner_at(w.x.floor() as i32, w.y.floor() as i32) == Some(owner))
        .map(|w| map.find_walkable_near(w, 1.0))
        .filter(|d| reachable(*d))
        .collect();
    water.sort_by(|a, b| a.distance_squared(home).partial_cmp(&b.distance_squared(home)).unwrap());

    // Reachable markets (nearest first).
    let mut trade: Vec<Vec2> = markets.iter().map(|m| m.pos).filter(|d| reachable(*d)).collect();
    trade.sort_by(|a, b| a.distance_squared(home).partial_cmp(&b.distance_squared(home)).unwrap());

    // Priority: water first, then trade — so water is covered before trade.
    let mut priority: Vec<(CaravanJob, Vec2)> = Vec::new();
    priority.extend(water.into_iter().map(|d| (CaravanJob::Water, d)));
    priority.extend(trade.into_iter().map(|d| (CaravanJob::Trade, d)));

    let cars: Vec<Entity> = caravans.iter().map(|(e, _)| e).collect();
    let mut plan: HashMap<Entity, (CaravanJob, Vec2)> = HashMap::new();
    for (i, entity) in cars.iter().enumerate() {
        if let Some(&job_target) = priority.get(i) {
            plan.insert(*entity, job_target);
        }
    }

    for (entity, mut caravan) in &mut caravans {
        caravan.home = home;
        caravan.cargo = 0;
        match plan.remove(&entity) {
            Some((job, target)) => {
                caravan.job = job;
                caravan.target = target;
                // Return to the hub first, then start the cycle (handles reassignment
                // from anywhere in the field).
                begin_leg(&mut caravan, &map, home);
                caravan.state = CaravanState::ToStore;
            }
            None => {
                caravan.state = CaravanState::Idle;
                caravan.route.clear();
                caravan.wp = 0;
            }
        }
    }
}

/// Move each caravan along its route, branching the load/unload effects by job.
fn drive_caravans(
    time: Res<Time>,
    map: Res<Map>,
    mut caravans: Query<&mut Caravan>,
    mut stores: Query<&mut WaterStore>,
    mut settlements: Query<&mut Settlement>,
    mut outbox: MessageWriter<OutgoingEvent>,
) {
    let dt = time.delta_secs();
    let max_step = CARAVAN_SPEED * dt;
    for mut c in &mut caravans {
        c.prev = c.pos;
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
                    match c.job {
                        CaravanJob::Water => c.cargo = c.capacity, // draw from the well
                        CaravanJob::Trade => {
                            // Sell goods at the market.
                            let amount = c.cargo;
                            c.cargo = 0;
                            if amount > 0 {
                                if let Ok(mut s) = settlements.single_mut() {
                                    let earned = amount * GOODS_PRICE;
                                    s.treasury += earned;
                                    outbox.write(OutgoingEvent(SimEvent::GoodsSold { amount, earned }));
                                }
                            }
                        }
                    }
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
                    match c.job {
                        CaravanJob::Water => {
                            let amount = c.cargo;
                            c.cargo = 0;
                            if let Ok(mut store) = stores.single_mut() {
                                store.stored += amount;
                                outbox.write(OutgoingEvent(SimEvent::WaterDelivered {
                                    amount,
                                    stored: store.stored,
                                }));
                            }
                        }
                        CaravanJob::Trade => {
                            // Load goods to carry to market.
                            if let Ok(mut s) = settlements.single_mut() {
                                let load = c.capacity.min(s.goods);
                                s.goods -= load;
                                c.cargo = load;
                            }
                        }
                    }
                    let target = c.target;
                    begin_leg(&mut c, &map, target);
                    c.state = CaravanState::ToSource;
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
