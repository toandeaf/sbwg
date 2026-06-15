//! Static scene presentation: the camera, the terrain grid, the territory tint,
//! and the sim event log.

use bevy::prelude::*;
use protocol::{SimEvent, Terrain};
use sim::{Map, OutgoingEvent, Territory};

use crate::{tile_to_world, TILE_PX};

/// Camera + terrain + territory + event log.
pub struct WorldViewPlugin;

impl Plugin for WorldViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_camera, spawn_tile_grid))
            .add_systems(Update, (draw_territory_overlay, log_sim_events));
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Draw the map once, colouring each tile by terrain (DESIGN §6.2/§13).
fn spawn_tile_grid(mut commands: Commands, map: Res<Map>) {
    for y in 0..map.height {
        for x in 0..map.width {
            let (r, g, b) = match map.terrain_at(x, y) {
                Terrain::Sand => (0.82, 0.70, 0.47),
                Terrain::Oasis => (0.20, 0.55, 0.45),
                Terrain::Well => (0.30, 0.52, 0.72),
            };
            // a touch of per-tile shade so the grid reads as texture, not stripes
            let shade = ((x * 7 + y * 13).rem_euclid(5)) as f32 * 0.015;
            let centre = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let world = tile_to_world(centre, &map);
            commands.spawn((
                Sprite::from_color(Color::srgb(r + shade, g + shade, b + shade), Vec2::splat(TILE_PX - 1.0)),
                Transform::from_translation(world.extend(0.0)),
            ));
        }
    }
}

/// Tint claimed tiles a faint gold so territory is visible. Runs once, after the
/// sim has stamped the territory grid (DESIGN §8).
fn draw_territory_overlay(
    mut done: Local<bool>,
    mut commands: Commands,
    map: Res<Map>,
    territory: Res<Territory>,
) {
    if *done {
        return;
    }
    *done = true;
    for y in 0..map.height {
        for x in 0..map.width {
            if territory.owner_at(x, y).is_some() {
                let centre = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let world = tile_to_world(centre, &map);
                commands.spawn((
                    Sprite::from_color(Color::srgba(0.95, 0.82, 0.25, 0.15), Vec2::splat(TILE_PX - 1.0)),
                    Transform::from_translation(world.extend(0.25)),
                ));
            }
        }
    }
}

/// Consume the sim's outbound events (demonstrates the message seam).
fn log_sim_events(mut inbox: MessageReader<OutgoingEvent>) {
    for OutgoingEvent(event) in inbox.read() {
        match event {
            SimEvent::FocusChanged { player, at } => {
                info!("focus: player {player} -> ({}, {})", at.x, at.y);
            }
            SimEvent::WaterDelivered { amount, stored } => {
                info!("caravan delivered {amount} water (settlement now holds {stored})");
            }
            SimEvent::Ticked { tick } => {
                if tick % 50 == 0 {
                    info!("sim tick {tick}");
                }
            }
        }
    }
}
