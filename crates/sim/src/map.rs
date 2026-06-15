//! Authoritative world state: the tile map + passability grid, the territory
//! ownership grid, the tick counter, and the deterministic RNG.

use bevy::prelude::*;
use protocol::{PlayerId, Terrain};

/// Logical size of the scaffold map, in tiles (DESIGN §6.2).
pub const MAP_W: i32 = 32;
pub const MAP_H: i32 = 24;

/// Monotonic tick counter for the authoritative sim.
#[derive(Resource, Default, Debug)]
pub struct SimTick(pub u64);

/// The authoritative tile map: extents, per-tile terrain, and a passability
/// grid (DESIGN §6.2). `blocked` is the collision layer — buildings and water —
/// shared by real movers and the client's cosmetic swarm.
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

    pub(crate) fn idx(&self, x: i32, y: i32) -> usize {
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

/// Per-tile ownership layer (DESIGN §8 territory). A tile is claimed by the owner
/// of any nearby building (its footprint plus a margin).
#[derive(Resource, Debug)]
pub struct Territory {
    pub width: i32,
    pub height: i32,
    /// -1 = unclaimed, otherwise the owning `PlayerId`.
    pub owner: Vec<i32>,
}

impl Default for Territory {
    fn default() -> Self {
        Self {
            width: MAP_W,
            height: MAP_H,
            owner: vec![-1; (MAP_W * MAP_H) as usize],
        }
    }
}

impl Territory {
    pub(crate) fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    pub(crate) fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.width + x) as usize
    }

    /// Owner of a tile, if any.
    pub fn owner_at(&self, x: i32, y: i32) -> Option<PlayerId> {
        if !self.in_bounds(x, y) {
            return None;
        }
        let o = self.owner[self.idx(x, y)];
        (o >= 0).then_some(o as PlayerId)
    }

    /// Is a continuous tile-space position inside `player`'s territory?
    pub fn owned_by(&self, p: Vec2, player: PlayerId) -> bool {
        self.owner_at(p.x.floor() as i32, p.y.floor() as i32) == Some(player)
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
    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-ish float in `[-1.0, 1.0)`.
    pub(crate) fn signed_unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    }
}

/// Initialises the world-state resources.
pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimTick>()
            .init_resource::<Map>()
            .init_resource::<Territory>()
            .init_resource::<SimRng>();
    }
}
