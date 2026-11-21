//! Apex Enforcer Gorgon: a three-phase boss.
//!
//! - **Phase 1, Arena Suppression (100-66 % HP):** chaingun fire, missile
//!   salvos every 14 s and laser grid floor traps every 8 s.
//! - **Phase 2, Vertical Skyline Chase (66-33 % HP):** jump-jets up the
//!   shaft (the parkour controller is suspended via the externally driven
//!   `GrapplePull` state) while airstrike markers rain on the player.
//! - **Phase 3, Overclocked Cyber-Melee (33-0 % HP):** drops back down and
//!   chases with dual thermal claymores; the fourth hit of the combo is an
//!   unblockable red sweep that only a perfect parry stops.
//!
//! Breaking the boss's 500 posture stuns it and opens a finisher.

use crate::perception::{AiBlackboard, EYE_OFFSET};
use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::utils::HashSet;
use game_combat::projectiles::Projectile;
use game_combat::{
    BallisticProfile, MeleeCombatState, MeleeHitEvent, MeleeInput, MeleeWeaponClass,
    RangedHitEvent, RangedInput,
};
use game_core::identifiers::PlayerCharacterMarker;
use game_core::input::ViewAngles;
use game_parkour::shapecasts::CHARACTER_HALF_HEIGHT;
use game_parkour::{ParkourController, ParkourInput, ParkourState};
use std::time::Duration;

pub const GORGON_HEALTH: f32 = 25_000.0;
pub const GORGON_POSTURE: f32 = 500.0;
pub const PHASE_2_HEALTH_FRACTION: f32 = 0.66;
pub const PHASE_3_HEALTH_FRACTION: f32 = 0.33;
pub const ENRAGE_HEALTH_FRACTION: f32 = 0.10;
/// Enraged timers run this much faster.
pub const ENRAGE_TEMPO: f32 = 1.5;
pub const POSTURE_STUN_SECONDS: f32 = 4.0;
/// Finisher damage as a fraction of max health.
pub const FINISHER_DAMAGE_FRACTION: f32 = 0.12;
pub const ASCENT_HEIGHT: f32 = 30.0;
pub const ASCENT_SPEED: f32 = 6.0;
pub const AIRSTRIKE_INTERVAL_SECONDS: f32 = 4.0;
pub const AIRSTRIKE_FUSE_SECONDS: f32 = 1.5;
pub const AIRSTRIKE_RADIUS: f32 = 3.5;
pub const AIRSTRIKE_DAMAGE: f32 = 120.0;
pub const LASER_GRID_DURATION_SECONDS: f32 = 3.0;
pub const LASER_GRID_DAMAGE: f32 = 40.0;
pub const LASER_GRID_HALF_EXTENTS: Vec3 = Vec3::new(6.0, 0.3, 0.6);
pub const MISSILE_SALVO_COUNT: u32 = 6;
pub const MISSILE_DAMAGE: f32 = 30.0;
pub const CLAYMORE_REACH: f32 = 3.2;

/// Boss state.
#[derive(Component, Reflect, Debug, Clone, PartialEq)]
#[reflect(Component, Debug)]
pub struct BossGorgon {
    pub current_phase: u8,
    pub health_max: f32,
    pub health_current: f32,
    pub posture_max: f32,
    pub posture_current: f32,
    pub laser_grid_timer: Timer,
    pub missile_salvo_timer: Timer,
    pub airstrike_timer: Timer,
    pub is_enraged: bool,
    /// Remaining posture-break stun; the finisher window while positive.
    pub stun_timer: f32,
    /// Arena floor height, for the jump-jet ascent.
    pub arena_floor_y: f32,
}

impl Default for BossGorgon {
    fn default() -> Self {
        Self {
            current_phase: 1,
            health_max: GORGON_HEALTH,
            health_current: GORGON_HEALTH,
            posture_max: GORGON_POSTURE,
            posture_current: 0.0,
            laser_grid_timer: Timer::from_seconds(8.0, TimerMode::Repeating),
            missile_salvo_timer: Timer::from_seconds(14.0, TimerMode::Repeating),
            airstrike_timer: Timer::from_seconds(AIRSTRIKE_INTERVAL_SECONDS, TimerMode::Repeating),
            is_enraged: false,
            stun_timer: 0.0,
            arena_floor_y: 0.0,
        }
    }
}

/// Phase for a health fraction: 1 above 66 %, 2 above 33 %, else 3.
pub fn phase_for_health_fraction(fraction: f32) -> u8 {
    if fraction > PHASE_2_HEALTH_FRACTION {
        1
    } else if fraction > PHASE_3_HEALTH_FRACTION {
        2
    } else {
        3
    }
}

impl BossGorgon {
    pub fn health_fraction(&self) -> f32 {
        (self.health_current / self.health_max.max(1.0)).clamp(0.0, 1.0)
    }

    pub fn is_defeated(&self) -> bool {
        self.health_current <= 0.0
    }

    pub fn is_stunned(&self) -> bool {
        self.stun_timer > 0.0
    }

    /// A finisher can be performed while posture is broken.
    pub fn finisher_available(&self) -> bool {
        self.is_stunned() && !self.is_defeated()
    }

    /// Applies damage and returns every phase transition it caused, in
    /// order (a single huge hit from phase 1 reports 1->2 and 2->3).
    pub fn apply_damage(&mut self, damage: f32) -> Vec<(u8, u8)> {
        if !damage.is_finite() || damage <= 0.0 || self.is_defeated() {
            return Vec::new();
        }
        self.health_current = (self.health_current - damage).max(0.0);
        self.is_enraged = self.health_fraction() <= ENRAGE_HEALTH_FRACTION;
        let target = phase_for_health_fraction(self.health_fraction());
        let mut transitions = Vec::new();
        while self.current_phase < target {
            transitions.push((self.current_phase, self.current_phase + 1));
            self.current_phase += 1;
        }
        transitions
    }

    /// Adds posture damage; breaking it stuns. Returns whether it broke.
    pub fn add_posture(&mut self, amount: f32) -> bool {
        if self.is_stunned() || !amount.is_finite() || amount <= 0.0 {
            return false;
        }
        self.posture_current = (self.posture_current + amount).min(self.posture_max);
        if self.posture_current >= self.posture_max {
            self.stun_timer = POSTURE_STUN_SECONDS;
            return true;
        }
        false
    }

    /// Performs the finisher if available: heavy damage, ends the stun and
    /// resets posture. Returns the damage dealt and any phase transitions.
    pub fn execute_finisher(&mut self) -> Option<(f32, Vec<(u8, u8)>)> {
        if !self.finisher_available() {
            return None;
        }
        let damage = self.health_max * FINISHER_DAMAGE_FRACTION;
        self.stun_timer = 0.0;
        self.posture_current = 0.0;
        let transitions = self.apply_damage(damage);
        Some((damage, transitions))
    }

    /// Advances timers. Returns which phase-specific attacks are due.
    pub fn tick(&mut self, dt: f32) -> GorgonCues {
        let dt = dt.max(0.0);
        if self.is_stunned() {
            self.stun_timer = (self.stun_timer - dt).max(0.0);
            if !self.is_stunned() {
                self.posture_current = 0.0;
            }
            return GorgonCues::default();
        }
        let tempo = if self.is_enraged { ENRAGE_TEMPO } else { 1.0 };
        let delta = Duration::from_secs_f32(dt * tempo);
        match self.current_phase {
            1 => {
                self.laser_grid_timer.tick(delta);
                self.missile_salvo_timer.tick(delta);
                GorgonCues {
                    laser_grid: self.laser_grid_timer.just_finished(),
                    missile_salvo: self.missile_salvo_timer.just_finished(),
                    airstrike: false,
                }
            }
            2 => {
                self.airstrike_timer.tick(delta);
                GorgonCues {
                    airstrike: self.airstrike_timer.just_finished(),
                    ..Default::default()
                }
            }
            _ => GorgonCues::default(),
        }
    }
}

/// Attacks due this tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GorgonCues {
    pub laser_grid: bool,
    pub missile_salvo: bool,
    pub airstrike: bool,
}

/// The boss moved to a new phase.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BossPhaseTransitionEvent {
    pub boss: Entity,
    pub from_phase: u8,
    pub to_phase: u8,
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BossDefeatedEvent {
    pub boss: Entity,
}

/// A boss hazard (laser grid, airstrike) hurt a player.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct HazardHitEvent {
    pub hazard: Entity,
    pub target: Entity,
    pub damage: f32,
}

/// A floor-level laser grid that hurts players standing in it, once each.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct LaserGridTrap {
    pub half_extents: Vec3,
    pub remaining_seconds: f32,
    pub damage: f32,
    pub already_hit: HashSet<Entity>,
}

/// A marked airstrike that detonates after its fuse.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AirstrikeMarker {
    pub fuse_seconds: f32,
    pub radius: f32,
    pub damage: f32,
}

/// Dual thermal claymores, overclocked chaingun, jump-jets.
pub fn spawn_boss_gorgon(commands: &mut Commands, position: Vec3) -> Entity {
    let chaingun = BallisticProfile {
        rounds_per_minute: 900,
        magazine_capacity: 300,
        current_ammo: 300,
        reserve_ammo: 3000,
        base_damage: 18.0,
        status_effect_affinity: None,
        ..BallisticProfile::pulse_assault_rifle()
    };
    commands
        .spawn((
            Name::new("Apex Enforcer Gorgon"),
            BossGorgon {
                arena_floor_y: position.y - CHARACTER_HALF_HEIGHT,
                ..Default::default()
            },
            MeleeCombatState {
                weapon_class: MeleeWeaponClass::ThermalClaymores,
                posture_max: GORGON_POSTURE,
                ..Default::default()
            },
            chaingun,
            ParkourController::default(),
            ViewAngles::default(),
            AiBlackboard::default(),
            Transform::from_translation(position),
            RigidBody::Kinematic,
            Collider::capsule(0.4, 1.0),
        ))
        .id()
}

/// Applies hits to the boss and reports phase changes and defeat.
pub fn apply_damage_to_gorgon(
    mut commands: Commands,
    mut melee_hits: EventReader<MeleeHitEvent>,
    mut ranged_hits: EventReader<RangedHitEvent>,
    mut bosses: Query<&mut BossGorgon>,
    mut transitions: EventWriter<BossPhaseTransitionEvent>,
    mut defeated: EventWriter<BossDefeatedEvent>,
) {
    let hits = melee_hits
        .read()
        .map(|h| (h.target, h.damage))
        .chain(ranged_hits.read().map(|h| (h.target, h.damage)))
        .collect::<Vec<_>>();
    for (target, damage) in hits {
        let Ok(mut boss) = bosses.get_mut(target) else {
            continue;
        };
        let was_defeated = boss.is_defeated();
        for (from_phase, to_phase) in boss.apply_damage(damage) {
            transitions.send(BossPhaseTransitionEvent {
                boss: target,
                from_phase,
                to_phase,
            });
        }
        if boss.is_defeated() && !was_defeated {
            defeated.send(BossDefeatedEvent { boss: target });
            commands.entity(target).despawn_recursive();
        }
    }
}

/// Mirrors melee posture into the boss and stuns on a posture break.
pub fn sync_gorgon_posture(mut bosses: Query<(&mut BossGorgon, &mut MeleeCombatState)>) {
    for (mut boss, mut melee) in &mut bosses {
        if boss.is_stunned() {
            continue;
        }
        if melee.is_posture_broken {
            boss.posture_current = boss.posture_max;
            boss.stun_timer = POSTURE_STUN_SECONDS;
        } else {
            boss.posture_current = melee.posture_current;
        }
        melee.posture_max = boss.posture_max;
    }
}

type GorgonQuery<'a> = (
    Entity,
    &'a mut BossGorgon,
    &'a mut Transform,
    &'a mut ViewAngles,
    &'a mut ParkourInput,
    &'a mut ParkourState,
    &'a mut RangedInput,
    &'a mut MeleeInput,
);

/// Drives the boss through its phases.
pub fn update_gorgon_boss_fsm(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    players: Query<(Entity, &Transform), (With<PlayerCharacterMarker>, Without<BossGorgon>)>,
    mut bosses: Query<GorgonQuery>,
) {
    let dt = time.delta_secs();
    for (
        entity,
        mut boss,
        mut transform,
        mut view,
        mut parkour,
        mut state,
        mut ranged,
        mut melee,
    ) in &mut bosses
    {
        let cues = boss.tick(dt);
        ranged.trigger_held = false;
        parkour.move_direction = Vec2::ZERO;
        parkour.sprint = false;
        if boss.is_stunned() || boss.is_defeated() {
            continue;
        }
        let Some((_, target)) = players
            .iter()
            .map(|(e, t)| (e, t.translation))
            .min_by(|a, b| {
                a.1.distance_squared(transform.translation)
                    .total_cmp(&b.1.distance_squared(transform.translation))
            })
        else {
            continue;
        };
        let eye = transform.translation + Vec3::Y * EYE_OFFSET;
        *view = ViewAngles::looking_along(target + Vec3::Y * 0.4 - eye);
        parkour.yaw = view.yaw;

        match boss.current_phase {
            1 => {
                ranged.trigger_held = true;
                if cues.laser_grid {
                    for offset in [-3.0, 3.0] {
                        let center =
                            Vec3::new(target.x, boss.arena_floor_y + 0.3, target.z + offset);
                        commands.spawn((
                            Name::new("laser_grid"),
                            Transform::from_translation(center),
                            LaserGridTrap {
                                half_extents: LASER_GRID_HALF_EXTENTS,
                                remaining_seconds: LASER_GRID_DURATION_SECONDS,
                                damage: LASER_GRID_DAMAGE,
                                already_hit: HashSet::default(),
                            },
                        ));
                    }
                }
                if cues.missile_salvo {
                    let launch = eye + Vec3::Y;
                    for i in 0..MISSILE_SALVO_COUNT {
                        let angle = std::f32::consts::TAU * i as f32 / MISSILE_SALVO_COUNT as f32;
                        let aim = target + Vec3::new(angle.cos(), 0.0, angle.sin()) * 2.0;
                        commands.spawn((
                            Name::new("gorgon_missile"),
                            Transform::from_translation(launch),
                            Projectile {
                                velocity: (aim - launch).normalize_or_zero() * 50.0,
                                gravity_scale: 0.3,
                                lifetime: Timer::from_seconds(3.0, TimerMode::Once),
                                damage: MISSILE_DAMAGE,
                                headshot_multiplier: 1.0,
                                status_effect: None,
                                owner: entity,
                            },
                        ));
                    }
                }
            }
            2 => {
                // Jump-jets: the parkour controller holds while the boss climbs.
                *state = ParkourState::GrapplePull;
                let ceiling = boss.arena_floor_y + ASCENT_HEIGHT;
                transform.translation.y =
                    (transform.translation.y + ASCENT_SPEED * dt).min(ceiling);
                if cues.airstrike {
                    commands.spawn((
                        Name::new("airstrike_marker"),
                        Transform::from_translation(target),
                        AirstrikeMarker {
                            fuse_seconds: AIRSTRIKE_FUSE_SECONDS,
                            radius: AIRSTRIKE_RADIUS,
                            damage: AIRSTRIKE_DAMAGE,
                        },
                    ));
                }
            }
            _ => {
                if *state == ParkourState::GrapplePull {
                    // Cut the jets and drop back into the arena.
                    *state = ParkourState::JumpDescent;
                }
                let offset = target - transform.translation;
                let planar = Vec2::new(offset.x, offset.z);
                if planar.length() > CLAYMORE_REACH * 0.8 {
                    parkour.move_direction = planar.normalize_or_zero();
                    parkour.sprint = true;
                }
                if planar.length() <= CLAYMORE_REACH {
                    melee.light_pressed = true;
                }
            }
        }
    }
}

/// Ticks laser grids and airstrike markers and damages players they catch.
pub fn update_boss_hazards(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    players: Query<(Entity, &Transform), With<PlayerCharacterMarker>>,
    mut lasers: Query<(Entity, &Transform, &mut LaserGridTrap), Without<PlayerCharacterMarker>>,
    mut airstrikes: Query<
        (Entity, &Transform, &mut AirstrikeMarker),
        Without<PlayerCharacterMarker>,
    >,
    mut hits: EventWriter<HazardHitEvent>,
) {
    let dt = time.delta_secs();
    for (hazard, transform, mut laser) in &mut lasers {
        for (player, player_transform) in &players {
            let feet = player_transform.translation - Vec3::Y * CHARACTER_HALF_HEIGHT;
            let local = (feet - transform.translation).abs();
            let inside = local.x <= laser.half_extents.x
                && local.z <= laser.half_extents.z
                && local.y <= laser.half_extents.y + 0.1;
            if inside && laser.already_hit.insert(player) {
                hits.send(HazardHitEvent {
                    hazard,
                    target: player,
                    damage: laser.damage,
                });
            }
        }
        laser.remaining_seconds -= dt;
        if laser.remaining_seconds <= 0.0 {
            commands.entity(hazard).despawn();
        }
    }
    for (hazard, transform, mut strike) in &mut airstrikes {
        strike.fuse_seconds -= dt;
        if strike.fuse_seconds > 0.0 {
            continue;
        }
        for (player, player_transform) in &players {
            let offset = player_transform.translation - transform.translation;
            if Vec2::new(offset.x, offset.z).length() <= strike.radius {
                hits.send(HazardHitEvent {
                    hazard,
                    target: player,
                    damage: strike.damage,
                });
            }
        }
        commands.entity(hazard).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_defaults() {
        let b = BossGorgon::default();
        assert_eq!(b.current_phase, 1);
        assert_eq!((b.health_max, b.health_current), (25_000.0, 25_000.0));
        assert_eq!(b.posture_max, 500.0);
        assert_eq!(b.laser_grid_timer.duration().as_secs_f32(), 8.0);
        assert_eq!(b.missile_salvo_timer.duration().as_secs_f32(), 14.0);
        assert!(!b.is_enraged);
    }

    #[test]
    fn phase_thresholds() {
        assert_eq!(phase_for_health_fraction(1.0), 1);
        assert_eq!(phase_for_health_fraction(0.67), 1);
        assert_eq!(phase_for_health_fraction(0.66), 2);
        assert_eq!(phase_for_health_fraction(0.34), 2);
        assert_eq!(phase_for_health_fraction(0.33), 3);
        assert_eq!(phase_for_health_fraction(0.0), 3);
    }

    #[test]
    fn damage_below_66_then_33_percent_advances_phases() {
        let mut b = BossGorgon::default();
        assert!(
            b.apply_damage(8_000.0).is_empty(),
            "68 % health: still phase 1"
        );
        assert_eq!(b.apply_damage(500.0), vec![(1, 2)], "below 66 %");
        assert_eq!(b.current_phase, 2);
        assert!(b.apply_damage(4_000.0).is_empty());
        assert_eq!(b.apply_damage(4_500.0), vec![(2, 3)], "below 33 %");
        assert_eq!(b.current_phase, 3);
        assert!(b.apply_damage(100.0).is_empty(), "no phase 4");
    }

    #[test]
    fn one_massive_hit_reports_both_transitions() {
        let mut b = BossGorgon::default();
        assert_eq!(b.apply_damage(20_000.0), vec![(1, 2), (2, 3)]);
    }

    #[test]
    fn enrage_and_defeat() {
        let mut b = BossGorgon::default();
        b.apply_damage(22_000.0);
        assert!(!b.is_enraged, "12 % health");
        b.apply_damage(1_000.0);
        assert!(b.is_enraged, "8 % health");
        b.apply_damage(1e9);
        assert!(b.is_defeated());
        assert!(b.apply_damage(10.0).is_empty());
    }

    #[test]
    fn posture_break_stuns_and_opens_the_finisher() {
        let mut b = BossGorgon::default();
        assert!(!b.finisher_available());
        assert!(b.execute_finisher().is_none());
        assert!(!b.add_posture(300.0));
        assert!(b.add_posture(250.0), "500 posture breaks");
        assert!(b.is_stunned() && b.finisher_available());
        assert_eq!(b.tick(1.0), GorgonCues::default(), "stunned: no attacks");

        let (damage, transitions) = b.execute_finisher().expect("finisher");
        assert_eq!(damage, 3_000.0);
        assert!(transitions.is_empty());
        assert!(!b.is_stunned() && b.posture_current == 0.0);
        assert!(b.execute_finisher().is_none(), "one finisher per break");
    }

    #[test]
    fn stun_wears_off_and_resets_posture() {
        let mut b = BossGorgon::default();
        b.add_posture(500.0);
        b.tick(POSTURE_STUN_SECONDS + 0.01);
        assert!(!b.is_stunned());
        assert_eq!(b.posture_current, 0.0);
    }

    #[test]
    fn phase_timers_cue_their_attacks() {
        let mut b = BossGorgon::default();
        assert!(b.tick(8.0).laser_grid);
        assert!(b.tick(6.0).missile_salvo, "14 s salvo");
        b.apply_damage(9_000.0);
        let cues = b.tick(AIRSTRIKE_INTERVAL_SECONDS);
        assert!(
            cues.airstrike && !cues.laser_grid,
            "phase 2 only calls airstrikes"
        );
        b.apply_damage(9_000.0);
        assert_eq!(b.tick(20.0), GorgonCues::default(), "phase 3 is melee");
    }

    #[test]
    fn enraged_timers_run_faster() {
        let mut b = BossGorgon {
            is_enraged: true,
            ..Default::default()
        };
        assert!(b.tick(8.0 / ENRAGE_TEMPO + 0.01).laser_grid);
    }
}
