//! Ranged weapon archetypes: ammo, fire rate, spread bloom and reloads.
//!
//! [`BallisticProfile`] is both the static tuning (from the combat spec) and
//! the live weapon state. Fire timing carries the remainder between shots
//! while the trigger is held, so the average rate matches the authored RPM
//! exactly at the 64 Hz tick (resetting the cooldown each shot would round
//! every interval up to whole ticks: 950 RPM would drop to 768).

use crate::recoil::hash_unit;
use crate::status_effects::StatusEffectType;
use bevy::prelude::*;
use game_core::math::sanitize_f32;
use serde::{Deserialize, Serialize};

/// Firearm families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Debug, PartialEq, Hash)]
pub enum RangedWeaponArchetype {
    KineticPistol,
    SmartSubmachineGun,
    PulseAssaultRifle,
    PlasmaShotgun,
    ChargeRailgun,
    TechSniperRifle,
}

/// Weapon tuning plus live state.
#[derive(Component, Reflect, Debug, Clone, PartialEq)]
#[reflect(Component, Debug)]
#[require(crate::projectiles::RangedInput)]
pub struct BallisticProfile {
    pub archetype: RangedWeaponArchetype,
    pub magazine_capacity: u32,
    pub current_ammo: u32,
    pub reserve_ammo: u32,
    pub rounds_per_minute: u32,
    /// 0.0 = hitscan; otherwise projectile speed in m/s.
    pub projectile_velocity_mps: f32,
    /// Damage per trigger pull, split evenly across pellets.
    pub base_damage: f32,
    pub headshot_multiplier: f32,
    pub base_spread_degrees: f32,
    pub max_spread_degrees: f32,
    pub spread_bloom_per_shot: f32,
    /// Spread recovered per second, degrees.
    pub spread_recovery_rate: f32,
    pub recoil_pitch_degrees: f32,
    pub recoil_yaw_random: f32,
    pub tactical_reload_seconds: f32,
    pub empty_reload_seconds: f32,
    pub status_effect_affinity: Option<StatusEffectType>,
    pub pellets_per_shot: u32,
    /// Fraction of 9.8 m/s^2 applied to projectiles.
    pub projectile_gravity_scale: f32,
    // --- live state ---
    pub current_spread_degrees: f32,
    /// Seconds until the next shot may fire (may go negative while the
    /// trigger is held, carrying the remainder).
    pub fire_cooldown: f32,
    pub reload_timer: f32,
    pub is_reloading: bool,
    /// Shots fired so far; seeds deterministic spread and recoil.
    pub shots_fired: u32,
}

/// One trigger pull.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotEvent {
    pub archetype: RangedWeaponArchetype,
    pub shot_index: u32,
    /// Spread cone half-angle at the moment of firing, degrees.
    pub spread_degrees: f32,
    /// Per-pellet deviation from the aim, `(yaw, pitch)` in degrees.
    pub pellet_deviations: Vec<Vec2>,
    pub damage_per_pellet: f32,
    pub headshot_multiplier: f32,
    pub projectile_velocity_mps: f32,
    pub projectile_gravity_scale: f32,
    pub status_effect: Option<StatusEffectType>,
    pub recoil_pitch_degrees: f32,
    pub recoil_yaw_random: f32,
}

#[allow(clippy::too_many_arguments)]
const fn profile(
    archetype: RangedWeaponArchetype,
    magazine: u32,
    reserve: u32,
    rpm: u32,
    velocity: f32,
    damage: f32,
    headshot: f32,
    spread: [f32; 4],
    recoil: [f32; 2],
    reloads: [f32; 2],
    status: Option<StatusEffectType>,
    pellets: u32,
    gravity_scale: f32,
) -> BallisticProfile {
    BallisticProfile {
        archetype,
        magazine_capacity: magazine,
        current_ammo: magazine,
        reserve_ammo: reserve,
        rounds_per_minute: rpm,
        projectile_velocity_mps: velocity,
        base_damage: damage,
        headshot_multiplier: headshot,
        base_spread_degrees: spread[0],
        max_spread_degrees: spread[1],
        spread_bloom_per_shot: spread[2],
        spread_recovery_rate: spread[3],
        recoil_pitch_degrees: recoil[0],
        recoil_yaw_random: recoil[1],
        tactical_reload_seconds: reloads[0],
        empty_reload_seconds: reloads[1],
        status_effect_affinity: status,
        pellets_per_shot: pellets,
        projectile_gravity_scale: gravity_scale,
        current_spread_degrees: spread[0],
        fire_cooldown: 0.0,
        reload_timer: 0.0,
        is_reloading: false,
        shots_fired: 0,
    }
}

use RangedWeaponArchetype as A;
use StatusEffectType as S;

/// The six archetypes, tuned per the combat spec.
pub static RANGED_WEAPON_CONFIGS: [BallisticProfile; 6] = [
    profile(
        A::KineticPistol,
        12,
        72,
        420,
        0.0,
        38.0,
        2.2,
        [0.4, 2.2, 0.35, 6.0],
        [1.4, 0.3],
        [1.4, 1.8],
        None,
        1,
        0.0,
    ),
    profile(
        A::SmartSubmachineGun,
        40,
        240,
        950,
        450.0,
        18.0,
        1.75,
        [1.2, 4.5, 0.15, 8.5],
        [0.6, 0.45],
        [1.6, 2.2],
        Some(S::ShockEmp),
        1,
        0.2,
    ),
    profile(
        A::PulseAssaultRifle,
        30,
        180,
        680,
        0.0,
        32.0,
        2.0,
        [0.3, 3.2, 0.25, 7.0],
        [1.1, 0.25],
        [1.8, 2.5],
        Some(S::ThermalBurn),
        1,
        0.0,
    ),
    profile(
        A::PlasmaShotgun,
        8,
        48,
        90,
        320.0,
        140.0,
        1.5,
        [4.8, 7.5, 1.2, 4.5],
        [4.2, 1.0],
        [2.4, 3.2],
        Some(S::CorrosiveAcid),
        10,
        0.35,
    ),
    profile(
        A::ChargeRailgun,
        4,
        24,
        45,
        1800.0,
        285.0,
        3.0,
        [0.05, 0.5, 0.4, 2.0],
        [6.5, 0.2],
        [3.0, 3.8],
        Some(S::ShockEmp),
        1,
        0.0,
    ),
    profile(
        A::TechSniperRifle,
        5,
        30,
        60,
        0.0,
        220.0,
        3.5,
        [0.02, 1.8, 0.8, 3.5],
        [5.2, 0.15],
        [2.6, 3.4],
        Some(S::CryoGlitch),
        1,
        0.0,
    ),
];

impl BallisticProfile {
    pub fn for_archetype(archetype: RangedWeaponArchetype) -> Self {
        RANGED_WEAPON_CONFIGS
            .iter()
            .find(|p| p.archetype == archetype)
            .cloned()
            .unwrap_or_else(|| RANGED_WEAPON_CONFIGS[0].clone())
    }

    pub fn kinetic_pistol() -> Self {
        Self::for_archetype(A::KineticPistol)
    }
    pub fn smart_submachine_gun() -> Self {
        Self::for_archetype(A::SmartSubmachineGun)
    }
    pub fn pulse_assault_rifle() -> Self {
        Self::for_archetype(A::PulseAssaultRifle)
    }
    pub fn plasma_shotgun() -> Self {
        Self::for_archetype(A::PlasmaShotgun)
    }
    pub fn charge_railgun() -> Self {
        Self::for_archetype(A::ChargeRailgun)
    }
    pub fn tech_sniper_rifle() -> Self {
        Self::for_archetype(A::TechSniperRifle)
    }

    pub fn is_hitscan(&self) -> bool {
        self.projectile_velocity_mps <= 0.0
    }

    /// Seconds between shots.
    pub fn fire_interval(&self) -> f32 {
        60.0 / self.rounds_per_minute.max(1) as f32
    }

    pub fn can_fire(&self) -> bool {
        self.current_ammo > 0 && !self.is_reloading && self.fire_cooldown <= 0.0
    }

    /// Fires one round: spends ammo, blooms spread (capped at max) and
    /// starts the fire interval. Pellet deviations are drawn uniformly over
    /// the spread cone from a seed derived from `shots_fired`.
    pub fn fire(&mut self) -> Option<ShotEvent> {
        if !self.can_fire() {
            return None;
        }
        self.current_ammo -= 1;
        let spread = self.current_spread_degrees;
        let shot_index = self.shots_fired;
        self.shots_fired = self.shots_fired.wrapping_add(1);
        self.current_spread_degrees =
            (spread + self.spread_bloom_per_shot).min(self.max_spread_degrees);
        self.fire_cooldown += self.fire_interval();

        let pellets = self.pellets_per_shot.max(1);
        let pellet_deviations = (0..pellets)
            .map(|pellet| spread_deviation(spread, shot_index, pellet))
            .collect();
        Some(ShotEvent {
            archetype: self.archetype,
            shot_index,
            spread_degrees: spread,
            pellet_deviations,
            damage_per_pellet: self.base_damage / pellets as f32,
            headshot_multiplier: self.headshot_multiplier,
            projectile_velocity_mps: self.projectile_velocity_mps,
            projectile_gravity_scale: self.projectile_gravity_scale,
            status_effect: self.status_effect_affinity,
            recoil_pitch_degrees: self.recoil_pitch_degrees,
            recoil_yaw_random: self.recoil_yaw_random,
        })
    }

    /// Starts reloading: the empty reload when the magazine is dry, the
    /// tactical reload otherwise. Fails if already reloading, full or out of
    /// reserve ammo.
    pub fn start_reload(&mut self) -> bool {
        if self.is_reloading
            || self.current_ammo >= self.magazine_capacity
            || self.reserve_ammo == 0
        {
            return false;
        }
        self.is_reloading = true;
        self.reload_timer = if self.current_ammo == 0 {
            self.empty_reload_seconds
        } else {
            self.tactical_reload_seconds
        };
        true
    }

    /// Moves rounds from reserve into the magazine.
    pub fn finish_reload(&mut self) {
        let needed = self.magazine_capacity.saturating_sub(self.current_ammo);
        let moved = needed.min(self.reserve_ammo);
        self.current_ammo += moved;
        self.reserve_ammo -= moved;
        self.is_reloading = false;
        self.reload_timer = 0.0;
    }

    /// Decays spread toward the base value at `spread_recovery_rate` deg/s.
    pub fn update_spread(&mut self, dt: f32) {
        let dt = sanitize_f32(dt).max(0.0);
        self.current_spread_degrees = (self.current_spread_degrees
            - self.spread_recovery_rate * dt)
            .max(self.base_spread_degrees);
    }

    /// Advances cooldown, reload and spread. While the trigger is held the
    /// cooldown may go below zero so the next shot keeps the exact rhythm;
    /// otherwise it rests at zero (no stored burst).
    pub fn tick(&mut self, dt: f32, trigger_held: bool) {
        let dt = sanitize_f32(dt).max(0.0);
        self.fire_cooldown -= dt;
        if !trigger_held {
            self.fire_cooldown = self.fire_cooldown.max(0.0);
        } else {
            // Never bank more than one interval of catch-up.
            self.fire_cooldown = self.fire_cooldown.max(-self.fire_interval());
        }
        if self.is_reloading {
            self.reload_timer -= dt;
            if self.reload_timer <= 0.0 {
                self.finish_reload();
            }
        }
        self.update_spread(dt);
    }
}

/// Uniform random deviation inside a cone of half-angle `spread_degrees`.
pub fn spread_deviation(spread_degrees: f32, shot_index: u32, pellet: u32) -> Vec2 {
    let seed = shot_index.wrapping_mul(0x9E37_79B1) ^ pellet.wrapping_mul(0x85EB_CA77);
    let radius = spread_degrees.max(0.0) * hash_unit(seed).sqrt();
    let angle = std::f32::consts::TAU * hash_unit(seed ^ 0x27D4_EB2F);
    Vec2::new(radius * angle.cos(), radius * angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    #[test]
    fn all_six_profiles_match_spec() {
        let expected = [
            (A::KineticPistol, 420, 0.0, 38.0, 2.2, 12, 72, None, 1.4),
            (
                A::SmartSubmachineGun,
                950,
                450.0,
                18.0,
                1.75,
                40,
                240,
                Some(S::ShockEmp),
                0.6,
            ),
            (
                A::PulseAssaultRifle,
                680,
                0.0,
                32.0,
                2.0,
                30,
                180,
                Some(S::ThermalBurn),
                1.1,
            ),
            (
                A::PlasmaShotgun,
                90,
                320.0,
                140.0,
                1.5,
                8,
                48,
                Some(S::CorrosiveAcid),
                4.2,
            ),
            (
                A::ChargeRailgun,
                45,
                1800.0,
                285.0,
                3.0,
                4,
                24,
                Some(S::ShockEmp),
                6.5,
            ),
            (
                A::TechSniperRifle,
                60,
                0.0,
                220.0,
                3.5,
                5,
                30,
                Some(S::CryoGlitch),
                5.2,
            ),
        ];
        assert_eq!(RANGED_WEAPON_CONFIGS.len(), 6);
        for (archetype, rpm, velocity, damage, headshot, mag, reserve, status, recoil) in expected {
            let p = BallisticProfile::for_archetype(archetype);
            assert_eq!(p.archetype, archetype);
            assert_eq!(p.rounds_per_minute, rpm, "{archetype:?}");
            assert_eq!(p.projectile_velocity_mps, velocity, "{archetype:?}");
            assert_eq!(p.is_hitscan(), velocity == 0.0);
            assert_eq!(
                (p.base_damage, p.headshot_multiplier),
                (damage, headshot),
                "{archetype:?}"
            );
            assert_eq!(
                (p.magazine_capacity, p.current_ammo, p.reserve_ammo),
                (mag, mag, reserve)
            );
            assert_eq!(p.status_effect_affinity, status, "{archetype:?}");
            assert_eq!(p.recoil_pitch_degrees, recoil);
            assert!(p.base_spread_degrees < p.max_spread_degrees);
            assert!(p.tactical_reload_seconds < p.empty_reload_seconds);
            assert_eq!(p.current_spread_degrees, p.base_spread_degrees);
        }
        let shotgun = BallisticProfile::plasma_shotgun();
        assert_eq!(shotgun.pellets_per_shot, 10);
        assert_eq!(
            (shotgun.base_spread_degrees, shotgun.max_spread_degrees),
            (4.8, 7.5)
        );
        let rail = BallisticProfile::charge_railgun();
        assert_eq!(
            (rail.base_spread_degrees, rail.max_spread_degrees),
            (0.05, 0.5)
        );
        let sniper = BallisticProfile::tech_sniper_rifle();
        assert_eq!(
            (sniper.base_spread_degrees, sniper.max_spread_degrees),
            (0.02, 1.8)
        );
        assert_eq!(BallisticProfile::kinetic_pistol().max_spread_degrees, 2.2);
    }

    #[test]
    fn firing_consumes_rounds_and_respects_the_interval() {
        let mut p = BallisticProfile::kinetic_pistol();
        assert!(p.fire().is_some());
        assert_eq!(p.current_ammo, 11);
        assert!(!p.can_fire(), "interval not elapsed");
        assert!(p.fire().is_none());
        p.tick(p.fire_interval(), true);
        assert!(p.fire().is_some());
        assert_eq!(p.current_ammo, 10);
    }

    #[test]
    fn held_trigger_matches_authored_rpm_at_64_hz() {
        for archetype in [
            A::SmartSubmachineGun,
            A::PulseAssaultRifle,
            A::KineticPistol,
        ] {
            let mut p = BallisticProfile::for_archetype(archetype);
            p.current_ammo = 10_000;
            p.magazine_capacity = 10_000;
            let mut shots = 0;
            for _ in 0..(64 * 10) {
                p.tick(DT, true);
                if p.fire().is_some() {
                    shots += 1;
                }
            }
            let expected = p.rounds_per_minute as f32 / 6.0; // shots in 10 s
            assert!(
                (shots as f32 - expected).abs() <= 1.0,
                "{archetype:?}: {shots} shots in 10 s, expected {expected}"
            );
        }
    }

    #[test]
    fn released_trigger_does_not_bank_shots() {
        let mut p = BallisticProfile::smart_submachine_gun();
        p.fire();
        for _ in 0..64 {
            p.tick(DT, false);
        }
        assert_eq!(p.fire_cooldown, 0.0);
        assert!(p.fire().is_some());
        assert!(p.fire().is_none(), "one shot per interval after a pause");
    }

    #[test]
    fn empty_magazine_cannot_fire() {
        let mut p = BallisticProfile::charge_railgun();
        for _ in 0..4 {
            p.fire_cooldown = 0.0;
            assert!(p.fire().is_some());
        }
        p.fire_cooldown = 0.0;
        assert!(!p.can_fire());
        assert!(p.fire().is_none());
    }

    #[test]
    fn tactical_versus_empty_reload() {
        let mut p = BallisticProfile::pulse_assault_rifle();
        p.fire();
        assert!(p.start_reload());
        assert_eq!(p.reload_timer, 1.8, "tactical");
        assert!(!p.can_fire(), "no firing while reloading");
        assert!(!p.start_reload(), "already reloading");
        let ticks = (1.8 / DT).ceil() as u32;
        for _ in 0..ticks {
            p.tick(DT, false);
        }
        assert!(!p.is_reloading);
        assert_eq!((p.current_ammo, p.reserve_ammo), (30, 179));

        let mut dry = BallisticProfile::pulse_assault_rifle();
        dry.current_ammo = 0;
        assert!(dry.start_reload());
        assert_eq!(dry.reload_timer, 2.5, "empty reload is longer");
        dry.finish_reload();
        assert_eq!((dry.current_ammo, dry.reserve_ammo), (30, 150));
    }

    #[test]
    fn reload_is_limited_by_reserve_and_refused_when_full() {
        let mut p = BallisticProfile::kinetic_pistol();
        assert!(!p.start_reload(), "magazine full");
        p.current_ammo = 2;
        p.reserve_ammo = 4;
        assert!(p.start_reload());
        p.finish_reload();
        assert_eq!((p.current_ammo, p.reserve_ammo), (6, 0));
        p.current_ammo = 0;
        assert!(!p.start_reload(), "no reserve");
    }

    #[test]
    fn spread_blooms_to_max_and_recovers_to_base() {
        let mut p = BallisticProfile::kinetic_pistol();
        for _ in 0..20 {
            p.fire_cooldown = 0.0;
            p.current_ammo = 12;
            p.fire();
        }
        assert_eq!(p.current_spread_degrees, 2.2, "capped at max");
        p.update_spread(0.1);
        assert!(
            (p.current_spread_degrees - 1.6).abs() < 1e-4,
            "6 deg/s recovery"
        );
        p.update_spread(10.0);
        assert_eq!(p.current_spread_degrees, 0.4, "settles at base");
    }

    #[test]
    fn pellets_stay_inside_the_cone_and_are_deterministic() {
        let mut a = BallisticProfile::plasma_shotgun();
        let mut b = BallisticProfile::plasma_shotgun();
        let shot_a = a.fire().expect("shot");
        let shot_b = b.fire().expect("shot");
        assert_eq!(shot_a, shot_b);
        assert_eq!(shot_a.pellet_deviations.len(), 10);
        assert!((shot_a.damage_per_pellet - 14.0).abs() < 1e-5);
        for d in &shot_a.pellet_deviations {
            assert!(d.length() <= 4.8 + 1e-4);
        }
        let distinct: std::collections::HashSet<_> = shot_a
            .pellet_deviations
            .iter()
            .map(|d| (d.x.to_bits(), d.y.to_bits()))
            .collect();
        assert_eq!(distinct.len(), 10, "pellets spread apart");
    }
}
