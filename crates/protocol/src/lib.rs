//! Wire types shared across the sim/client boundary.
//!
//! Pure data only — **no Bevy dependency**. These are the *serialisable* command
//! and event payloads that will eventually cross the network (DESIGN §17.3).
//! In-engine they are carried inside Bevy messages defined in the `sim` crate,
//! which keeps the wire format decoupled from the engine's messaging.

use serde::{Deserialize, Serialize};

/// Identifies one of the (up to 4) human players.
pub type PlayerId = u32;

/// A cell on the fine tile grid (DESIGN §6.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TilePos {
    pub x: i32,
    pub y: i32,
}

impl TilePos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Terrain of a tile (DESIGN §6.2/§13). Water-bearing tiles (oasis/well) are the
/// position-bound primary resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Terrain {
    Sand,
    Oasis,
    Well,
}

impl Terrain {
    /// Does this tile provide water?
    pub fn is_water(self) -> bool {
        matches!(self, Terrain::Oasis | Terrain::Well)
    }
}

/// Client → sim. Player intent. Stub set; grows with the design.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PlayerCommand {
    /// Designate a place as the current focus of intent (placeholder).
    SetFocus { player: PlayerId, at: TilePos },
}

/// Sim → client. Notable state changes. Stub set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SimEvent {
    /// The authoritative simulation advanced one tick.
    Ticked { tick: u64 },
    /// A player's focus moved.
    FocusChanged { player: PlayerId, at: TilePos },
    /// A caravan dropped a load of water at a settlement.
    WaterDelivered { amount: u32, stored: u32 },
}
