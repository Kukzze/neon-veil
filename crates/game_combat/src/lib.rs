//! Melee and ranged combat systems.

pub mod combat_dummy;
pub mod combo_trees;
pub mod hitboxes;
pub mod melee;
pub mod projectiles;
pub mod ranged;
pub mod recoil;
pub mod status_effects;

pub use combat_dummy::CombatDummyPlugin;
pub use hitboxes::MeleeHitEvent;
pub use melee::{
    AttackFrameData, AttackPhase, MeleeCombatState, MeleeInput, MeleeWeaponClass, ParryResult,
};
pub use projectiles::{Projectile, RangedHitEvent, RangedImpactEvent, RangedInput, ShotFiredEvent};
pub use ranged::{BallisticProfile, RangedWeaponArchetype};
pub use recoil::RecoilSpring;
pub use status_effects::{
    ActiveStatusEffects, StatusEffect, StatusEffectTickEvent, StatusEffectType,
};

use avian3d::prelude::SpatialQueryPipeline;
use bevy::prelude::*;
use game_core::{CoreGameplayPlugin, CoreSimulationSet};
use game_parkour::controller::update_parkour_controller;
use hitboxes::{apply_melee_input, update_melee_combat_simulation};
use projectiles::{
    apply_ranged_impacts, apply_ranged_input, simulate_projectiles, sync_status_speed_multipliers,
    update_ranged_weapons, update_status_effects,
};
use recoil::update_recoil_springs;

/// Melee and ranged simulation in the fixed tick.
///
/// Input systems run in the Input set; simulation runs in Simulate after the
/// parkour controller, so attacks see this tick's locomotion. Hit detection
/// needs Avian's spatial query pipeline (`PhysicsPlugins`). Works with or
/// without [`CombatDummyPlugin`]; dummies present in the world are targets.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CoreGameplayPlugin>() {
            app.add_plugins(CoreGameplayPlugin);
        }
        let has_physics = resource_exists::<SpatialQueryPipeline>;
        app.register_type::<MeleeCombatState>()
            .register_type::<MeleeInput>()
            .register_type::<MeleeWeaponClass>()
            .register_type::<AttackPhase>()
            .register_type::<ParryResult>()
            .register_type::<BallisticProfile>()
            .register_type::<RangedWeaponArchetype>()
            .register_type::<RangedInput>()
            .register_type::<Projectile>()
            .register_type::<RecoilSpring>()
            .register_type::<ActiveStatusEffects>()
            .register_type::<StatusEffectType>()
            .add_event::<MeleeHitEvent>()
            .add_event::<RangedImpactEvent>()
            .add_event::<RangedHitEvent>()
            .add_event::<ShotFiredEvent>()
            .add_event::<StatusEffectTickEvent>()
            .add_systems(
                FixedUpdate,
                (
                    (apply_melee_input, apply_ranged_input).in_set(CoreSimulationSet::Input),
                    update_melee_combat_simulation
                        .in_set(CoreSimulationSet::Simulate)
                        .after(update_parkour_controller)
                        .run_if(has_physics),
                    (
                        update_ranged_weapons.run_if(has_physics),
                        update_recoil_springs,
                        simulate_projectiles.run_if(has_physics),
                        apply_ranged_impacts,
                        update_status_effects,
                        sync_status_speed_multipliers,
                    )
                        .chain()
                        .in_set(CoreSimulationSet::Simulate)
                        .after(update_parkour_controller),
                ),
            );
    }
}
