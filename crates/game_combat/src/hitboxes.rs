//! Active-frame hitbox sweeps and damage resolution.
//!
//! While an attack is in its Active phase, an oriented box is placed at the
//! attacker's feet + `rotation * hitbox_offset` and overlapping colliders are
//! collected with Avian's `SpatialQuery::shape_intersections`. Each target is
//! hit at most once per swing. Combat dummies take damage directly;
//! fighters with `MeleeCombatState` resolve the hit through their guard.
//! Being swept every tick of the active window, the box covers the swing's
//! motion as it happens.

use crate::combat_dummy::{apply_dummy_damage, spawn_floating_combat_text, CombatDummy};
use crate::melee::{step_melee, AttackFrameData, MeleeCombatState, MeleeInput, ParryResult};
use avian3d::prelude::*;
use bevy::prelude::*;
use game_core::input::PlayerInputState;
use game_core::math::safe_normalize;
use game_core::protocol::InputActions;
use game_parkour::shapecasts::CHARACTER_HALF_HEIGHT;
use game_parkour::{KinematicVelocity, ParkourState};

/// A guard covers attacks from within 60 degrees of facing (120 degree arc).
pub const GUARD_ARC_COS: f32 = 0.5;
/// Knockback scale through a block.
pub const BLOCKED_KNOCKBACK_SCALE: f32 = 0.5;
/// Upward share of radial knockback from area attacks.
pub const AREA_KNOCKBACK_LIFT: f32 = 0.3;

/// One landed melee hit.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct MeleeHitEvent {
    pub attacker: Entity,
    pub target: Entity,
    pub attack_name: &'static str,
    /// Damage that got through after guard resolution.
    pub damage: f32,
    pub posture_damage: f32,
    pub parry_result: ParryResult,
    /// Knockback velocity applied to the target.
    pub knockback: Vec3,
}

/// World-space center and rotation of `attack`'s hitbox for an attacker
/// whose capsule center is at `attacker.translation`.
pub fn hitbox_pose(attacker: &Transform, attack: &AttackFrameData) -> (Vec3, Quat) {
    let feet = attacker.translation - Vec3::Y * CHARACTER_HALF_HEIGHT;
    (
        feet + attacker.rotation * attack.hitbox_offset,
        attacker.rotation,
    )
}

/// Whether `target` is inside an area attack's radius (always true for
/// non-area attacks).
pub fn within_area(attack: &AttackFrameData, center: Vec3, target: Vec3) -> bool {
    if attack.area_radius <= 0.0 {
        return true;
    }
    let offset = target - center;
    Vec2::new(offset.x, offset.z).length() <= attack.area_radius
}

/// Filters overlap candidates down to new targets for this swing and records
/// them in `hit_entities`.
pub fn register_new_hits(
    state: &mut MeleeCombatState,
    attacker: Entity,
    candidates: impl IntoIterator<Item = Entity>,
) -> Vec<Entity> {
    candidates
        .into_iter()
        .filter(|&e| e != attacker && state.hit_entities.insert(e))
        .collect()
}

/// Whether a defender facing along its transform covers an attack coming
/// from `attacker_position`.
pub fn guard_covers(defender: &Transform, attacker_position: Vec3) -> bool {
    let forward = defender.rotation * Vec3::NEG_Z;
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let to_attacker = attacker_position - defender.translation;
    let to_attacker = Vec3::new(to_attacker.x, 0.0, to_attacker.z).normalize_or_zero();
    forward.dot(to_attacker) >= GUARD_ARC_COS
}

/// Knockback velocity for a hit on a target at `target_position`.
pub fn knockback_vector(
    attack: &AttackFrameData,
    attacker_rotation: Quat,
    hitbox_center: Vec3,
    target_position: Vec3,
) -> Vec3 {
    let direction = if attack.area_radius > 0.0 {
        let radial = target_position - hitbox_center;
        let radial = Vec3::new(radial.x, 0.0, radial.z).normalize_or_zero();
        safe_normalize(radial + Vec3::Y * AREA_KNOCKBACK_LIFT, Vec3::Y)
    } else {
        safe_normalize(attacker_rotation * attack.knockback_direction, Vec3::ZERO)
    };
    direction * attack.knockback_impulse
}

/// Turns fresh packets into melee presses (FIRE -> light, MELEE_HEAVY ->
/// heavy, PARRY -> guard held). Each packet is read once.
pub fn apply_melee_input(mut fighters: Query<(&PlayerInputState, &mut MeleeInput)>) {
    for (state, mut input) in &mut fighters {
        let Some(packet) = state.latest() else {
            continue;
        };
        if input.last_packet_tick == Some(packet.tick) {
            continue;
        }
        let pressed = packet.actions & !input.previous_actions;
        input.light_pressed |= pressed & InputActions::FIRE != 0;
        input.heavy_pressed |= pressed & InputActions::MELEE_HEAVY != 0;
        input.parry_held = packet.has_action(InputActions::PARRY);
        input.previous_actions = packet.actions;
        input.last_packet_tick = Some(packet.tick);
    }
}

struct PendingHit {
    attacker: Entity,
    attacker_position: Vec3,
    attacker_rotation: Quat,
    hitbox_center: Vec3,
    attack: AttackFrameData,
    target: Entity,
}

type FighterQuery<'a> = (
    Entity,
    &'a mut MeleeCombatState,
    &'a mut MeleeInput,
    &'a Transform,
    Option<&'a ParkourState>,
    Option<&'a mut KinematicVelocity>,
);

/// Steps every fighter's melee state, sweeps active hitboxes and resolves
/// hits against dummies and other fighters.
#[allow(clippy::too_many_arguments)]
pub fn update_melee_combat_simulation(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    mut hits_out: EventWriter<MeleeHitEvent>,
    mut fighters: Query<FighterQuery>,
    mut dummies: Query<(&mut CombatDummy, &Transform), Without<MeleeCombatState>>,
    transforms: Query<&Transform>,
) {
    let dt = time.delta_secs();
    let mut pending = Vec::new();

    for (entity, mut state, mut input, transform, parkour, velocity) in &mut fighters {
        let outcome = step_melee(&mut state, &mut input, parkour.copied(), dt);
        if let (Some(started), Some(mut velocity)) = (outcome.started, velocity) {
            if started.attacker_dive_speed > 0.0 {
                velocity.0.y = -started.attacker_dive_speed;
            }
        }
        if !outcome.hitbox_active {
            continue;
        }
        let Some(attack) = state.current_attack else {
            continue;
        };
        let (center, rotation) = hitbox_pose(transform, &attack);
        let shape = Collider::cuboid(
            attack.hitbox_extents.x,
            attack.hitbox_extents.y,
            attack.hitbox_extents.z,
        );
        let filter = SpatialQueryFilter::default().with_excluded_entities([entity]);
        let candidates = spatial
            .shape_intersections(&shape, center, rotation, &filter)
            .into_iter()
            .filter(|target| {
                transforms
                    .get(*target)
                    .is_ok_and(|t| within_area(&attack, center, t.translation))
            });
        for target in register_new_hits(&mut state, entity, candidates) {
            pending.push(PendingHit {
                attacker: entity,
                attacker_position: transform.translation,
                attacker_rotation: rotation,
                hitbox_center: center,
                attack,
                target,
            });
        }
    }

    for hit in pending {
        let attack = &hit.attack;
        if let Ok((mut dummy, dummy_transform)) = dummies.get_mut(hit.target) {
            let outcome =
                apply_dummy_damage(&mut dummy, attack.base_damage, false, attack.posture_damage);
            spawn_floating_combat_text(
                &mut commands,
                dummy_transform.translation,
                outcome.damage,
                outcome.is_critical,
            );
            hits_out.send(MeleeHitEvent {
                attacker: hit.attacker,
                target: hit.target,
                attack_name: attack.name,
                damage: outcome.damage,
                posture_damage: attack.posture_damage,
                parry_result: ParryResult::Hit,
                knockback: knockback_vector(
                    attack,
                    hit.attacker_rotation,
                    hit.hitbox_center,
                    dummy_transform.translation,
                ),
            });
            continue;
        }

        let mut posture_to_attacker = 0.0;
        let resolved = fighters.get_mut(hit.target).map(
            |(_, mut defender, _, defender_transform, _, velocity)| {
                let covered = guard_covers(defender_transform, hit.attacker_position);
                let (result, damage) = defender.resolve_incoming_attack_from(
                    attack.base_damage,
                    attack.posture_damage,
                    &mut posture_to_attacker,
                    covered,
                );
                let scale = match result {
                    ParryResult::PerfectParry => 0.0,
                    ParryResult::Blocked => BLOCKED_KNOCKBACK_SCALE,
                    ParryResult::Hit => 1.0,
                };
                let knockback = knockback_vector(
                    attack,
                    hit.attacker_rotation,
                    hit.hitbox_center,
                    defender_transform.translation,
                ) * scale;
                if let Some(mut velocity) = velocity {
                    velocity.0 += knockback;
                }
                (result, damage, knockback, defender_transform.translation)
            },
        );
        let Ok((result, damage, knockback, target_position)) = resolved else {
            // Not a combat target (scenery, triggers).
            continue;
        };

        if let Ok((_, mut attacker, ..)) = fighters.get_mut(hit.attacker) {
            attacker.add_posture(posture_to_attacker);
            if result == ParryResult::PerfectParry {
                // Deflected: the swing is staggered out.
                attacker.cancel_attack();
            }
        }
        if damage > 0.0 {
            spawn_floating_combat_text(&mut commands, target_position, damage, false);
        }
        let posture_damage = match result {
            ParryResult::PerfectParry => 0.0,
            ParryResult::Blocked => attack.posture_damage * crate::melee::BLOCK_POSTURE_FRACTION,
            ParryResult::Hit => attack.posture_damage,
        };
        hits_out.send(MeleeHitEvent {
            attacker: hit.attacker,
            target: hit.target,
            attack_name: attack.name,
            damage,
            posture_damage,
            parry_result: result,
            knockback,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combo_trees::{CYBER_KATANA_COMBO_TREE, SLIDE_UPPERCUT, WALL_RUN_PLUNGE};
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn hitbox_sits_in_front_at_chest_height() {
        let attacker = Transform::from_xyz(0.0, 0.9, 0.0);
        let (center, _) = hitbox_pose(&attacker, &CYBER_KATANA_COMBO_TREE[0]);
        assert!(
            center.abs_diff_eq(Vec3::new(0.0, 1.0, -1.0), 1e-5),
            "{center:?}"
        );

        // Facing +X (yaw -90 degrees): the box swings round with the attacker.
        let turned =
            Transform::from_xyz(0.0, 0.9, 0.0).with_rotation(Quat::from_rotation_y(-FRAC_PI_2));
        let (center, rotation) = hitbox_pose(&turned, &CYBER_KATANA_COMBO_TREE[0]);
        assert!(
            center.abs_diff_eq(Vec3::new(1.0, 1.0, 0.0), 1e-5),
            "{center:?}"
        );
        assert_eq!(rotation, turned.rotation);
    }

    #[test]
    fn each_target_is_hit_once_per_swing() {
        let mut s = MeleeCombatState::default();
        s.start_attack(CYBER_KATANA_COMBO_TREE[0]);
        let me = Entity::from_raw(1);
        let a = Entity::from_raw(2);
        let b = Entity::from_raw(3);
        assert_eq!(
            register_new_hits(&mut s, me, [me, a, b]),
            vec![a, b],
            "never hits self"
        );
        assert!(
            register_new_hits(&mut s, me, [a, b]).is_empty(),
            "already hit this swing"
        );
        s.start_attack(CYBER_KATANA_COMBO_TREE[1]);
        assert_eq!(register_new_hits(&mut s, me, [a]), vec![a]);
    }

    #[test]
    fn area_attacks_are_radius_limited() {
        let plunge = WALL_RUN_PLUNGE;
        assert!(within_area(&plunge, Vec3::ZERO, Vec3::new(3.0, -2.0, 2.0)));
        assert!(
            !within_area(&plunge, Vec3::ZERO, Vec3::new(3.5, 0.0, 3.5)),
            "box corner"
        );
        assert!(within_area(
            &CYBER_KATANA_COMBO_TREE[0],
            Vec3::ZERO,
            Vec3::splat(100.0)
        ));
    }

    #[test]
    fn guard_arc_is_frontal() {
        let defender = Transform::from_xyz(0.0, 0.9, 0.0); // facing -Z
        assert!(guard_covers(&defender, Vec3::new(0.0, 0.9, -2.0)));
        assert!(
            guard_covers(&defender, Vec3::new(1.0, 0.9, -2.0)),
            "26 degrees off"
        );
        assert!(!guard_covers(&defender, Vec3::new(2.0, 0.9, 0.0)), "flank");
        assert!(!guard_covers(&defender, Vec3::new(0.0, 0.9, 2.0)), "behind");
        let turned = defender.with_rotation(Quat::from_rotation_y(PI));
        assert!(guard_covers(&turned, Vec3::new(0.0, 0.9, 2.0)));
    }

    #[test]
    fn knockback_directions() {
        let forward = knockback_vector(
            &CYBER_KATANA_COMBO_TREE[0],
            Quat::IDENTITY,
            Vec3::ZERO,
            Vec3::ZERO,
        );
        assert!(forward.abs_diff_eq(Vec3::new(0.0, 0.0, -3.5), 1e-5));

        let up = knockback_vector(&SLIDE_UPPERCUT, Quat::IDENTITY, Vec3::ZERO, Vec3::ZERO);
        assert!(
            up.y > up.z.abs() && (up.length() - 11.0).abs() < 1e-4,
            "uppercut launches: {up:?}"
        );

        let radial = knockback_vector(
            &WALL_RUN_PLUNGE,
            Quat::IDENTITY,
            Vec3::ZERO,
            Vec3::new(3.0, 0.0, 0.0),
        );
        assert!(
            radial.x > 0.0 && radial.y > 0.0 && radial.z.abs() < 1e-5,
            "outward: {radial:?}"
        );
    }

    #[test]
    fn packets_become_melee_presses_once() {
        use game_core::protocol::{NetworkTick, PlayerInputPacket};
        let mut app = App::new();
        app.add_systems(Update, apply_melee_input);
        let mut state = PlayerInputState::default();
        state.push(PlayerInputPacket {
            tick: NetworkTick(1),
            actions: InputActions::FIRE | InputActions::PARRY,
            ..Default::default()
        });
        let e = app.world_mut().spawn((state, MeleeInput::default())).id();
        app.update();
        let input = *app.world().get::<MeleeInput>(e).expect("input");
        assert!(input.light_pressed && input.parry_held && !input.heavy_pressed);

        // Same packet again: no new press.
        app.world_mut()
            .get_mut::<MeleeInput>(e)
            .expect("input")
            .light_pressed = false;
        app.update();
        assert!(
            !app.world()
                .get::<MeleeInput>(e)
                .expect("input")
                .light_pressed
        );

        // Held FIRE in the next packet is not a new press; MELEE_HEAVY is.
        app.world_mut()
            .get_mut::<PlayerInputState>(e)
            .expect("state")
            .push(PlayerInputPacket {
                tick: NetworkTick(2),
                actions: InputActions::FIRE | InputActions::MELEE_HEAVY,
                ..Default::default()
            });
        app.update();
        let input = *app.world().get::<MeleeInput>(e).expect("input");
        assert!(!input.light_pressed && input.heavy_pressed && !input.parry_held);
    }
}
