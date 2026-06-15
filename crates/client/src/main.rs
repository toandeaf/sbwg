//! Single-player client: a full Bevy app with rendering that embeds the headless
//! [`SimPlugin`] in-process. Everything is drawn as **coloured squares**
//! (placeholder art).
//!
//! Two render paths, mirroring DESIGN §17.4:
//!   * **Real movers** — the sim's authoritative `Mover` (a "leader"), drawn as
//!     one square interpolated between sim ticks.
//!   * **Cosmetic swarm** — the masses. The sim only tracks a settlement's
//!     `population` *number*; the client spawns that many client-only dots that
//!     mill about locally. They never touch the sim and add zero netcode cost.
//!
//! The client talks to the sim only through messages ([`IncomingCommand`] /
//! [`OutgoingEvent`]) and by reading public sim state for rendering — the
//! eventual network boundary (DESIGN §17.3).

use bevy::prelude::*;
use bevy::time::Fixed;
use protocol::{PlayerCommand, PlayerId, SimEvent, Terrain, TilePos};
use sim::{IncomingCommand, Map, Mover, OutgoingEvent, Settlement, SimPlugin, SIM_HZ};

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
                    attach_leader_sprite,
                    sync_leader_transform,
                    attach_settlement_sprite,
                    reconcile_swarm,
                    animate_swarm,
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

// ---- Real mover (the leader) ----------------------------------------------

/// Give the sim's leader entity a sprite so it renders (red, distinct).
fn attach_leader_sprite(
    mut commands: Commands,
    map: Res<Map>,
    new_movers: Query<(Entity, &Mover), Without<Sprite>>,
) {
    for (entity, mover) in &new_movers {
        let world = tile_to_world(mover.pos, &map);
        commands.entity(entity).insert((
            Sprite::from_color(Color::srgb(0.85, 0.15, 0.15), Vec2::splat(TILE_PX * 0.8)),
            Transform::from_translation(world.extend(2.0)),
        ));
    }
}

/// Draw the leader interpolated between its last two sim ticks, so motion is
/// smooth at frame rate despite the 10 Hz sim (DESIGN §17.4).
fn sync_leader_transform(
    map: Res<Map>,
    fixed_time: Res<Time<Fixed>>,
    mut movers: Query<(&Mover, &mut Transform)>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (mover, mut transform) in &mut movers {
        let interpolated = mover.prev.lerp(mover.pos, alpha);
        let world = tile_to_world(interpolated, &map);
        transform.translation.x = world.x;
        transform.translation.y = world.y;
    }
}

// ---- Settlement ------------------------------------------------------------

/// Draw the settlement as a chunky mud-brick square.
fn attach_settlement_sprite(
    mut commands: Commands,
    map: Res<Map>,
    new_settlements: Query<(Entity, &Settlement), Without<Sprite>>,
) {
    for (entity, settlement) in &new_settlements {
        let world = tile_to_world(settlement.pos, &map);
        commands.entity(entity).insert((
            Sprite::from_color(Color::srgb(0.35, 0.22, 0.12), Vec2::splat(TILE_PX * 1.6)),
            Transform::from_translation(world.extend(0.5)),
        ));
    }
}

// ---- Cosmetic swarm (the masses) ------------------------------------------

/// A client-only crowd figure. Purely cosmetic — never seen by the sim. Wanders
/// near its settlement, sliding around blocked tiles by reading the sim's
/// authoritative passability grid (DESIGN §17.4: cosmetic collision over
/// authoritative data).
#[derive(Component)]
struct SwarmDot {
    pos: Vec2,
    vel: Vec2,
    home: Vec2,
    seed: u32,
}

/// Walk speed of a swarm figure, in tiles/second.
const SWARM_SPEED: f32 = 1.2;
/// How far a swarm figure roams from its settlement.
const SWARM_RADIUS: f32 = 4.0;

/// Cheap integer hash for deterministic, dependency-free dot steering.
fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Keep the number of swarm dots equal to the settlement's population: the sim
/// owns the count, the client owns the crowd. (Single-settlement for now.)
fn reconcile_swarm(
    mut commands: Commands,
    map: Res<Map>,
    settlements: Query<&Settlement>,
    dots: Query<Entity, With<SwarmDot>>,
) {
    let Ok(settlement) = settlements.single() else { return };
    let target = settlement.population as usize;
    let current = dots.iter().count();

    if current < target {
        for i in current..target {
            // Spread spawns across nearby walkable rings so the crowd disperses.
            let start = map.find_walkable_near(settlement.pos, 1.5 + (i % 4) as f32 * 0.8);
            let world = tile_to_world(start, &map);
            commands.spawn((
                SwarmDot {
                    pos: start,
                    vel: Vec2::ZERO,
                    home: settlement.pos,
                    seed: (i as u32).wrapping_mul(2_654_435_761) | 1,
                },
                Sprite::from_color(Color::srgb(0.22, 0.17, 0.13), Vec2::splat(TILE_PX * 0.3)),
                Transform::from_translation(world.extend(1.0)),
            ));
        }
    } else if current > target {
        for entity in dots.iter().take(current - target) {
            commands.entity(entity).despawn();
        }
    }
}

/// Wander each swarm dot, sliding around blocked tiles and staying near home.
/// Per-frame and client-local — no interpolation needed.
fn animate_swarm(time: Res<Time>, map: Res<Map>, mut dots: Query<(&mut SwarmDot, &mut Transform)>) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    for (mut dot, mut transform) in &mut dots {
        // Pick a slowly-changing heading (new direction roughly every 2s).
        let bucket = (t * 0.5) as u32;
        let angle = hash32(dot.seed ^ bucket) as f32 / u32::MAX as f32 * std::f32::consts::TAU;
        let desired = Vec2::new(angle.cos(), angle.sin()) * SWARM_SPEED;
        dot.vel = dot.vel.lerp(desired, (dt * 2.0).min(1.0));

        // Axis-separated move with tile collision (read authoritative grid).
        let step = dot.vel * dt;
        let mut np = dot.pos;
        let try_x = Vec2::new(dot.pos.x + step.x, dot.pos.y);
        if map.walkable_at(try_x) {
            np.x = try_x.x;
        } else {
            dot.vel.x = -dot.vel.x;
        }
        let try_y = Vec2::new(np.x, dot.pos.y + step.y);
        if map.walkable_at(try_y) {
            np.y = try_y.y;
        } else {
            dot.vel.y = -dot.vel.y;
        }

        // Contain near home.
        let off = np - dot.home;
        if off.length() > SWARM_RADIUS {
            let n = off.normalize_or_zero();
            np = dot.home + n * SWARM_RADIUS;
            let v = dot.vel;
            dot.vel = v - 2.0 * v.dot(n) * n;
        }

        dot.pos = np;
        let world = tile_to_world(np, &map);
        transform.translation.x = world.x;
        transform.translation.y = world.y;
    }
}

// ---- Input / events --------------------------------------------------------

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
