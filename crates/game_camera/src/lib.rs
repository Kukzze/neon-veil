//! FPP / TPP / photo camera rig.
//!
//! A camera entity with [`CameraRigController`] follows its `target`
//! character: it reads the target's `ViewAngles` (owned by the input crate),
//! places itself with the first-person eye rig or the third-person spring
//! arm, and widens its field of view with speed and Flow.

pub mod fpp;
pub mod tpp;

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter, SpatialQueryPipeline};
use bevy::prelude::*;
use bevy::transform::TransformSystem;
use game_combat::RecoilSpring;
use game_core::input::{clamp_pitch, ViewAngles};
use game_input::PlayerAction;
use game_parkour::shapecasts::CHARACTER_HALF_HEIGHT;
use game_parkour::{FlowState, KinematicVelocity, ParkourPhysicsConstants, ParkourState};
use leafwing_input_manager::prelude::ActionState;
use serde::{Deserialize, Serialize};

pub const BASE_FIELD_OF_VIEW_DEGREES: f32 = 90.0;
pub const MAX_FIELD_OF_VIEW_DEGREES: f32 = 108.0;
/// Rate at which the field of view follows its target, 1/s.
pub const FIELD_OF_VIEW_BLEND_RATE: f32 = 4.0;
/// First-person roll while wall-running, tilting the head away from the wall.
pub const WALL_RUN_ROLL_DEGREES: f32 = 8.0;
pub const ROLL_BLEND_RATE: f32 = 8.0;

/// Which camera rig is active.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default, Debug, PartialEq, Hash)]
pub enum CameraPerspectiveMode {
    FirstPerson,
    #[default]
    ThirdPersonRightShoulder,
    ThirdPersonLeftShoulder,
    ThirdPersonWide,
    /// Detached camera that holds its position; only the view rotates.
    PhotoMode,
}

impl CameraPerspectiveMode {
    /// Next mode in the toggle cycle FPP -> TPP right -> TPP left -> TPP wide
    /// -> FPP. Photo mode is not part of the cycle; toggling leaves it for
    /// the default right-shoulder view.
    pub fn cycle_mode(&self) -> Self {
        match self {
            Self::FirstPerson => Self::ThirdPersonRightShoulder,
            Self::ThirdPersonRightShoulder => Self::ThirdPersonLeftShoulder,
            Self::ThirdPersonLeftShoulder => Self::ThirdPersonWide,
            Self::ThirdPersonWide => Self::FirstPerson,
            Self::PhotoMode => Self::ThirdPersonRightShoulder,
        }
    }

    pub fn is_third_person(&self) -> bool {
        matches!(
            self,
            Self::ThirdPersonRightShoulder | Self::ThirdPersonLeftShoulder | Self::ThirdPersonWide
        )
    }
}

/// Camera rig state, placed on the camera entity.
///
/// Offsets are measured from the target's feet in its yaw-rotated local
/// space (Bevy convention: +X right, +Y up, -Z forward).
#[derive(Component, Reflect, Debug, Clone, PartialEq)]
#[reflect(Component, Default, Debug)]
#[require(Transform)]
pub struct CameraRigController {
    /// Character the camera follows.
    pub target: Option<Entity>,
    pub current_mode: CameraPerspectiveMode,
    pub target_mode: CameraPerspectiveMode,
    pub fpp_eye_offset: Vec3,
    pub tpp_shoulder_offset: Vec3,
    pub tpp_arm_length: f32,
    pub current_arm_length: f32,
    pub min_arm_length: f32,
    pub spherecast_radius: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    /// Degrees.
    pub field_of_view: f32,
    /// Degrees.
    pub target_field_of_view: f32,
    /// Head-bob phase in vertical stride cycles.
    pub head_bob_phase: f32,
    /// Head-bob fade weight in `[0, 1]`.
    pub head_bob_weight: f32,
    /// Where photo mode froze the camera.
    pub photo_anchor: Option<Vec3>,
}

impl Default for CameraRigController {
    fn default() -> Self {
        Self {
            target: None,
            current_mode: CameraPerspectiveMode::ThirdPersonRightShoulder,
            target_mode: CameraPerspectiveMode::ThirdPersonRightShoulder,
            // 12 cm in front of the head center (forward is -Z).
            fpp_eye_offset: Vec3::new(0.0, 1.68, -0.12),
            tpp_shoulder_offset: Vec3::new(0.65, 1.55, 0.0),
            tpp_arm_length: 2.8,
            current_arm_length: 2.8,
            min_arm_length: 0.4,
            spherecast_radius: 0.22,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            field_of_view: BASE_FIELD_OF_VIEW_DEGREES,
            target_field_of_view: BASE_FIELD_OF_VIEW_DEGREES,
            head_bob_phase: 0.0,
            head_bob_weight: 0.0,
            photo_anchor: None,
        }
    }
}

impl CameraRigController {
    /// A rig following `target`.
    pub fn following(target: Entity) -> Self {
        Self {
            target: Some(target),
            ..Default::default()
        }
    }

    /// `EulerRot::YXZ` rotation from yaw, pitch and roll.
    pub fn rotation(&self) -> Quat {
        self.rotation_with_kick(Vec2::ZERO)
    }

    /// Rotation with a recoil kick added, in degrees (`x` = pitch up,
    /// `y` = yaw left). Pitch stays clamped to +-89 degrees.
    pub fn rotation_with_kick(&self, kick_degrees: Vec2) -> Quat {
        Quat::from_euler(
            EulerRot::YXZ,
            self.yaw + kick_degrees.y.to_radians(),
            clamp_pitch(self.pitch + kick_degrees.x.to_radians()),
            self.roll,
        )
    }

    /// Applies a pending `target_mode` change.
    pub fn apply_mode_change(&mut self, current_position: Vec3) {
        if self.target_mode == self.current_mode {
            return;
        }
        if self.target_mode.is_third_person() && !self.current_mode.is_third_person() {
            // Grow the arm out from the head instead of popping to full length.
            self.current_arm_length = self.min_arm_length;
        }
        self.photo_anchor =
            (self.target_mode == CameraPerspectiveMode::PhotoMode).then_some(current_position);
        self.current_mode = self.target_mode;
    }
}

/// Field of view for a planar speed and Flow value: base 90 degrees, rising
/// to 108 with speed beyond sprinting (up to terminal slide speed) or Flow,
/// whichever is greater.
pub fn target_field_of_view(
    planar_speed: f32,
    flow: f32,
    constants: &ParkourPhysicsConstants,
) -> f32 {
    let speed_range = (constants.slide_max_speed - constants.sprint_speed).max(1e-3);
    let speed_t = ((planar_speed - constants.sprint_speed) / speed_range).clamp(0.0, 1.0);
    let flow_t = (flow / 100.0).clamp(0.0, 1.0);
    let t = if speed_t.is_finite() { speed_t } else { 0.0 }.max(if flow_t.is_finite() {
        flow_t
    } else {
        0.0
    });
    BASE_FIELD_OF_VIEW_DEGREES + (MAX_FIELD_OF_VIEW_DEGREES - BASE_FIELD_OF_VIEW_DEGREES) * t
}

/// Exponential approach of `current` toward `target` at `rate` per second.
pub fn approach(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-rate * dt.max(0.0)).exp())
}

/// First-person roll target for a parkour state.
pub fn roll_for_state(state: Option<ParkourState>) -> f32 {
    match state {
        Some(ParkourState::WallRunRight) => WALL_RUN_ROLL_DEGREES.to_radians(),
        Some(ParkourState::WallRunLeft) => -WALL_RUN_ROLL_DEGREES.to_radians(),
        _ => 0.0,
    }
}

/// Cycles the rig mode when the target's player presses
/// `ToggleCameraPerspective`.
pub fn cycle_camera_perspective(
    players: Query<&ActionState<PlayerAction>>,
    mut rigs: Query<&mut CameraRigController>,
) {
    for mut rig in &mut rigs {
        let Some(state) = rig.target.and_then(|t| players.get(t).ok()) else {
            continue;
        };
        if state.just_pressed(&PlayerAction::ToggleCameraPerspective) {
            rig.target_mode = rig.current_mode.cycle_mode();
        }
    }
}

type TargetQuery<'a> = (
    &'a Transform,
    Option<&'a ViewAngles>,
    Option<&'a KinematicVelocity>,
    Option<&'a ParkourState>,
    Option<&'a FlowState>,
    Option<&'a RecoilSpring>,
    Option<&'a OriginAtFeet>,
);

/// The target's origin is at its feet (networked players), not at the
/// centre of a capsule (physics-driven characters).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OriginAtFeet;

/// Places every rig camera relative to its target.
pub fn update_camera_rig(
    time: Res<Time>,
    constants: Option<Res<ParkourPhysicsConstants>>,
    spatial: SpatialQuery,
    targets: Query<TargetQuery, Without<CameraRigController>>,
    mut rigs: Query<(
        &mut CameraRigController,
        &mut Transform,
        Option<&mut Projection>,
    )>,
) {
    let dt = time.delta_secs();
    let defaults = ParkourPhysicsConstants::default();
    let constants = constants.as_deref().unwrap_or(&defaults);

    for (mut rig, mut transform, projection) in &mut rigs {
        let Some(target) = rig.target else {
            continue;
        };
        let Ok((target_transform, view, velocity, state, flow, recoil, at_feet)) =
            targets.get(target)
        else {
            continue;
        };

        rig.apply_mode_change(transform.translation);
        let view = view.copied().unwrap_or_default();
        rig.yaw = view.yaw;
        rig.pitch = clamp_pitch(view.pitch);
        let first_person = rig.current_mode == CameraPerspectiveMode::FirstPerson;
        let roll_target = if first_person {
            roll_for_state(state.copied())
        } else {
            0.0
        };
        rig.roll = approach(rig.roll, roll_target, ROLL_BLEND_RATE, dt);

        let feet = target_transform.translation - Vec3::Y * CHARACTER_HALF_HEIGHT;
        let planar_speed = velocity.map_or(0.0, KinematicVelocity::planar_speed);
        let kick = recoil.map_or(Vec2::ZERO, |r| r.current_offset);
        let rotation = rig.rotation_with_kick(kick);

        let position = match rig.current_mode {
            CameraPerspectiveMode::FirstPerson => {
                let grounded = state.is_some_and(ParkourState::is_grounded);
                let (phase, weight) = fpp::advance_head_bob(
                    rig.head_bob_phase,
                    rig.head_bob_weight,
                    planar_speed,
                    grounded,
                    dt,
                );
                rig.head_bob_phase = phase;
                rig.head_bob_weight = weight;
                // The eye stands on the real feet, wherever the origin is.
                let true_feet = if at_feet.is_some() {
                    target_transform.translation
                } else {
                    feet
                };
                fpp::eye_position(
                    true_feet,
                    rig.yaw,
                    rig.fpp_eye_offset,
                    fpp::head_bob_offset(phase, weight),
                )
            }
            CameraPerspectiveMode::PhotoMode => rig.photo_anchor.unwrap_or(transform.translation),
            _ => {
                let caster = tpp::SpatialSphereCaster {
                    query: &spatial,
                    filter: SpatialQueryFilter::default().with_excluded_entities([target]),
                };
                tpp::third_person_position(&caster, feet, &mut rig, rotation, dt)
            }
        };
        transform.translation = position;
        transform.rotation = rotation;

        let flow_value = flow.map_or(0.0, |f| f.current_flow);
        rig.target_field_of_view = target_field_of_view(planar_speed, flow_value, constants);
        rig.field_of_view = approach(
            rig.field_of_view,
            rig.target_field_of_view,
            FIELD_OF_VIEW_BLEND_RATE,
            dt,
        );
        if let Some(mut projection) = projection {
            if let Projection::Perspective(perspective) = projection.as_mut() {
                perspective.fov = rig.field_of_view.to_radians();
            }
        }
    }
}

/// Registers the rig and its systems. The rig update needs Avian's spatial
/// query pipeline (`PhysicsPlugins`) for spring-arm occlusion and is skipped
/// while it is absent.
pub struct CameraRigPlugin;

impl Plugin for CameraRigPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CameraPerspectiveMode>()
            .register_type::<CameraRigController>()
            .add_systems(Update, cycle_camera_perspective)
            .add_systems(
                PostUpdate,
                update_camera_rig
                    .run_if(resource_exists::<SpatialQueryPipeline>)
                    .before(TransformSystem::TransformPropagate),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perspective_cycle() {
        use CameraPerspectiveMode::*;
        assert_eq!(FirstPerson.cycle_mode(), ThirdPersonRightShoulder);
        assert_eq!(
            ThirdPersonRightShoulder.cycle_mode(),
            ThirdPersonLeftShoulder
        );
        assert_eq!(ThirdPersonLeftShoulder.cycle_mode(), ThirdPersonWide);
        assert_eq!(ThirdPersonWide.cycle_mode(), FirstPerson);
        assert_eq!(PhotoMode.cycle_mode(), ThirdPersonRightShoulder);

        let mut mode = FirstPerson;
        for _ in 0..4 {
            mode = mode.cycle_mode();
        }
        assert_eq!(mode, FirstPerson, "full cycle returns to start");
        assert_eq!(CameraPerspectiveMode::default(), ThirdPersonRightShoulder);
    }

    #[test]
    fn rig_defaults_match_spec() {
        let r = CameraRigController::default();
        assert_eq!(
            r.current_mode,
            CameraPerspectiveMode::ThirdPersonRightShoulder
        );
        assert_eq!(r.tpp_shoulder_offset, Vec3::new(0.65, 1.55, 0.0));
        assert_eq!(r.fpp_eye_offset.y, 1.68);
        assert_eq!(r.tpp_arm_length, 2.8);
        assert_eq!(r.min_arm_length, 0.4);
        assert_eq!(r.spherecast_radius, 0.22);
        assert_eq!(r.field_of_view, 90.0);
    }

    #[test]
    fn recoil_kick_pitches_the_view_up() {
        let r = CameraRigController::default();
        let rest = r.rotation() * Vec3::NEG_Z;
        let kicked = r.rotation_with_kick(Vec2::new(2.0, 0.0)) * Vec3::NEG_Z;
        assert!(kicked.y > rest.y);
        assert!((kicked.angle_between(rest).to_degrees() - 2.0).abs() < 1e-3);
        let clamped = CameraRigController {
            pitch: 1.55,
            ..Default::default()
        }
        .rotation_with_kick(Vec2::new(10.0, 0.0));
        assert!((clamped * Vec3::Y).y > 0.0, "kick never flips the camera");
    }

    #[test]
    fn pitch_clamped_to_89_degrees_in_rotation() {
        let r = CameraRigController {
            pitch: 3.0,
            ..Default::default()
        };
        let forward = r.rotation() * Vec3::NEG_Z;
        let pitch = forward.y.asin().to_degrees();
        assert!((pitch - 89.0).abs() < 1e-2, "pitch {pitch}");
        let up = r.rotation() * Vec3::Y;
        assert!(up.y > 0.0, "camera never flips upside down");
    }

    #[test]
    fn entering_third_person_grows_arm_and_photo_anchors() {
        let mut r = CameraRigController {
            current_mode: CameraPerspectiveMode::FirstPerson,
            target_mode: CameraPerspectiveMode::ThirdPersonRightShoulder,
            ..Default::default()
        };
        r.apply_mode_change(Vec3::ZERO);
        assert_eq!(r.current_arm_length, r.min_arm_length);

        r.target_mode = CameraPerspectiveMode::PhotoMode;
        r.apply_mode_change(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(r.photo_anchor, Some(Vec3::new(1.0, 2.0, 3.0)));
        r.target_mode = CameraPerspectiveMode::FirstPerson;
        r.apply_mode_change(Vec3::ZERO);
        assert_eq!(r.photo_anchor, None);
    }

    #[test]
    fn field_of_view_rises_with_speed_and_flow() {
        let c = ParkourPhysicsConstants::default();
        assert_eq!(target_field_of_view(0.0, 0.0, &c), 90.0);
        assert_eq!(
            target_field_of_view(12.8, 0.0, &c),
            90.0,
            "sprinting alone: no kick"
        );
        assert!((target_field_of_view(25.0, 0.0, &c) - 108.0).abs() < 1e-4);
        assert!(
            (target_field_of_view(40.0, 0.0, &c) - 108.0).abs() < 1e-4,
            "capped"
        );
        assert!((target_field_of_view(0.0, 50.0, &c) - 99.0).abs() < 1e-4);
        assert!((target_field_of_view(0.0, 100.0, &c) - 108.0).abs() < 1e-4);
        assert_eq!(target_field_of_view(f32::NAN, f32::NAN, &c), 90.0);
    }

    #[test]
    fn wall_run_roll_tilts_away_from_wall() {
        assert!(roll_for_state(Some(ParkourState::WallRunRight)) > 0.0);
        assert!(roll_for_state(Some(ParkourState::WallRunLeft)) < 0.0);
        assert_eq!(roll_for_state(Some(ParkourState::Sprinting)), 0.0);
        assert_eq!(roll_for_state(None), 0.0);
    }

    #[test]
    fn approach_converges_monotonically() {
        let mut v = 90.0;
        for _ in 0..240 {
            let next = approach(v, 108.0, FIELD_OF_VIEW_BLEND_RATE, 1.0 / 60.0);
            assert!(next >= v && next <= 108.0);
            v = next;
        }
        assert!((v - 108.0).abs() < 0.01);
    }
}
