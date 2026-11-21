//! Melee and ranged combat systems.

pub mod combat_dummy;
pub mod combo_trees;
pub mod hitboxes;
pub mod melee;

pub use combat_dummy::CombatDummyPlugin;
pub use hitboxes::MeleeHitEvent;
pub use melee::{
    AttackFrameData, AttackPhase, MeleeCombatState, MeleeInput, MeleeWeaponClass, ParryResult,
};

use avian3d::prelude::SpatialQueryPipeline;
use bevy::prelude::*;
use game_core::{CoreGameplayPlugin, CoreSimulationSet};
use game_parkour::controller::update_parkour_controller;
use hitboxes::{apply_melee_input, update_melee_combat_simulation};

/// Melee simulation: input, frame data, hitbox sweeps, guard resolution.
///
/// Runs each fixed tick after the parkour controller, so parkour-chained
/// attacks see this tick's locomotion state. Hit detection needs Avian's
/// spatial query pipeline (`PhysicsPlugins`). Works with or without
/// [`CombatDummyPlugin`]; dummies present in the world are valid targets.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CoreGameplayPlugin>() {
            app.add_plugins(CoreGameplayPlugin);
        }
        app.register_type::<MeleeCombatState>()
            .register_type::<MeleeInput>()
            .register_type::<MeleeWeaponClass>()
            .register_type::<AttackPhase>()
            .register_type::<ParryResult>()
            .add_event::<MeleeHitEvent>()
            .add_systems(
                FixedUpdate,
                (
                    apply_melee_input.in_set(CoreSimulationSet::Input),
                    update_melee_combat_simulation
                        .in_set(CoreSimulationSet::Simulate)
                        .after(update_parkour_controller)
                        .run_if(resource_exists::<SpatialQueryPipeline>),
                ),
            );
    }
}
