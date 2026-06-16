//! Population dynamics: water-gated logistic growth (DESIGN §13).
//!
//! Each interval the settlement consumes water from the store (≈ population/10).
//! Population **grows** when supply is beating demand (the store is trending up),
//! **declines** (emigration) when the store runs dry, and is capped by a
//! housing-style ceiling. Growth keys off the store *trend* rather than its
//! level, so a fat one-off buffer can't fuel growth that supply won't sustain —
//! caravan throughput is the real ceiling. Food and housing become additional
//! `min()` caps later.

use bevy::prelude::*;
use protocol::SimEvent;

use crate::caravan::WaterStore;
use crate::entity::Settlement;
use crate::messages::OutgoingEvent;

/// Seconds between growth evaluations.
const GROWTH_INTERVAL: f32 = 1.0;
/// Water demand per interval ≈ population / this.
const WATER_DEMAND_DIVISOR: u32 = 10;
/// Housing-style upper bound (placeholder for a real housing cap, DESIGN §13).
const SETTLEMENT_CAP: u32 = 200;
/// Floor so a parched town dwindles rather than vanishing.
const MIN_POP: u32 = 5;

fn update_population(
    time: Res<Time>,
    mut elapsed: Local<f32>,
    mut last_level: Local<Option<u32>>,
    mut settlements: Query<&mut Settlement>,
    mut stores: Query<&mut WaterStore>,
    mut outbox: MessageWriter<OutgoingEvent>,
) {
    *elapsed += time.delta_secs();
    if *elapsed < GROWTH_INTERVAL {
        return;
    }
    *elapsed -= GROWTH_INTERVAL;

    let Ok(mut settlement) = settlements.single_mut() else { return };
    let Ok(mut store) = stores.single_mut() else { return };

    // Did inflow beat demand since last interval? (store level trending up)
    let level = store.stored;
    let surplus = level > last_level.unwrap_or(level);
    *last_level = Some(level);

    let demand = (settlement.population / WATER_DEMAND_DIVISOR).max(1);
    store.stored = store.stored.saturating_sub(demand);

    let before = settlement.population;
    if store.stored == 0 {
        // Ran dry → emigration.
        settlement.population = settlement.population.saturating_sub(1).max(MIN_POP);
    } else if surplus && settlement.population < SETTLEMENT_CAP {
        // Comfortable, growing supply + room → grow.
        settlement.population += 1;
    }

    if settlement.population != before {
        outbox.write(OutgoingEvent(SimEvent::PopulationChanged {
            population: settlement.population,
        }));
    }
}

/// Water-gated population growth.
pub struct PopulationPlugin;

impl Plugin for PopulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, update_population);
    }
}
