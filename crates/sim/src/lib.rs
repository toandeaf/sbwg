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
use protocol::{PlayerCommand, SimEvent, Terrain};

/// Simulation ticks per second (DESIGN §17.3: ~5–20 Hz strategic).
pub const SIM_HZ: f64 = 10.0;

/// Logical size of the scaffold map, in tiles (DESIGN §6.2).
pub const MAP_W: i32 = 32;
pub const MAP_H: i32 = 24;

/// Starting population of the scaffold settlement. The client renders this many
/// cosmetic swarm figures (DESIGN §17.4): the sim tracks the *number*, the
/// client draws the *crowd*.
pub const SETTLEMENT_POP: u32 = 60;

/// Half-width (in tiles) of a building's blocking footprint.
const BUILDING_FOOTPRINT_R: i32 = 1;

/// How far the lone "leader" — a real, authoritative, interpolated mover (the
/// other half of §17.4) — wanders from home.
const LEADER_RADIUS: f32 = 4.0;

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

/// The authoritative tile map: extents, per-tile terrain, and a passability
/// grid (DESIGN §6.2). `blocked` is the collision layer — buildings and water —
/// shared by real movers (here) and the client's cosmetic swarm.
#[derive(Resource, Debug)]
pub struct Map {
    pub width: i32,
    pub height: i32,
    pub tiles: Vec<Terrain>,
    pub blocked: Vec<bool>,
}

impl Map {
    /// Deterministic terrain: mostly sand with a scatter of oases and wells.
    /// Water starts blocked; building footprints get stamped in later.
    fn generate(width: i32, height: i32, seed: u64) -> Self {
        let mut rng = SimRng(seed);
        let mut tiles = vec![Terrain::Sand; (width * height) as usize];
        for (kind, count) in [(Terrain::Oasis, 3usize), (Terrain::Well, 8usize)] {
            for _ in 0..count {
                let x = (rng.next_u64() % width as u64) as i32;
                let y = (rng.next_u64() % height as u64) as i32;
                let idx = (y * width + x) as usize;
                if tiles[idx] == Terrain::Sand {
                    tiles[idx] = kind;
                }
            }
        }
        let blocked = tiles.iter().map(|t| t.is_water()).collect();
        Self { width, height, tiles, blocked }
    }

    pub fn terrain_at(&self, x: i32, y: i32) -> Terrain {
        self.tiles[self.idx(x, y)]
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.width + x) as usize
    }

    /// True if a unit may stand on this tile (in-bounds and not blocked).
    pub fn is_walkable(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y) && !self.blocked[self.idx(x, y)]
    }

    /// Walkability test for a continuous tile-space position.
    pub fn walkable_at(&self, p: Vec2) -> bool {
        self.is_walkable(p.x.floor() as i32, p.y.floor() as i32)
    }

    /// Spiral out from `centre` for the first walkable tile centre at least
    /// `min_dist` tiles away. Used to place things outside obstacles.
    pub fn find_walkable_near(&self, centre: Vec2, min_dist: f32) -> Vec2 {
        let start = min_dist.max(0.0) as i32;
        for ring in start..(start + 10) {
            for k in 0..24 {
                let a = k as f32 / 24.0 * std::f32::consts::TAU;
                let p = centre + Vec2::new(a.cos(), a.sin()) * ring as f32;
                if self.walkable_at(p) {
                    return p;
                }
            }
        }
        centre
    }
}

impl Default for Map {
    fn default() -> Self {
        Self::generate(MAP_W, MAP_H, 0x00C0_FFEE_1234_5678)
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

/// A place where people live. Its `population` is the cohort the client draws as
/// a cosmetic swarm (DESIGN §14/§17.4).
#[derive(Component, Debug)]
pub struct Settlement {
    /// Continuous tile-space position (DESIGN §6.2).
    pub pos: Vec2,
    pub population: u32,
}

/// A real, authoritative agent the client draws as a single interpolated square
/// (stands in for a leader/caravan — DESIGN §17.4). `prev` holds last tick's
/// position so the client can interpolate between 10 Hz steps.
#[derive(Component, Debug, Clone, Copy)]
pub struct Mover {
    pub home: Vec2,
    pub prev: Vec2,
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
            .add_systems(Startup, spawn_settlement)
            // After settlements exist: stamp their footprints into the collision
            // grid, then place the leader on a walkable tile outside them.
            .add_systems(PostStartup, (stamp_obstacles, spawn_leader).chain())
            // One ordered chain per tick: ingest intent, step the world, count.
            .add_systems(
                FixedUpdate,
                (apply_commands, wander_leader, advance_tick).chain(),
            );
    }
}

fn spawn_settlement(mut commands: Commands, map: Res<Map>) {
    let pos = Vec2::new(map.width as f32 / 2.0, map.height as f32 / 2.0);
    commands.spawn(Settlement { pos, population: SETTLEMENT_POP });
}

/// Bake building footprints into the map's collision grid.
fn stamp_obstacles(mut map: ResMut<Map>, settlements: Query<&Settlement>) {
    for settlement in &settlements {
        let cx = settlement.pos.x.floor() as i32;
        let cy = settlement.pos.y.floor() as i32;
        for dy in -BUILDING_FOOTPRINT_R..=BUILDING_FOOTPRINT_R {
            for dx in -BUILDING_FOOTPRINT_R..=BUILDING_FOOTPRINT_R {
                let (x, y) = (cx + dx, cy + dy);
                if map.in_bounds(x, y) {
                    let idx = map.idx(x, y);
                    map.blocked[idx] = true;
                }
            }
        }
    }
}

fn spawn_leader(mut commands: Commands, map: Res<Map>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    // Start a few tiles from the settlement, on a walkable tile.
    let home = map.find_walkable_near(settlement.pos, 3.0);
    commands.spawn(Mover { home, prev: home, pos: home, vel: Vec2::ZERO });
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

/// Random-walk the leader, sliding along blocked tiles and staying near home.
fn wander_leader(map: Res<Map>, mut rng: ResMut<SimRng>, mut movers: Query<&mut Mover>) {
    for mut m in &mut movers {
        m.prev = m.pos; // snapshot for client interpolation
        m.vel.x += rng.signed_unit() * 0.06;
        m.vel.y += rng.signed_unit() * 0.06;
        m.vel = m.vel.clamp_length_max(0.5);

        // Axis-separated move so we slide along walls instead of sticking.
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

        // Keep within home radius.
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
    fn settlement_and_leader_spawn() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);

        let mut settlements = app.world_mut().query::<&Settlement>();
        assert_eq!(settlements.iter(app.world()).count(), 1);

        let mut leaders = app.world_mut().query::<&Mover>();
        assert_eq!(leaders.iter(app.world()).count(), 1);
    }

    #[test]
    fn map_has_water() {
        let map = Map::default();
        assert!(map.tiles.iter().any(|t| t.is_water()));
    }

    #[test]
    fn buildings_block_tiles() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);
        let map = app.world().resource::<Map>();
        // The settlement sits at map centre; its footprint must be impassable.
        assert!(!map.is_walkable(MAP_W / 2, MAP_H / 2));
    }

    #[test]
    fn leader_starts_walkable() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);
        let mut leaders = app.world_mut().query::<&Mover>();
        let world = app.world();
        let map = world.resource::<Map>();
        for mover in leaders.iter(world) {
            assert!(map.walkable_at(mover.pos), "leader spawned on a blocked tile");
        }
    }
}
