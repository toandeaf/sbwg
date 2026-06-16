//! Entity components and their per-tick behaviour (DESIGN §17.4 real movers).
//!
//! The settlement and buildings are placed by `setup`; here we define the shared
//! components and the FixedUpdate systems that step the world each tick. Caravan
//! logistics live in their own module (`caravan`).

use bevy::prelude::*;
use protocol::{PlayerCommand, PlayerId, SimEvent};

use crate::manpower;
use crate::map::{Map, SimRng, SimTick, Territory};
use crate::messages::{IncomingCommand, OutgoingEvent};

/// How far the lone "leader" — a real, authoritative, interpolated mover (the
/// other half of §17.4) — wanders from home.
const LEADER_RADIUS: f32 = 4.0;
const LEADER_MAX_SPEED: f32 = 0.12; // tiles/tick (~1.2 tiles/sec at 10 Hz)
const LEADER_ACCEL: f32 = 0.02; // random nudge per tick
const LEADER_DAMPING: f32 = 0.9; // velocity smoothing (less twitchy)

// ---- Components -------------------------------------------------------------

/// A place where people live. Its `population` is the cohort the client draws as
/// a cosmetic swarm (DESIGN §14/§17.4).
#[derive(Component, Debug)]
pub struct Settlement {
    pub owner: PlayerId,
    /// Continuous tile-space position (DESIGN §6.2).
    pub pos: Vec2,
    pub population: u32,
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

// ---- Systems ---------------------------------------------------------------

/// Drain inbound commands (player directives), mutate state, and publish events.
fn apply_commands(
    mut inbox: MessageReader<IncomingCommand>,
    mut outbox: MessageWriter<OutgoingEvent>,
    mut territory: ResMut<Territory>,
    settlements: Query<&Settlement>,
) {
    for IncomingCommand(cmd) in inbox.read() {
        match cmd {
            PlayerCommand::SetFocus { player, at } => {
                outbox.write(OutgoingEvent(SimEvent::FocusChanged {
                    player: *player,
                    at: *at,
                }));
            }
            PlayerCommand::ClaimArea { player, min, max } => {
                let Some(settlement) = settlements.iter().find(|s| s.owner == *player) else {
                    continue;
                };
                let (min, max) = (IVec2::new(min.x, min.y), IVec2::new(max.x, max.y));
                // Only claim what we have the manpower to hold (DESIGN §8).
                if manpower::can_claim(
                    &territory,
                    *player,
                    settlement.pos,
                    settlement.population,
                    min,
                    max,
                ) {
                    territory.claim_rect(*player, min, max);
                }
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
            .add_systems(
                FixedUpdate,
                (apply_commands, wander_leader, advance_tick).chain(),
            );
    }
}
