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
use std::f32::consts::{PI, TAU};

const BLEND_RATE: f32 = 10.0;
/// How fast a landing squash recovers, 1/s.
const LAND_RECOVERY: f32 = 3.2;

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
    /// Running along a wall.
    pub wall_run: bool,
    /// Climbing over an edge (mantle or vault).
    pub mantle: bool,
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
    /// Smoothed vertical speed, -1 falling to 1 rising.
    pub rise: f32,
    /// Landing squash, 1 on a hard touchdown, recovering to 0.
    pub land: f32,
    /// Wall-run and mantle blends.
    pub wall: f32,
    pub mantle: f32,
    /// Soft-tissue offset per side (x: bounce, y: sway), radians.
    pub jiggle: [Vec2; 2],
    pub jiggle_velocity: [Vec2; 2],
    pub last_vertical: f32,
    pub last_drop: f32,
    pub drop_velocity: f32,
    /// World position last frame and the smoothed velocity estimated from
    /// it (for bodies whose owner has no simulated velocity, such as
    /// remote co-op players).
    pub last_position: Option<Vec3>,
    pub estimated_velocity: Vec3,
}

/// Spring stiffness and damping of soft tissue (about 1.6 Hz, settles in ~1 s).
const JIGGLE_STIFFNESS: f32 = 100.0;
const JIGGLE_DAMPING: f32 = 7.0;
/// Radians of bounce per m/s^2 of vertical acceleration.
const JIGGLE_GAIN: f32 = 0.012;
const JIGGLE_LIMIT: f32 = 0.22;

impl PoseState {
    /// Advances the soft-tissue springs from the body's vertical
    /// acceleration and gait sway; `softness` 0 keeps them still.
    pub fn step_jiggle(&mut self, vertical_accel: f32, softness: f32, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let sway = (self.phase).cos() * 0.6 * self.gait * (1.0 + self.sprint);
        for side in 0..2 {
            let sign = if side == 0 { 1.0 } else { -1.0 };
            let drive = Vec2::new(
                vertical_accel * JIGGLE_GAIN,
                sway * sign * JIGGLE_GAIN * 4.0,
            ) * softness;
            let accel = -JIGGLE_STIFFNESS * self.jiggle[side]
                - JIGGLE_DAMPING * self.jiggle_velocity[side]
                + drive * JIGGLE_STIFFNESS;
            self.jiggle_velocity[side] += accel * dt;
            self.jiggle[side] = (self.jiggle[side] + self.jiggle_velocity[side] * dt)
                .clamp(Vec2::splat(-JIGGLE_LIMIT), Vec2::splat(JIGGLE_LIMIT));
        }
    }
}

/// How much a look's chest moves: feminine bodies, larger and softer more.
pub fn jiggle_softness(look: &crate::character::CharacterAppearance) -> f32 {
    let feminine = ((look.slider("gender") + 0.2) / 1.0).clamp(0.0, 1.0);
    let size = 0.5 + 0.5 * look.slider("cupsize");
    let firm = 0.5 + 0.5 * look.slider("firmness");
    feminine * (0.35 + 0.65 * size) * (1.0 - 0.55 * firm)
}

impl PoseState {
    pub fn advance(&mut self, input: &PoseInputs, dt: f32) {
        let t = 1.0 - (-BLEND_RATE * dt).exp();
        let ease = |v: &mut f32, target: f32| *v += (target - *v) * t;
        let moving = if input.wall_run {
            // Legs keep running along the wall.
            1.0
        } else if input.sliding || input.airborne {
            0.0
        } else {
            (input.speed / WALK_SPEED).min(1.0)
        };
        // Touchdown: a squash scaled by how hard the body came down.
        if self.air > 0.5 && !input.airborne && !input.sliding {
            self.land = (-self.last_vertical / 9.0).clamp(0.35, 1.0);
        }
        self.land = (self.land - dt * LAND_RECOVERY).max(0.0);
        ease(&mut self.gait, moving);
        ease(
            &mut self.sprint,
            ((input.speed - WALK_SPEED) / (SPRINT_SPEED - WALK_SPEED)).clamp(0.0, 1.0),
        );
        let flying = input.airborne && !input.wall_run && !input.mantle;
        ease(&mut self.air, if flying { 1.0 } else { 0.0 });
        ease(&mut self.wall, if input.wall_run { 1.0 } else { 0.0 });
        ease(&mut self.mantle, if input.mantle { 1.0 } else { 0.0 });
        ease(&mut self.rise, (input.vertical / 5.0).clamp(-1.0, 1.0));
        ease(&mut self.slide, if input.sliding { 1.0 } else { 0.0 });
        ease(&mut self.guard, if input.guarding { 1.0 } else { 0.0 });
        self.time += dt;
        self.phase =
            (self.phase + input.speed / stride_length(input.speed) * TAU * dt).rem_euclid(TAU);
    }

    /// 1 standing still on the ground, 0 moving, airborne or sliding.
    fn idle(&self) -> f32 {
        ((1.0 - self.gait) * (1.0 - self.air) * (1.0 - self.slide)).clamp(0.0, 1.0)
    }
}

/// Metres per full gait cycle (two steps): longer strides at speed, so the
/// cadence climbs from a jog to a sprint instead of the legs windmilling.
pub fn stride_length(speed: f32) -> f32 {
    1.7 + 0.36 * speed
}

/// Desired rig-space directions per limb bone for the current state.
pub fn limb_targets(state: &PoseState, input: &PoseInputs) -> Vec<(&'static str, Vec3)> {
    let gait = state.gait;
    let sprint = state.sprint * gait;
    let idle = state.idle();
    let air = state.air;
    let land = state.land;
    // Weight shift while standing: slow, so the stance never looks frozen.
    let shift = (state.time * 0.55).sin() * idle;
    let down = |side: f32, out: f32| Vec3::new(side * out, -1.0, 0.0).normalize();
    let rot_x = |angle: f32, v: Vec3| Quat::from_rotation_x(angle) * v;

    let mut out = Vec::with_capacity(12);
    for (side, thigh_name, calf_name, foot_name, offset) in [
        (1.0f32, "thigh_l", "calf_l", "foot_l", 0.0),
        (-1.0, "thigh_r", "calf_r", "foot_r", PI),
    ] {
        let s = state.phase + offset;
        // How far forward this leg is (-1 back, 1 front) and whether it is
        // travelling forward (foot in the air) or back (planted).
        let front = s.sin();
        let swinging = s.cos();
        let reach = 0.42 + 0.3 * sprint;
        let thigh_gait = -front * reach * if front > 0.0 { 1.0 } else { 0.75 };
        let knee_gait = 0.12
            + (0.75 + 0.75 * sprint) * swinging.max(0.0).powf(1.4)
            + 0.12 * (1.0 - swinging.abs());
        // Airborne: rising tucks one knee up, falling reaches for the ground.
        let lead = if side > 0.0 { 1.0 } else { 0.0 };
        let thigh_rise = -0.95 * lead - 0.1;
        let knee_rise = 1.35 * lead + 0.7 * (1.0 - lead);
        let thigh_fall = -0.32 - 0.18 * lead;
        let knee_fall = 0.35;
        let rising = state.rise.max(0.0);
        let thigh_air = thigh_fall + (thigh_rise - thigh_fall) * rising;
        let knee_air = knee_fall + (knee_rise - knee_fall) * rising;
        // Standing: the loaded leg straight, the other knee eased.
        let loaded = if side > 0.0 { shift } else { -shift };
        let thigh_idle = -0.04 + 0.05 * (-loaded).max(0.0);
        let knee_idle = 0.06 + 0.16 * (-loaded).max(0.0);
        let slide_thigh = if side > 0.0 { -1.35 } else { -0.55 };
        let slide_knee = if side > 0.0 { 0.1 } else { 1.5 };

        let mut thigh = thigh_gait * gait + thigh_air * air + thigh_idle * idle;
        let mut knee = knee_gait * gait + knee_air * air + knee_idle * idle;
        thigh = thigh * (1.0 - state.slide) + slide_thigh * state.slide;
        knee = knee * (1.0 - state.slide) + slide_knee * state.slide;
        // Mantle: knees drawn up to clear the edge.
        thigh = thigh * (1.0 - state.mantle) - 1.15 * state.mantle;
        knee = knee * (1.0 - state.mantle) + 1.75 * state.mantle;
        // Landing squash: hips sink, knees fold.
        thigh -= 0.45 * land;
        knee += 0.9 * land;
        let thigh_dir = rot_x(thigh, down(side, 0.05 + 0.03 * idle));
        let calf_dir = rot_x(knee, thigh_dir);
        // Feet stay near level: toe-off pushes the toe down, heel strike
        // lifts it, in the air the toes point down.
        let toe = 0.35 * (-front).max(0.0) * swinging.max(0.0) * gait
            - 0.2 * front.max(0.0) * (1.0 - swinging.abs()) * gait
            + 0.45 * air;
        let foot_dir = rot_x(toe, Vec3::new(0.0, -0.35, 1.0).normalize());
        out.push((thigh_name, thigh_dir));
        out.push((calf_name, calf_dir));
        out.push((foot_name, foot_dir));
    }

    for (side, upper, lower, offset) in [
        (1.0f32, "upperarm_l", "lowerarm_l", PI),
        (-1.0, "upperarm_r", "lowerarm_r", 0.0),
    ] {
        // Arms swing against the opposite leg.
        let front = (state.phase + offset).sin();
        let swing = -front * (0.32 + 0.5 * sprint) * gait;
        let breath = (state.time * 1.7).sin() * 0.02 * idle;
        // Arms out for balance in the air and along a wall.
        let hang = 0.13
            + 0.06 * idle
            + 0.5 * air * (1.0 - state.rise.max(0.0))
            + 0.2 * land
            + 0.8 * state.wall;
        let mut arm = rot_x(
            swing - 0.06 * idle + breath - 0.5 * air * state.rise.max(0.0),
            down(side, hang),
        );
        let mut elbow =
            -0.28 * idle - (0.35 + 1.05 * sprint) * gait - 0.25 * front.max(0.0) * gait - 0.5 * air;
        if state.mantle > 0.01 {
            // Both hands reach ahead and up to plant on the edge.
            let plant = Vec3::new(side * 0.28, 0.5, 1.0).normalize();
            arm = arm.lerp(plant, state.mantle).normalize();
            elbow = elbow * (1.0 - state.mantle) - 0.55 * state.mantle;
        }
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

/// Extra rig-space rotations for the hips, spine and head.
fn spine_offsets(state: &PoseState) -> [(&'static str, Quat); 5] {
    let gait = state.gait;
    let idle = state.idle();
    let lean = 0.07 * gait + 0.22 * state.sprint * gait - 0.45 * state.slide
        + 0.35 * state.mantle
        + 0.12 * state.wall
        + 0.28 * state.land
        + 0.12 * state.air * state.rise.max(0.0);
    // Hips turn with the forward leg and the shoulders counter-rotate.
    let twist = state.phase.sin() * (0.1 + 0.06 * state.sprint) * gait;
    // Hips roll over the stance leg; at rest, over the loaded leg.
    let roll = state.phase.cos() * 0.05 * gait + (state.time * 0.55).sin() * 0.035 * idle;
    let breathe = (state.time * 1.7).sin() * 0.018 * (0.6 + 0.4 * idle);
    // A slow look around while standing.
    let look = (state.time * 0.23).sin() * 0.16 * idle + (state.time * 0.61).sin() * 0.04 * idle;
    [
        (
            "pelvis",
            Quat::from_rotation_y(-twist * 0.8) * Quat::from_rotation_z(roll),
        ),
        (
            "spine_01",
            Quat::from_rotation_x(lean * 0.4) * Quat::from_rotation_z(-roll * 0.6),
        ),
        (
            "spine_02",
            Quat::from_rotation_y(twist) * Quat::from_rotation_x(lean * 0.4 + breathe),
        ),
        (
            "spine_03",
            Quat::from_rotation_x(lean * 0.2 - breathe * 0.5),
        ),
        (
            "neck_01",
            Quat::from_rotation_y(look - twist * 0.6) * Quat::from_rotation_x(-lean * 0.7),
        ),
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
    // Limbs aim at their named chain child (rigs also hang twist bones off
    // them); anything else at its first child.
    let child_of = |i: usize| {
        let wanted = match skeleton.names[i].as_str() {
            "thigh_l" => Some("calf_l"),
            "thigh_r" => Some("calf_r"),
            "calf_l" => Some("foot_l"),
            "calf_r" => Some("foot_r"),
            "upperarm_l" => Some("lowerarm_l"),
            "upperarm_r" => Some("lowerarm_r"),
            "lowerarm_l" => Some("hand_l"),
            "lowerarm_r" => Some("hand_r"),
            _ => None,
        };
        wanted
            .and_then(|n| skeleton.index.get(n).copied())
            .filter(|&c| skeleton.parents[c] == Some(i))
            .or_else(|| (0..count).find(|&c| skeleton.parents[c] == Some(i)))
    };
    let mut done = vec![false; count];
    for _ in 0..count {
        for i in 0..count {
            if done[i] || skeleton.parents[i].is_some_and(|p| !done[p]) {
                continue;
            }
            let parent_world =
                skeleton.parents[i].map_or(skeleton.base_rotation, |p| posed_world[p]);
            let inherited = parent_world * skeleton.rest_local_rotation[i];
            let name = skeleton.names[i].as_str();
            let mut world = inherited;
            if let Some((_, offset)) = spine.iter().find(|(n, _)| *n == name) {
                world = *offset * inherited;
            }
            let side = match name {
                "breast_l" => Some(0),
                "breast_r" => Some(1),
                _ => None,
            };
            if let Some(side) = side {
                let j = state.jiggle[side];
                world = Quat::from_rotation_x(j.x) * Quat::from_rotation_z(j.y) * inherited;
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
    // Hips dip as the legs spread, sink into slides and landings.
    let drop = 0.42 * state.slide
        + state.gait * (0.025 + 0.035 * state.sprint) * state.phase.sin().abs()
        + 0.16 * state.land;
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
    roots: Query<(), Or<(With<OperativeBody>, With<super::rigged::RiggedModel>)>>,
    sources: Query<(
        Option<&KinematicVelocity>,
        Option<&ClientPredictionState>,
        Option<&MeleeCombatState>,
    )>,
    mut joints: Query<(&JointRest, &mut Transform)>,
    globals: Query<&GlobalTransform>,
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
        // Players drive their avatar's body; enemies drive their own.
        let source = parents.get(body.root).ok().and_then(|p| {
            let owner = avatars.get(p.get()).map_or(p.get(), |a| a.source);
            sources.get(owner).ok()
        });
        if let Ok(global) = globals.get(body.root) {
            let position = global.translation();
            if let Some(last) = state.last_position {
                if dt > 0.0 {
                    let raw = (position - last) / dt;
                    // Teleports and respawns are not motion.
                    let raw = if raw.length() > 40.0 { Vec3::ZERO } else { raw };
                    let blend = 1.0 - (-dt * 10.0).exp();
                    state.estimated_velocity = state.estimated_velocity.lerp(raw, blend);
                }
            }
            state.last_position = Some(position);
        }
        let estimated = state.estimated_velocity;
        let input = source.map_or_else(PoseInputs::default, |(velocity, prediction, melee)| {
            let movement = prediction.map(|p| p.movement);
            let velocity = velocity.map_or(estimated, |v| v.0);
            PoseInputs {
                speed: Vec2::new(velocity.x, velocity.z).length(),
                airborne: movement.is_some_and(|m| !m.grounded),
                sliding: movement.is_some_and(|m| m.sliding),
                wall_run: movement.is_some_and(|m| m.on_wall),
                mantle: movement.is_some_and(|m| m.mantle_ticks > 0),
                vertical: velocity.y,
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
        if dt > 0.0 {
            // Body vertical acceleration: velocity changes plus the gait bob
            // (a hip drop lowers the chest).
            let velocity_change = (input.vertical - state.last_vertical) / dt;
            let drop_velocity = (drop - state.last_drop) / dt;
            let drop_accel = (drop_velocity - state.drop_velocity) / dt;
            let accel = (velocity_change - drop_accel).clamp(-40.0, 40.0);
            let softness = body.applied.as_ref().map_or(0.0, jiggle_softness);
            state.step_jiggle(accel, softness, dt);
            state.last_vertical = input.vertical;
            state.last_drop = drop;
            state.drop_velocity = drop_velocity;
        }
        let pelvis = body.skeleton.index.get("pelvis").copied();
        for (i, joint) in body.skeleton.joints.iter().enumerate() {
            if let Ok((rest, mut transform)) = joints.get_mut(*joint) {
                transform.rotation = local[i];
                transform.translation = rest.translation;
                if Some(i) == pelvis {
                    // Drop the hips in rig space, expressed in the parent frame.
                    let parent_rot = body.skeleton.parents[i]
                        .map_or(body.skeleton.base_rotation, |p| {
                            body.skeleton.rest_world_rotation[p]
                        });
                    transform.translation +=
                        parent_rot.inverse() * Vec3::NEG_Y * drop * body.skeleton.units_per_metre;
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
            uvs: vec![],
            vid_weights: vec![],
            shaped: vec![],
            albedo: None,
            skeleton: Skeleton {
                joints: vec![Entity::PLACEHOLDER; 4],
                names,
                parents: vec![None, Some(0), Some(1), Some(2)],
                rest_local_rotation: vec![Quat::IDENTITY; 4],
                rest_world_rotation: vec![Quat::IDENTITY; 4],
                index,
                base_rotation: Quat::IDENTITY,
                units_per_metre: 1.0,
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
    fn mantling_plants_both_hands_ahead_and_up() {
        let state = PoseState {
            mantle: 1.0,
            ..default()
        };
        let targets = limb_targets(&state, &PoseInputs::default());
        for arm in ["upperarm_l", "upperarm_r"] {
            let (_, dir) = targets.iter().find(|(n, _)| *n == arm).expect("arm");
            assert!(dir.z > 0.6 && dir.y > 0.2, "{arm}: {dir}");
        }
        let (_, thigh) = targets
            .iter()
            .find(|(n, _)| *n == "thigh_l")
            .expect("thigh");
        assert!(thigh.z > 0.6, "knee drawn up: {thigh}");
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
    fn soft_tissue_bounces_then_settles() {
        let mut state = PoseState::default();
        state.step_jiggle(30.0, 1.0, 1.0 / 60.0);
        for _ in 0..5 {
            state.step_jiggle(30.0, 1.0, 1.0 / 60.0);
        }
        assert!(state.jiggle[0].x > 0.02, "an upward jolt pulls tissue down");
        for _ in 0..240 {
            state.step_jiggle(0.0, 1.0, 1.0 / 60.0);
        }
        assert!(state.jiggle[0].length() < 0.005, "settles within seconds");
        let mut firm = PoseState::default();
        firm.step_jiggle(30.0, 0.0, 1.0 / 60.0);
        assert_eq!(firm.jiggle[0], Vec2::ZERO, "no softness, no motion");
        assert!(state.jiggle[0].x.abs() <= JIGGLE_LIMIT);
    }

    #[test]
    fn softness_follows_the_look() {
        use crate::character::{CharacterAppearance, Preset};
        let masculine = CharacterAppearance::preset(Preset::Bruiser);
        let feminine = CharacterAppearance::preset(Preset::Wraith);
        assert_eq!(jiggle_softness(&masculine), 0.0);
        assert!(jiggle_softness(&feminine) > 0.2);
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
