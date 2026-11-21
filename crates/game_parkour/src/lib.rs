//! Kinematic parkour locomotion engine and state machine.

pub mod input_buffers;
pub mod kinematics;
pub mod state_machine;

pub use input_buffers::LocomotionBuffer;
pub use kinematics::{KinematicVelocity, ParkourPhysicsConstants};
pub use state_machine::ParkourState;

use bevy::prelude::*;
use game_core::{CoreGameplayPlugin, CoreSimulationSet};
use input_buffers::update_locomotion_buffers;
use kinematics::sanitize_kinematic_velocities;

/// Registers parkour physics constants, reflection and fixed-tick systems.
///
/// Adds [`CoreGameplayPlugin`] if it is not already present, since the
/// systems run in its fixed-step [`CoreSimulationSet`]s.
pub struct ParkourPlugin;

impl Plugin for ParkourPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CoreGameplayPlugin>() {
            app.add_plugins(CoreGameplayPlugin);
        }
        app.init_resource::<ParkourPhysicsConstants>()
            .register_type::<ParkourPhysicsConstants>()
            .register_type::<ParkourState>()
            .register_type::<KinematicVelocity>()
            .register_type::<LocomotionBuffer>()
            .add_systems(
                FixedUpdate,
                (
                    update_locomotion_buffers.in_set(CoreSimulationSet::Input),
                    sanitize_kinematic_velocities.in_set(CoreSimulationSet::Sanitize),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    #[test]
    fn plugin_registers_resources_types_and_fixed_systems() {
        let mut app = App::new();
        app.add_plugins(ParkourPlugin);

        assert!(app.world().contains_resource::<ParkourPhysicsConstants>());
        {
            let registry = app.world().resource::<AppTypeRegistry>().read();
            for id in [
                TypeId::of::<ParkourState>(),
                TypeId::of::<KinematicVelocity>(),
                TypeId::of::<LocomotionBuffer>(),
                TypeId::of::<ParkourPhysicsConstants>(),
            ] {
                assert!(registry.get(id).is_some());
            }
        }

        let entity = app
            .world_mut()
            .spawn((
                ParkourState::Idle,
                LocomotionBuffer::default(),
                KinematicVelocity(Vec3::new(f32::NAN, 0.0, 0.0)),
            ))
            .id();
        app.world_mut().run_schedule(FixedUpdate);

        let world = app.world();
        assert_eq!(
            world
                .get::<LocomotionBuffer>(entity)
                .map(|b| b.coyote_timer),
            Some(0.15)
        );
        assert_eq!(
            world.get::<KinematicVelocity>(entity).map(|v| v.0),
            Some(Vec3::ZERO)
        );
    }

    #[test]
    fn plugin_coexists_with_core_plugin_added_first() {
        let mut app = App::new();
        app.add_plugins((CoreGameplayPlugin, ParkourPlugin));
        app.world_mut().run_schedule(FixedUpdate);
    }
}
