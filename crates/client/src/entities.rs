//! Presentation of sim entities: the authoritative leader, the buildings, the
//! caravans with their camel trains, and the cosmetic population swarm
//! (DESIGN §17.4).

use bevy::prelude::*;
use bevy::time::Fixed;
use protocol::PlayerId;
use sim::{Building, Caravan, Map, Mover, Settlement, Territory};

use crate::{tile_to_world, TILE_PX};

/// Walk speed of a swarm figure, in tiles/second.
const SWARM_SPEED: f32 = 1.2;
/// Most camels to draw trailing a caravan.
const MAX_VISUAL_CAMELS: u32 = 5;
/// Spacing between trailing camels, in pixels.
const CAMEL_SPACING: f32 = TILE_PX * 0.45;

/// Renders and animates the sim's entities each frame.
pub struct EntityViewPlugin;

impl Plugin for EntityViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_swarm_sprite).add_systems(
            Update,
            (
                attach_leader_sprite,
                sync_leader_transform,
                attach_building_sprites,
                attach_caravan_visual,
                (sync_caravan_transform, follow_caravan_train).chain(),
                reconcile_swarm,
                animate_swarm,
            ),
        );
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

// ---- Buildings -------------------------------------------------------------

/// Draw each building as a mud-brick rectangle sized to its tile footprint.
fn attach_building_sprites(
    mut commands: Commands,
    map: Res<Map>,
    new_buildings: Query<(Entity, &Building), Without<Sprite>>,
) {
    for (entity, building) in &new_buildings {
        let world = tile_to_world(building.center(), &map);
        let size = Vec2::new(
            building.size.x as f32 * TILE_PX - 2.0,
            building.size.y as f32 * TILE_PX - 2.0,
        );
        // slight per-building shade variation
        let v = ((building.tile.x * 5 + building.tile.y * 9).rem_euclid(4)) as f32 * 0.03;
        commands.entity(entity).insert((
            Sprite::from_color(Color::srgb(0.40 + v, 0.27 + v, 0.15 + v), size),
            Transform::from_translation(world.extend(0.5)),
        ));
    }
}

// ---- Cosmetic swarm (the masses) ------------------------------------------

/// A client-only crowd figure. Purely cosmetic — never seen by the sim. Wanders
/// across its settlement's territory, sliding around blocked tiles by reading
/// the sim's authoritative grids (DESIGN §8/§17.4).
#[derive(Component)]
struct SwarmDot {
    pos: Vec2,
    vel: Vec2,
    seed: u32,
    /// The settlement owner whose territory bounds this figure (DESIGN §8).
    owner: PlayerId,
}

/// Shared handles for the population walk sprite-sheet (4 dirs × 9 frames).
#[derive(Resource)]
struct SwarmSprite {
    image: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
}

const WALK_COLS: u32 = 9;
const WALK_ROWS: u32 = 4;
const WALK_FRAME: u32 = 60; // px per frame in walk.png (540×240)
const WALK_FPS: f32 = 8.0;
const SWARM_SPRITE_SIZE: f32 = TILE_PX * 1.3;

/// Load walk.png and build a 9×4 texture-atlas layout once at startup.
fn load_swarm_sprite(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let image = asset_server.load("walk.png");
    let layout =
        TextureAtlasLayout::from_grid(UVec2::splat(WALK_FRAME), WALK_COLS, WALK_ROWS, None, None);
    commands.insert_resource(SwarmSprite { image, layout: layouts.add(layout) });
}

/// Sprite-sheet row for a heading: 0 down, 1 left, 2 right, 3 up.
fn direction_row(vel: Vec2) -> usize {
    if vel.length_squared() < 1e-4 {
        return 0; // idle → face the camera
    }
    if vel.x.abs() > vel.y.abs() {
        if vel.x < 0.0 { 1 } else { 2 }
    } else if vel.y < 0.0 {
        0
    } else {
        3
    }
}

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
    territory: Res<Territory>,
    swarm: Res<SwarmSprite>,
    settlements: Query<&Settlement>,
    dots: Query<Entity, With<SwarmDot>>,
) {
    let Ok(settlement) = settlements.single() else { return };
    let target = settlement.population as usize;
    let current = dots.iter().count();

    if current < target {
        // Every walkable tile this settlement owns — spread the crowd over all of it.
        let mut spots = Vec::new();
        for y in 0..map.height {
            for x in 0..map.width {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                if map.walkable_at(p) && territory.owned_by(p, settlement.owner) {
                    spots.push(p);
                }
            }
        }
        if spots.is_empty() {
            return;
        }
        for i in current..target {
            // Even stride across the territory so they start dispersed.
            let start = spots[(i * spots.len() / target.max(1)) % spots.len()];
            let world = tile_to_world(start, &map);
            let mut sprite = Sprite::from_atlas_image(
                swarm.image.clone(),
                TextureAtlas { layout: swarm.layout.clone(), index: 0 },
            );
            sprite.custom_size = Some(Vec2::splat(SWARM_SPRITE_SIZE));
            commands.spawn((
                SwarmDot {
                    pos: start,
                    vel: Vec2::ZERO,
                    seed: (i as u32).wrapping_mul(2_654_435_761) | 1,
                    owner: settlement.owner,
                },
                sprite,
                Transform::from_translation(world.extend(1.0)),
            ));
        }
    } else if current > target {
        for entity in dots.iter().take(current - target) {
            commands.entity(entity).despawn();
        }
    }
}

/// Wander each swarm dot, sliding around blocked tiles and confined to its
/// owner's territory (DESIGN §8). Per-frame and client-local.
fn animate_swarm(
    time: Res<Time>,
    map: Res<Map>,
    territory: Res<Territory>,
    mut dots: Query<(&mut SwarmDot, &mut Sprite, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    for (mut dot, mut sprite, mut transform) in &mut dots {
        // Slowly-changing random heading; they roam freely across the territory.
        let bucket = (t * 0.5) as u32;
        let angle = hash32(dot.seed ^ bucket) as f32 / u32::MAX as f32 * std::f32::consts::TAU;
        let wander = Vec2::new(angle.cos(), angle.sin()) * SWARM_SPEED;
        dot.vel = dot.vel.lerp(wander, (dt * 2.0).min(1.0));

        // Axis-separated move; a tile must be walkable AND in our territory.
        let owner = dot.owner;
        let step = dot.vel * dt;
        let walkable = |p: Vec2| map.walkable_at(p) && territory.owned_by(p, owner);
        let mut np = dot.pos;
        let try_x = Vec2::new(dot.pos.x + step.x, dot.pos.y);
        if walkable(try_x) {
            np.x = try_x.x;
        } else {
            dot.vel.x = -dot.vel.x;
        }
        let try_y = Vec2::new(np.x, dot.pos.y + step.y);
        if walkable(try_y) {
            np.y = try_y.y;
        } else {
            dot.vel.y = -dot.vel.y;
        }

        dot.pos = np;
        let world = tile_to_world(np, &map);
        transform.translation.x = world.x;
        transform.translation.y = world.y;

        // Walk-cycle frame + face the direction of travel.
        let frame = ((t * WALK_FPS) as usize).wrapping_add(dot.seed as usize) % WALK_COLS as usize;
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = direction_row(dot.vel) * WALK_COLS as usize + frame;
        }
    }
}

// ---- Caravans (real movers, with a camel train) ----------------------------

/// One camel in a caravan's trail. An independent entity (not a child), pulled
/// along behind the segment ahead of it each frame — so the train bends along
/// the path instead of rotating rigidly.
#[derive(Component)]
struct Camel {
    caravan: Entity,
    index: u32,
    pos: Vec2,
}

/// Give a caravan its lead sprite and spawn its trail of independent camels.
fn attach_caravan_visual(
    mut commands: Commands,
    map: Res<Map>,
    new_caravans: Query<(Entity, &Caravan), Without<Sprite>>,
) {
    for (entity, caravan) in &new_caravans {
        let head = tile_to_world(caravan.pos, &map);
        commands.entity(entity).insert((
            Sprite::from_color(Color::srgb(0.45, 0.30, 0.15), Vec2::splat(TILE_PX * 0.5)),
            Transform::from_translation(head.extend(2.0)),
        ));
        let n = caravan.camels.min(MAX_VISUAL_CAMELS);
        for k in 0..n {
            // String the train out behind; follow_caravan_train corrects it.
            let pos = head - Vec2::new((k + 1) as f32 * CAMEL_SPACING, 0.0);
            commands.spawn((
                Camel { caravan: entity, index: k, pos },
                Sprite::from_color(Color::srgb(0.78, 0.62, 0.40), Vec2::splat(TILE_PX * 0.35)),
                Transform::from_translation(pos.extend(2.0)),
            ));
        }
    }
}

/// Interpolate the caravan lead between sim ticks (no rotation — the train shows
/// direction by trailing along the path).
fn sync_caravan_transform(
    map: Res<Map>,
    fixed_time: Res<Time<Fixed>>,
    mut caravans: Query<(&Caravan, &mut Transform)>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (caravan, mut transform) in &mut caravans {
        let interpolated = caravan.prev.lerp(caravan.pos, alpha);
        let world = tile_to_world(interpolated, &map);
        transform.translation.x = world.x;
        transform.translation.y = world.y;
    }
}

/// Pull each camel to sit `CAMEL_SPACING` behind the segment ahead of it (the
/// lead for index 0, the previous camel otherwise). A follow-the-leader chain:
/// the train trails and bends fluidly instead of snapping.
fn follow_caravan_train(
    caravans: Query<(Entity, &Transform), With<Caravan>>,
    mut camels: Query<(&mut Camel, &mut Transform), Without<Caravan>>,
) {
    use std::collections::HashMap;

    let heads: HashMap<Entity, Vec2> =
        caravans.iter().map(|(e, t)| (e, t.translation.truncate())).collect();

    // Group camels per caravan so we can solve each chain front-to-back.
    let mut groups: HashMap<Entity, Vec<(u32, Vec2)>> = HashMap::new();
    for (camel, _) in &camels {
        groups.entry(camel.caravan).or_default().push((camel.index, camel.pos));
    }

    let mut solved: HashMap<(Entity, u32), Vec2> = HashMap::new();
    for (caravan, mut list) in groups {
        let Some(&head) = heads.get(&caravan) else { continue };
        list.sort_by_key(|(index, _)| *index);
        let mut ahead = head;
        for (index, pos) in list {
            let mut dir = pos - ahead;
            if dir.length_squared() < 1e-6 {
                dir = Vec2::new(-1.0, 0.0);
            }
            let np = ahead + dir.normalize() * CAMEL_SPACING;
            solved.insert((caravan, index), np);
            ahead = np;
        }
    }

    for (mut camel, mut transform) in &mut camels {
        if let Some(&np) = solved.get(&(camel.caravan, camel.index)) {
            camel.pos = np;
            transform.translation.x = np.x;
            transform.translation.y = np.y;
        }
    }
}
