//! Client presentation + input, as a composable Bevy plugin.
//!
//! Pure static code — the runnable executable lives in `binaries/`. Logic is
//! split into [`world`] (camera + terrain + territory + event log), [`entities`]
//! (leader, buildings, caravans, swarm), and [`player_input`] (input + camera
//! control). Each is a [`Plugin`]; [`ClientPlugin`] composes them.

use bevy::prelude::*;
use sim::Map;

mod entities;
mod player_input;
mod world;

/// On-screen size of one tile, in pixels (at default zoom).
pub(crate) const TILE_PX: f32 = 24.0;

/// Continuous tile-space → centred world pixels.
pub(crate) fn tile_to_world(p: Vec2, map: &Map) -> Vec2 {
    Vec2::new(
        (p.x - map.width as f32 / 2.0) * TILE_PX,
        (p.y - map.height as f32 / 2.0) * TILE_PX,
    )
}

/// Bundles the client's presentation and input plugins.
pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            world::WorldViewPlugin,
            entities::EntityViewPlugin,
            player_input::PlayerInputPlugin,
        ));
    }
}
