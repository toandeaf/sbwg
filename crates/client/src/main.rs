//! Single-player client: a full Bevy app with rendering that embeds the headless
//! [`SimPlugin`] in-process. For now everything is drawn as **coloured squares**
//! (placeholder art) — the map tiles and the movers.
//!
//! The client talks to the sim only through messages ([`IncomingCommand`] /
//! [`OutgoingEvent`]) and by reading the sim's public state for rendering. That
//! mirrors the eventual network boundary (DESIGN §17.3/§17.4): swap the in-process
//! messages for serialised wire traffic and nothing else changes.

use bevy::prelude::*;
use bevy::time::Fixed;
use protocol::{PlayerCommand, PlayerId, SimEvent, TilePos};
use sim::{IncomingCommand, Map, Mover, OutgoingEvent, SimPlugin, SIM_HZ};

/// On-screen size of one tile, in pixels (at default zoom).
const TILE_PX: f32 = 24.0;
/// The local player. Multiplayer comes later (DESIGN §17.3).
const ME: PlayerId = 0;

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

struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_camera, spawn_tile_grid))
            .add_systems(
                Update,
                (
                    attach_mover_sprites,
                    sync_mover_transforms,
                    click_to_focus,
                    log_sim_events,
                    pan_zoom_camera,
                ),
            );
    }
}

/// Continuous tile-space → centred world pixels.
fn tile_to_world(p: Vec2, map: &Map) -> Vec2 {
    Vec2::new(
        (p.x - map.width as f32 / 2.0) * TILE_PX,
        (p.y - map.height as f32 / 2.0) * TILE_PX,
    )
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Draw the map once as a grid of coloured squares (desert palette).
fn spawn_tile_grid(mut commands: Commands, map: Res<Map>) {
    for y in 0..map.height {
        for x in 0..map.width {
            let centre = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let world = tile_to_world(centre, &map);
            let shade = (((x * 7 + y * 13).rem_euclid(5)) as f32) * 0.02;
            let color = Color::srgb(0.80 + shade, 0.68 + shade, 0.45 + shade);
            commands.spawn((
                Sprite::from_color(color, Vec2::splat(TILE_PX - 1.0)),
                Transform::from_translation(world.extend(0.0)),
            ));
        }
    }
}

/// Give freshly-spawned sim movers a sprite so they render. The sim owns the
/// entity + logical position; the client only attaches presentation.
fn attach_mover_sprites(
    mut commands: Commands,
    map: Res<Map>,
    new_movers: Query<(Entity, &Mover), Without<Sprite>>,
) {
    for (entity, mover) in &new_movers {
        let world = tile_to_world(mover.pos, &map);
        commands.entity(entity).insert((
            Sprite::from_color(Color::srgb(0.92, 0.86, 0.20), Vec2::splat(TILE_PX * 0.5)),
            Transform::from_translation(world.extend(1.0)),
        ));
    }
}

/// Mirror each mover's logical position into its render transform every frame.
fn sync_mover_transforms(map: Res<Map>, mut movers: Query<(&Mover, &mut Transform)>) {
    for (mover, mut transform) in &mut movers {
        let world = tile_to_world(mover.pos, &map);
        transform.translation.x = world.x;
        transform.translation.y = world.y;
    }
}

/// Left-click a tile → send a `SetFocus` command into the sim.
fn click_to_focus(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    map: Res<Map>,
    mut outbox: MessageWriter<IncomingCommand>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cameras.single() else { return };
    let Ok(world) = camera.viewport_to_world_2d(cam_tf, cursor) else { return };

    let tx = (world.x / TILE_PX + map.width as f32 / 2.0).floor() as i32;
    let ty = (world.y / TILE_PX + map.height as f32 / 2.0).floor() as i32;
    outbox.write(IncomingCommand(PlayerCommand::SetFocus {
        player: ME,
        at: TilePos::new(tx, ty),
    }));
}

/// Consume the sim's outbound events (demonstrates the message seam).
fn log_sim_events(mut inbox: MessageReader<OutgoingEvent>) {
    for OutgoingEvent(event) in inbox.read() {
        match event {
            SimEvent::FocusChanged { player, at } => {
                info!("focus: player {player} -> ({}, {})", at.x, at.y);
            }
            SimEvent::Ticked { tick } => {
                if tick % 50 == 0 {
                    info!("sim tick {tick}");
                }
            }
        }
    }
}

/// WASD to pan, Q/E to zoom out/in (camera transform only — no projection fuss).
fn pan_zoom_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    let Ok(mut transform) = camera.single_mut() else { return };
    let dt = time.delta_secs();

    let mut pan = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        pan.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        pan.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        pan.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        pan.x += 1.0;
    }
    transform.translation += (pan * 400.0 * dt).extend(0.0);

    let mut zoom = 0.0;
    if keys.pressed(KeyCode::KeyQ) {
        zoom += 1.0; // zoom out
    }
    if keys.pressed(KeyCode::KeyE) {
        zoom -= 1.0; // zoom in
    }
    let scale = (transform.scale.x * (1.0 + zoom * dt)).clamp(0.2, 5.0);
    transform.scale = Vec3::new(scale, scale, 1.0);
}
