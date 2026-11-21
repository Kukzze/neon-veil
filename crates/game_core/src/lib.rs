//! Shared core ECS primitives, deterministic math, identifiers and network protocol.
//!
//! `game_core` is the root of the crate graph and must never depend on any
//! other `game_*` crate.

pub mod identifiers;
pub mod math;
pub mod protocol;
pub mod time;

use bevy::prelude::*;
use identifiers::{NetworkAuthority, PlayerCharacterMarker};
use time::{NetworkTick, NETWORK_TICK_RATE_HZ};

/// Ordered phases of every fixed simulation step in [`FixedUpdate`].
///
/// Gameplay crates place their systems in these sets instead of ordering
/// against each other directly.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoreSimulationSet {
    /// Read and validate input for this tick.
    Input,
    /// Movement, combat and AI simulation.
    Simulate,
    /// Strip NaN/Inf from simulation output before it reaches transforms or
    /// the network.
    Sanitize,
}

/// The simulation tick currently being executed, advanced once per fixed step.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CurrentNetworkTick(pub NetworkTick);

/// Configures the 64 Hz fixed timestep, the shared tick counter, core system
/// sets and reflection for the core components.
pub struct CoreGameplayPlugin;

impl Plugin for CoreGameplayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(f64::from(NETWORK_TICK_RATE_HZ)))
            .init_resource::<CurrentNetworkTick>()
            .register_type::<PlayerCharacterMarker>()
            .register_type::<NetworkAuthority>()
            .configure_sets(
                FixedUpdate,
                (
                    CoreSimulationSet::Input,
                    CoreSimulationSet::Simulate,
                    CoreSimulationSet::Sanitize,
                )
                    .chain(),
            )
            .add_systems(FixedFirst, advance_network_tick);
    }
}

fn advance_network_tick(mut tick: ResMut<CurrentNetworkTick>) {
    tick.0 = tick.0.next();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(CoreGameplayPlugin);
        app
    }

    #[test]
    fn plugin_configures_64hz_fixed_timestep() {
        let app = app();
        let fixed = app.world().resource::<Time<Fixed>>();
        assert_eq!(fixed.timestep(), Duration::from_micros(15_625));
        assert_eq!(fixed.timestep(), time::FIXED_TIMESTEP);
    }

    #[test]
    fn tick_advances_once_per_fixed_step() {
        let mut app = app();
        assert_eq!(
            app.world().resource::<CurrentNetworkTick>().0,
            NetworkTick::ZERO
        );
        for _ in 0..3 {
            app.world_mut().run_schedule(FixedFirst);
        }
        assert_eq!(
            app.world().resource::<CurrentNetworkTick>().0,
            NetworkTick(3)
        );
    }

    #[test]
    fn core_components_are_reflected() {
        let app = app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(registry
            .get(std::any::TypeId::of::<PlayerCharacterMarker>())
            .is_some());
        assert!(registry
            .get(std::any::TypeId::of::<NetworkAuthority>())
            .is_some());
    }
}
