//! Player input: turning clicks/directives into sim commands, and the camera.

use bevy::prelude::*;
use protocol::{PlayerCommand, PlayerId, TilePos};
use sim::{economy, manpower, IncomingCommand, Map, Settlement, Territory};

use crate::{tile_to_world, TILE_PX};

/// The local player. Multiplayer comes later (DESIGN §17.3).
const ME: PlayerId = 0;

/// In-progress claim drag: the tile where the drag began.
#[derive(Resource, Default)]
struct ClaimDrag {
    start: Option<IVec2>,
}

/// The translucent rectangle previewing a claim drag.
#[derive(Component)]
struct ClaimPreview;

/// The translucent cursor previewing where a building would go.
#[derive(Component)]
struct BuildPreview;

/// Input → commands, the claim/build directives, and camera pan/zoom.
pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClaimDrag>()
            .add_systems(Startup, (spawn_claim_preview, spawn_build_preview))
            .add_systems(Update, (click_to_focus, claim_land, build_structure, pan_zoom_camera));
    }
}

/// Tile under the cursor, if the cursor is over the world.
fn cursor_tile(
    windows: &Query<&Window>,
    cameras: &Query<(&Camera, &GlobalTransform)>,
    map: &Map,
) -> Option<IVec2> {
    let window = windows.single().ok()?;
    let cursor = window.cursor_position()?;
    let (camera, cam_tf) = cameras.single().ok()?;
    let world = camera.viewport_to_world_2d(cam_tf, cursor).ok()?;
    let tx = (world.x / TILE_PX + map.width as f32 / 2.0).floor() as i32;
    let ty = (world.y / TILE_PX + map.height as f32 / 2.0).floor() as i32;
    Some(IVec2::new(tx, ty))
}

/// Left-click a tile → send a `SetFocus` command (suppressed while claiming).
fn click_to_focus(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    map: Res<Map>,
    mut outbox: MessageWriter<IncomingCommand>,
) {
    // C = claim, B = build — both suppress focus clicks.
    if keys.pressed(KeyCode::KeyC) || keys.pressed(KeyCode::KeyB) || !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(tile) = cursor_tile(&windows, &cameras, &map) else { return };
    outbox.write(IncomingCommand(PlayerCommand::SetFocus {
        player: ME,
        at: TilePos::new(tile.x, tile.y),
    }));
}

fn spawn_build_preview(mut commands: Commands) {
    commands.spawn((
        BuildPreview,
        Sprite::from_color(Color::srgba(0.3, 0.9, 0.4, 0.4), Vec2::splat(TILE_PX)),
        Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)),
        Visibility::Hidden,
    ));
}

/// Directive: hold **B** and click a tile to build there (costs wealth). The
/// cursor previews green where you can build, red where you can't.
fn build_structure(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    map: Res<Map>,
    territory: Res<Territory>,
    settlements: Query<&Settlement>,
    mut outbox: MessageWriter<IncomingCommand>,
    mut preview: Query<(&mut Sprite, &mut Transform, &mut Visibility), With<BuildPreview>>,
) {
    let cursor = keys.pressed(KeyCode::KeyB).then(|| cursor_tile(&windows, &cameras, &map)).flatten();
    let Some(tile) = cursor else {
        if let Ok((_, _, mut vis)) = preview.single_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    };

    let treasury = settlements.iter().find(|s| s.owner == ME).map(|s| s.treasury).unwrap_or(0);
    let buildable = economy::can_build(&map, &territory, treasury, ME, tile);

    if let Ok((mut sprite, mut transform, mut vis)) = preview.single_mut() {
        let centre = Vec2::new(tile.x as f32 + 0.5, tile.y as f32 + 0.5);
        sprite.custom_size = Some(Vec2::splat(TILE_PX));
        sprite.color = if buildable {
            Color::srgba(0.3, 0.9, 0.4, 0.45)
        } else {
            Color::srgba(0.9, 0.2, 0.2, 0.45)
        };
        transform.translation = tile_to_world(centre, &map).extend(3.0);
        *vis = Visibility::Visible;
    }

    if buttons.just_pressed(MouseButton::Left) {
        outbox.write(IncomingCommand(PlayerCommand::Build {
            player: ME,
            at: TilePos::new(tile.x, tile.y),
        }));
    }
}

fn spawn_claim_preview(mut commands: Commands) {
    commands.spawn((
        ClaimPreview,
        Sprite::from_color(Color::srgba(0.3, 0.9, 0.4, 0.25), Vec2::splat(TILE_PX)),
        Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)),
        Visibility::Hidden,
    ));
}

/// Directive: hold **C** and click-drag to claim the tiles under the selection.
fn claim_land(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    map: Res<Map>,
    territory: Res<Territory>,
    settlements: Query<&Settlement>,
    mut drag: ResMut<ClaimDrag>,
    mut outbox: MessageWriter<IncomingCommand>,
    mut preview: Query<(&mut Sprite, &mut Transform, &mut Visibility), With<ClaimPreview>>,
) {
    // Not in claim mode → cancel any drag and hide the preview.
    if !keys.pressed(KeyCode::KeyC) {
        drag.start = None;
        if let Ok((_, _, mut vis)) = preview.single_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    }

    let cursor = cursor_tile(&windows, &cameras, &map);
    if buttons.just_pressed(MouseButton::Left) {
        drag.start = cursor;
    }

    // Live preview of the selection rectangle — green if we can hold it, red if not.
    if let (Some(start), Some(end)) = (drag.start, cursor) {
        let (min, max) = (start.min(end), start.max(end));
        let affordable = settlements
            .iter()
            .find(|s| s.owner == ME)
            .is_none_or(|s| manpower::can_claim(&territory, ME, s.pos, s.population, min, max));
        if let Ok((mut sprite, mut transform, mut vis)) = preview.single_mut() {
            let w = (max.x - min.x + 1) as f32;
            let h = (max.y - min.y + 1) as f32;
            let centre = Vec2::new((min.x + max.x) as f32 / 2.0 + 0.5, (min.y + max.y) as f32 / 2.0 + 0.5);
            sprite.custom_size = Some(Vec2::new(w * TILE_PX, h * TILE_PX));
            sprite.color = if affordable {
                Color::srgba(0.3, 0.9, 0.4, 0.25)
            } else {
                Color::srgba(0.9, 0.2, 0.2, 0.35)
            };
            transform.translation = tile_to_world(centre, &map).extend(3.0);
            *vis = Visibility::Visible;
        }
    } else if let Ok((_, _, mut vis)) = preview.single_mut() {
        *vis = Visibility::Hidden;
    }

    // Commit the claim on release.
    if buttons.just_released(MouseButton::Left) {
        if let (Some(start), Some(end)) = (drag.start, cursor) {
            let (min, max) = (start.min(end), start.max(end));
            outbox.write(IncomingCommand(PlayerCommand::ClaimArea {
                player: ME,
                min: TilePos::new(min.x, min.y),
                max: TilePos::new(max.x, max.y),
            }));
            info!("claim directive: ({}, {})..=({}, {})", min.x, min.y, max.x, max.y);
        }
        drag.start = None;
        if let Ok((_, _, mut vis)) = preview.single_mut() {
            *vis = Visibility::Hidden;
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
