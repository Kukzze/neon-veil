//! Melee presentation: the drawn katana swings through each combo hit
//! with a light trail, the guard stance holds it across the body, slides
//! crouch the avatar, and the local character faces the camera yaw so
//! hitboxes sweep where the player is looking.

use crate::client_network::{ClientPredictionState, LocalPredictedPlayer};
use crate::visuals::{AvatarBody, OperativeAvatar, SheathedBlade, AVATAR_HIP_HEIGHT};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use game_combat::{AttackPhase, MeleeCombatState};
use game_core::input::ViewAngles;
use std::f32::consts::FRAC_PI_2;

/// Half the angle a horizontal swing sweeps, radians.
pub const SWING_HALF_ANGLE: f32 = 1.35;
/// Right shoulder, in the hip-pivoted body frame.
const SHOULDER: Vec3 = Vec3::new(0.27, 0.3, -0.05);
/// Distance from the shoulder to the blade's centre.
const BLADE_REACH: f32 = 0.62;
const SLIDE_HIP_HEIGHT: f32 = 0.5;
const SLIDE_LEAN_BACK: f32 = 0.5;
const POSE_EASE_RATE: f32 = 14.0;

/// The katana in hand, shown while attacking or guarding.
#[derive(Component, Debug, Clone, Copy)]
pub struct DrawnKatana {
    pub source: Entity,
}

/// Light arc left by the blade while its hitbox is live.
#[derive(Component, Debug, Clone, Copy)]
pub struct SlashTrail {
    pub source: Entity,
}

/// A flat ring sector in the XZ plane, centred on -Z.
pub fn slash_arc_mesh(inner: f32, outer: f32, half_angle: f32, segments: u32) -> Mesh {
    let segments = segments.max(1);
    let mut positions = Vec::with_capacity(2 * (segments as usize + 1));
    let mut uvs = Vec::with_capacity(positions.capacity());
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let angle = -half_angle + 2.0 * half_angle * t;
        let dir = Vec3::new(-angle.sin(), 0.0, -angle.cos());
        positions.push((dir * inner).to_array());
        positions.push((dir * outer).to_array());
        uvs.push([t, 0.0]);
        uvs.push([t, 1.0]);
    }
    let mut indices = Vec::with_capacity(6 * segments as usize);
    for i in 0..segments {
        let a = 2 * i;
        indices.extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// Plane each combo hit swings in, relative to the shoulder.
fn swing_plane(combo_index: u8) -> Quat {
    match combo_index % 3 {
        0 => Quat::from_rotation_x(-0.15),
        1 => Quat::from_rotation_x(-0.3) * Quat::from_rotation_z(0.35),
        // Overhead finisher: a vertical arc.
        _ => Quat::from_rotation_z(FRAC_PI_2),
    }
}

/// Blade direction within the swing plane at progress `t` in [0, 1].
fn swing_angle(combo_index: u8, t: f32) -> f32 {
    let eased = 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3);
    let (from, to) = match combo_index % 3 {
        0 => (-SWING_HALF_ANGLE, SWING_HALF_ANGLE),
        1 => (SWING_HALF_ANGLE, -SWING_HALF_ANGLE),
        _ => (-SWING_HALF_ANGLE, SWING_HALF_ANGLE * 0.7),
    };
    from + (to - from) * eased
}

/// Where the drawn blade sits for this melee state, if it is drawn.
pub fn katana_pose(state: &MeleeCombatState) -> Option<Transform> {
    let point_along = |rotation: Quat| {
        // Blade meshes run along +Y; point them along `rotation * -Z`.
        let blade = rotation * Quat::from_rotation_x(-FRAC_PI_2);
        Transform::from_translation(SHOULDER + rotation * Vec3::NEG_Z * BLADE_REACH)
            .with_rotation(blade)
    };
    match (state.current_attack, state.attack_phase) {
        (Some(attack), phase) if phase != AttackPhase::Idle => {
            let t = state.frame_counter as f32 / attack.total_frames().max(1) as f32;
            let index = state.current_combo_index;
            let plane = swing_plane(index);
            Some(point_along(
                plane * Quat::from_rotation_y(swing_angle(index, t)),
            ))
        }
        _ if state.is_parrying => {
            // Guard: blade held across the body, tip up and to the left.
            Some(point_along(
                Quat::from_rotation_y(0.9) * Quat::from_rotation_x(0.55),
            ))
        }
        _ => None,
    }
}

/// Orientation of the trail for the current hit while its hitbox is live.
pub fn trail_pose(state: &MeleeCombatState) -> Option<Transform> {
    (state.attack_phase == AttackPhase::Active).then(|| {
        Transform::from_translation(SHOULDER).with_rotation(swing_plane(state.current_combo_index))
    })
}

fn show(visibility: &mut Visibility, on: bool) {
    let wanted = if on {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *visibility != wanted {
        *visibility = wanted;
    }
}

#[allow(clippy::type_complexity)]
pub fn animate_katana(
    fighters: Query<&MeleeCombatState>,
    mut blades: Query<
        (&DrawnKatana, &mut Transform, &mut Visibility),
        (Without<SlashTrail>, Without<SheathedBlade>),
    >,
    mut trails: Query<
        (&SlashTrail, &mut Transform, &mut Visibility),
        (Without<DrawnKatana>, Without<SheathedBlade>),
    >,
    mut sheathed: Query<
        (&Parent, &mut Visibility),
        (
            With<SheathedBlade>,
            Without<DrawnKatana>,
            Without<SlashTrail>,
        ),
    >,
    bodies: Query<&Parent, With<AvatarBody>>,
    avatars: Query<&OperativeAvatar>,
) {
    let mut drawn_sources = Vec::new();
    for (blade, mut transform, mut visibility) in &mut blades {
        let pose = fighters.get(blade.source).ok().and_then(katana_pose);
        if let Some(pose) = pose {
            *transform = pose;
            drawn_sources.push(blade.source);
        }
        show(&mut visibility, pose.is_some());
    }
    for (trail, mut transform, mut visibility) in &mut trails {
        let pose = fighters.get(trail.source).ok().and_then(trail_pose);
        if let Some(pose) = pose {
            *transform = pose;
        }
        show(&mut visibility, pose.is_some());
    }
    for (body, mut visibility) in &mut sheathed {
        let source = bodies
            .get(body.get())
            .ok()
            .and_then(|root| avatars.get(root.get()).ok())
            .map(|a| a.source);
        show(
            &mut visibility,
            source.is_none_or(|s| !drawn_sources.contains(&s)),
        );
    }
}

/// Lowers and leans back the body while its character slides.
pub fn pose_avatar_bodies(
    time: Res<Time>,
    predictions: Query<&ClientPredictionState>,
    avatars: Query<&OperativeAvatar>,
    mut bodies: Query<(&Parent, &mut Transform), With<AvatarBody>>,
) {
    let t = 1.0 - (-POSE_EASE_RATE * time.delta_secs()).exp();
    for (root, mut transform) in &mut bodies {
        let sliding = avatars
            .get(root.get())
            .ok()
            .and_then(|a| predictions.get(a.source).ok())
            .is_some_and(|p| p.movement.sliding);
        let (height, lean) = if sliding {
            (SLIDE_HIP_HEIGHT, SLIDE_LEAN_BACK)
        } else {
            (AVATAR_HIP_HEIGHT, 0.0)
        };
        transform.translation.y += (height - transform.translation.y) * t;
        let target = Quat::from_rotation_x(lean);
        transform.rotation = transform.rotation.slerp(target, t);
    }
}

/// Turns the local character to the camera yaw; melee hitboxes are placed
/// in the character's frame.
pub fn face_local_player_to_camera(
    mut players: Query<(&ViewAngles, &mut Transform), With<LocalPredictedPlayer>>,
) {
    for (view, mut transform) in &mut players {
        transform.rotation = Quat::from_rotation_y(view.yaw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_combat::combo_trees::combo_tree_for;
    use game_combat::MeleeWeaponClass;

    fn attacking(index: u8, frame: u32) -> MeleeCombatState {
        let tree = combo_tree_for(MeleeWeaponClass::CyberKatana);
        let mut state = MeleeCombatState::default();
        state.start_attack(tree[usize::from(index) % tree.len()]);
        state.current_combo_index = index;
        state.frame_counter = frame;
        state
    }

    #[test]
    fn blade_is_sheathed_when_idle() {
        let state = MeleeCombatState::default();
        assert!(katana_pose(&state).is_none());
        assert!(trail_pose(&state).is_none());
    }

    #[test]
    fn swings_sweep_and_alternate_direction() {
        let start = katana_pose(&attacking(0, 0)).expect("drawn");
        let attack = attacking(0, 0).current_attack.expect("attack");
        let end = katana_pose(&attacking(0, attack.total_frames())).expect("drawn");
        assert!(start.translation.x > SHOULDER.x, "first hit starts right");
        assert!(end.translation.x < SHOULDER.x, "and ends left");
        let second = katana_pose(&attacking(1, 0)).expect("drawn");
        assert!(second.translation.x < SHOULDER.x, "second hit starts left");
        let reach = start.translation.distance(SHOULDER);
        assert!((reach - BLADE_REACH).abs() < 1e-4);
    }

    #[test]
    fn guard_draws_the_blade() {
        let state = MeleeCombatState {
            is_parrying: true,
            ..default()
        };
        assert!(katana_pose(&state).is_some());
    }

    #[test]
    fn arc_mesh_has_a_strip_of_quads() {
        let mesh = slash_arc_mesh(0.5, 1.0, 1.0, 8);
        assert_eq!(mesh.count_vertices(), 18);
        assert_eq!(mesh.indices().map(Indices::len), Some(48));
    }
}
