//! Procedural animation for operative bodies. Limbs are aimed at desired
//! directions in rig space (the body faces +Z, its left is +X), so poses
//! hold on every body shape and do not depend on bone roll.

use super::body::{BodyInstance, JointRest, OperativeBody};
use crate::client_network::ClientPredictionState;
use crate::visuals::OperativeAvatar;
use bevy::prelude::*;
use game_combat::{AttackPhase, MeleeCombatState};
use game_parkour::KinematicVelocity;
use game_server::{SPRINT_SPEED, WALK_SPEED};

/// Stride length (one full cycle covers two steps), metres.
const STRIDE: f32 = 2.6;
const BLEND_RATE: f32 = 10.0;

/// Gameplay state a pose is built from.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PoseInputs {
    pub speed: f32,
    pub airborne: bool,
    pub sliding: bool,
    pub vertical: f32,
    /// Combo index and progress (0..1) of the running attack.
    pub attack: Option<(u8, f32)>,
    pub guarding: bool,
}

/// Smoothed animation state of one body.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct PoseState {
    pub phase: f32,
    pub time: f32,
    pub gait: f32,
    pub sprint: f32,
    pub air: f32,
    pub slide: f32,
    pub guard: f32,
}

impl PoseState {
    pub fn advance(&mut self, input: &PoseInputs, dt: f32) {
        let t = 1.0 - (-BLEND_RATE * dt).exp();
        let ease = |v: &mut f32, target: f32| *v += (target - *v) * t;
        let moving = if input.sliding || input.airborne {
            0.0
        } else {
            (input.speed / WALK_SPEED).min(1.0)
        };
        ease(&mut self.gait, moving);
        ease(
            &mut self.sprint,
            ((input.speed - WALK_SPEED) / (SPRINT_SPEED - WALK_SPEED)).clamp(0.0, 1.0),
        );
        ease(&mut self.air, if input.airborne { 1.0 } else { 0.0 });
        ease(&mut self.slide, if input.sliding { 1.0 } else { 0.0 });
        ease(&mut self.guard, if input.guarding { 1.0 } else { 0.0 });
        self.time += dt;
        self.phase = (self.phase + input.speed / STRIDE * std::f32::consts::TAU * dt)
            .rem_euclid(std::f32::consts::TAU);
    }
}

/// Desired rig-space directions per limb bone for the current state.
pub fn limb_targets(state: &PoseState, input: &PoseInputs) -> Vec<(&'static str, Vec3)> {
    let swing = state.phase.sin();
    let gait = state.gait;
    let amp_leg = (0.45 + 0.35 * state.sprint) * gait;
    let amp_arm = (0.35 + 0.45 * state.sprint) * gait;
    let down = |side: f32, out: f32| Vec3::new(side * out, -1.0, 0.0).normalize();
    let rot_x = |angle: f32, v: Vec3| Quat::from_rotation_x(angle) * v;

    let mut out = Vec::with_capacity(8);
    for (side, name_thigh, name_calf, sign) in [
        (1.0, "thigh_l", "calf_l", -1.0),
        (-1.0, "thigh_r", "calf_r", 1.0),
    ] {
        // Forward swing is a negative rotation about X for a hanging limb.
        let leg_swing = sign * swing * amp_leg;
        let tuck = -0.55 * state.air;
        let slide_leg = if side > 0.0 { -1.35 } else { -0.55 };
        let thigh_angle = (leg_swing + tuck) * (1.0 - state.slide) + slide_leg * state.slide;
        let thigh = rot_x(thigh_angle, down(side, 0.06));
        let knee = (0.15 + 0.65 * (sign * (state.phase + 0.9).sin()).max(0.0) * gait)
            * (1.0 - state.slide)
            + 0.9 * state.air
            + if side > 0.0 { 0.1 } else { 1.5 } * state.slide;
        out.push((name_thigh, thigh));
        out.push((name_calf, rot_x(knee, thigh)));
    }

    for (side, upper, lower, sign) in [
        (1.0, "upperarm_l", "lowerarm_l", 1.0),
        (-1.0, "upperarm_r", "lowerarm_r", -1.0),
    ] {
        let arm_swing = sign * swing * amp_arm;
        let mut arm = rot_x(arm_swing, down(side, 0.16));
        let mut elbow = -0.25 - 0.9 * state.sprint * gait - 0.3 * state.air;
        if state.slide > 0.0 {
            let braced = Vec3::new(side * 0.8, -0.35, -0.3).normalize();
            arm = arm.lerp(braced, state.slide).normalize();
        }
        if side < 0.0 {
            if let Some((combo, progress)) = input.attack {
                // Sword arm: sweep across the body in the combo's plane.
                let angle = crate::combat_fx::swing_angle(combo, progress);
                let forward = Vec3::new(0.0, 0.15, 1.0).normalize();
                arm = Quat::from_rotation_y(-angle) * forward;
                if combo % 3 == 2 {
                    arm = Quat::from_rotation_x(-angle) * Vec3::new(0.0, 0.0, 1.0);
                }
                elbow = -0.2;
            } else if state.guard > 0.01 {
                let guard = Vec3::new(-0.2, 0.25, 1.0).normalize();
                arm = arm.lerp(guard, state.guard).normalize();
                elbow = elbow * (1.0 - state.guard) - 1.4 * state.guard;
            }
        }
        out.push((upper, arm));
        out.push((lower, rot_x(elbow, arm)));
    }
    out
}

/// Extra rig-space rotations for the spine and head.
fn spine_offsets(state: &PoseState) -> [(&'static str, Quat); 3] {
    let lean = 0.08 * state.gait + 0.18 * state.sprint * state.gait - 0.45 * state.slide;
    let twist = state.phase.sin() * 0.12 * state.gait;
    let breathe = (state.time * 1.6).sin() * 0.015;
    [
        ("spine_01", Quat::from_rotation_x(lean * 0.5)),
        (
            "spine_02",
            Quat::from_rotation_y(twist) * Quat::from_rotation_x(lean * 0.5 + breathe),
        ),
        ("neck_01", Quat::from_rotation_x(-lean * 0.6)),
    ]
}

/// Local joint rotations that realize the targets, plus the pelvis drop.
pub fn evaluate_pose(
    body: &BodyInstance,
    state: &PoseState,
    input: &PoseInputs,
) -> (Vec<Quat>, f32) {
    let skeleton = &body.skeleton;
    let count = skeleton.joints.len();
    let mut posed_world = vec![Quat::IDENTITY; count];
    let mut local = skeleton.rest_local_rotation.clone();
    let targets = limb_targets(state, input);
    let spine = spine_offsets(state);
    // Each limb bone points at its child joint.
    let child_of = |i: usize| (0..count).find(|&c| skeleton.parents[c] == Some(i));
    let mut done = vec![false; count];
    for _ in 0..count {
        for i in 0..count {
            if done[i] || skeleton.parents[i].is_some_and(|p| !done[p]) {
                continue;
            }
            let parent_world = skeleton.parents[i].map_or(Quat::IDENTITY, |p| posed_world[p]);
            let inherited = parent_world * skeleton.rest_local_rotation[i];
            let name = skeleton.names[i].as_str();
            let mut world = inherited;
            if let Some((_, offset)) = spine.iter().find(|(n, _)| *n == name) {
                world = *offset * inherited;
            }
            if let (Some((_, desired)), Some(child)) =
                (targets.iter().find(|(n, _)| *n == name), child_of(i))
            {
                if let (Some(head), Some(tail)) =
                    (body.rest_heads.get(i), body.rest_heads.get(child))
                {
                    let rest_dir = (*tail - *head).normalize_or_zero();
                    if rest_dir != Vec3::ZERO {
                        let bone_local = skeleton.rest_world_rotation[i].inverse() * rest_dir;
                        let current = inherited * bone_local;
                        world = Quat::from_rotation_arc(current, *desired) * inherited;
                    }
                }
            }
            posed_world[i] = world;
            local[i] = parent_world.inverse() * world;
            done[i] = true;
        }
    }
    let drop = 0.42 * state.slide + 0.04 * state.gait * (2.0 * state.phase).cos().abs();
    (local, drop)
}

/// Poses every prepared body from its avatar's gameplay state (bodies
/// without one, like the creator preview, idle).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn animate_bodies(
    time: Res<Time>,
    mut commands: Commands,
    mut bodies: Query<(Entity, &BodyInstance, Option<&mut PoseState>)>,
    parents: Query<&Parent>,
    avatars: Query<&OperativeAvatar>,
    roots: Query<(), With<OperativeBody>>,
    sources: Query<(
        &KinematicVelocity,
        Option<&ClientPredictionState>,
        Option<&MeleeCombatState>,
    )>,
    mut joints: Query<(&JointRest, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, body, state) in &mut bodies {
        if body.rest_heads.is_empty() || !roots.contains(body.root) {
            continue;
        }
        let Some(mut state) = state else {
            commands.entity(entity).insert(PoseState::default());
            continue;
        };
        let source = parents
            .get(body.root)
            .ok()
            .and_then(|p| avatars.get(p.get()).ok())
            .and_then(|a| sources.get(a.source).ok());
        let input = source.map_or_else(PoseInputs::default, |(velocity, prediction, melee)| {
            let movement = prediction.map(|p| p.movement);
            PoseInputs {
                speed: Vec2::new(velocity.0.x, velocity.0.z).length(),
                airborne: movement.is_some_and(|m| !m.grounded),
                sliding: movement.is_some_and(|m| m.sliding),
                vertical: velocity.0.y,
                attack: melee.and_then(|m| {
                    let attack = m.current_attack?;
                    (m.attack_phase != AttackPhase::Idle).then(|| {
                        let progress = m.frame_counter as f32 / attack.total_frames().max(1) as f32;
                        (m.current_combo_index, progress.clamp(0.0, 1.0))
                    })
                }),
                guarding: melee.is_some_and(|m| m.is_parrying),
            }
        });
        state.advance(&input, dt);
        let (local, drop) = evaluate_pose(body, &state, &input);
        let pelvis = body.skeleton.index.get("pelvis").copied();
        for (i, joint) in body.skeleton.joints.iter().enumerate() {
            if let Ok((rest, mut transform)) = joints.get_mut(*joint) {
                transform.rotation = local[i];
                transform.translation = rest.translation;
                if Some(i) == pelvis {
                    // Drop the hips in rig space, expressed in the parent frame.
                    let parent_rot = body.skeleton.parents[i]
                        .map_or(Quat::IDENTITY, |p| body.skeleton.rest_world_rotation[p]);
                    transform.translation += parent_rot.inverse() * Vec3::NEG_Y * drop;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::body::{Region, Skeleton};
    use std::collections::HashMap;

    /// Hip -> thigh -> calf -> foot, standing straight down the -Y axis.
    fn leg() -> BodyInstance {
        let names: Vec<String> = ["pelvis", "thigh_l", "calf_l", "foot_l"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let index: HashMap<String, usize> = names
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, n)| (n, i))
            .collect();
        BodyInstance {
            root: Entity::PLACEHOLDER,
            vids: vec![],
            indices: vec![],
            regions: vec![Region::Suit],
            skin_blend: vec![0.0],
            skeleton: Skeleton {
                joints: vec![Entity::PLACEHOLDER; 4],
                names,
                parents: vec![None, Some(0), Some(1), Some(2)],
                rest_local_rotation: vec![Quat::IDENTITY; 4],
                rest_world_rotation: vec![Quat::IDENTITY; 4],
                index,
            },
            rest_heads: vec![
                Vec3::new(0.1, 1.0, 0.0),
                Vec3::new(0.1, 0.95, 0.0),
                Vec3::new(0.1, 0.5, 0.0),
                Vec3::new(0.1, 0.05, 0.0),
            ],
            applied: None,
        }
    }

    fn world_dir(body: &BodyInstance, local: &[Quat], bone: usize, child: usize) -> Vec3 {
        let mut rot = Quat::IDENTITY;
        let mut chain = vec![bone];
        while let Some(p) = body.skeleton.parents[*chain.last().unwrap()] {
            chain.push(p);
        }
        for i in chain.iter().rev() {
            rot *= local[*i];
        }
        rot * (body.rest_heads[child] - body.rest_heads[bone]).normalize()
    }

    #[test]
    fn standing_leg_points_down_and_running_swings_it() {
        let body = leg();
        let idle = PoseState::default();
        let (local, drop) = evaluate_pose(&body, &idle, &PoseInputs::default());
        assert!(world_dir(&body, &local, 1, 2).y < -0.95, "thigh hangs down");
        assert!(drop.abs() < 1e-4);
        let stride = PoseState {
            phase: std::f32::consts::FRAC_PI_2,
            gait: 1.0,
            ..default()
        };
        let (local, _) = evaluate_pose(&body, &stride, &PoseInputs::default());
        assert!(
            world_dir(&body, &local, 1, 2).z > 0.3,
            "left thigh swings forward"
        );
    }

    #[test]
    fn slides_drop_the_hips_and_kick_the_leg_out() {
        let body = leg();
        let slide = PoseState {
            slide: 1.0,
            ..default()
        };
        let (local, drop) = evaluate_pose(&body, &slide, &PoseInputs::default());
        assert!(drop > 0.4);
        assert!(
            world_dir(&body, &local, 1, 2).z > 0.8,
            "leading leg extends forward"
        );
    }

    #[test]
    fn state_blends_toward_inputs() {
        let mut state = PoseState::default();
        let running = PoseInputs {
            speed: SPRINT_SPEED,
            ..default()
        };
        for _ in 0..60 {
            state.advance(&running, 1.0 / 60.0);
        }
        assert!(state.gait > 0.95 && state.sprint > 0.95);
        assert!(state.phase >= 0.0 && state.phase < std::f32::consts::TAU);
    }
}
