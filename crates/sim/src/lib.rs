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

/// Town generation (DESIGN §6.2: a dense cluster of ~4–6-tile buildings).
const TOWN_RADIUS: f32 = 7.0;
const BUILDING_TARGET: usize = 13;

/// How far the lone "leader" — a real, authoritative, interpolated mover (the
/// other half of §17.4) — wanders from home.
const LEADER_RADIUS: f32 = 4.0;
const LEADER_MAX_SPEED: f32 = 0.12; // tiles/tick (~1.2 tiles/sec at 10 Hz)
const LEADER_ACCEL: f32 = 0.02; // random nudge per tick
const LEADER_DAMPING: f32 = 0.9; // velocity smoothing (less twitchy)

// Caravan tuning (DESIGN §13: water logistics).
const CARAVAN_COUNT: usize = 3;
const CARAVAN_SPEED: f32 = 2.0; // tiles/second
const CARAVAN_CAPACITY: u32 = 50; // units of water per trip
const CARAVAN_CAMELS: u32 = 6;
const CARAVAN_PEOPLE: u32 = 4;
const LOAD_SECS: f32 = 1.5;
const UNLOAD_SECS: f32 = 1.5;
const ARRIVE_EPS: f32 = 0.15; // tiles

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

    /// Centres of all water tiles (DESIGN §13: water sources).
    pub fn water_tiles(&self) -> Vec<Vec2> {
        let mut out = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.terrain_at(x, y).is_water() {
                    out.push(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                }
            }
        }
        out
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
/// a cosmetic swarm (DESIGN §14/§17.4); `water_stored` is filled by caravans.
#[derive(Component, Debug)]
pub struct Settlement {
    /// Continuous tile-space position (DESIGN §6.2).
    pub pos: Vec2,
    pub population: u32,
    pub water_stored: u32,
}

/// A physical structure occupying a tile rectangle. Buildings block movement
/// and together form a settlement's footprint (DESIGN §6.2).
#[derive(Component, Debug)]
pub struct Building {
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

// ---- Plugin ----------------------------------------------------------------

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimTick>()
            .init_resource::<Map>()
            .init_resource::<SimRng>()
            .add_message::<IncomingCommand>()
            .add_message::<OutgoingEvent>()
            .add_systems(Startup, (spawn_settlement, build_town))
            // After buildings exist: stamp footprints into the collision grid,
            // then place the leader and caravans on walkable tiles outside them.
            .add_systems(
                PostStartup,
                (stamp_obstacles, spawn_leader, spawn_caravans).chain(),
            )
            // One ordered chain per tick.
            .add_systems(
                FixedUpdate,
                (apply_commands, wander_leader, drive_caravans, advance_tick).chain(),
            );
    }
}

fn spawn_settlement(mut commands: Commands, map: Res<Map>) {
    let pos = Vec2::new(map.width as f32 / 2.0, map.height as f32 / 2.0);
    commands.spawn(Settlement { pos, population: SETTLEMENT_POP, water_stored: 0 });
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

/// Lay out a clustered town around the settlement anchor, leaving 1-tile gaps as
/// streets so the crowd can move between buildings (DESIGN §6.2/§17.4).
fn build_town(mut commands: Commands, map: Res<Map>, mut rng: ResMut<SimRng>) {
    let cx = (map.width / 2) as f32;
    let cy = (map.height / 2) as f32;
    let mut placed: Vec<(i32, i32, i32, i32)> = Vec::new();

    // Central great-house, covering the centre tile.
    let centre = IVec2::new(map.width / 2 - 1, map.height / 2 - 1);
    commands.spawn(Building { tile: centre, size: IVec2::new(2, 2) });
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
                commands.spawn(Building { tile: IVec2::new(x, y), size });
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

fn spawn_leader(mut commands: Commands, map: Res<Map>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    let home = map.find_walkable_near(settlement.pos, 3.0);
    commands.spawn(Mover { home, prev: home, pos: home, vel: Vec2::ZERO });
}

/// Spawn caravans, each assigned to a water source, dropping at the settlement.
fn spawn_caravans(mut commands: Commands, map: Res<Map>, settlements: Query<&Settlement>) {
    let Ok(settlement) = settlements.single() else { return };
    let waters = map.water_tiles();
    if waters.is_empty() {
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The sim must step without any rendering or real-time plugins.
    #[test]
    fn sim_advances_ticks_headlessly() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        // drive_caravans reads Res<Time>; a real headless server gets it from
        // MinimalPlugins, but the bare test App needs it inserted manually.
        app.init_resource::<Time>();
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
    fn town_has_buildings() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        let mut buildings = app.world_mut().query::<&Building>();
        assert!(buildings.iter(app.world()).count() >= 10);
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

    #[test]
    fn caravans_spawn_with_walkable_endpoints() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);

        let mut caravans = app.world_mut().query::<&Caravan>();
        let world = app.world();
        let map = world.resource::<Map>();
        let all: Vec<&Caravan> = caravans.iter(world).collect();
        assert_eq!(all.len(), CARAVAN_COUNT);
        for c in all {
            assert!(map.walkable_at(c.source), "caravan source is blocked");
            assert!(map.walkable_at(c.dest), "caravan dest is blocked");
            assert_eq!(c.state, CaravanState::ToSource);
            assert_eq!(c.cargo, 0);
        }
    }
}
