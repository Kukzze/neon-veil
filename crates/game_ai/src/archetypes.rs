//! Enemy archetype: stats, behavior trees, spawning and the systems that
//! run them.
//!
//! Ground enemies are parkour characters driven through `ParkourInput`, so
//! they sprint, vault and wall-run exactly like the player. Swarm drones fly
//! with their own hover controller.

use crate::actions::{
    run_action, AiAction, AiContext, AiIntent, AiWorld, MovementSense, TargetInfo,
};
use crate::behavior_tree::{BehaviorNode, BehaviorTree};
use crate::perception::{AiBlackboard, AiPerception, AlertState, EYE_OFFSET, SIGHT_TARGET_OFFSET};
use avian3d::prelude::*;
use bevy::prelude::*;
use game_combat::projectiles::PROJECTILE_GRAVITY;
use game_combat::{
    ActiveStatusEffects, AttackFrameData, AttackPhase, BallisticProfile, FrontalShield,
    MeleeCombatState, MeleeHitEvent, MeleeInput, MeleeWeaponClass, Projectile, RangedHitEvent,
    RangedInput, RangedWeaponArchetype, StatusEffect, StatusEffectTickEvent, StatusEffectType,
};
use game_core::identifiers::PlayerCharacterMarker;
use game_core::input::{clamp_pitch, ViewAngles};
use game_core::math::wrap_angle_radians;
use game_parkour::{
    KinematicVelocity, ParkourController, ParkourInput, ParkourSensorReadings, ParkourState,
};
use serde::{Deserialize, Serialize};

/// Thrown grenade speed along its arc, m/s.
pub const GRENADE_THROW_SPEED: f32 = 14.0;
pub const GRENADE_DAMAGE: f32 = 15.0;
pub const MISSILE_SPEED: f32 = 60.0;
pub const MISSILE_DAMAGE: f32 = 35.0;
pub const DRONE_HOVER_ALTITUDE: f32 = 4.0;
pub const DRONE_SPEED: f32 = 9.0;
pub const DRONE_CHARGE_SPEED: f32 = 15.0;
/// A charging drone detonates this close to its target, m.
pub const DRONE_DETONATION_RANGE: f32 = 1.3;
pub const DRONE_DETONATION_DAMAGE: f32 = 60.0;
/// The target counts as visible if seen this recently, s.
const VISIBLE_WITHIN_SECONDS: f32 = 0.05;

/// Enemy families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Debug, PartialEq, Hash)]
pub enum EnemyArchetype {
    /// Fast hovering flyer; rapid pulse bursts, kamikaze charge when hurt.
    SwarmDrone,
    /// Infantry with a sidearm; uses cover and throws EMP grenades.
    StreetGrunt,
    /// Acrobatic melee elite; wall-runs to flank and guards against swings.
    CyberNinja,
    /// Frontal energy shield, plasma shotgun, slow to turn.
    ShieldedHeavy,
    /// Support hacker; keeps distance and uploads Cryo Glitch.
    Netrunner,
    /// Quad-walker miniboss: chaingun, ground stomp, missile pods.
    MechaEnforcer,
}

/// Fixed per-archetype tuning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArchetypeStats {
    pub health: f32,
    pub posture_max: f32,
    /// Maximum view turn rate, degrees per second.
    pub turn_rate_degrees: f32,
    pub ranged: Option<RangedWeaponArchetype>,
    pub melee: MeleeWeaponClass,
    pub flying: bool,
}

pub const fn archetype_stats(archetype: EnemyArchetype) -> ArchetypeStats {
    use EnemyArchetype::*;
    use RangedWeaponArchetype as R;
    let (health, posture_max, turn_rate_degrees, ranged, melee, flying) = match archetype {
        SwarmDrone => (
            80.0,
            40.0,
            720.0,
            Some(R::SmartSubmachineGun),
            MeleeWeaponClass::CyberKatana,
            true,
        ),
        StreetGrunt => (
            250.0,
            100.0,
            360.0,
            Some(R::KineticPistol),
            MeleeWeaponClass::CyberKatana,
            false,
        ),
        CyberNinja => (
            400.0,
            150.0,
            720.0,
            None,
            MeleeWeaponClass::MantisDualBlades,
            false,
        ),
        ShieldedHeavy => (
            1200.0,
            300.0,
            60.0,
            Some(R::PlasmaShotgun),
            MeleeWeaponClass::KineticHeavyGauntlet,
            false,
        ),
        Netrunner => (
            180.0,
            80.0,
            360.0,
            Some(R::KineticPistol),
            MeleeWeaponClass::CyberKatana,
            false,
        ),
        MechaEnforcer => (
            3500.0,
            600.0,
            120.0,
            Some(R::PulseAssaultRifle),
            MeleeWeaponClass::KineticHeavyGauntlet,
            false,
        ),
    };
    ArchetypeStats {
        health,
        posture_max,
        turn_rate_degrees,
        ranged,
        melee,
        flying,
    }
}

/// Mecha Enforcer ground stomp: 6 m shockwave.
pub const MECHA_STOMP: AttackFrameData = AttackFrameData {
    name: "Mecha_Ground_Stomp",
    startup_frames: 18,
    active_frames: 6,
    recovery_frames: 30,
    cancel_frame: 40,
    base_damage: 80.0,
    posture_damage: 50.0,
    knockback_impulse: 12.0,
    hitbox_extents: Vec3::new(12.0, 2.0, 12.0),
    hitbox_offset: Vec3::new(0.0, 0.5, 0.0),
    knockback_direction: Vec3::Y,
    area_radius: 6.0,
    attacker_dive_speed: 0.0,
    unblockable: false,
};

/// Health and posture of an enemy.
#[derive(Component, Reflect, Debug, Clone, PartialEq)]
#[reflect(Component, Debug)]
#[require(AiBlackboard, AiPerception, AiIntent, ViewAngles)]
pub struct Enemy {
    pub archetype: EnemyArchetype,
    pub health: f32,
    pub health_max: f32,
    pub posture: f32,
    pub posture_max: f32,
}

impl Enemy {
    pub fn new(archetype: EnemyArchetype) -> Self {
        let stats = archetype_stats(archetype);
        Self {
            archetype,
            health: stats.health,
            health_max: stats.health,
            posture: 0.0,
            posture_max: stats.posture_max,
        }
    }

    pub fn health_fraction(&self) -> f32 {
        (self.health / self.health_max.max(1.0)).clamp(0.0, 1.0)
    }

    pub fn is_defeated(&self) -> bool {
        self.health <= 0.0
    }

    /// Applies damage; returns whether this hit defeated the enemy.
    pub fn apply_damage(&mut self, damage: f32) -> bool {
        if self.is_defeated() || !damage.is_finite() || damage <= 0.0 {
            return false;
        }
        self.health = (self.health - damage).max(0.0);
        self.is_defeated()
    }
}

/// The enemy's behavior tree.
#[derive(Component, Debug, Clone)]
pub struct EnemyBrain {
    pub tree: BehaviorTree<AiAction>,
}

impl EnemyBrain {
    pub fn for_archetype(archetype: EnemyArchetype) -> Self {
        Self {
            tree: BehaviorTree::new(behavior_tree_for(archetype)),
        }
    }
}

/// Hover flight for swarm drones.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component, Debug)]
pub struct DroneFlight {
    /// Cruise height (world Y).
    pub altitude: f32,
    pub speed: f32,
}

/// An enemy was defeated.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct EnemyDefeatedEvent {
    pub enemy: Entity,
    pub archetype: EnemyArchetype,
    pub position: Vec3,
}

/// A charging drone blew itself up.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct DroneDetonationEvent {
    pub drone: Entity,
    pub position: Vec3,
    pub target: Option<Entity>,
    pub damage: f32,
}

fn leaf(action: AiAction) -> BehaviorNode<AiAction> {
    BehaviorNode::leaf(action)
}

/// Combat branch, then investigating, then patrolling.
fn standard_tree(combat: Vec<BehaviorNode<AiAction>>) -> BehaviorNode<AiAction> {
    BehaviorNode::selector([
        BehaviorNode::sequence([leaf(AiAction::InCombat), BehaviorNode::selector(combat)]),
        BehaviorNode::sequence([leaf(AiAction::IsInvestigating), leaf(AiAction::Investigate)]),
        leaf(AiAction::Patrol),
    ])
}

/// The behavior tree each archetype runs.
pub fn behavior_tree_for(archetype: EnemyArchetype) -> BehaviorNode<AiAction> {
    use AiAction::*;
    use BehaviorNode as N;
    match archetype {
        EnemyArchetype::SwarmDrone => standard_tree(vec![
            N::sequence([leaf(HealthBelow(0.35)), leaf(ChargeTarget)]),
            leaf(RangedAttack { max_range: 20.0 }),
            leaf(Chase { stop_distance: 8.0 }),
        ]),
        EnemyArchetype::StreetGrunt => standard_tree(vec![
            N::sequence([
                leaf(HealthBelow(0.5)),
                leaf(TakeCover),
                leaf(RangedAttack { max_range: 30.0 }),
            ]),
            N::cooldown(
                8.0,
                leaf(ThrowGrenade {
                    status: StatusEffectType::ShockEmp,
                }),
            ),
            leaf(RangedAttack { max_range: 30.0 }),
            leaf(Chase {
                stop_distance: 12.0,
            }),
        ]),
        EnemyArchetype::CyberNinja => standard_tree(vec![
            N::sequence([leaf(TargetAttacking), leaf(Guard)]),
            N::sequence([leaf(TargetWithin(2.2)), leaf(MeleeAttack { reach: 2.2 })]),
            leaf(WallRunFlank),
            leaf(Chase { stop_distance: 1.8 }),
        ]),
        EnemyArchetype::ShieldedHeavy => standard_tree(vec![
            N::sequence([
                leaf(TargetWithin(12.0)),
                leaf(RangedAttack { max_range: 12.0 }),
            ]),
            leaf(Chase { stop_distance: 8.0 }),
        ]),
        EnemyArchetype::Netrunner => standard_tree(vec![
            N::cooldown(
                6.0,
                leaf(Hack {
                    status: StatusEffectType::CryoGlitch,
                    range: 30.0,
                }),
            ),
            leaf(Reposition {
                min: 15.0,
                max: 25.0,
            }),
            leaf(RangedAttack { max_range: 25.0 }),
            leaf(Chase {
                stop_distance: 20.0,
            }),
        ]),
        EnemyArchetype::MechaEnforcer => standard_tree(vec![
            N::sequence([
                leaf(TargetWithin(5.0)),
                N::cooldown(
                    6.0,
                    leaf(SpecialAttack {
                        attack: MECHA_STOMP,
                        reach: 5.0,
                    }),
                ),
            ]),
            N::cooldown(10.0, leaf(MissileSalvo { count: 6 })),
            leaf(RangedAttack { max_range: 35.0 }),
            leaf(Chase {
                stop_distance: 10.0,
            }),
        ]),
    }
}

/// The ranged weapon an archetype carries.
pub fn weapon_for(archetype: EnemyArchetype) -> Option<BallisticProfile> {
    let base = BallisticProfile::for_archetype(archetype_stats(archetype).ranged?);
    Some(if archetype == EnemyArchetype::MechaEnforcer {
        // Rotary chaingun: the rifle action spun up.
        BallisticProfile {
            rounds_per_minute: 1100,
            magazine_capacity: 200,
            current_ammo: 200,
            reserve_ammo: 2000,
            base_damage: 14.0,
            status_effect_affinity: None,
            ..base
        }
    } else {
        base
    })
}

/// Spawns an enemy of `archetype` with its capsule center at `position`,
/// facing -Z.
pub fn spawn_enemy(commands: &mut Commands, archetype: EnemyArchetype, position: Vec3) -> Entity {
    let stats = archetype_stats(archetype);
    let mut entity = commands.spawn((
        Name::new(format!("{archetype:?}")),
        Enemy::new(archetype),
        EnemyBrain::for_archetype(archetype),
        Transform::from_translation(position),
        RigidBody::Kinematic,
        MeleeCombatState {
            weapon_class: stats.melee,
            posture_max: stats.posture_max,
            ..Default::default()
        },
    ));
    if let Some(weapon) = weapon_for(archetype) {
        entity.insert(weapon);
    }
    if stats.flying {
        entity.insert((
            Collider::sphere(0.5),
            KinematicVelocity::default(),
            DroneFlight {
                altitude: position.y.max(DRONE_HOVER_ALTITUDE),
                speed: DRONE_SPEED,
            },
        ));
    } else {
        entity.insert((Collider::capsule(0.4, 1.0), ParkourController::default()));
    }
    if archetype == EnemyArchetype::ShieldedHeavy {
        entity.insert(FrontalShield::default());
    }
    entity.id()
}

pub fn spawn_swarm_drone(commands: &mut Commands, position: Vec3) -> Entity {
    spawn_enemy(commands, EnemyArchetype::SwarmDrone, position)
}
pub fn spawn_street_grunt(commands: &mut Commands, position: Vec3) -> Entity {
    spawn_enemy(commands, EnemyArchetype::StreetGrunt, position)
}
pub fn spawn_cyber_ninja(commands: &mut Commands, position: Vec3) -> Entity {
    spawn_enemy(commands, EnemyArchetype::CyberNinja, position)
}
pub fn spawn_shielded_heavy(commands: &mut Commands, position: Vec3) -> Entity {
    spawn_enemy(commands, EnemyArchetype::ShieldedHeavy, position)
}
pub fn spawn_netrunner(commands: &mut Commands, position: Vec3) -> Entity {
    spawn_enemy(commands, EnemyArchetype::Netrunner, position)
}
pub fn spawn_mecha_enforcer(commands: &mut Commands, position: Vec3) -> Entity {
    spawn_enemy(commands, EnemyArchetype::MechaEnforcer, position)
}

/// [`AiWorld`] backed by Avian's spatial query pipeline.
pub struct SpatialAiWorld<'a, 'w, 's> {
    pub query: &'a SpatialQuery<'w, 's>,
    pub filter: SpatialQueryFilter,
}

impl AiWorld for SpatialAiWorld<'_, '_, '_> {
    fn clear_line(&self, from: Vec3, to: Vec3) -> bool {
        let offset = to - from;
        let Ok(direction) = Dir3::new(offset) else {
            return true;
        };
        self.query
            .cast_ray(from, direction, offset.length(), true, &self.filter)
            .is_none()
    }
}

/// Movement readings from the parkour probes.
pub fn movement_sense(
    sensors: Option<&ParkourSensorReadings>,
    state: Option<&ParkourState>,
    velocity: Option<&KinematicVelocity>,
) -> MovementSense {
    let state = state.copied();
    let Some(r) = sensors else {
        return MovementSense {
            grounded: state.map_or(true, |s| s.is_grounded()),
            parkour_state: state,
            planar_speed: velocity.map_or(0.0, KinematicVelocity::planar_speed),
            ..Default::default()
        };
    };
    let climbable = r.can_vault_low || r.can_vault_mid || r.can_vault_high || r.can_ledge_mantle;
    MovementSense {
        grounded: state.is_some_and(|s| s.is_grounded()),
        wall_left: r.is_left_wall_detected,
        wall_right: r.is_right_wall_detected,
        blocked_ahead: r.obstacle_distance > 0.0 && !climbable,
        planar_speed: velocity.map_or(0.0, KinematicVelocity::planar_speed),
        parkour_state: state,
    }
}

type BrainQuery<'a> = (
    Entity,
    &'a Transform,
    &'a ViewAngles,
    &'a Enemy,
    &'a AiPerception,
    &'a mut AiBlackboard,
    &'a mut AiIntent,
    &'a mut EnemyBrain,
    Option<&'a ParkourSensorReadings>,
    Option<&'a ParkourState>,
    Option<&'a KinematicVelocity>,
);

/// Ticks every enemy's behavior tree into a fresh [`AiIntent`].
pub fn run_behavior_trees(
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    mut enemies: Query<BrainQuery>,
    targets: Query<(&Transform, Option<&MeleeCombatState>), Without<Enemy>>,
) {
    let dt = time.delta_secs();
    for (
        entity,
        transform,
        view,
        enemy,
        perception,
        mut blackboard,
        mut intent,
        mut brain,
        sensors,
        state,
        velocity,
    ) in &mut enemies
    {
        *intent = AiIntent::default();
        let target = blackboard.target_entity.and_then(|t| {
            let (target_transform, melee) = targets.get(t).ok()?;
            Some(TargetInfo {
                entity: t,
                position: target_transform.translation,
                attacking: melee.is_some_and(|m| {
                    matches!(m.attack_phase, AttackPhase::Startup | AttackPhase::Active)
                }),
                visible: blackboard.time_since_target_seen <= VISIBLE_WITHIN_SECONDS,
            })
        });
        if blackboard.target_entity.is_some() && target.is_none() {
            // Target despawned.
            blackboard.target_entity = None;
        }
        let ctx = AiContext {
            position: transform.translation,
            forward: view.forward(),
            health_fraction: enemy.health_fraction(),
            alert_state: perception.alert_state,
            target,
            movement: movement_sense(sensors, state, velocity),
        };
        let world = SpatialAiWorld {
            query: &spatial,
            filter: SpatialQueryFilter::default().with_excluded_entities([entity]),
        };
        let intent = &mut *intent;
        let blackboard = &mut *blackboard;
        brain.tree.tick(dt, &mut |action| {
            run_action(action, &ctx, blackboard, intent, &world)
        });
    }
}

/// Yaw and pitch that look from `eye` toward `target`.
fn look_angles(eye: Vec3, target: Vec3) -> Option<(f32, f32)> {
    let d = (target - eye).try_normalize()?;
    Some(((-d.x).atan2(-d.z), d.y.clamp(-1.0, 1.0).asin()))
}

/// Turns `view` toward the intent's look target (or movement direction) at
/// no more than `turn_rate_degrees` per second.
pub fn steer_view(
    view: &mut ViewAngles,
    eye: Vec3,
    intent: &AiIntent,
    turn_rate_degrees: f32,
    dt: f32,
) {
    let desired = intent
        .look_at
        .and_then(|target| look_angles(eye, target + Vec3::Y * SIGHT_TARGET_OFFSET))
        .or_else(|| {
            (intent.move_direction != Vec2::ZERO).then(|| {
                let d = intent.move_direction;
                ((-d.x).atan2(-d.y), 0.0)
            })
        });
    let Some((yaw, pitch)) = desired else {
        return;
    };
    let max_step = turn_rate_degrees.to_radians() * dt.max(0.0);
    let yaw_error = wrap_angle_radians(yaw - view.yaw);
    view.yaw = wrap_angle_radians(view.yaw + yaw_error.clamp(-max_step, max_step));
    let pitch_error = pitch - view.pitch;
    view.pitch = clamp_pitch(view.pitch + pitch_error.clamp(-max_step, max_step));
}

/// Launch velocity for a lob from `from` landing at `to` after
/// `distance / speed` seconds under full projectile gravity.
pub fn lob_velocity(from: Vec3, to: Vec3, speed: f32) -> Vec3 {
    let offset = to - from;
    let planar = Vec3::new(offset.x, 0.0, offset.z);
    let time = (planar.length() / speed.max(0.1)).clamp(0.4, 2.0);
    let vy = (offset.y + 0.5 * PROJECTILE_GRAVITY * time * time) / time;
    planar / time + Vec3::Y * vy
}

type IntentQuery<'a> = (
    Entity,
    &'a AiIntent,
    &'a Enemy,
    &'a Transform,
    &'a mut ViewAngles,
    Option<&'a mut ParkourInput>,
    Option<&'a mut MeleeInput>,
    Option<&'a mut MeleeCombatState>,
    Option<&'a mut RangedInput>,
);

/// Turns intents into the same inputs a player produces, and spawns
/// grenades, hacks and missiles.
pub fn apply_ai_intents(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut enemies: Query<IntentQuery>,
    mut afflicted: Query<&mut ActiveStatusEffects, Without<Enemy>>,
) {
    let dt = time.delta_secs();
    for (entity, intent, enemy, transform, mut view, parkour, melee_input, melee, ranged) in
        &mut enemies
    {
        let eye = transform.translation + Vec3::Y * EYE_OFFSET;
        steer_view(
            &mut view,
            eye,
            intent,
            archetype_stats(enemy.archetype).turn_rate_degrees,
            dt,
        );

        if let Some(mut input) = parkour {
            input.move_direction = intent.move_direction.clamp_length_max(1.0);
            input.sprint = intent.sprint;
            input.crouch = intent.crouch;
            input.jump_pressed |= intent.jump;
            input.yaw = view.yaw;
        }
        if let Some(mut input) = melee_input {
            input.light_pressed |= intent.melee_light;
            input.heavy_pressed |= intent.melee_heavy;
            input.parry_held = intent.guard;
        }
        if let (Some(attack), Some(mut melee)) = (intent.special_attack, melee) {
            if melee.current_attack.is_none() && !melee.is_posture_broken {
                melee.start_attack(attack);
            }
        }
        if let Some(mut input) = ranged {
            input.trigger_held = intent.trigger_held;
        }

        if let Some(grenade) = intent.grenade {
            commands.spawn((
                Name::new("emp_grenade"),
                Transform::from_translation(eye),
                Projectile {
                    velocity: lob_velocity(eye, grenade.target_position, GRENADE_THROW_SPEED),
                    gravity_scale: 1.0,
                    lifetime: Timer::from_seconds(3.0, TimerMode::Once),
                    damage: GRENADE_DAMAGE,
                    headshot_multiplier: 1.0,
                    status_effect: Some(grenade.status),
                    owner: entity,
                },
            ));
        }
        if let Some(hack) = intent.hack {
            let effect = StatusEffect::new(hack.status);
            match afflicted.get_mut(hack.target) {
                Ok(mut effects) => effects.apply_effect(effect),
                Err(_) => {
                    let mut effects = ActiveStatusEffects::default();
                    effects.apply_effect(effect);
                    commands.entity(hack.target).insert(effects);
                }
            }
        }
        if let Some(target) = intent.missile_target.filter(|_| intent.missiles > 0) {
            for i in 0..intent.missiles {
                // Fan the salvo out around the target.
                let angle = std::f32::consts::TAU * i as f32 / intent.missiles as f32;
                let aim = target + Vec3::new(angle.cos(), 0.0, angle.sin()) * 1.5;
                let launch = eye + Vec3::Y * 0.8;
                commands.spawn((
                    Name::new("micro_missile"),
                    Transform::from_translation(launch),
                    Projectile {
                        velocity: (aim - launch).normalize_or_zero() * MISSILE_SPEED,
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
}

/// Hover flight: follows the intent's movement at cruise altitude, or dives
/// at the target while charging.
pub fn update_drone_flight(
    time: Res<Time<Fixed>>,
    mut drones: Query<(
        &AiIntent,
        &DroneFlight,
        &ViewAngles,
        &mut Transform,
        &mut KinematicVelocity,
    )>,
) {
    let dt = time.delta_secs();
    for (intent, flight, view, mut transform, mut velocity) in &mut drones {
        let speed = if intent.charge {
            DRONE_CHARGE_SPEED
        } else {
            flight.speed
        };
        let planar = intent.move_direction.clamp_length_max(1.0) * speed;
        let target_y = match (intent.charge, intent.look_at) {
            (true, Some(target)) => target.y + SIGHT_TARGET_OFFSET,
            _ => flight.altitude,
        };
        let vy = ((target_y - transform.translation.y) * 3.0).clamp(-speed, speed);
        velocity.0 = Vec3::new(planar.x, vy, planar.y);
        transform.translation += velocity.0 * dt;
        transform.rotation = Quat::from_rotation_y(view.yaw);
    }
}

/// Charging drones blow up next to their target.
pub fn detonate_charging_drones(
    mut commands: Commands,
    drones: Query<(Entity, &AiIntent, &AiBlackboard, &Transform), With<DroneFlight>>,
    mut detonations: EventWriter<DroneDetonationEvent>,
) {
    for (entity, intent, blackboard, transform) in &drones {
        let Some(target) = intent.look_at.filter(|_| intent.charge) else {
            continue;
        };
        if transform
            .translation
            .distance(target + Vec3::Y * SIGHT_TARGET_OFFSET)
            <= DRONE_DETONATION_RANGE
        {
            detonations.send(DroneDetonationEvent {
                drone: entity,
                position: transform.translation,
                target: blackboard.target_entity,
                damage: DRONE_DETONATION_DAMAGE,
            });
            commands.entity(entity).despawn_recursive();
        }
    }
}

/// Applies melee, ranged and status damage to enemies. Being hurt by a
/// player engages them immediately.
pub fn apply_damage_to_enemies(
    mut melee_hits: EventReader<MeleeHitEvent>,
    mut ranged_hits: EventReader<RangedHitEvent>,
    mut status_ticks: EventReader<StatusEffectTickEvent>,
    players: Query<&Transform, With<PlayerCharacterMarker>>,
    mut enemies: Query<(&mut Enemy, &mut AiPerception, &mut AiBlackboard)>,
) {
    let hits = melee_hits
        .read()
        .map(|h| (h.target, Some(h.attacker), h.damage))
        .chain(
            ranged_hits
                .read()
                .map(|h| (h.target, Some(h.shooter), h.damage)),
        )
        .chain(status_ticks.read().map(|t| (t.target, None, t.tick.damage)))
        .collect::<Vec<_>>();
    for (target, attacker, damage) in hits {
        let Ok((mut enemy, mut perception, mut blackboard)) = enemies.get_mut(target) else {
            continue;
        };
        enemy.apply_damage(damage);
        let Some(attacker) = attacker else {
            continue;
        };
        if let Ok(attacker_transform) = players.get(attacker) {
            if perception.alert_state != AlertState::Combat {
                perception.enter_combat(&mut blackboard, attacker, attacker_transform.translation);
            }
        }
    }
}

/// Mirrors melee posture into [`Enemy::posture`].
pub fn sync_enemy_posture(
    mut enemies: Query<(&mut Enemy, &MeleeCombatState), Changed<MeleeCombatState>>,
) {
    for (mut enemy, melee) in &mut enemies {
        enemy.posture = melee.posture_current;
        enemy.posture_max = melee.posture_max;
    }
}

/// Removes defeated enemies.
pub fn despawn_defeated_enemies(
    mut commands: Commands,
    enemies: Query<(Entity, &Enemy, &Transform)>,
    mut defeated: EventWriter<EnemyDefeatedEvent>,
) {
    for (entity, enemy, transform) in &enemies {
        if enemy.is_defeated() {
            defeated.send(EnemyDefeatedEvent {
                enemy: entity,
                archetype: enemy.archetype,
                position: transform.translation,
            });
            commands.entity(entity).despawn_recursive();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior_tree::BehaviorStatus;

    const ALL: [EnemyArchetype; 6] = [
        EnemyArchetype::SwarmDrone,
        EnemyArchetype::StreetGrunt,
        EnemyArchetype::CyberNinja,
        EnemyArchetype::ShieldedHeavy,
        EnemyArchetype::Netrunner,
        EnemyArchetype::MechaEnforcer,
    ];

    #[test]
    fn archetype_stats_match_design() {
        let hp: Vec<f32> = ALL.iter().map(|a| archetype_stats(*a).health).collect();
        assert_eq!(hp, vec![80.0, 250.0, 400.0, 1200.0, 180.0, 3500.0]);
        assert!(archetype_stats(EnemyArchetype::SwarmDrone).flying);
        assert_eq!(
            archetype_stats(EnemyArchetype::CyberNinja).melee,
            MeleeWeaponClass::MantisDualBlades
        );
        assert_eq!(
            archetype_stats(EnemyArchetype::ShieldedHeavy).ranged,
            Some(RangedWeaponArchetype::PlasmaShotgun)
        );
        assert!(
            archetype_stats(EnemyArchetype::ShieldedHeavy).turn_rate_degrees < 90.0,
            "slow to turn"
        );
        assert!(weapon_for(EnemyArchetype::CyberNinja).is_none());
        assert_eq!(
            weapon_for(EnemyArchetype::MechaEnforcer).map(|w| w.rounds_per_minute),
            Some(1100)
        );
    }

    #[test]
    fn enemy_damage_and_defeat() {
        let mut grunt = Enemy::new(EnemyArchetype::StreetGrunt);
        assert!(!grunt.apply_damage(100.0));
        assert_eq!(grunt.health_fraction(), 0.6);
        assert!(!grunt.apply_damage(f32::NAN) && !grunt.apply_damage(-5.0));
        assert!(grunt.apply_damage(500.0));
        assert_eq!(grunt.health, 0.0);
        assert!(!grunt.apply_damage(10.0), "already defeated");
    }

    /// Runs one tree tick with a fixed context and returns the leaves hit.
    fn trace(archetype: EnemyArchetype, ctx: &AiContext) -> (BehaviorStatus, AiIntent) {
        struct Open;
        impl AiWorld for Open {
            fn clear_line(&self, _: Vec3, _: Vec3) -> bool {
                true
            }
        }
        let mut tree = BehaviorTree::new(behavior_tree_for(archetype));
        let mut bb = AiBlackboard::default();
        let mut intent = AiIntent::default();
        let status = tree.tick(0.0, &mut |a| {
            run_action(a, ctx, &mut bb, &mut intent, &Open)
        });
        (status, intent)
    }

    fn combat_ctx(distance: f32) -> AiContext {
        AiContext {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            health_fraction: 1.0,
            alert_state: AlertState::Combat,
            target: Some(TargetInfo {
                entity: Entity::from_raw(3),
                position: Vec3::new(0.0, 0.0, -distance),
                attacking: false,
                visible: true,
            }),
            movement: MovementSense {
                grounded: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn archetype_trees_pick_their_signature_moves() {
        let (_, grunt) = trace(EnemyArchetype::StreetGrunt, &combat_ctx(10.0));
        assert!(grunt.grenade.is_some(), "grunt opens with an EMP grenade");

        let (_, ninja) = trace(EnemyArchetype::CyberNinja, &combat_ctx(1.5));
        assert!(ninja.melee_light, "ninja slashes in reach");
        let (_, ninja_far) = trace(EnemyArchetype::CyberNinja, &combat_ctx(20.0));
        assert!(
            ninja_far.sprint && !ninja_far.melee_light,
            "ninja closes the gap"
        );

        let (_, heavy) = trace(EnemyArchetype::ShieldedHeavy, &combat_ctx(10.0));
        assert!(heavy.trigger_held, "shotgun inside 12 m");
        let (_, heavy_far) = trace(EnemyArchetype::ShieldedHeavy, &combat_ctx(20.0));
        assert!(!heavy_far.trigger_held && heavy_far.move_direction != Vec2::ZERO);

        let (_, netrunner) = trace(EnemyArchetype::Netrunner, &combat_ctx(10.0));
        assert_eq!(
            netrunner.hack.map(|h| h.status),
            Some(StatusEffectType::CryoGlitch)
        );

        let (_, mecha) = trace(EnemyArchetype::MechaEnforcer, &combat_ctx(4.0));
        assert_eq!(
            mecha.special_attack.map(|a| a.name),
            Some("Mecha_Ground_Stomp")
        );

        let hurt_drone = AiContext {
            health_fraction: 0.2,
            ..combat_ctx(10.0)
        };
        assert!(
            trace(EnemyArchetype::SwarmDrone, &hurt_drone).1.charge,
            "kamikaze when hurt"
        );
        assert!(
            !trace(EnemyArchetype::SwarmDrone, &combat_ctx(10.0))
                .1
                .charge
        );
    }

    #[test]
    fn ninja_guards_against_incoming_swings() {
        let mut ctx = combat_ctx(1.8);
        if let Some(t) = ctx.target.as_mut() {
            t.attacking = true;
        }
        let (_, intent) = trace(EnemyArchetype::CyberNinja, &ctx);
        assert!(intent.guard && !intent.melee_light);
    }

    #[test]
    fn unaware_enemies_patrol_or_idle() {
        let calm = AiContext {
            alert_state: AlertState::Unaware,
            target: None,
            ..combat_ctx(10.0)
        };
        for archetype in ALL {
            let (status, intent) = trace(archetype, &calm);
            assert_eq!(
                status,
                BehaviorStatus::Failure,
                "{archetype:?}: no waypoints, nothing to do"
            );
            assert!(!intent.trigger_held && !intent.melee_light);
        }
    }

    #[test]
    fn view_turns_at_the_archetype_rate() {
        let intent = AiIntent {
            look_at: Some(Vec3::new(0.0, -SIGHT_TARGET_OFFSET, 10.0)),
            ..Default::default()
        };
        let mut view = ViewAngles::default();
        steer_view(&mut view, Vec3::ZERO, &intent, 60.0, 0.5);
        assert!(
            (view.yaw.abs().to_degrees() - 30.0).abs() < 1e-3,
            "60 deg/s for 0.5 s"
        );
        for _ in 0..10 {
            steer_view(&mut view, Vec3::ZERO, &intent, 60.0, 0.5);
        }
        assert!(
            (view.yaw.abs() - std::f32::consts::PI).abs() < 1e-3,
            "faces +Z"
        );
    }

    #[test]
    fn lobbed_grenades_land_on_target() {
        let from = Vec3::new(0.0, 1.7, 0.0);
        let to = Vec3::new(0.0, 0.9, -12.0);
        let mut position = from;
        let mut velocity = lob_velocity(from, to, GRENADE_THROW_SPEED);
        let dt = 1.0 / 64.0;
        let mut closest = f32::INFINITY;
        for _ in 0..200 {
            velocity.y -= PROJECTILE_GRAVITY * dt;
            position += velocity * dt;
            closest = closest.min(position.distance(to));
        }
        assert!(closest < 0.3, "arc passes through the target: {closest}");
    }
}
