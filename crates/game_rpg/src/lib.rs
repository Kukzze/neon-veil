//! RPG progression: skills, the OSRS XP curve, vitals and cyber-mods.

pub mod attributes;
pub mod mod_catalog;
pub mod mod_system;
pub mod skills;
pub mod xp_table;

pub use attributes::{
    resolve_damage, CharacterAttributes, DamageEvent, DamageOutcome, DeathEvent, Health, Shields,
};
pub use mod_catalog::{find_mod, find_mod_by_name, MOD_CATALOG};
pub use mod_system::{CyberModCard, ModLoadout, ModLoadoutError, ModPolarity};
pub use skills::{CharacterSkills, LevelUpEvent, SkillType, XpGrantEvent};

use attributes::{apply_combat_damage, recompute_character_attributes, update_player_shields};
use bevy::prelude::*;
use game_combat::hitboxes::update_melee_combat_simulation;
use game_combat::projectiles::{apply_ranged_impacts, update_status_effects};
use game_combat::CombatPlugin;
use game_core::CoreSimulationSet;
use skills::apply_xp_grants;

/// Skills, attribute derivation, damage intake and shield recharge in the
/// fixed tick. Damage is applied after combat resolves its hits. Adds
/// [`CombatPlugin`] when missing (its hit events feed the damage pipeline).
pub struct RpgPlugin;

impl Plugin for RpgPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CombatPlugin>() {
            app.add_plugins(CombatPlugin);
        }
        app.register_type::<CharacterSkills>()
            .register_type::<SkillType>()
            .register_type::<Health>()
            .register_type::<Shields>()
            .register_type::<CharacterAttributes>()
            .register_type::<ModPolarity>()
            .add_event::<XpGrantEvent>()
            .add_event::<LevelUpEvent>()
            .add_event::<DamageEvent>()
            .add_event::<DeathEvent>()
            .add_systems(
                FixedUpdate,
                (
                    apply_xp_grants,
                    recompute_character_attributes,
                    apply_combat_damage
                        .after(update_melee_combat_simulation)
                        .after(apply_ranged_impacts)
                        .after(update_status_effects),
                    update_player_shields,
                )
                    .chain()
                    .in_set(CoreSimulationSet::Simulate),
            );
    }
}
