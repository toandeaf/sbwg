//! Player input: turning clicks into sim commands, and driving the camera.

use bevy::prelude::*;
use protocol::{PlayerCommand, PlayerId, TilePos};
use sim::{IncomingCommand, Map};

use crate::TILE_PX;

/// The local player. Multiplayer comes later (DESIGN §17.3).
const ME: PlayerId = 0;

/// Input → commands, and camera pan/zoom.
pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (click_to_focus, pan_zoom_camera));
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
