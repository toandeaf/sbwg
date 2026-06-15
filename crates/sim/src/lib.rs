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
//! behaviour, [`setup`] the world generation, [`messages`] the boundary seam.
//! Each is a [`Plugin`]; [`SimPlugin`] composes them.

use bevy::prelude::*;

mod entity;
mod map;
mod messages;
mod setup;

pub use entity::{Building, Caravan, CaravanState, Mover, Settlement};
pub use map::{Map, Territory, SimTick, MAP_H, MAP_W};
pub use messages::{IncomingCommand, OutgoingEvent};

/// Simulation ticks per second (DESIGN §17.3: ~5–20 Hz strategic).
pub const SIM_HZ: f64 = 10.0;

/// The headless authoritative simulation, as one composable plugin.
pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((map::MapPlugin, entity::EntityPlugin, setup::SetupPlugin));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{CARAVAN_COUNT, TOWN_OWNER};
    use bevy::prelude::*;

    /// The sim must step without any rendering or real-time plugins.
    #[test]
    fn sim_advances_ticks_headlessly() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        // drive_caravans reads Res<Time>; a real headless server gets it from
        // MinimalPlugins, but the bare test App needs it inserted manually.
        app.init_resource::<Time>();
        app.world_mut().run_schedule(Startup);
        for _ in 0..5 {
            app.world_mut().run_schedule(FixedUpdate);
        }
        assert_eq!(app.world().resource::<SimTick>().0, 5);
    }

    #[test]
    fn settlement_and_leader_spawn() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);

        let mut settlements = app.world_mut().query::<&Settlement>();
        assert_eq!(settlements.iter(app.world()).count(), 1);

        let mut leaders = app.world_mut().query::<&Mover>();
        assert_eq!(leaders.iter(app.world()).count(), 1);
    }

    #[test]
    fn town_has_buildings() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        let mut buildings = app.world_mut().query::<&Building>();
        assert!(buildings.iter(app.world()).count() >= 10);
    }

    #[test]
    fn buildings_claim_territory() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);
        let territory = app.world().resource::<Territory>();
        // The central building's tile is claimed; a far corner is not.
        assert_eq!(territory.owner_at(MAP_W / 2, MAP_H / 2), Some(TOWN_OWNER));
        assert_eq!(territory.owner_at(0, 0), None);
    }

    #[test]
    fn map_has_water() {
        let map = Map::default();
        assert!(map.tiles.iter().any(|t| t.is_water()));
    }

    #[test]
    fn buildings_block_tiles() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);
        let map = app.world().resource::<Map>();
        assert!(!map.is_walkable(MAP_W / 2, MAP_H / 2));
    }

    #[test]
    fn leader_starts_walkable() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
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
    fn caravans_spawn_with_walkable_endpoints() {
        let mut app = App::new();
        app.add_plugins(SimPlugin);
        app.world_mut().run_schedule(Startup);
        app.world_mut().run_schedule(PostStartup);

        let mut caravans = app.world_mut().query::<&Caravan>();
        let world = app.world();
        let map = world.resource::<Map>();
        let all: Vec<&Caravan> = caravans.iter(world).collect();
        assert_eq!(all.len(), CARAVAN_COUNT);
        for c in all {
            assert!(map.walkable_at(c.source), "caravan source is blocked");
            assert!(map.walkable_at(c.dest), "caravan dest is blocked");
            assert_eq!(c.state, CaravanState::ToSource);
            assert_eq!(c.cargo, 0);
        }
    }
}
