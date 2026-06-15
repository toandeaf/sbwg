//! Headless authoritative simulation (DESIGN §17.2).
//!
//! Runs as a Bevy plugin so it can be embedded in the client (single-player,
//! in-process) today and hosted in a headless server app tomorrow without code
//! changes. Nothing in here renders or knows the client exists.
//!
//! **Decoupling (DESIGN: lean on Bevy's messages):** the outside world feeds in
//! [`IncomingCommand`]s and the sim publishes [`OutgoingEvent`]s. Systems never
//! call each other directly — they read/write messages and shared components.
//! That message seam is exactly where the network layer will later plug in.
//!
//! Structure: [`map`] holds world state, [`entity`] the components + per-tick
//! behaviour, [`caravan`] the water-haul logistics, [`setup`] the world
//! generation, [`path`] A* pathfinding, [`messages`] the boundary seam. Each
//! gameplay module is a [`Plugin`]; [`SimPlugin`] composes them.

use bevy::prelude::*;

mod caravan;
mod entity;
mod map;
mod messages;
mod path;
mod setup;

pub use caravan::{Caravan, CaravanState, WaterStore};
pub use entity::{Building, Mover, Settlement};
pub use map::{Map, SimTick, Territory, MAP_H, MAP_W};
pub use messages::{IncomingCommand, OutgoingEvent};

/// Simulation ticks per second (DESIGN §17.3: ~5–20 Hz strategic).
pub const SIM_HZ: f64 = 10.0;

/// The headless authoritative simulation, as one composable plugin.
pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            map::MapPlugin,
            entity::EntityPlugin,
            caravan::CaravanPlugin,
            setup::SetupPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{CARAVAN_COUNT, TOWN_OWNER};
    use bevy::prelude::*;

    /// Build a sim, run startup, and (optionally) a few ticks. Time is inserted
    /// because the bare test App lacks the MinimalPlugins a server would add.
    fn headless_app() -> App {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.init_resource::<Time>();
        app
    }

    #[test]
    fn sim_advances_ticks_headlessly() {
        let mut app = headless_app();
        app.world_mut().run_schedule(Startup);
        for _ in 0..5 {
            app.world_mut().run_schedule(FixedUpdate);
        }
        assert_eq!(app.world().resource::<SimTick>().0, 5);
    }

    #[test]
    fn settlement_and_leader_spawn() {
        let mut app = headless_app();
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);

        let mut settlements = app.world_mut().query::<&Settlement>();
        assert_eq!(settlements.iter(app.world()).count(), 1);

        let mut leaders = app.world_mut().query::<&Mover>();
        assert_eq!(leaders.iter(app.world()).count(), 1);
    }

    #[test]
    fn town_has_buildings() {
        let mut app = headless_app();
        app.world_mut().run_schedule(Startup);
        let mut buildings = app.world_mut().query::<&Building>();
        assert!(buildings.iter(app.world()).count() >= 10);
    }

    #[test]
    fn buildings_claim_territory() {
        let mut app = headless_app();
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);
        let territory = app.world().resource::<Territory>();
        assert_eq!(territory.owner_at(MAP_W / 2, MAP_H / 2), Some(TOWN_OWNER));
        assert_eq!(territory.owner_at(0, 0), None);
    }

    #[test]
    fn claim_rect_marks_tiles() {
        let mut territory = Territory::default();
        territory.claim_rect(TOWN_OWNER, IVec2::new(2, 3), IVec2::new(4, 5));
        assert_eq!(territory.owner_at(2, 3), Some(TOWN_OWNER));
        assert_eq!(territory.owner_at(4, 5), Some(TOWN_OWNER));
        assert_eq!(territory.owner_at(1, 3), None);
        assert_eq!(territory.owner_at(5, 5), None);
    }

    #[test]
    fn map_has_water() {
        let map = Map::default();
        assert!(map.tiles.iter().any(|t| t.is_water()));
    }

    #[test]
    fn buildings_block_tiles() {
        let mut app = headless_app();
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);
        let map = app.world().resource::<Map>();
        assert!(!map.is_walkable(MAP_W / 2, MAP_H / 2));
    }

    #[test]
    fn leader_starts_walkable() {
        let mut app = headless_app();
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);
        let mut leaders = app.world_mut().query::<&Mover>();
        let world = app.world();
        let map = world.resource::<Map>();
        for mover in leaders.iter(world) {
            assert!(map.walkable_at(mover.pos), "leader spawned on a blocked tile");
        }
    }

    #[test]
    fn caravans_spawn_idle() {
        let mut app = headless_app();
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);
        let mut caravans = app.world_mut().query::<&Caravan>();
        let world = app.world();
        let all: Vec<&Caravan> = caravans.iter(world).collect();
        assert_eq!(all.len(), CARAVAN_COUNT);
        for c in all {
            assert_eq!(c.state, CaravanState::Idle);
            assert!(c.tour.is_empty());
        }
    }

    #[test]
    fn claiming_water_assigns_a_caravan() {
        let mut app = headless_app();
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);

        // Caravans start idle — nothing is claimed yet.
        {
            let mut caravans = app.world_mut().query::<&Caravan>();
            let world = app.world();
            assert!(caravans.iter(world).all(|c| c.state == CaravanState::Idle));
        }

        // Claim the nearest water source (simulating the claim directive).
        let centre = Vec2::new(MAP_W as f32 / 2.0, MAP_H as f32 / 2.0);
        let water = {
            let map = app.world().resource::<Map>();
            let mut waters = map.water_tiles();
            waters.sort_by(|a, b| {
                a.distance_squared(centre).partial_cmp(&b.distance_squared(centre)).unwrap()
            });
            waters[0]
        };
        let tile = IVec2::new(water.x.floor() as i32, water.y.floor() as i32);
        app.world_mut()
            .resource_mut::<Territory>()
            .claim_rect(TOWN_OWNER, tile - IVec2::ONE, tile + IVec2::ONE);

        // The assignment system now puts a caravan on the route.
        app.world_mut().run_schedule(FixedUpdate);
        let mut caravans = app.world_mut().query::<&Caravan>();
        let world = app.world();
        let assigned = caravans.iter(world).filter(|c| c.state != CaravanState::Idle).count();
        assert!(assigned >= 1, "claiming water should assign a caravan");
        assert!(caravans.iter(world).any(|c| c.route.len() >= 2), "expected a routed caravan");
    }

    #[test]
    fn caravans_distribute_across_sources() {
        let mut app = headless_app();
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);

        // Claim the two nearest water sources.
        let centre = Vec2::new(MAP_W as f32 / 2.0, MAP_H as f32 / 2.0);
        let waters = {
            let map = app.world().resource::<Map>();
            let mut w = map.water_tiles();
            w.sort_by(|a, b| {
                a.distance_squared(centre).partial_cmp(&b.distance_squared(centre)).unwrap()
            });
            w
        };
        for water in waters.iter().take(2) {
            let tile = IVec2::new(water.x.floor() as i32, water.y.floor() as i32);
            app.world_mut()
                .resource_mut::<Territory>()
                .claim_rect(TOWN_OWNER, tile - IVec2::ONE, tile + IVec2::ONE);
        }

        app.world_mut().run_schedule(FixedUpdate);
        let mut caravans = app.world_mut().query::<&Caravan>();
        let world = app.world();
        let working = caravans.iter(world).filter(|c| c.state != CaravanState::Idle).count();
        assert!(working >= 2, "two claimed sources should engage two caravans, got {working}");
    }
}
