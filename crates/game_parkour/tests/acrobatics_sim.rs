//! Headless simulations of the full parkour stack against real Avian
//! colliders: probes, controller, collide-and-slide and Flow, ticking at a
//! fixed 64 Hz.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use game_core::debug_draw::DebugShapecastQueue;
use game_parkour::controller::GROUND_HOVER;
use game_parkour::shapecasts::CHARACTER_HALF_HEIGHT;
use game_parkour::{FlowState, KinematicVelocity, ParkourController, ParkourInput, ParkourState};
use std::collections::HashSet;
use std::time::Duration;

const STANDING_Y: f32 = CHARACTER_HALF_HEIGHT + GROUND_HOVER;

fn sim_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        HierarchyPlugin,
        PhysicsPlugins::default(),
        game_parkour::ParkourPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_nanos(
        15_625_000,
    )));
    app
}

fn spawn_box(app: &mut App, center: Vec3, size: Vec3) {
    app.world_mut().spawn((
        Transform::from_translation(center),
        RigidBody::Static,
        Collider::cuboid(size.x, size.y, size.z),
    ));
}

fn spawn_ground(app: &mut App) {
    spawn_box(app, Vec3::new(0.0, -0.1, 0.0), Vec3::new(200.0, 0.2, 200.0));
}

/// Lets the physics pipeline index the static world before characters spawn.
fn settle(app: &mut App) {
    for _ in 0..3 {
        app.update();
    }
}

fn spawn_character(app: &mut App, center: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            ParkourController::default(),
            Transform::from_translation(center),
        ))
        .id()
}

fn set_input(app: &mut App, entity: Entity, input: ParkourInput) {
    *app.world_mut()
        .get_mut::<ParkourInput>(entity)
        .expect("character input") = input;
}

fn state(app: &App, entity: Entity) -> ParkourState {
    *app.world().get::<ParkourState>(entity).expect("state")
}

fn position(app: &App, entity: Entity) -> Vec3 {
    app.world()
        .get::<Transform>(entity)
        .expect("transform")
        .translation
}

fn sprint_forward() -> ParkourInput {
    ParkourInput {
        move_direction: Vec2::new(0.0, -1.0),
        sprint: true,
        ..Default::default()
    }
}

#[test]
fn sprinting_character_vaults_a_low_obstacle() {
    let mut app = sim_app();
    spawn_ground(&mut app);
    // Low vault: 0.75 m tall, 0.6 m deep, front face at z = -5.7.
    spawn_box(
        &mut app,
        Vec3::new(0.0, 0.375, -6.0),
        Vec3::new(4.0, 0.75, 0.6),
    );
    settle(&mut app);

    let mut queue = DebugShapecastQueue::default();
    queue.set_recording(true);
    app.insert_resource(queue);

    let runner = spawn_character(&mut app, Vec3::new(0.0, STANDING_Y, 0.0));
    let mut seen = HashSet::new();
    let mut peak_during_vault = f32::NEG_INFINITY;
    for _ in 0..90 {
        set_input(&mut app, runner, sprint_forward());
        app.update();
        let s = state(&app, runner);
        seen.insert(s);
        if s == ParkourState::VaultLow {
            peak_during_vault = peak_during_vault.max(position(&app, runner).y);
        }
    }

    assert!(seen.contains(&ParkourState::Sprinting), "states: {seen:?}");
    assert!(seen.contains(&ParkourState::VaultLow), "states: {seen:?}");
    // Feet cleared the 0.75 m top.
    assert!(
        peak_during_vault - CHARACTER_HALF_HEIGHT > 0.75,
        "peak feet height {}",
        peak_during_vault - CHARACTER_HALF_HEIGHT
    );
    let end = position(&app, runner);
    assert!(
        end.z < -8.0,
        "should be well past the obstacle, z = {}",
        end.z
    );
    assert!(
        (end.y - STANDING_Y).abs() < 0.05,
        "back on the ground, y = {}",
        end.y
    );
    assert!(state(&app, runner).is_grounded());
    let speed = app
        .world()
        .get::<KinematicVelocity>(runner)
        .expect("velocity")
        .planar_speed();
    assert!(
        (speed - 12.8).abs() < 0.1,
        "momentum preserved, speed = {speed}"
    );

    let casts = app.world().resource::<DebugShapecastQueue>();
    assert!(casts.len() >= 6, "probes were recorded for the overlay");
}

#[test]
fn character_cannot_walk_through_walls() {
    let mut app = sim_app();
    spawn_ground(&mut app);
    // 3 m tall wall: too tall to vault or mantle. Front face at z = -3.
    spawn_box(
        &mut app,
        Vec3::new(0.0, 1.5, -3.25),
        Vec3::new(10.0, 3.0, 0.5),
    );
    settle(&mut app);

    let runner = spawn_character(&mut app, Vec3::new(0.0, STANDING_Y, 0.0));
    for _ in 0..90 {
        set_input(&mut app, runner, sprint_forward());
        app.update();
    }
    let end = position(&app, runner);
    assert!(end.z > -3.0 + 0.35, "penetrated the wall: z = {}", end.z);
    assert!(
        end.z < -2.0,
        "should be pressed up against it: z = {}",
        end.z
    );
    assert!(state(&app, runner).is_grounded());
}

#[test]
fn wall_run_then_wall_kick_awards_flow() {
    let mut app = sim_app();
    spawn_ground(&mut app);
    // Corridor walls at x = +-3 (inner faces at +-2.75) from z = -2.5 to -27.5.
    for x in [-3.0, 3.0] {
        spawn_box(
            &mut app,
            Vec3::new(x, 3.0, -15.0),
            Vec3::new(0.5, 6.0, 25.0),
        );
    }
    settle(&mut app);

    let runner = spawn_character(&mut app, Vec3::new(2.0, STANDING_Y, 0.0));
    for tick in 0..30 {
        let mut input = sprint_forward();
        input.jump_pressed = tick == 24;
        set_input(&mut app, runner, input);
        app.update();
    }

    let mut wall_ran = false;
    for _ in 0..60 {
        set_input(&mut app, runner, sprint_forward());
        app.update();
        if state(&app, runner) == ParkourState::WallRunRight {
            wall_ran = true;
            break;
        }
    }
    assert!(
        wall_ran,
        "never started a wall run, state {:?}",
        state(&app, runner)
    );

    for _ in 0..10 {
        set_input(&mut app, runner, sprint_forward());
        app.update();
        assert_eq!(state(&app, runner), ParkourState::WallRunRight);
        let y_velocity = app
            .world()
            .get::<KinematicVelocity>(runner)
            .expect("velocity")
            .0
            .y;
        assert!(y_velocity >= -1.25 - 1e-3, "sinking slower than gravity");
    }

    let mut kick = sprint_forward();
    kick.jump_pressed = true;
    set_input(&mut app, runner, kick);
    app.update();
    assert_eq!(state(&app, runner), ParkourState::WallKick);
    let v = app
        .world()
        .get::<KinematicVelocity>(runner)
        .expect("velocity")
        .0;
    assert!(v.x < -8.0, "kicked away from the right wall: {v:?}");
    assert!(v.y > 7.0 && v.z < -10.0, "up and forward: {v:?}");
    let flow = app.world().get::<FlowState>(runner).expect("flow");
    assert!(
        (flow.current_flow - 15.0).abs() < 0.5,
        "flow {}",
        flow.current_flow
    );
}

#[test]
fn long_drop_is_a_hard_landing_but_a_buffered_slide_rolls() {
    let mut app = sim_app();
    spawn_ground(&mut app);
    settle(&mut app);

    let hard = spawn_character(&mut app, Vec3::new(-5.0, 12.0, 0.0));
    let roller = spawn_character(&mut app, Vec3::new(5.0, 12.0, 0.0));

    let mut hard_landed = false;
    let mut rolled = false;
    for _ in 0..120 {
        let roller_y = position(&app, roller).y;
        set_input(
            &mut app,
            roller,
            ParkourInput {
                slide_pressed: roller_y < STANDING_Y + 1.5 && !rolled,
                ..Default::default()
            },
        );
        app.update();
        hard_landed |= state(&app, hard) == ParkourState::HardLanding;
        rolled |= state(&app, roller) == ParkourState::LandingRoll;
    }
    assert!(hard_landed, "a 12 m fall lands hard");
    assert!(rolled, "a buffered slide turns the landing into a roll");
    assert_eq!(
        state(&app, hard),
        ParkourState::Idle,
        "recovers after the lock"
    );
    for e in [hard, roller] {
        let y = position(&app, e).y;
        assert!(
            (y - STANDING_Y).abs() < 0.05,
            "resting on the ground, y = {y}"
        );
    }
    let flow = app.world().get::<FlowState>(roller).expect("flow");
    assert!(flow.current_flow > 0.0, "roll granted flow");
}

#[test]
fn jump_at_a_ledge_mantles_onto_it() {
    let mut app = sim_app();
    spawn_ground(&mut app);
    // 1.6 m ledge, 4 m deep, front face at z = -2.
    spawn_box(
        &mut app,
        Vec3::new(0.0, 0.8, -4.0),
        Vec3::new(4.0, 1.6, 4.0),
    );
    settle(&mut app);

    let climber = spawn_character(&mut app, Vec3::new(0.0, STANDING_Y, -1.2));
    app.update();
    set_input(
        &mut app,
        climber,
        ParkourInput {
            jump_pressed: true,
            ..Default::default()
        },
    );
    let mut mantled = false;
    for _ in 0..60 {
        app.update();
        mantled |= state(&app, climber) == ParkourState::LedgeMantle;
    }
    assert!(mantled, "never mantled");
    let end = position(&app, climber);
    assert!(
        (end.y - (1.6 + STANDING_Y)).abs() < 0.05,
        "standing on the ledge, y = {}",
        end.y
    );
    assert!(end.z < -2.0 - 0.3, "on top of the ledge, z = {}", end.z);
    assert!(state(&app, climber).is_grounded());
}
