//! Boundary messages across the sim/client seam (DESIGN §17.3).
//!
//! The outside world feeds in [`IncomingCommand`]s; the sim publishes
//! [`OutgoingEvent`]s. This is the seam the network layer will later occupy.

use bevy::prelude::*;
use protocol::{PlayerCommand, SimEvent};

/// Outside world → sim. Wraps a wire [`PlayerCommand`]; the seam where the
/// network layer will later deserialise into the sim.
#[derive(Message, Debug, Clone)]
pub struct IncomingCommand(pub PlayerCommand);

/// Sim → outside world. Wraps a wire [`SimEvent`]; the seam where the network
/// layer will later serialise out to clients.
#[derive(Message, Debug, Clone)]
pub struct OutgoingEvent(pub SimEvent);
