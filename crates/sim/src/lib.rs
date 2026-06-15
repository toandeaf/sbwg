//! Headless authoritative simulation (DESIGN §17.2).
//!
//! Runs as a Bevy plugin so it can be embedded in the client (single-player,
//! in-process) today and hosted in a headless server app tomorrow without code
//! changes. Nothing in here renders or knows the client exists.
//!
//! **Decoupling (DESIGN: lean on Bevy's messages):** the outside world feeds in
//! [`IncomingCommand`]s and the sim publishes [`OutgoingEvent`]s. Systems never
//! call each other directly — they read/write messages and shared components.
//! That message seam is exactly where the network layer will later plug in.

use bevy::prelude::*;
use protocol::{PlayerCommand, SimEvent};

/// Simulation ticks per second (DESIGN §17.3: ~5–20 Hz strategic).
pub const SIM_HZ: f64 = 10.0;

/// Logical size of the scaffold map, in tiles (DESIGN §6.2).
pub const MAP_W: i32 = 32;
pub const MAP_H: i32 = 24;

/// Placeholder "movers" the scaffold wanders around — proto-ants / real movers
/// (DESIGN §17.4). Stand in for leaders/caravans/parties for now.
const MOVER_COUNT: usize = 24;

// ---- Boundary messages -----------------------------------------------------

/// Outside world → sim. Wraps a wire [`PlayerCommand`]; the seam where the
/// network layer will later deserialise into the sim.
#[derive(Message, Debug, Clone)]
pub struct IncomingCommand(pub PlayerCommand);

/// Sim → outside world. Wraps a wire [`SimEvent`]; the seam where the network
/// layer will later serialise out to clients.
#[derive(Message, Debug, Clone)]
pub struct OutgoingEvent(pub SimEvent);

// ---- Resources -------------------------------------------------------------

/// Monotonic tick counter for the authoritative sim.
#[derive(Resource, Default, Debug)]
pub struct SimTick(pub u64);

/// The tile grid extents.
#[derive(Resource, Debug)]
pub struct Map {
    pub width: i32,
    pub height: i32,
}

impl Default for Map {
    fn default() -> Self {
        Self { width: MAP_W, height: MAP_H }
    }
}

/// Deterministic RNG so the sim stays reproducible for tests/replay. We are
/// server-authoritative rather than lockstep (DESIGN §17.3), but cheap
/// reproducibility is still worth keeping.
#[derive(Resource, Debug)]
pub struct SimRng(u64);

impl Default for SimRng {
    fn default() -> Self {
        Self(0x9E37_79B9_7F4A_7C15)
    }
}

impl SimRng {
    /// xorshift64* — tiny, dependency-free.
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-ish float in `[-1.0, 1.0)`.
    fn signed_unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    }
}

// ---- Components -------------------------------------------------------------

/// A simulated agent the client draws as a coloured square. Position is the
/// sim's truth, in continuous tile-space (not cell-snapped — DESIGN §6.2).
#[derive(Component, Debug, Clone, Copy)]
pub struct Mover {
    pub pos: Vec2,
    pub vel: Vec2,
}

// ---- Plugin ----------------------------------------------------------------

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimTick>()
            .init_resource::<Map>()
            .init_resource::<SimRng>()
            .add_message::<IncomingCommand>()
            .add_message::<OutgoingEvent>()
            .add_systems(Startup, spawn_movers)
            // One ordered chain per tick: ingest intent, step the world, count.
            .add_systems(
                FixedUpdate,
                (apply_commands, wander_movers, advance_tick).chain(),
            );
    }
}

fn spawn_movers(mut commands: Commands, map: Res<Map>, mut rng: ResMut<SimRng>) {
    for _ in 0..MOVER_COUNT {
        let pos = Vec2::new(
            (rng.signed_unit() * 0.5 + 0.5) * map.width as f32,
            (rng.signed_unit() * 0.5 + 0.5) * map.height as f32,
        );
        commands.spawn(Mover { pos, vel: Vec2::ZERO });
    }
}

/// Drain inbound commands, mutate state, and publish a state-change event.
fn apply_commands(
    mut inbox: MessageReader<IncomingCommand>,
    mut outbox: MessageWriter<OutgoingEvent>,
) {
    for IncomingCommand(cmd) in inbox.read() {
        match cmd {
            PlayerCommand::SetFocus { player, at } => {
                // Placeholder behaviour: acknowledge via an event for now.
                outbox.write(OutgoingEvent(SimEvent::FocusChanged {
                    player: *player,
                    at: *at,
                }));
            }
        }
    }
}

/// Random-walk the movers within the map bounds (placeholder motion).
fn wander_movers(map: Res<Map>, mut rng: ResMut<SimRng>, mut movers: Query<&mut Mover>) {
    let (w, h) = (map.width as f32, map.height as f32);
    for mut m in &mut movers {
        m.vel.x += rng.signed_unit() * 0.05;
        m.vel.y += rng.signed_unit() * 0.05;
        m.vel = m.vel.clamp_length_max(0.3);
        let mut p = m.pos + m.vel;
        if p.x < 0.0 || p.x > w {
            m.vel.x = -m.vel.x;
            p.x = p.x.clamp(0.0, w);
        }
        if p.y < 0.0 || p.y > h {
            m.vel.y = -m.vel.y;
            p.y = p.y.clamp(0.0, h);
        }
        m.pos = p;
    }
}

fn advance_tick(mut tick: ResMut<SimTick>, mut outbox: MessageWriter<OutgoingEvent>) {
    tick.0 += 1;
    outbox.write(OutgoingEvent(SimEvent::Ticked { tick: tick.0 }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sim must step without any rendering or real-time plugins.
    #[test]
    fn sim_advances_ticks_headlessly() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        for _ in 0..5 {
            app.world_mut().run_schedule(FixedUpdate);
        }
        assert_eq!(app.world().resource::<SimTick>().0, 5);
    }

    #[test]
    fn movers_spawn() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        let mut q = app.world_mut().query::<&Mover>();
        assert_eq!(q.iter(app.world()).count(), MOVER_COUNT);
    }
}
