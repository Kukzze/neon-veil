//! Headless AI simulations: real Avian colliders, the full parkour and
//! combat stack, and the AI plugin, ticking at 64 Hz.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use game_ai::{
    spawn_boss_gorgon, spawn_cyber_ninja, spawn_shielded_heavy, spawn_street_grunt,
    spawn_swarm_drone, AiBlackboard, AiPerception, AlertState, BossGorgon,
    BossPhaseTransitionEvent, DroneDetonationEvent, Enemy, GameAiPlugin,
};
use game_combat::{
    BallisticProfile, MeleeCombatState, RangedHitEvent, RangedInput, ShotFiredEvent,
};
use game_core::identifiers::PlayerCharacterMarker;
use game_core::input::ViewAngles;
use game_parkour::{ParkourController, ParkourState};
use std::f32::consts::PI;
use std::time::Duration;

const STANDING_Y: f32 = 0.93;

fn sim_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        HierarchyPlugin,
        PhysicsPlugins::default(),
        GameAiPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_nanos(
        15_625_000,
    )));
    spawn_box(
        &mut app,
        Vec3::new(0.0, -0.1, 0.0),
        Vec3::new(200.0, 0.2, 200.0),
    );
    app
}

fn spawn_box(app: &mut App, center: Vec3, size: Vec3) {
    app.world_mut().spawn((
        Transform::from_translation(center),
        RigidBody::Static,
        Collider::cuboid(size.x, size.y, size.z),
    ));
}

fn settle(app: &mut App) {
    for _ in 0..3 {
        app.update();
    }
}

fn spawn_with(app: &mut App, spawn: impl FnOnce(&mut Commands) -> Entity) -> Entity {
    let world = app.world_mut();
    let entity = spawn(&mut world.commands());
    world.flush();
    entity
}

fn spawn_player(app: &mut App, position: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            PlayerCharacterMarker,
            ParkourController::default(),
            MeleeCombatState::default(),
            Transform::from_translation(position),
            RigidBody::Kinematic,
            Collider::capsule(0.4, 1.0),
        ))
        .id()
}

fn face(app: &mut App, entity: Entity, yaw: f32) {
    app.world_mut()
        .get_mut::<ViewAngles>(entity)
        .expect("view")
        .yaw = yaw;
}

fn alert(app: &App, enemy: Entity) -> AlertState {
    app.world()
        .get::<AiPerception>(enemy)
        .expect("perception")
        .alert_state
}

fn position(app: &App, entity: Entity) -> Vec3 {
    app.world()
        .get::<Transform>(entity)
        .expect("transform")
        .translation
}

fn drain<E: Event + Clone>(app: &mut App) -> Vec<E> {
    app.world_mut()
        .resource_mut::<Events<E>>()
        .drain()
        .collect()
}

#[test]
fn enemy_sees_player_in_cone_with_clear_line_of_sight() {
    let mut app = sim_app();
    settle(&mut app);
    let player = spawn_player(&mut app, Vec3::new(1.0, STANDING_Y, -10.0));
    let grunt = spawn_with(&mut app, |c| {
        spawn_street_grunt(c, Vec3::new(0.0, STANDING_Y, 0.0))
    });
    app.update();
    app.update();
    assert_eq!(alert(&app, grunt), AlertState::Combat);
    let bb = app.world().get::<AiBlackboard>(grunt).expect("blackboard");
    assert_eq!(bb.target_entity, Some(player));
}

#[test]
fn wall_blocks_line_of_sight() {
    let mut app = sim_app();
    spawn_box(
        &mut app,
        Vec3::new(0.0, 2.0, -5.0),
        Vec3::new(8.0, 4.0, 0.5),
    );
    settle(&mut app);
    spawn_player(&mut app, Vec3::new(0.0, STANDING_Y, -10.0));
    let grunt = spawn_with(&mut app, |c| {
        spawn_street_grunt(c, Vec3::new(0.0, STANDING_Y, 0.0))
    });
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(
        alert(&app, grunt),
        AlertState::Unaware,
        "wall hides the player"
    );
}

#[test]
fn player_behind_the_enemy_is_not_seen() {
    let mut app = sim_app();
    settle(&mut app);
    spawn_player(&mut app, Vec3::new(0.0, STANDING_Y, 8.0));
    let grunt = spawn_with(&mut app, |c| {
        spawn_street_grunt(c, Vec3::new(0.0, STANDING_Y, 0.0))
    });
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(
        alert(&app, grunt),
        AlertState::Unaware,
        "180 degrees outside a 90 degree cone"
    );
}

/// A sniper at the origin shoots a Shielded Heavy 10 m away facing `yaw`.
fn shoot_heavy(heavy_yaw: f32) -> (RangedHitEvent, f32) {
    let mut app = sim_app();
    settle(&mut app);
    let player = spawn_player(&mut app, Vec3::new(0.0, STANDING_Y, 0.0));
    let heavy = spawn_with(&mut app, |c| {
        spawn_shielded_heavy(c, Vec3::new(0.0, STANDING_Y, -10.0))
    });
    face(&mut app, heavy, heavy_yaw);
    app.update();
    let eye = Vec3::new(0.0, STANDING_Y + 0.78, 0.0);
    app.world_mut().entity_mut(player).insert((
        BallisticProfile::tech_sniper_rifle(),
        ViewAngles::looking_along(Vec3::new(0.0, STANDING_Y + 0.3, -10.0) - eye),
    ));
    app.world_mut()
        .get_mut::<RangedInput>(player)
        .expect("input")
        .trigger_held = true;
    app.update();
    let hit = drain::<RangedHitEvent>(&mut app)
        .into_iter()
        .find(|h| h.shooter == player && h.target == heavy)
        .expect("the shot reached the heavy");
    (hit, app.world().get::<Enemy>(heavy).expect("enemy").health)
}

#[test]
fn shielded_heavy_deflects_frontal_fire_but_not_rear_fire() {
    let (front, health) = shoot_heavy(PI); // facing the shooter
    assert!(front.deflected);
    assert_eq!(front.damage, 0.0);
    assert_eq!(health, 1200.0);

    let (rear, health) = shoot_heavy(0.0); // facing away
    assert!(!rear.deflected);
    assert_eq!(rear.damage, 220.0);
    assert_eq!(health, 980.0, "full damage from behind");
}

#[test]
fn cyber_ninja_wall_runs_along_a_corridor_to_flank() {
    let mut app = sim_app();
    for x in [-3.0, 3.0] {
        spawn_box(
            &mut app,
            Vec3::new(x, 3.0, -15.0),
            Vec3::new(0.5, 6.0, 25.0),
        );
    }
    settle(&mut app);
    spawn_player(&mut app, Vec3::new(0.0, STANDING_Y, -20.0));
    let ninja = spawn_with(&mut app, |c| {
        spawn_cyber_ninja(c, Vec3::new(2.0, STANDING_Y, 0.0))
    });

    let mut wall_ran = false;
    for _ in 0..128 {
        app.update();
        let state = *app.world().get::<ParkourState>(ninja).expect("state");
        wall_ran |= matches!(
            state,
            ParkourState::WallRunLeft | ParkourState::WallRunRight
        );
    }
    assert_eq!(alert(&app, ninja), AlertState::Combat);
    assert!(wall_ran, "the ninja took to the wall");
    assert!(
        position(&app, ninja).z < -8.0,
        "and covered ground toward the target"
    );
}

#[test]
fn gorgon_moves_through_its_three_phases() {
    let mut app = sim_app();
    settle(&mut app);
    let player = spawn_player(&mut app, Vec3::new(0.0, STANDING_Y, 0.0));
    let boss = spawn_with(&mut app, |c| {
        spawn_boss_gorgon(c, Vec3::new(0.0, STANDING_Y, -15.0))
    });
    app.update();

    let hit = |app: &mut App, damage: f32| {
        app.world_mut().send_event(RangedHitEvent {
            shooter: player,
            target: boss,
            damage,
            headshot: false,
            status_applied: None,
            deflected: false,
        });
        app.update();
    };

    hit(&mut app, 9_000.0);
    let transitions = drain::<BossPhaseTransitionEvent>(&mut app);
    assert_eq!(transitions.len(), 1);
    assert_eq!((transitions[0].from_phase, transitions[0].to_phase), (1, 2));
    for _ in 0..64 {
        app.update();
    }
    assert_eq!(
        *app.world().get::<ParkourState>(boss).expect("state"),
        ParkourState::GrapplePull
    );
    assert!(
        position(&app, boss).y > 5.0,
        "jump-jets up the shaft: {}",
        position(&app, boss).y
    );

    hit(&mut app, 8_000.0);
    let transitions = drain::<BossPhaseTransitionEvent>(&mut app);
    assert_eq!((transitions[0].from_phase, transitions[0].to_phase), (2, 3));
    assert_eq!(
        app.world()
            .get::<BossGorgon>(boss)
            .expect("boss")
            .current_phase,
        3
    );
    let start = position(&app, boss).distance(position(&app, player));
    for _ in 0..(64 * 4) {
        app.update();
    }
    assert!(position(&app, boss).y < 1.5, "dropped back into the arena");
    assert!(
        position(&app, boss).distance(position(&app, player)) < start - 5.0,
        "closing in for melee"
    );
}

#[test]
fn swarm_drone_hovers_then_kamikazes_when_hurt() {
    let mut app = sim_app();
    settle(&mut app);
    spawn_player(&mut app, Vec3::new(0.0, STANDING_Y, 0.0));
    let drone = spawn_with(&mut app, |c| {
        spawn_swarm_drone(c, Vec3::new(0.0, 4.0, -12.0))
    });
    face(&mut app, drone, PI);
    for _ in 0..64 {
        app.update();
    }
    assert_eq!(alert(&app, drone), AlertState::Combat);
    assert!(
        (position(&app, drone).y - 4.0).abs() < 0.3,
        "holds altitude"
    );

    app.world_mut()
        .get_mut::<Enemy>(drone)
        .expect("enemy")
        .health = 20.0;
    let mut detonated = Vec::new();
    for _ in 0..192 {
        app.update();
        detonated.extend(drain::<DroneDetonationEvent>(&mut app));
        if !detonated.is_empty() {
            break;
        }
    }
    assert_eq!(detonated.len(), 1, "charged in and blew up");
    assert!(app.world().get_entity(drone).is_err(), "drone is gone");
}

#[test]
fn grunt_raises_the_alarm_returns_fire_and_the_ninja_flanks() {
    let mut app = sim_app();
    settle(&mut app);
    let player = spawn_player(&mut app, Vec3::new(0.0, STANDING_Y, 0.0));
    // Both enemies face away from the player.
    let grunt = spawn_with(&mut app, |c| {
        spawn_street_grunt(c, Vec3::new(0.0, STANDING_Y, -12.0))
    });
    let ninja = spawn_with(&mut app, |c| {
        spawn_cyber_ninja(c, Vec3::new(6.0, STANDING_Y, -30.0))
    });
    app.update();
    assert_eq!(alert(&app, grunt), AlertState::Unaware);
    assert_eq!(alert(&app, ninja), AlertState::Unaware);

    let eye = Vec3::new(0.0, STANDING_Y + 0.78, 0.0);
    app.world_mut().entity_mut(player).insert((
        BallisticProfile::kinetic_pistol(),
        ViewAngles::looking_along(Vec3::new(0.0, STANDING_Y + 0.3, -12.0) - eye),
    ));
    let ninja_start = position(&app, ninja).distance(position(&app, player));

    let mut shots = Vec::new();
    let mut hits = Vec::new();
    for tick in 0..(64 * 3) {
        app.world_mut()
            .get_mut::<RangedInput>(player)
            .expect("input")
            .trigger_held = tick == 0;
        app.update();
        shots.extend(drain::<ShotFiredEvent>(&mut app));
        hits.extend(drain::<RangedHitEvent>(&mut app));
    }

    assert!(
        hits.iter()
            .any(|h| h.shooter == player && h.target == grunt),
        "the player hit the grunt"
    );
    let grunt_enemy = app.world().get::<Enemy>(grunt).expect("grunt");
    assert!(grunt_enemy.health < grunt_enemy.health_max);
    assert_eq!(alert(&app, grunt), AlertState::Combat);
    assert!(
        shots.iter().any(|s| s.shooter == grunt),
        "the grunt returned fire"
    );
    assert!(
        hits.iter()
            .any(|h| h.shooter == grunt && h.target == player),
        "and connected"
    );

    assert_eq!(
        alert(&app, ninja),
        AlertState::Combat,
        "alarm reached the ninja"
    );
    let ninja_bb = app.world().get::<AiBlackboard>(ninja).expect("blackboard");
    assert_eq!(ninja_bb.target_entity, Some(player));
    let ninja_end = position(&app, ninja).distance(position(&app, player));
    assert!(
        ninja_end < ninja_start - 8.0,
        "ninja closed from {ninja_start:.1} m to {ninja_end:.1} m"
    );
}
