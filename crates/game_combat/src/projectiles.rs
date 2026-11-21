//! Shooting, projectile ballistics and ranged hit resolution.
//!
//! Hitscan weapons resolve instantly with one ray from the shooter's eye.
//! Projectile weapons spawn [`Projectile`]s that fly under gravity and sweep
//! a ray along each tick's travel, so fast rounds never tunnel through thin
//! geometry. Both paths emit [`RangedImpactEvent`]s, resolved in one place:
//! headshots, damage on dummies, floating text and status effects.

use crate::combat_dummy::{apply_dummy_damage, spawn_floating_combat_text, CombatDummy};
use crate::melee::MeleeCombatState;
use crate::ranged::{BallisticProfile, ShotEvent};
use crate::recoil::RecoilSpring;
use crate::status_effects::{
    ActiveStatusEffects, StatusEffect, StatusEffectTickEvent, StatusEffectType,
};
use avian3d::prelude::*;
use bevy::prelude::*;
use game_core::input::{PlayerInputState, ViewAngles};
use game_core::math::{safe_normalize, sanitize_vec3};
use game_core::protocol::{InputActions, NetworkTick};
use game_parkour::MovementSpeedMultiplier;

/// Gravity applied to projectiles before their gravity scale, m/s^2.
pub const PROJECTILE_GRAVITY: f32 = 9.8;
pub const PROJECTILE_LIFETIME_SECONDS: f32 = 3.0;
pub const HITSCAN_RANGE_METERS: f32 = 300.0;
/// Eye height above the shooter's capsule center (eye at 1.68 m).
pub const SHOOTER_EYE_OFFSET: f32 = 0.78;
/// Hits higher than this above the target's center are headshots
/// (top 0.35 m of a 1.8 m capsule).
pub const HEADSHOT_HEIGHT_OFFSET: f32 = 0.55;

/// Trigger and reload intent, filled from packets or written by AI.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct RangedInput {
    pub trigger_held: bool,
    /// Edge; cleared once read.
    pub reload_pressed: bool,
    pub previous_actions: u32,
    #[reflect(ignore)]
    pub last_packet_tick: Option<NetworkTick>,
}

/// A round in flight.
#[derive(Component, Reflect, Debug, Clone, PartialEq)]
#[reflect(Component, Debug)]
pub struct Projectile {
    pub velocity: Vec3,
    pub gravity_scale: f32,
    pub lifetime: Timer,
    pub damage: f32,
    pub headshot_multiplier: f32,
    pub status_effect: Option<StatusEffectType>,
    pub owner: Entity,
}

/// A ray or projectile struck a collider.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct RangedImpactEvent {
    pub shooter: Entity,
    pub target: Entity,
    pub point: Vec3,
    pub damage: f32,
    pub headshot_multiplier: f32,
    pub status_effect: Option<StatusEffectType>,
}

/// Damage landed on a combat target.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct RangedHitEvent {
    pub shooter: Entity,
    pub target: Entity,
    pub damage: f32,
    pub headshot: bool,
    pub status_applied: Option<StatusEffectType>,
}

/// A trigger pull, for audio / VFX / networking.
#[derive(Event, Debug, Clone, PartialEq)]
pub struct ShotFiredEvent {
    pub shooter: Entity,
    pub origin: Vec3,
    pub shot: ShotEvent,
}

/// Whether a hit at `point` on a target centered at `target_center` is a
/// headshot.
pub fn is_headshot(point: Vec3, target_center: Vec3) -> bool {
    point.y - target_center.y > HEADSHOT_HEIGHT_OFFSET
}

/// Unit direction for a pellet: the aim, then the recoil kick, then the
/// pellet's spread deviation, all as yaw/pitch rotations.
pub fn shot_direction(aim: Quat, recoil: Quat, deviation_degrees: Vec2) -> Vec3 {
    let deviation = Quat::from_euler(
        EulerRot::YXZ,
        deviation_degrees.x.to_radians(),
        deviation_degrees.y.to_radians(),
        0.0,
    );
    safe_normalize(aim * recoil * deviation * Vec3::NEG_Z, aim * Vec3::NEG_Z)
}

/// Advances a projectile by `dt` (semi-implicit Euler: gravity first).
/// Returns the segment travelled.
pub fn step_projectile(position: Vec3, projectile: &mut Projectile, dt: f32) -> (Vec3, Vec3) {
    projectile.velocity.y -= PROJECTILE_GRAVITY * projectile.gravity_scale * dt;
    projectile.velocity = sanitize_vec3(projectile.velocity);
    (position, sanitize_vec3(position + projectile.velocity * dt))
}

/// Reads trigger (FIRE held) and reload (RELOAD edge) from fresh packets.
pub fn apply_ranged_input(mut shooters: Query<(&PlayerInputState, &mut RangedInput)>) {
    for (state, mut input) in &mut shooters {
        let Some(packet) = state.latest() else {
            continue;
        };
        if input.last_packet_tick == Some(packet.tick) {
            continue;
        }
        let pressed = packet.actions & !input.previous_actions;
        input.trigger_held = packet.has_action(InputActions::FIRE);
        input.reload_pressed |= pressed & InputActions::RELOAD != 0;
        input.previous_actions = packet.actions;
        input.last_packet_tick = Some(packet.tick);
    }
}

type ShooterQuery<'a> = (
    Entity,
    &'a mut BallisticProfile,
    &'a mut RangedInput,
    &'a Transform,
    Option<&'a ViewAngles>,
    Option<&'a mut RecoilSpring>,
);

/// Ticks weapons, handles reloads and fires: hitscan rays resolve now,
/// projectile rounds are spawned. Kicks the shooter's recoil spring.
pub fn update_ranged_weapons(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    mut shooters: Query<ShooterQuery>,
    mut impacts: EventWriter<RangedImpactEvent>,
    mut fired: EventWriter<ShotFiredEvent>,
) {
    let dt = time.delta_secs();
    for (entity, mut weapon, mut input, transform, view, recoil) in &mut shooters {
        if std::mem::take(&mut input.reload_pressed) {
            weapon.start_reload();
        }
        weapon.tick(dt, input.trigger_held);
        if !input.trigger_held {
            continue;
        }
        if weapon.current_ammo == 0 {
            weapon.start_reload();
            continue;
        }
        let Some(shot) = weapon.fire() else {
            continue;
        };

        let aim = view.map_or(transform.rotation, ViewAngles::rotation);
        let recoil_rotation = recoil
            .as_deref()
            .map_or(Quat::IDENTITY, RecoilSpring::rotation);
        let origin = transform.translation + Vec3::Y * SHOOTER_EYE_OFFSET;
        let filter = SpatialQueryFilter::default().with_excluded_entities([entity]);

        for deviation in &shot.pellet_deviations {
            let direction = shot_direction(aim, recoil_rotation, *deviation);
            if weapon.is_hitscan() {
                let Ok(ray) = Dir3::new(direction) else {
                    continue;
                };
                if let Some(hit) =
                    spatial.cast_ray(origin, ray, HITSCAN_RANGE_METERS, true, &filter)
                {
                    impacts.send(RangedImpactEvent {
                        shooter: entity,
                        target: hit.entity,
                        point: origin + direction * hit.distance,
                        damage: shot.damage_per_pellet,
                        headshot_multiplier: shot.headshot_multiplier,
                        status_effect: shot.status_effect,
                    });
                }
            } else {
                commands.spawn((
                    Name::new("projectile"),
                    Transform::from_translation(origin),
                    Projectile {
                        velocity: direction * shot.projectile_velocity_mps,
                        gravity_scale: shot.projectile_gravity_scale,
                        lifetime: Timer::from_seconds(PROJECTILE_LIFETIME_SECONDS, TimerMode::Once),
                        damage: shot.damage_per_pellet,
                        headshot_multiplier: shot.headshot_multiplier,
                        status_effect: shot.status_effect,
                        owner: entity,
                    },
                ));
            }
        }

        if let Some(mut recoil) = recoil {
            recoil.punch(
                shot.recoil_pitch_degrees,
                shot.recoil_yaw_random,
                shot.shot_index as f32,
            );
        }
        fired.send(ShotFiredEvent {
            shooter: entity,
            origin,
            shot,
        });
    }
}

/// Moves projectiles and sweeps each tick's travel for hits.
pub fn simulate_projectiles(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    mut projectiles: Query<(Entity, &mut Projectile, &mut Transform)>,
    mut impacts: EventWriter<RangedImpactEvent>,
) {
    let dt = time.delta_secs();
    for (entity, mut projectile, mut transform) in &mut projectiles {
        projectile
            .lifetime
            .tick(std::time::Duration::from_secs_f32(dt.max(0.0)));
        let (from, to) = step_projectile(transform.translation, &mut projectile, dt);
        let travel = to - from;
        let distance = travel.length();
        let filter = SpatialQueryFilter::default().with_excluded_entities([projectile.owner]);
        let hit = Dir3::new(travel)
            .ok()
            .and_then(|dir| spatial.cast_ray(from, dir, distance, true, &filter));
        if let Some(hit) = hit {
            impacts.send(RangedImpactEvent {
                shooter: projectile.owner,
                target: hit.entity,
                point: from + travel / distance * hit.distance,
                damage: projectile.damage,
                headshot_multiplier: projectile.headshot_multiplier,
                status_effect: projectile.status_effect,
            });
            commands.entity(entity).despawn();
        } else if projectile.lifetime.finished() {
            commands.entity(entity).despawn();
        } else {
            transform.translation = to;
        }
    }
}

/// Resolves impacts on combat targets: dummies and fighters. Impacts on
/// scenery are ignored (the projectile was already stopped).
pub fn apply_ranged_impacts(
    mut commands: Commands,
    mut impacts: EventReader<RangedImpactEvent>,
    mut dummies: Query<(&mut CombatDummy, &Transform, &mut ActiveStatusEffects)>,
    mut fighters: Query<
        (&Transform, Option<&mut ActiveStatusEffects>),
        (With<MeleeCombatState>, Without<CombatDummy>),
    >,
    mut hits: EventWriter<RangedHitEvent>,
) {
    for impact in impacts.read() {
        let status = impact.status_effect;
        let (center, damage, headshot) =
            if let Ok((mut dummy, transform, mut effects)) = dummies.get_mut(impact.target) {
                let headshot = is_headshot(impact.point, transform.translation);
                let damage = impact.damage
                    * if headshot {
                        impact.headshot_multiplier
                    } else {
                        1.0
                    };
                let outcome = apply_dummy_damage(&mut dummy, damage, headshot, 0.0);
                if let Some(effect_type) = status {
                    effects.apply_effect(StatusEffect::new(effect_type));
                }
                (transform.translation, outcome.damage, headshot)
            } else if let Ok((transform, effects)) = fighters.get_mut(impact.target) {
                let headshot = is_headshot(impact.point, transform.translation);
                let damage = impact.damage
                    * if headshot {
                        impact.headshot_multiplier
                    } else {
                        1.0
                    };
                if let Some(effect_type) = status {
                    match effects {
                        Some(mut effects) => effects.apply_effect(StatusEffect::new(effect_type)),
                        None => {
                            let mut effects = ActiveStatusEffects::default();
                            effects.apply_effect(StatusEffect::new(effect_type));
                            commands.entity(impact.target).insert(effects);
                        }
                    }
                }
                (transform.translation, damage, headshot)
            } else {
                continue;
            };

        spawn_floating_combat_text(&mut commands, center, damage, headshot);
        hits.send(RangedHitEvent {
            shooter: impact.shooter,
            target: impact.target,
            damage,
            headshot,
            status_applied: status,
        });
    }
}

/// Ticks status effects; Thermal Burn damages dummies directly.
pub fn update_status_effects(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut afflicted: Query<(
        Entity,
        &mut ActiveStatusEffects,
        Option<&mut CombatDummy>,
        Option<&Transform>,
    )>,
    mut ticks_out: EventWriter<StatusEffectTickEvent>,
) {
    let dt = time.delta_secs();
    for (entity, mut effects, dummy, transform) in &mut afflicted {
        if effects.effects.is_empty() {
            continue;
        }
        let ticks = effects.tick(dt);
        let mut dummy = dummy;
        for tick in ticks {
            if tick.damage > 0.0 {
                if let (Some(dummy), Some(transform)) = (dummy.as_deref_mut(), transform) {
                    let outcome = apply_dummy_damage(dummy, tick.damage, false, 0.0);
                    spawn_floating_combat_text(
                        &mut commands,
                        transform.translation,
                        outcome.damage,
                        false,
                    );
                }
            }
            ticks_out.send(StatusEffectTickEvent {
                target: entity,
                tick,
            });
        }
    }
}

/// Mirrors status slows into the parkour controller's speed multiplier.
pub fn sync_status_speed_multipliers(
    mut commands: Commands,
    mut afflicted: Query<(
        Entity,
        &ActiveStatusEffects,
        Option<&mut MovementSpeedMultiplier>,
    )>,
) {
    for (entity, effects, multiplier) in &mut afflicted {
        let wanted = effects.speed_multiplier();
        match multiplier {
            Some(mut m) if m.0 != wanted => m.0 = wanted,
            Some(_) => {}
            None if wanted != 1.0 => {
                commands
                    .entity(entity)
                    .insert(MovementSpeedMultiplier(wanted));
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headshot_zone_is_the_top_of_the_capsule() {
        let center = Vec3::new(0.0, 0.9, 0.0);
        assert!(is_headshot(Vec3::new(0.0, 1.6, 0.0), center));
        assert!(!is_headshot(Vec3::new(0.0, 1.3, 0.0), center));
        assert!(!is_headshot(Vec3::new(0.0, 0.2, 0.0), center));
    }

    #[test]
    fn shot_direction_composes_aim_recoil_and_spread() {
        assert!(shot_direction(Quat::IDENTITY, Quat::IDENTITY, Vec2::ZERO)
            .abs_diff_eq(Vec3::NEG_Z, 1e-6));
        let up = shot_direction(Quat::IDENTITY, Quat::from_rotation_x(0.1), Vec2::ZERO);
        assert!(up.y > 0.0, "recoil pitches the shot up");
        let left = shot_direction(Quat::IDENTITY, Quat::IDENTITY, Vec2::new(5.0, 0.0));
        assert!(left.x < 0.0 && (left.angle_between(Vec3::NEG_Z).to_degrees() - 5.0).abs() < 1e-3);
        let turned = shot_direction(
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Quat::IDENTITY,
            Vec2::ZERO,
        );
        assert!(turned.abs_diff_eq(Vec3::NEG_X, 1e-6));
    }

    #[test]
    fn projectiles_fall_under_scaled_gravity() {
        let mut p = Projectile {
            velocity: Vec3::new(0.0, 0.0, -450.0),
            gravity_scale: 0.2,
            lifetime: Timer::from_seconds(3.0, TimerMode::Once),
            damage: 18.0,
            headshot_multiplier: 1.75,
            status_effect: None,
            owner: Entity::PLACEHOLDER,
        };
        let mut position = Vec3::ZERO;
        for _ in 0..64 {
            position = step_projectile(position, &mut p, 1.0 / 64.0).1;
        }
        assert!((p.velocity.y - -1.96).abs() < 1e-3, "0.2 g for 1 s");
        assert!((position.z - -450.0).abs() < 1e-2);
        assert!(position.y < 0.0 && position.y > -1.0);
    }

    #[test]
    fn packets_drive_trigger_and_reload() {
        use game_core::protocol::PlayerInputPacket;
        let mut app = App::new();
        app.add_systems(Update, apply_ranged_input);
        let mut state = PlayerInputState::default();
        state.push(PlayerInputPacket {
            tick: NetworkTick(1),
            actions: InputActions::FIRE | InputActions::RELOAD,
            ..Default::default()
        });
        let e = app.world_mut().spawn((state, RangedInput::default())).id();
        app.update();
        let input = *app.world().get::<RangedInput>(e).expect("input");
        assert!(input.trigger_held && input.reload_pressed);

        app.world_mut()
            .get_mut::<RangedInput>(e)
            .expect("input")
            .reload_pressed = false;
        app.world_mut()
            .get_mut::<PlayerInputState>(e)
            .expect("state")
            .push(PlayerInputPacket {
                tick: NetworkTick(2),
                actions: InputActions::RELOAD,
                ..Default::default()
            });
        app.update();
        let input = *app.world().get::<RangedInput>(e).expect("input");
        assert!(!input.trigger_held, "trigger released");
        assert!(!input.reload_pressed, "held reload is not a new press");
    }
}
