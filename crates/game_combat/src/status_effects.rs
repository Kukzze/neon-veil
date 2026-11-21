//! Cybernetic elemental status effects.
//!
//! Effects refresh their duration and gain a stack (up to
//! [`MAX_STATUS_STACKS`]) when re-applied. Periodic effects (Thermal Burn
//! damage, Corrosive Acid armor shred) tick every 0.5 s and scale with
//! stacks.

use bevy::prelude::*;
use game_core::math::sanitize_f32;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const MAX_STATUS_STACKS: u32 = 5;
pub const STATUS_TICK_SECONDS: f32 = 0.5;
/// Movement speed while Cryo Glitch is active (45 % slow).
pub const CRYO_SPEED_MULTIPLIER: f32 = 0.55;

/// Elemental effect families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Debug, PartialEq, Hash)]
pub enum StatusEffectType {
    /// Stuns cybernetic enemies, disables shield recharge.
    ShockEmp,
    /// Damage over time, melts armor rating.
    ThermalBurn,
    /// Slows movement speed by 45 %.
    CryoGlitch,
    /// Shreds physical armor durability permanently.
    CorrosiveAcid,
}

impl StatusEffectType {
    /// Default duration in seconds.
    pub const fn default_duration(self) -> f32 {
        match self {
            Self::ShockEmp => 3.0,
            Self::ThermalBurn => 4.0,
            Self::CryoGlitch => 3.5,
            Self::CorrosiveAcid => 6.0,
        }
    }

    /// Default potency: damage per tick per stack (burn), armor shred per
    /// tick per stack (acid), slow fraction (cryo), or 1.0 (EMP).
    pub const fn default_potency(self) -> f32 {
        match self {
            Self::ShockEmp => 1.0,
            Self::ThermalBurn => 6.0,
            Self::CryoGlitch => 1.0 - CRYO_SPEED_MULTIPLIER,
            Self::CorrosiveAcid => 2.0,
        }
    }

    pub const fn is_periodic(self) -> bool {
        matches!(self, Self::ThermalBurn | Self::CorrosiveAcid)
    }
}

/// One active effect.
#[derive(Debug, Clone, PartialEq, Reflect)]
#[reflect(Debug)]
pub struct StatusEffect {
    pub effect_type: StatusEffectType,
    pub duration_timer: Timer,
    /// Repeating 0.5 s timer driving periodic effects.
    pub tick_timer: Timer,
    pub potency: f32,
    pub stacks: u32,
}

impl StatusEffect {
    /// An effect with its default duration and potency.
    pub fn new(effect_type: StatusEffectType) -> Self {
        Self::with(
            effect_type,
            effect_type.default_duration(),
            effect_type.default_potency(),
        )
    }

    pub fn with(effect_type: StatusEffectType, duration_seconds: f32, potency: f32) -> Self {
        Self {
            effect_type,
            duration_timer: Timer::from_seconds(
                sanitize_f32(duration_seconds).max(0.0),
                TimerMode::Once,
            ),
            tick_timer: Timer::from_seconds(STATUS_TICK_SECONDS, TimerMode::Repeating),
            potency: sanitize_f32(potency).max(0.0),
            stacks: 1,
        }
    }
}

/// Result of one periodic tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusEffectTick {
    pub effect_type: StatusEffectType,
    /// Health damage (Thermal Burn).
    pub damage: f32,
    /// Armor durability removed (Corrosive Acid).
    pub armor_shred: f32,
}

/// A periodic tick landed on `target`.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct StatusEffectTickEvent {
    pub target: Entity,
    pub tick: StatusEffectTick,
}

/// Effects currently affecting an entity.
#[derive(Component, Reflect, Debug, Default, Clone, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct ActiveStatusEffects {
    pub effects: Vec<StatusEffect>,
}

impl ActiveStatusEffects {
    /// Adds `effect`, or refreshes the existing one of the same type: its
    /// duration restarts, potency takes the stronger value and it gains a
    /// stack (max [`MAX_STATUS_STACKS`]).
    pub fn apply_effect(&mut self, effect: StatusEffect) {
        if let Some(existing) = self
            .effects
            .iter_mut()
            .find(|e| e.effect_type == effect.effect_type)
        {
            existing.duration_timer = effect.duration_timer;
            existing.potency = existing.potency.max(effect.potency);
            existing.stacks = (existing.stacks + 1).min(MAX_STATUS_STACKS);
        } else {
            self.effects.push(effect);
        }
    }

    pub fn get(&self, effect_type: StatusEffectType) -> Option<&StatusEffect> {
        self.effects.iter().find(|e| e.effect_type == effect_type)
    }

    pub fn has(&self, effect_type: StatusEffectType) -> bool {
        self.get(effect_type).is_some()
    }

    /// 0.55 while Cryo Glitch is active, otherwise 1.0.
    pub fn speed_multiplier(&self) -> f32 {
        if self.has(StatusEffectType::CryoGlitch) {
            CRYO_SPEED_MULTIPLIER
        } else {
            1.0
        }
    }

    /// Shock EMP blocks shield recharge and stuns cybernetics.
    pub fn is_emp_active(&self) -> bool {
        self.has(StatusEffectType::ShockEmp)
    }

    /// Advances every effect, returning periodic ticks, and removes expired
    /// effects. A tick that lands exactly as the effect expires still counts.
    pub fn tick(&mut self, dt: f32) -> Vec<StatusEffectTick> {
        let delta = Duration::from_secs_f32(sanitize_f32(dt).max(0.0));
        let mut ticks = Vec::new();
        for effect in &mut self.effects {
            effect.duration_timer.tick(delta);
            if effect.effect_type.is_periodic() {
                effect.tick_timer.tick(delta);
                let per_tick = effect.potency * effect.stacks as f32;
                for _ in 0..effect.tick_timer.times_finished_this_tick() {
                    ticks.push(match effect.effect_type {
                        StatusEffectType::ThermalBurn => StatusEffectTick {
                            effect_type: effect.effect_type,
                            damage: per_tick,
                            armor_shred: 0.0,
                        },
                        _ => StatusEffectTick {
                            effect_type: effect.effect_type,
                            damage: 0.0,
                            armor_shred: per_tick,
                        },
                    });
                }
            }
        }
        self.effects.retain(|e| !e.duration_timer.finished());
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    fn run(effects: &mut ActiveStatusEffects, seconds: f32) -> Vec<StatusEffectTick> {
        let ticks = (seconds / DT).round() as u32;
        (0..ticks).flat_map(|_| effects.tick(DT)).collect()
    }

    #[test]
    fn cryo_glitch_slows_to_55_percent() {
        let mut e = ActiveStatusEffects::default();
        assert_eq!(e.speed_multiplier(), 1.0);
        e.apply_effect(StatusEffect::new(StatusEffectType::CryoGlitch));
        assert_eq!(e.speed_multiplier(), 0.55);
        e.apply_effect(StatusEffect::new(StatusEffectType::CryoGlitch));
        assert_eq!(e.speed_multiplier(), 0.55, "slow does not stack");
    }

    #[test]
    fn shock_emp_blocks_shield_recharge() {
        let mut e = ActiveStatusEffects::default();
        assert!(!e.is_emp_active());
        e.apply_effect(StatusEffect::new(StatusEffectType::ShockEmp));
        assert!(e.is_emp_active());
        run(&mut e, 3.1);
        assert!(!e.is_emp_active(), "wears off after 3 s");
    }

    #[test]
    fn thermal_burn_ticks_every_half_second() {
        let mut e = ActiveStatusEffects::default();
        e.apply_effect(StatusEffect::new(StatusEffectType::ThermalBurn));
        let ticks = run(&mut e, 4.0);
        assert_eq!(ticks.len(), 8, "4 s / 0.5 s");
        assert!(ticks
            .iter()
            .all(|t| t.damage == 6.0 && t.armor_shred == 0.0));
        assert!(e.effects.is_empty(), "expired and pruned");
    }

    #[test]
    fn corrosive_acid_shreds_armor() {
        let mut e = ActiveStatusEffects::default();
        e.apply_effect(StatusEffect::new(StatusEffectType::CorrosiveAcid));
        let ticks = run(&mut e, 1.0);
        assert_eq!(ticks.len(), 2);
        assert!(ticks
            .iter()
            .all(|t| t.armor_shred == 2.0 && t.damage == 0.0));
    }

    #[test]
    fn stacks_scale_damage_and_cap_at_five() {
        let mut e = ActiveStatusEffects::default();
        for _ in 0..8 {
            e.apply_effect(StatusEffect::new(StatusEffectType::ThermalBurn));
        }
        assert_eq!(e.effects.len(), 1);
        assert_eq!(
            e.get(StatusEffectType::ThermalBurn).map(|b| b.stacks),
            Some(5)
        );
        let ticks = run(&mut e, 0.5);
        assert_eq!(ticks[0].damage, 30.0);
    }

    #[test]
    fn reapplying_refreshes_duration() {
        let mut e = ActiveStatusEffects::default();
        e.apply_effect(StatusEffect::new(StatusEffectType::CryoGlitch));
        run(&mut e, 3.0);
        e.apply_effect(StatusEffect::new(StatusEffectType::CryoGlitch));
        run(&mut e, 3.0);
        assert!(
            e.has(StatusEffectType::CryoGlitch),
            "refreshed at 3 s, lasts until 6.5 s"
        );
        run(&mut e, 0.6);
        assert!(!e.has(StatusEffectType::CryoGlitch));
        assert_eq!(e.speed_multiplier(), 1.0);
    }

    #[test]
    fn independent_effects_coexist() {
        let mut e = ActiveStatusEffects::default();
        e.apply_effect(StatusEffect::new(StatusEffectType::ShockEmp));
        e.apply_effect(StatusEffect::new(StatusEffectType::CryoGlitch));
        assert!(e.is_emp_active() && e.speed_multiplier() == 0.55);
        assert_eq!(e.effects.len(), 2);
    }

    #[test]
    fn hostile_values_are_sanitized() {
        let effect = StatusEffect::with(StatusEffectType::ThermalBurn, f32::NAN, f32::INFINITY);
        assert_eq!(effect.potency, 0.0);
        let mut e = ActiveStatusEffects::default();
        e.apply_effect(effect);
        assert!(e.tick(f32::NAN).is_empty());
    }
}
