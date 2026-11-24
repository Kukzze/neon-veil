//! Health, shields and attributes derived from skills and mods.
//!
//! Damage flows through one pipeline for players and enemies: shields
//! absorb raw damage first, then armor (`DR = armor / (armor + 300)`)
//! reduces whatever bleeds through to health. Shields recharge after a delay
//! unless a Shock EMP is active.

use crate::mod_system::ModLoadout;
use crate::skills::{CharacterSkills, SkillType};
use bevy::prelude::*;
use game_combat::{ActiveStatusEffects, MeleeHitEvent, RangedHitEvent, StatusEffectTickEvent};
use game_core::math::sanitize_f32;
use game_parkour::FlowState;
use std::time::Duration;

pub const BASE_HEALTH: f32 = 100.0;
pub const BASE_SHIELDS: f32 = 100.0;
pub const BASE_SHIELD_RECHARGE_RATE: f32 = 25.0;
pub const SHIELD_RECHARGE_DELAY_SECONDS: f32 = 3.0;
/// Armor constant in `DR = armor / (armor + ARMOR_CONSTANT)`.
pub const ARMOR_CONSTANT: f32 = 300.0;
pub const BASE_STAMINA: f32 = 100.0;
pub const BASE_STAMINA_REGEN: f32 = 20.0;

#[inline]
fn amount(x: f32) -> f32 {
    sanitize_f32(x).max(0.0)
}

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self::new(BASE_HEALTH)
    }
}

impl Health {
    pub fn new(max: f32) -> Self {
        let max = amount(max).max(1.0);
        Self { current: max, max }
    }

    /// Removes up to `amount` health; returns the damage actually taken.
    pub fn take_damage(&mut self, damage: f32) -> f32 {
        let taken = amount(damage).min(self.current);
        self.current -= taken;
        taken
    }

    pub fn heal(&mut self, healing: f32) {
        if !self.is_dead() {
            self.current = (self.current + amount(healing)).min(self.max);
        }
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }

    pub fn fraction(&self) -> f32 {
        (self.current / self.max.max(1.0)).clamp(0.0, 1.0)
    }

    /// Changes the maximum. Raising it grants the extra health; lowering it
    /// clamps the current value.
    pub fn set_max(&mut self, max: f32) {
        let max = amount(max).max(1.0);
        if max > self.max && !self.is_dead() {
            self.current += max - self.max;
        }
        self.max = max;
        self.current = self.current.min(max);
    }
}

#[derive(Component, Reflect, Debug, Clone, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct Shields {
    pub current: f32,
    pub max: f32,
    /// Shield points restored per second once recharging.
    pub recharge_rate: f32,
    /// Restarted by every hit; recharge begins when it finishes.
    pub recharge_delay_timer: Timer,
}

impl Default for Shields {
    fn default() -> Self {
        let mut recharge_delay_timer =
            Timer::from_seconds(SHIELD_RECHARGE_DELAY_SECONDS, TimerMode::Once);
        recharge_delay_timer.tick(recharge_delay_timer.duration());
        Self {
            current: BASE_SHIELDS,
            max: BASE_SHIELDS,
            recharge_rate: BASE_SHIELD_RECHARGE_RATE,
            recharge_delay_timer,
        }
    }
}

impl Shields {
    /// Absorbs damage and returns the part that bleeds through to health.
    /// Any hit restarts the recharge delay.
    pub fn take_damage(&mut self, damage: f32) -> f32 {
        let damage = amount(damage);
        if damage <= 0.0 {
            return 0.0;
        }
        self.recharge_delay_timer.reset();
        let absorbed = damage.min(self.current);
        self.current -= absorbed;
        damage - absorbed
    }

    /// Advances the delay and recharges when it has elapsed and no EMP is
    /// active.
    pub fn tick(&mut self, dt: f32, emp_active: bool) {
        let dt = amount(dt);
        self.recharge_delay_timer.tick(Duration::from_secs_f32(dt));
        if self.recharge_delay_timer.finished() && !emp_active && self.current < self.max {
            self.current = (self.current + self.recharge_rate * dt).min(self.max);
        }
    }

    pub fn set_max(&mut self, max: f32) {
        self.max = amount(max);
        self.current = self.current.min(self.max);
    }
}

/// Stats derived from skills and installed mods.
#[derive(Component, Reflect, Debug, Clone, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct CharacterAttributes {
    pub max_health: f32,
    pub max_shields: f32,
    pub shield_recharge_rate: f32,
    pub armor_rating: f32,
    /// Armor permanently stripped by Corrosive Acid.
    pub armor_shred: f32,
    pub stamina_max: f32,
    pub stamina_regen: f32,
    pub sprint_speed_multiplier: f32,
    pub melee_damage_multiplier: f32,
    pub ballistics_recoil_multiplier: f32,
}

impl Default for CharacterAttributes {
    fn default() -> Self {
        Self::derive(&CharacterSkills::default(), None)
    }
}

impl CharacterAttributes {
    /// Attributes for `skills`, scaled by `mods`:
    /// - Endurance: +5 health, +2 shields and +3 armor per level past 1.
    /// - Agility: +1 stamina, +1 % stamina regen, +0.2 % sprint per level.
    /// - Cyber-Melee: +1 % melee damage per level.
    /// - Ballistics: -0.4 % recoil per level.
    pub fn derive(skills: &CharacterSkills, mods: Option<&ModLoadout>) -> Self {
        Self::derive_with(skills, mods, None)
    }

    /// Like [`Self::derive`], plus the passive tree's bonuses.
    pub fn derive_with(
        skills: &CharacterSkills,
        mods: Option<&ModLoadout>,
        passive: Option<&crate::passive_tree::PassiveBonuses>,
    ) -> Self {
        use crate::passive_tree::PassiveStat as P;
        let flat = |stat| passive.map_or(0.0, |p| p.get(stat));
        let pct = |stat| passive.map_or(1.0, |p| p.scale(stat));
        let levels = |skill| (skills.get_boosted_level(skill) as f32 - 1.0).max(0.0);
        let endurance = levels(SkillType::Endurance);
        let agility = levels(SkillType::Agility);
        let melee = levels(SkillType::CyberMelee);
        let ballistics = levels(SkillType::Ballistics);
        let m = |stat: &str| mods.map_or(1.0, |l| l.aggregate_stat_multiplier(stat));
        Self {
            max_health: (BASE_HEALTH + 5.0 * endurance + flat(P::MaxHealth))
                * m("max_health")
                * pct(P::MaxHealthPercent),
            max_shields: (BASE_SHIELDS + 2.0 * endurance + flat(P::MaxShields))
                * m("max_shields")
                * pct(P::MaxShieldsPercent),
            shield_recharge_rate: BASE_SHIELD_RECHARGE_RATE
                * m("shield_recharge_rate")
                * pct(P::ShieldRecharge),
            armor_rating: 3.0 * endurance * m("armor_rating"),
            armor_shred: 0.0,
            stamina_max: (BASE_STAMINA + agility + flat(P::Stamina)) * m("stamina_max"),
            stamina_regen: BASE_STAMINA_REGEN * (1.0 + 0.01 * agility) * m("stamina_efficiency"),
            sprint_speed_multiplier: (1.0 + 0.002 * agility) * m("sprint_speed"),
            melee_damage_multiplier: (1.0 + 0.01 * melee) * m("melee_damage") * pct(P::MeleeDamage),
            ballistics_recoil_multiplier: ((1.0 - 0.004 * ballistics) / m("recoil_reduction"))
                .max(0.05),
        }
    }

    pub fn effective_armor(&self) -> f32 {
        (self.armor_rating - self.armor_shred).max(0.0)
    }

    /// Damage reduction from armor, 0..1.
    pub fn damage_reduction(&self) -> f32 {
        let armor = self.effective_armor();
        armor / (armor + ARMOR_CONSTANT)
    }
}

/// How a hit was split between shields and health.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DamageOutcome {
    pub shield_damage: f32,
    pub health_damage: f32,
    pub killed: bool,
}

/// Runs `damage` through shields, then armor, then health.
pub fn resolve_damage(
    damage: f32,
    health: &mut Health,
    shields: Option<&mut Shields>,
    attributes: Option<&CharacterAttributes>,
) -> DamageOutcome {
    let damage = amount(damage);
    let was_alive = !health.is_dead();
    let (bleed, shield_damage) = match shields {
        Some(shields) => {
            let bleed = shields.take_damage(damage);
            (bleed, damage - bleed)
        }
        None => (damage, 0.0),
    };
    let reduction = attributes.map_or(0.0, CharacterAttributes::damage_reduction);
    let health_damage = health.take_damage(bleed * (1.0 - reduction));
    DamageOutcome {
        shield_damage,
        health_damage,
        killed: was_alive && health.is_dead(),
    }
}

/// Damage from any non-combat source (hazards, explosions, falls).
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct DamageEvent {
    pub target: Entity,
    pub source: Option<Entity>,
    pub amount: f32,
}

/// A character's health reached zero.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeathEvent {
    pub entity: Entity,
    pub killer: Option<Entity>,
}

/// Recomputes attributes when skills or mods change and pushes the results
/// into health, shields and stamina. Acid shred carries over.
pub fn recompute_character_attributes(
    mut characters: Query<
        (
            &CharacterSkills,
            Option<&ModLoadout>,
            &mut CharacterAttributes,
            Option<&mut Health>,
            Option<&mut Shields>,
            Option<&mut FlowState>,
            Option<&crate::passive_tree::PassiveBonuses>,
        ),
        Or<(
            Changed<CharacterSkills>,
            Changed<ModLoadout>,
            Changed<crate::passive_tree::PassiveBonuses>,
        )>,
    >,
) {
    for (skills, mods, mut attributes, health, shields, flow, passive) in &mut characters {
        let shred = attributes.armor_shred;
        *attributes = CharacterAttributes {
            armor_shred: shred,
            ..CharacterAttributes::derive_with(skills, mods, passive)
        };
        if let Some(mut health) = health {
            health.set_max(attributes.max_health);
        }
        if let Some(mut shields) = shields {
            shields.set_max(attributes.max_shields);
            shields.recharge_rate = attributes.shield_recharge_rate;
        }
        if let Some(mut flow) = flow {
            flow.stamina_max = attributes.stamina_max;
            flow.stamina_regen_rate = attributes.stamina_regen;
            flow.stamina_current = flow.stamina_current.min(flow.stamina_max);
        }
    }
}

type VitalsQuery<'a> = (
    &'a mut Health,
    Option<&'a mut Shields>,
    Option<&'a mut CharacterAttributes>,
);

/// Applies every damage source to characters with [`Health`].
#[allow(clippy::too_many_arguments)]
pub fn apply_combat_damage(
    mut melee_hits: EventReader<MeleeHitEvent>,
    mut ranged_hits: EventReader<RangedHitEvent>,
    mut status_ticks: EventReader<StatusEffectTickEvent>,
    mut damage_events: EventReader<DamageEvent>,
    mut vitals: Query<VitalsQuery>,
    scales: Query<&crate::passive_tree::MeleeDamageScale>,
    incoming: Query<&crate::passive_tree::IncomingDamageScale>,
    mut deaths: EventWriter<DeathEvent>,
) {
    let mut hits: Vec<(Entity, Option<Entity>, f32, f32)> = Vec::new();
    // Melee hits scale with the attacker's melee damage (skills, mods and
    // the passive tree).
    let melee: Vec<_> = melee_hits
        .read()
        .map(|h| (h.target, h.attacker, h.damage))
        .collect();
    for (target, attacker, damage) in melee {
        let scale = vitals
            .get(attacker)
            .ok()
            .and_then(|(_, _, attributes)| attributes.map(|a| a.melee_damage_multiplier))
            .unwrap_or(1.0)
            * scales.get(attacker).map_or(1.0, |s| s.0);
        hits.push((target, Some(attacker), damage * scale, 0.0));
    }
    hits.extend(
        ranged_hits
            .read()
            .map(|h| (h.target, Some(h.shooter), h.damage, 0.0)),
    );
    hits.extend(
        status_ticks
            .read()
            .map(|t| (t.target, None, t.tick.damage, t.tick.armor_shred)),
    );
    hits.extend(
        damage_events
            .read()
            .map(|d| (d.target, d.source, d.amount, 0.0)),
    );

    for (target, source, damage, armor_shred) in hits {
        let damage = damage * incoming.get(target).map_or(1.0, |s| s.0);
        let Ok((mut health, shields, attributes)) = vitals.get_mut(target) else {
            continue;
        };
        let mut attributes = attributes;
        if armor_shred > 0.0 {
            if let Some(attributes) = attributes.as_deref_mut() {
                attributes.armor_shred =
                    (attributes.armor_shred + armor_shred).min(attributes.armor_rating);
            }
        }
        if damage <= 0.0 {
            continue;
        }
        let outcome = resolve_damage(
            damage,
            &mut health,
            shields.map(Mut::into_inner),
            attributes.as_deref(),
        );
        if outcome.killed {
            deaths.send(DeathEvent {
                entity: target,
                killer: source,
            });
        }
    }
}

/// Recharges shields on the fixed clock; EMP blocks recharge.
pub fn update_player_shields(
    time: Res<Time<Fixed>>,
    mut shields: Query<(&mut Shields, Option<&ActiveStatusEffects>)>,
) {
    let dt = time.delta_secs();
    for (mut shield, effects) in &mut shields {
        let emp = effects.is_some_and(ActiveStatusEffects::is_emp_active);
        shield.tick(dt, emp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_catalog::find_mod_by_name;
    use crate::xp_table::level_to_xp;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    #[test]
    fn health_damage_heal_and_death() {
        let mut h = Health::new(100.0);
        assert_eq!(h.take_damage(30.0), 30.0);
        assert_eq!(h.take_damage(500.0), 70.0, "only what was left");
        assert!(h.is_dead());
        h.heal(50.0);
        assert!(h.is_dead(), "no healing the dead");
        let mut h = Health::new(100.0);
        h.take_damage(40.0);
        h.heal(100.0);
        assert_eq!(h.current, 100.0);
        assert_eq!(h.take_damage(f32::NAN), 0.0);
        assert_eq!(h.take_damage(-10.0), 0.0);
    }

    #[test]
    fn shields_absorb_before_health() {
        let mut h = Health::new(100.0);
        let mut s = Shields::default();
        let out = resolve_damage(60.0, &mut h, Some(&mut s), None);
        assert_eq!((out.shield_damage, out.health_damage), (60.0, 0.0));
        assert_eq!((s.current, h.current), (40.0, 100.0));
    }

    #[test]
    fn bleed_through_150_on_100_shields() {
        let mut h = Health::new(100.0);
        let mut s = Shields::default();
        let out = resolve_damage(150.0, &mut h, Some(&mut s), None);
        assert_eq!((out.shield_damage, out.health_damage), (100.0, 50.0));
        assert_eq!((s.current, h.current), (0.0, 50.0));
        assert!(!out.killed);
        let out = resolve_damage(80.0, &mut h, Some(&mut s), None);
        assert!(out.killed);
    }

    #[test]
    fn armor_reduces_only_bleed_through() {
        let attributes = CharacterAttributes {
            armor_rating: 300.0,
            ..Default::default()
        };
        assert_eq!(attributes.damage_reduction(), 0.5);
        let mut h = Health::new(200.0);
        let mut s = Shields::default();
        let out = resolve_damage(150.0, &mut h, Some(&mut s), Some(&attributes));
        assert_eq!((out.shield_damage, out.health_damage), (100.0, 25.0));
        let shredded = CharacterAttributes {
            armor_shred: 300.0,
            ..attributes
        };
        assert_eq!(shredded.damage_reduction(), 0.0, "acid strips all armor");
    }

    #[test]
    fn shields_recharge_after_3_second_delay() {
        let mut s = Shields::default();
        s.take_damage(100.0);
        for _ in 0..(64 * 3 - 1) {
            s.tick(DT, false);
        }
        assert_eq!(s.current, 0.0, "still waiting at 2.98 s");
        s.tick(DT, false);
        s.tick(DT, false);
        assert!(s.current > 0.0, "recharging after 3 s");
        for _ in 0..(64 * 4) {
            s.tick(DT, false);
        }
        assert_eq!(s.current, s.max, "25/s refills 100 in 4 s");
    }

    #[test]
    fn hits_restart_the_delay_and_emp_blocks_recharge() {
        let mut s = Shields::default();
        s.take_damage(50.0);
        for _ in 0..(64 * 2) {
            s.tick(DT, false);
        }
        s.take_damage(1.0);
        for _ in 0..(64 * 2) {
            s.tick(DT, false);
        }
        assert_eq!(s.current, 49.0, "second hit restarted the 3 s delay");
        for _ in 0..(64 * 5) {
            s.tick(DT, true);
        }
        assert_eq!(s.current, 49.0, "EMP blocks recharge");
    }

    #[test]
    fn endurance_and_mods_scale_attributes() {
        let base = CharacterAttributes::default();
        assert_eq!(
            (base.max_health, base.max_shields, base.armor_rating),
            (100.0, 100.0, 0.0)
        );

        let mut skills = CharacterSkills::default();
        skills.add_xp(SkillType::Endurance, level_to_xp(99));
        skills.add_xp(SkillType::Ballistics, level_to_xp(99));
        let maxed = CharacterAttributes::derive(&skills, None);
        assert_eq!(maxed.max_health, 100.0 + 5.0 * 98.0);
        assert_eq!(maxed.armor_rating, 294.0);
        assert!(maxed.damage_reduction() > 0.49 && maxed.damage_reduction() < 0.5);
        assert!((maxed.ballistics_recoil_multiplier - 0.608).abs() < 1e-4);

        let mut loadout = ModLoadout::default();
        loadout
            .try_install(0, find_mod_by_name("Vitality").expect("card").with_rank(2))
            .expect("fits");
        let modded = CharacterAttributes::derive(&CharacterSkills::default(), Some(&loadout));
        assert!((modded.max_health - 220.0).abs() < 1e-4, "+120 % health");
    }

    fn damage_app() -> App {
        let mut app = App::new();
        app.add_event::<MeleeHitEvent>()
            .add_event::<RangedHitEvent>()
            .add_event::<StatusEffectTickEvent>()
            .add_event::<DamageEvent>()
            .add_event::<DeathEvent>()
            .add_systems(
                Update,
                (recompute_character_attributes, apply_combat_damage).chain(),
            );
        app
    }

    #[test]
    fn combat_events_drain_shields_then_health_then_kill() {
        use game_combat::status_effects::{StatusEffectTick, StatusEffectType};
        let mut app = damage_app();
        let shooter = app.world_mut().spawn_empty().id();
        let target = app
            .world_mut()
            .spawn((
                Health::new(100.0),
                Shields::default(),
                CharacterAttributes::default(),
            ))
            .id();
        app.world_mut().send_event(RangedHitEvent {
            shooter,
            target,
            damage: 150.0,
            headshot: false,
            status_applied: None,
            deflected: false,
        });
        app.world_mut().send_event(StatusEffectTickEvent {
            target,
            tick: StatusEffectTick {
                effect_type: StatusEffectType::ThermalBurn,
                damage: 6.0,
                armor_shred: 0.0,
            },
        });
        app.update();
        let world = app.world();
        assert_eq!(world.get::<Shields>(target).map(|s| s.current), Some(0.0));
        assert_eq!(
            world.get::<Health>(target).map(|h| h.current),
            Some(44.0),
            "50 bleed + 6 burn"
        );

        app.world_mut().send_event(DamageEvent {
            target,
            source: Some(shooter),
            amount: 100.0,
        });
        app.update();
        let deaths: Vec<_> = app
            .world_mut()
            .resource_mut::<Events<DeathEvent>>()
            .drain()
            .collect();
        assert_eq!(
            deaths,
            vec![DeathEvent {
                entity: target,
                killer: Some(shooter)
            }]
        );
    }

    #[test]
    fn leveling_endurance_raises_health_and_shields_live() {
        let mut app = damage_app();
        let player = app
            .world_mut()
            .spawn((
                CharacterSkills::default(),
                CharacterAttributes::default(),
                Health::new(100.0),
                Shields::default(),
            ))
            .id();
        app.update();
        app.world_mut()
            .get_mut::<CharacterSkills>(player)
            .expect("skills")
            .add_xp(SkillType::Endurance, level_to_xp(50));
        app.update();
        let world = app.world();
        assert_eq!(
            world.get::<Health>(player).map(|h| (h.current, h.max)),
            Some((345.0, 345.0))
        );
        assert_eq!(world.get::<Shields>(player).map(|s| s.max), Some(198.0));
        assert_eq!(
            world
                .get::<CharacterAttributes>(player)
                .map(|a| a.armor_rating),
            Some(147.0)
        );
    }

    #[test]
    fn raising_max_health_grants_the_difference() {
        let mut h = Health::new(100.0);
        h.take_damage(40.0);
        h.set_max(150.0);
        assert_eq!((h.current, h.max), (110.0, 150.0));
        h.set_max(80.0);
        assert_eq!((h.current, h.max), (80.0, 80.0));
    }
}
