//! Single-player client: a full Bevy app with rendering that embeds the headless
//! [`SimPlugin`] in-process. Everything is drawn as **coloured squares**
//! (placeholder art).
//!
//! The client talks to the sim only through messages and by reading public sim
//! state for rendering — the eventual network boundary (DESIGN §17.3). Logic is
//! split into [`world`] (camera + terrain + territory + event log), [`entities`]
//! (leader, buildings, caravans, swarm), and [`player_input`] (input + camera
//! control). Each is a [`Plugin`]; [`ClientPlugin`] composes them.

use bevy::prelude::*;
use bevy::time::Fixed;
use sim::{Map, SimPlugin, SIM_HZ};

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

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "sbwg (scaffold)".into(),
                ..default()
            }),
            ..default()
        }))
        // Drive the sim's FixedUpdate at the design tick rate.
        .insert_resource(Time::<Fixed>::from_hz(SIM_HZ))
        .add_plugins(SimPlugin)
        .add_plugins(ClientPlugin)
        .run();
}

/// Bundles the client's presentation and input plugins.
struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            world::WorldViewPlugin,
            entities::EntityViewPlugin,
            player_input::PlayerInputPlugin,
        ));
    }
}
