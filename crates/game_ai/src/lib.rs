//! Enemy AI: behavior trees, perception, six archetypes and the Gorgon boss.

pub mod actions;
pub mod archetypes;
pub mod behavior_tree;
pub mod boss_gorgon;
pub mod perception;

pub use actions::{AiAction, AiIntent};
pub use archetypes::{
    spawn_cyber_ninja, spawn_enemy, spawn_mecha_enforcer, spawn_netrunner, spawn_shielded_heavy,
    spawn_street_grunt, spawn_swarm_drone, DroneDetonationEvent, Enemy, EnemyArchetype, EnemyBrain,
    EnemyDefeatedEvent,
};
pub use behavior_tree::{BehaviorNode, BehaviorStatus, BehaviorTree};
pub use boss_gorgon::{
    spawn_boss_gorgon, BossDefeatedEvent, BossGorgon, BossPhaseTransitionEvent, HazardHitEvent,
};
pub use perception::{AiBlackboard, AiPerception, AlarmRaisedEvent, AlertState};

use archetypes::{
    apply_ai_intents, despawn_defeated_enemies, detonate_charging_drones, engage_attackers,
    run_behavior_trees, sync_enemy_posture, update_drone_flight,
};
use avian3d::prelude::SpatialQueryPipeline;
use bevy::prelude::*;
use boss_gorgon::{
    apply_damage_to_gorgon, sync_gorgon_posture, update_boss_hazards, update_gorgon_boss_fsm,
};
use game_combat::hitboxes::update_melee_combat_simulation;
use game_combat::projectiles::{apply_ranged_impacts, update_status_effects};
use game_combat::CombatPlugin;
use game_core::CoreSimulationSet;
use game_parkour::shapecasts::execute_parkour_spatial_probes;
use game_parkour::ParkourPlugin;
use game_rpg::attributes::apply_combat_damage;
use game_rpg::RpgPlugin;
use perception::{propagate_alarms, update_ai_perception};

/// Enemy perception, decision making, boss encounter and damage intake.
///
/// Each fixed tick: sense and decide in the Input set (after the parkour
/// probes, so trees see this tick's surroundings), turning decisions into
/// player-equivalent inputs; then, after combat resolves its hits in
/// Simulate, apply damage and remove the defeated. Adds [`ParkourPlugin`] and
/// [`CombatPlugin`] and `RpgPlugin` when missing; sensing needs Avian
/// (`PhysicsPlugins`). Damage itself goes through the RPG health pipeline.
pub struct GameAiPlugin;

impl Plugin for GameAiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ParkourPlugin>() {
            app.add_plugins(ParkourPlugin);
        }
        if !app.is_plugin_added::<CombatPlugin>() {
            app.add_plugins(CombatPlugin);
        }
        if !app.is_plugin_added::<RpgPlugin>() {
            app.add_plugins(RpgPlugin);
        }
        let has_physics = resource_exists::<SpatialQueryPipeline>;
        app.register_type::<Enemy>()
            .register_type::<EnemyArchetype>()
            .register_type::<AiPerception>()
            .register_type::<AiBlackboard>()
            .register_type::<AiIntent>()
            .register_type::<AlertState>()
            .register_type::<BossGorgon>()
            .register_type::<archetypes::DroneFlight>()
            .add_event::<AlarmRaisedEvent>()
            .add_event::<EnemyDefeatedEvent>()
            .add_event::<DroneDetonationEvent>()
            .add_event::<BossPhaseTransitionEvent>()
            .add_event::<BossDefeatedEvent>()
            .add_event::<HazardHitEvent>()
            .add_systems(
                FixedUpdate,
                (
                    (
                        update_ai_perception.run_if(has_physics),
                        propagate_alarms,
                        run_behavior_trees.run_if(has_physics),
                        apply_ai_intents,
                    )
                        .chain()
                        .after(execute_parkour_spatial_probes)
                        .in_set(CoreSimulationSet::Input),
                    (
                        update_drone_flight,
                        update_gorgon_boss_fsm,
                        update_boss_hazards,
                    )
                        .in_set(CoreSimulationSet::Simulate),
                    (
                        engage_attackers,
                        apply_damage_to_gorgon,
                        sync_enemy_posture,
                        sync_gorgon_posture,
                        detonate_charging_drones,
                        despawn_defeated_enemies,
                    )
                        .chain()
                        .after(apply_combat_damage)
                        .after(update_melee_combat_simulation)
                        .after(apply_ranged_impacts)
                        .after(update_status_effects)
                        .in_set(CoreSimulationSet::Simulate),
                ),
            );
    }
}
