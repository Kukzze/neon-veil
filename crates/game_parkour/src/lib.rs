//! Kinematic parkour locomotion engine and state machine.

pub mod controller;
pub mod flow_state;
pub mod input_buffers;
pub mod kinematics;
pub mod shapecasts;
pub mod state_machine;

pub use controller::{ParkourController, ParkourInput};
pub use flow_state::FlowState;
pub use input_buffers::LocomotionBuffer;
pub use kinematics::{KinematicVelocity, ParkourPhysicsConstants};
pub use shapecasts::ParkourSensorReadings;
pub use state_machine::ParkourState;

use avian3d::prelude::SpatialQueryPipeline;
use bevy::prelude::*;
use controller::update_parkour_controller;
use flow_state::update_flow_and_stamina;
use game_core::{CoreGameplayPlugin, CoreSimulationSet};
use input_buffers::update_locomotion_buffers;
use kinematics::sanitize_kinematic_velocities;
use shapecasts::execute_parkour_spatial_probes;

/// Registers parkour physics constants, reflection and fixed-tick systems.
///
/// Adds [`CoreGameplayPlugin`] if it is not already present, since the
/// systems run in its fixed-step [`CoreSimulationSet`]s. Probing and the
/// controller need Avian's spatial query pipeline (`PhysicsPlugins`) and are
/// skipped while it is absent.
pub struct ParkourPlugin;

impl Plugin for ParkourPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CoreGameplayPlugin>() {
            app.add_plugins(CoreGameplayPlugin);
        }
        let has_physics = resource_exists::<SpatialQueryPipeline>;
        app.init_resource::<ParkourPhysicsConstants>()
            .register_type::<ParkourPhysicsConstants>()
            .register_type::<ParkourState>()
            .register_type::<KinematicVelocity>()
            .register_type::<LocomotionBuffer>()
            .register_type::<FlowState>()
            .register_type::<ParkourSensorReadings>()
            .register_type::<ParkourInput>()
            .register_type::<ParkourController>()
            .add_systems(
                FixedUpdate,
                (
                    (
                        execute_parkour_spatial_probes.run_if(has_physics),
                        update_locomotion_buffers,
                    )
                        .in_set(CoreSimulationSet::Input),
                    (
                        update_parkour_controller.run_if(has_physics),
                        update_flow_and_stamina,
                    )
                        .chain()
                        .in_set(CoreSimulationSet::Simulate),
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
                TypeId::of::<FlowState>(),
                TypeId::of::<ParkourSensorReadings>(),
                TypeId::of::<ParkourInput>(),
                TypeId::of::<ParkourController>(),
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
        // Without physics the probe/controller systems are skipped, but the
        // rest of the fixed step still runs.
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
    fn controller_requires_all_parkour_components() {
        let mut world = World::new();
        let entity = world.spawn(ParkourController::default()).id();
        let e = world.entity(entity);
        assert!(e.contains::<Transform>());
        assert!(e.contains::<KinematicVelocity>());
        assert!(e.contains::<ParkourState>());
        assert!(e.contains::<LocomotionBuffer>());
        assert!(e.contains::<FlowState>());
        assert!(e.contains::<ParkourSensorReadings>());
        assert!(e.contains::<ParkourInput>());
    }

    #[test]
    fn plugin_coexists_with_core_plugin_added_first() {
        let mut app = App::new();
        app.add_plugins((CoreGameplayPlugin, ParkourPlugin));
        app.world_mut().run_schedule(FixedUpdate);
    }
}
