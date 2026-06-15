//! Entity components and their per-tick behaviour (DESIGN §17.4 real movers).
//!
//! The settlement and buildings are placed by `setup`; here we define the shared
//! components and the FixedUpdate systems that step the world each tick.

use bevy::prelude::*;
use protocol::{PlayerCommand, PlayerId, SimEvent};

use crate::map::{Map, SimRng, SimTick};
use crate::messages::{IncomingCommand, OutgoingEvent};

/// How far the lone "leader" — a real, authoritative, interpolated mover (the
/// other half of §17.4) — wanders from home.
const LEADER_RADIUS: f32 = 4.0;
const LEADER_MAX_SPEED: f32 = 0.12; // tiles/tick (~1.2 tiles/sec at 10 Hz)
const LEADER_ACCEL: f32 = 0.02; // random nudge per tick
const LEADER_DAMPING: f32 = 0.9; // velocity smoothing (less twitchy)

// Caravan behaviour tuning (DESIGN §13: water logistics).
const CARAVAN_SPEED: f32 = 2.0; // tiles/second
const LOAD_SECS: f32 = 1.5;
const UNLOAD_SECS: f32 = 1.5;
const ARRIVE_EPS: f32 = 0.15; // tiles

// ---- Components -------------------------------------------------------------

/// A place where people live. Its `population` is the cohort the client draws as
/// a cosmetic swarm (DESIGN §14/§17.4); `water_stored` is filled by caravans.
#[derive(Component, Debug)]
pub struct Settlement {
    pub owner: PlayerId,
    /// Continuous tile-space position (DESIGN §6.2).
    pub pos: Vec2,
    pub population: u32,
    pub water_stored: u32,
}

/// A physical structure occupying a tile rectangle. Buildings block movement
/// and together form a settlement's footprint (DESIGN §6.2).
#[derive(Component, Debug)]
pub struct Building {
    pub owner: PlayerId,
    /// Lower-left tile of the footprint.
    pub tile: IVec2,
    /// Footprint size in tiles.
    pub size: IVec2,
}

impl Building {
    /// Continuous tile-space centre of the footprint.
    pub fn center(&self) -> Vec2 {
        Vec2::new(
            self.tile.x as f32 + self.size.x as f32 / 2.0,
            self.tile.y as f32 + self.size.y as f32 / 2.0,
        )
    }
}

/// A real, authoritative agent the client draws as a single interpolated square
/// (stands in for a leader — DESIGN §17.4). `prev` holds last tick's position so
/// the client can interpolate between 10 Hz steps.
#[derive(Component, Debug, Clone, Copy)]
pub struct Mover {
    pub home: Vec2,
    pub prev: Vec2,
    pub pos: Vec2,
    pub vel: Vec2,
}

/// What a caravan is doing right now (deterministic, fixed behaviour — unlike
/// the cosmetic swarm).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaravanState {
    ToSource,
    Loading,
    ToDest,
    Unloading,
}

/// A real, authoritative goods-hauler: a body of camels + people that shuttles
/// water from a source to a settlement (DESIGN §13). Interpolated like a leader.
#[derive(Component, Debug)]
pub struct Caravan {
    pub prev: Vec2,
    pub pos: Vec2,
    pub camels: u32,
    pub people: u32,
    pub capacity: u32,
    pub cargo: u32,
    /// Walkable draw-point beside a water tile.
    pub source: Vec2,
    /// Walkable drop-point beside the settlement.
    pub dest: Vec2,
    pub state: CaravanState,
    /// Seconds remaining for the current load/unload.
    pub timer: f32,
}

// ---- Systems ---------------------------------------------------------------

/// Drain inbound commands, mutate state, and publish a state-change event.
fn apply_commands(
    mut inbox: MessageReader<IncomingCommand>,
    mut outbox: MessageWriter<OutgoingEvent>,
) {
    for IncomingCommand(cmd) in inbox.read() {
        match cmd {
            PlayerCommand::SetFocus { player, at } => {
                outbox.write(OutgoingEvent(SimEvent::FocusChanged {
                    player: *player,
                    at: *at,
                }));
            }
        }
    }
}

/// Random-walk the leader, sliding along blocked tiles and staying near home.
fn wander_leader(map: Res<Map>, mut rng: ResMut<SimRng>, mut movers: Query<&mut Mover>) {
    for mut m in &mut movers {
        m.prev = m.pos; // snapshot for client interpolation
        m.vel *= LEADER_DAMPING; // smooth out erratic direction changes
        m.vel.x += rng.signed_unit() * LEADER_ACCEL;
        m.vel.y += rng.signed_unit() * LEADER_ACCEL;
        m.vel = m.vel.clamp_length_max(LEADER_MAX_SPEED);

        let mut np = m.pos;
        let try_x = Vec2::new(m.pos.x + m.vel.x, m.pos.y);
        if map.walkable_at(try_x) {
            np.x = try_x.x;
        } else {
            m.vel.x = -m.vel.x;
        }
        let try_y = Vec2::new(np.x, m.pos.y + m.vel.y);
        if map.walkable_at(try_y) {
            np.y = try_y.y;
        } else {
            m.vel.y = -m.vel.y;
        }

        let off = np - m.home;
        if off.length() > LEADER_RADIUS {
            let n = off.normalize_or_zero();
            np = m.home + n * LEADER_RADIUS;
            let v = m.vel;
            m.vel = v - 2.0 * v.dot(n) * n;
        }
        m.pos = np;
    }
}

/// Step a point toward a target with axis-separated tile collision (wall-slide).
/// Returns the new position and whether it has effectively arrived.
fn step_toward(map: &Map, pos: Vec2, target: Vec2, max_step: f32) -> (Vec2, bool) {
    let to = target - pos;
    let dist = to.length();
    if dist <= ARRIVE_EPS {
        return (pos, true);
    }
    let step = to / dist * max_step.min(dist);
    let mut np = pos;
    let try_x = Vec2::new(pos.x + step.x, pos.y);
    if map.walkable_at(try_x) {
        np.x = try_x.x;
    }
    let try_y = Vec2::new(np.x, pos.y + step.y);
    if map.walkable_at(try_y) {
        np.y = try_y.y;
    }
    (np, (np - target).length() <= ARRIVE_EPS)
}

/// Run each caravan's fixed haul cycle: to water, load, to settlement, unload.
fn drive_caravans(
    time: Res<Time>,
    map: Res<Map>,
    mut caravans: Query<&mut Caravan>,
    mut settlements: Query<&mut Settlement>,
    mut outbox: MessageWriter<OutgoingEvent>,
) {
    let dt = time.delta_secs();
    let max_step = CARAVAN_SPEED * dt;
    for mut c in &mut caravans {
        c.prev = c.pos; // snapshot for client interpolation
        match c.state {
            CaravanState::ToSource => {
                let (np, arrived) = step_toward(&map, c.pos, c.source, max_step);
                c.pos = np;
                if arrived {
                    c.state = CaravanState::Loading;
                    c.timer = LOAD_SECS;
                }
            }
            CaravanState::Loading => {
                c.timer -= dt;
                if c.timer <= 0.0 {
                    c.cargo = c.capacity;
                    c.state = CaravanState::ToDest;
                }
            }
            CaravanState::ToDest => {
                let (np, arrived) = step_toward(&map, c.pos, c.dest, max_step);
                c.pos = np;
                if arrived {
                    c.state = CaravanState::Unloading;
                    c.timer = UNLOAD_SECS;
                }
            }
            CaravanState::Unloading => {
                c.timer -= dt;
                if c.timer <= 0.0 {
                    let amount = c.cargo;
                    c.cargo = 0;
                    if let Ok(mut settlement) = settlements.single_mut() {
                        settlement.water_stored += amount;
                        outbox.write(OutgoingEvent(SimEvent::WaterDelivered {
                            amount,
                            stored: settlement.water_stored,
                        }));
                    }
                    c.state = CaravanState::ToSource;
                }
            }
        }
    }
}

fn advance_tick(mut tick: ResMut<SimTick>, mut outbox: MessageWriter<OutgoingEvent>) {
    tick.0 += 1;
    outbox.write(OutgoingEvent(SimEvent::Ticked { tick: tick.0 }));
}

/// Registers the boundary messages and the per-tick behaviour systems.
pub struct EntityPlugin;

impl Plugin for EntityPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<IncomingCommand>()
            .add_message::<OutgoingEvent>()
            // One ordered chain per tick.
            .add_systems(
                FixedUpdate,
                (apply_commands, wander_leader, drive_caravans, advance_tick).chain(),
            );
    }
}
