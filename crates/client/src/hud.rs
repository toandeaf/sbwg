//! A minimal on-screen HUD: the settlement's population and stored water.

use bevy::prelude::*;
use sim::{Settlement, WaterStore};

/// Top-left readout of population + water.
pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud).add_systems(Update, update_hud);
    }
}

#[derive(Component)]
struct HudText;

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                left: Val::Px(8.0),
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        ))
        .with_children(|panel| {
            panel.spawn((
                HudText,
                Text::new("Population: --\nWater: --"),
                TextFont { font_size: 18.0, ..default() },
                TextColor(Color::srgb(0.95, 0.90, 0.75)),
            ));
        });
}

fn update_hud(
    settlements: Query<&Settlement>,
    stores: Query<&WaterStore>,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let population = settlements.iter().next().map(|s| s.population).unwrap_or(0);
    let water = stores.iter().next().map(|s| s.stored).unwrap_or(0);
    if let Ok(mut text) = hud.single_mut() {
        *text = Text::new(format!("Population: {population}\nWater: {water}"));
    }
}
