//! End-to-end: simulated keyboard/mouse -> leafwing -> view angles ->
//! input packet -> parkour controller -> Avian collisions -> camera rig.

use avian3d::prelude::*;
use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use game_camera::{CameraPerspectiveMode, CameraRigController, CameraRigPlugin};
use game_core::input::ViewAngles;
use game_input::{default_input_map, local_player_input_bundle, GameInputPlugin};
use game_parkour::controller::GROUND_HOVER;
use game_parkour::shapecasts::CHARACTER_HALF_HEIGHT;
use game_parkour::{KinematicVelocity, ParkourController, ParkourPlugin, ParkourState};
use leafwing_input_manager::prelude::*;
use std::time::Duration;

const STANDING_Y: f32 = CHARACTER_HALF_HEIGHT + GROUND_HOVER;

struct Sim {
    app: App,
    player: Entity,
    camera: Entity,
}

impl Sim {
    fn new(extra_boxes: &[(Vec3, Vec3)]) -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            TransformPlugin,
            HierarchyPlugin,
            PhysicsPlugins::default(),
            ParkourPlugin,
            GameInputPlugin,
            CameraRigPlugin,
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_nanos(
            15_625_000,
        )));

        let mut boxes = vec![(Vec3::new(0.0, -0.1, 0.0), Vec3::new(200.0, 0.2, 200.0))];
        boxes.extend_from_slice(extra_boxes);
        for (center, size) in boxes {
            app.world_mut().spawn((
                Transform::from_translation(center),
                RigidBody::Static,
                Collider::cuboid(size.x, size.y, size.z),
            ));
        }
        for _ in 0..3 {
            app.update();
        }

        let player = app
            .world_mut()
            .spawn((
                ParkourController::default(),
                Transform::from_xyz(0.0, STANDING_Y, 0.0),
                local_player_input_bundle(default_input_map()),
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn(CameraRigController::following(player))
            .id();
        app.update();
        Self {
            app,
            player,
            camera,
        }
    }

    fn run(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.app.update();
        }
    }

    fn press(&mut self, key: KeyCode) {
        key.press(self.app.world_mut());
    }

    fn release(&mut self, key: KeyCode) {
        key.release(self.app.world_mut());
    }

    fn state(&self) -> ParkourState {
        *self
            .app
            .world()
            .get::<ParkourState>(self.player)
            .expect("state")
    }

    fn player_position(&self) -> Vec3 {
        self.app
            .world()
            .get::<Transform>(self.player)
            .expect("player")
            .translation
    }

    fn camera_transform(&self) -> Transform {
        *self
            .app
            .world()
            .get::<Transform>(self.camera)
            .expect("camera")
    }

    fn rig(&self) -> &CameraRigController {
        self.app
            .world()
            .get::<CameraRigController>(self.camera)
            .expect("rig")
    }
}

#[test]
fn keyboard_drives_the_parkour_controller_end_to_end() {
    let mut sim = Sim::new(&[]);
    sim.press(KeyCode::KeyW);
    sim.press(KeyCode::ShiftLeft);
    sim.run(64);

    assert_eq!(sim.state(), ParkourState::Sprinting);
    let speed = sim
        .app
        .world()
        .get::<KinematicVelocity>(sim.player)
        .expect("velocity")
        .planar_speed();
    assert!((speed - 12.8).abs() < 0.05, "sprint speed {speed}");
    let p = sim.player_position();
    assert!(p.z < -8.0 && p.x.abs() < 1e-3, "ran forward (-Z): {p:?}");

    sim.press(KeyCode::Space);
    let mut jumped = false;
    for _ in 0..4 {
        sim.app.update();
        jumped |= sim.state() == ParkourState::JumpAscent;
    }
    assert!(jumped, "space started a jump");

    // Holding space must not re-trigger jumps after landing.
    let mut ascents = 0;
    let mut previous = sim.state();
    for _ in 0..120 {
        sim.app.update();
        let s = sim.state();
        if s == ParkourState::JumpAscent && previous != ParkourState::JumpAscent {
            ascents += 1;
        }
        previous = s;
    }
    assert_eq!(ascents, 0, "held jump re-triggered");
    sim.release(KeyCode::Space);
}

#[test]
fn mouse_look_steers_movement_and_camera() {
    let mut sim = Sim::new(&[]);
    // ~90 degrees to the right: 0.0022 rad/px * 714 px.
    MouseMove::default().set_axis_pair(sim.app.world_mut(), Vec2::new(714.0, 0.0));
    sim.app.update();
    let view = *sim.app.world().get::<ViewAngles>(sim.player).expect("view");
    assert!(
        (view.yaw.to_degrees() - -90.0).abs() < 0.5,
        "yaw {}",
        view.yaw.to_degrees()
    );

    sim.press(KeyCode::KeyW);
    sim.run(40);
    let p = sim.player_position();
    assert!(p.x > 2.0 && p.z.abs() < 0.05, "forward is now +X: {p:?}");

    let camera = sim.camera_transform();
    let forward = camera.rotation * Vec3::NEG_Z;
    assert!(
        forward.abs_diff_eq(Vec3::X, 1e-3),
        "camera looks along +X: {forward:?}"
    );
    assert!(
        camera.translation.x < p.x,
        "camera trails behind the player"
    );
}

#[test]
fn third_person_camera_sits_behind_the_shoulder() {
    let mut sim = Sim::new(&[]);
    sim.run(30);
    let feet = sim.player_position() - Vec3::Y * CHARACTER_HALF_HEIGHT;
    let camera = sim.camera_transform().translation;
    let expected = feet + Vec3::new(0.65, 1.55, 2.8);
    assert!(
        camera.abs_diff_eq(expected, 0.02),
        "camera {camera:?}, expected {expected:?}"
    );
}

#[test]
fn wall_behind_the_player_pulls_the_spring_arm_in() {
    // Wall face at z = +1.5, behind the player (the boom points to +Z).
    let mut sim = Sim::new(&[(Vec3::new(0.0, 3.0, 1.75), Vec3::new(10.0, 6.0, 0.5))]);
    sim.run(30);
    let rig = sim.rig();
    assert!(
        rig.current_arm_length < 1.5 - 0.22,
        "arm {} should stop before the wall",
        rig.current_arm_length
    );
    assert!(rig.current_arm_length >= rig.min_arm_length);
    let camera = sim.camera_transform().translation;
    assert!(
        camera.z + 0.22 < 1.5,
        "camera sphere is in front of the wall: {camera:?}"
    );
}

#[test]
fn toggle_key_cycles_into_first_person_at_eye_height() {
    let mut sim = Sim::new(&[]);
    assert_eq!(
        sim.rig().current_mode,
        CameraPerspectiveMode::ThirdPersonRightShoulder
    );
    for expected in [
        CameraPerspectiveMode::ThirdPersonLeftShoulder,
        CameraPerspectiveMode::ThirdPersonWide,
        CameraPerspectiveMode::FirstPerson,
    ] {
        sim.press(KeyCode::KeyV);
        sim.app.update();
        sim.release(KeyCode::KeyV);
        sim.app.update();
        assert_eq!(sim.rig().current_mode, expected);
    }
    sim.run(5);
    let feet = sim.player_position() - Vec3::Y * CHARACTER_HALF_HEIGHT;
    let camera = sim.camera_transform().translation;
    assert!(
        (camera.y - (feet.y + 1.68)).abs() < 1e-3,
        "eye height: {camera:?}"
    );
    assert!(
        (camera.z - (feet.z - 0.12)).abs() < 1e-3,
        "eye slightly forward: {camera:?}"
    );
}

#[test]
fn field_of_view_widens_with_flow() {
    let mut sim = Sim::new(&[]);
    sim.app
        .world_mut()
        .get_mut::<game_parkour::FlowState>(sim.player)
        .expect("flow")
        .add_flow(100.0);
    sim.run(5);
    let rig = sim.rig();
    assert!((rig.target_field_of_view - 108.0).abs() < 1e-3);
    assert!(
        rig.field_of_view > 90.0 && rig.field_of_view < 108.0,
        "eases toward the target"
    );
}
