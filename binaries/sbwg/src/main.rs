//! sbwg game executable: assembles the Bevy app from the `sim` and `client`
//! libraries and runs it. All behaviour lives in those crates — this is just the
//! runtime wiring.

use bevy::prelude::*;
use bevy::time::Fixed;
use client::ClientPlugin;
use sim::{SimPlugin, SIM_HZ};

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
