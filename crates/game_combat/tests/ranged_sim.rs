//! Headless ranged-combat simulations against real Avian colliders at 64 Hz.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use game_combat::combat_dummy::{calculate_dps, CombatDummy, CombatDummyType};
use game_combat::projectiles::{Projectile, SHOOTER_EYE_OFFSET};
use game_combat::{
    ActiveStatusEffects, BallisticProfile, CombatDummyPlugin, CombatPlugin, MeleeCombatState,
    RangedHitEvent, RangedInput, RecoilSpring, StatusEffectTickEvent, StatusEffectType,
};
use game_core::input::ViewAngles;
use game_parkour::{
    KinematicVelocity, MovementSpeedMultiplier, ParkourController, ParkourInput, ParkourPlugin,
};
use std::time::Duration;

const DUMMY_CENTER_Y: f32 = 0.9;
const SHOOTER_CENTER_Y: f32 = 0.9;

fn sim_app(with_dummy_plugin: bool) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        HierarchyPlugin,
        PhysicsPlugins::default(),
        CombatPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_nanos(
        15_625_000,
    )));
    if with_dummy_plugin {
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(CombatDummyPlugin);
    }
    app
}

fn settle(app: &mut App) {
    for _ in 0..3 {
        app.update();
    }
}

fn spawn_dummy(app: &mut App, position: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            CombatDummy::new(CombatDummyType::Static, position),
            Transform::from_translation(position),
            RigidBody::Kinematic,
            Collider::capsule(0.4, 1.0),
        ))
        .id()
}

/// A shooter at the origin aiming at `aim_point`.
fn spawn_shooter(app: &mut App, weapon: BallisticProfile, aim_point: Vec3) -> Entity {
    let position = Vec3::new(0.0, SHOOTER_CENTER_Y, 0.0);
    let eye = position + Vec3::Y * SHOOTER_EYE_OFFSET;
    app.world_mut()
        .spawn((
            weapon,
            RecoilSpring::default(),
            ViewAngles::looking_along(aim_point - eye),
            Transform::from_translation(position),
        ))
        .id()
}

fn set_trigger(app: &mut App, shooter: Entity, held: bool) {
    app.world_mut()
        .get_mut::<RangedInput>(shooter)
        .expect("input")
        .trigger_held = held;
}

fn tick(app: &mut App) -> Vec<RangedHitEvent> {
    app.update();
    app.world_mut()
        .resource_mut::<Events<RangedHitEvent>>()
        .drain()
        .collect()
}

fn weapon(app: &App, shooter: Entity) -> &BallisticProfile {
    app.world()
        .get::<BallisticProfile>(shooter)
        .expect("weapon")
}

#[test]
fn smg_burst_hits_dummy_applies_emp_and_kicks_recoil() {
    let mut app = sim_app(true);
    let dummy = spawn_dummy(&mut app, Vec3::new(0.0, DUMMY_CENTER_Y, -10.0));
    settle(&mut app);
    let shooter = spawn_shooter(
        &mut app,
        BallisticProfile::smart_submachine_gun(),
        Vec3::new(0.0, 1.1, -10.0),
    );

    let mut hits = Vec::new();
    set_trigger(&mut app, shooter, true);
    for _ in 0..64 {
        hits.extend(tick(&mut app));
        if weapon(&app, shooter).shots_fired == 5 {
            break;
        }
    }
    set_trigger(&mut app, shooter, false);
    assert_eq!(weapon(&app, shooter).current_ammo, 35, "5-round burst");

    let recoil_at_release = app
        .world()
        .get::<RecoilSpring>(shooter)
        .expect("recoil")
        .current_offset
        .x;
    assert!(recoil_at_release > 0.0, "recoil kicked the view up");

    for _ in 0..10 {
        hits.extend(tick(&mut app));
    }
    let world = app.world_mut();
    assert_eq!(
        world.query::<&Projectile>().iter(world).count(),
        0,
        "every round resolved"
    );
    assert!(
        hits.len() >= 4,
        "most of the burst connects at 10 m: {} hits",
        hits.len()
    );
    assert!(hits
        .iter()
        .all(|h| h.target == dummy && h.status_applied == Some(StatusEffectType::ShockEmp)));

    let d = app.world().get::<CombatDummy>(dummy).expect("dummy");
    let dealt: f32 = hits.iter().map(|h| h.damage).sum();
    assert!((d.total_damage_taken - dealt).abs() < 1e-3);
    assert!(calculate_dps(d) > 0.0);
    let effects = app
        .world()
        .get::<ActiveStatusEffects>(dummy)
        .expect("effects");
    assert!(effects.is_emp_active(), "Shock EMP applied");

    let mut previous = app
        .world()
        .get::<RecoilSpring>(shooter)
        .expect("recoil")
        .current_offset
        .x;
    for _ in 0..30 {
        app.update();
        let now = app
            .world()
            .get::<RecoilSpring>(shooter)
            .expect("recoil")
            .current_offset
            .x;
        assert!(
            now <= previous + 1e-6 && now >= 0.0,
            "settling without overshoot"
        );
        previous = now;
    }
    assert!(
        previous < recoil_at_release * 0.05,
        "back near center: {previous}"
    );
}

#[test]
fn sniper_headshot_is_instant_and_multiplied() {
    let mut app = sim_app(false);
    let dummy = spawn_dummy(&mut app, Vec3::new(0.0, DUMMY_CENTER_Y, -20.0));
    settle(&mut app);
    let shooter = spawn_shooter(
        &mut app,
        BallisticProfile::tech_sniper_rifle(),
        Vec3::new(0.0, DUMMY_CENTER_Y + 0.7, -20.0),
    );
    set_trigger(&mut app, shooter, true);
    let hits = tick(&mut app);
    assert_eq!(hits.len(), 1, "hitscan lands on the firing tick");
    assert!(hits[0].headshot);
    assert!((hits[0].damage - 220.0 * 3.5).abs() < 1e-3);
    let effects = app
        .world()
        .get::<ActiveStatusEffects>(dummy)
        .expect("effects");
    assert_eq!(effects.speed_multiplier(), 0.55, "Cryo Glitch applied");
}

#[test]
fn wall_stops_projectiles_before_the_target() {
    let mut app = sim_app(false);
    let dummy = spawn_dummy(&mut app, Vec3::new(0.0, DUMMY_CENTER_Y, -10.0));
    app.world_mut().spawn((
        Transform::from_xyz(0.0, 1.5, -5.0),
        RigidBody::Static,
        Collider::cuboid(6.0, 3.0, 0.3),
    ));
    settle(&mut app);
    let shooter = spawn_shooter(
        &mut app,
        BallisticProfile::charge_railgun(),
        Vec3::new(0.0, 1.1, -10.0),
    );
    set_trigger(&mut app, shooter, true);
    let mut hits = Vec::new();
    for _ in 0..20 {
        hits.extend(tick(&mut app));
    }
    assert!(hits.is_empty());
    assert_eq!(
        app.world()
            .get::<CombatDummy>(dummy)
            .expect("dummy")
            .total_damage_taken,
        0.0
    );
    let world = app.world_mut();
    assert_eq!(
        world.query::<&Projectile>().iter(world).count(),
        0,
        "rounds stopped by the wall"
    );
}

#[test]
fn emptying_the_magazine_triggers_an_empty_reload() {
    let mut app = sim_app(false);
    settle(&mut app);
    let shooter = spawn_shooter(
        &mut app,
        BallisticProfile::pulse_assault_rifle(),
        Vec3::new(0.0, 1.0, -50.0),
    );
    set_trigger(&mut app, shooter, true);
    let mut reload_started_at = None;
    for t in 0..600 {
        app.update();
        let w = weapon(&app, shooter);
        if reload_started_at.is_none() && w.is_reloading {
            reload_started_at = Some(t);
            assert_eq!(w.current_ammo, 0);
            assert!((w.reload_timer - 2.5).abs() < 0.05, "empty reload time");
        }
        if reload_started_at.is_some() && !w.is_reloading {
            break;
        }
    }
    let w = weapon(&app, shooter);
    assert!(reload_started_at.is_some());
    assert!(!w.is_reloading);
    assert!(
        w.current_ammo > 0 && w.reserve_ammo <= 150,
        "refilled from reserve"
    );
}

#[test]
fn thermal_burn_keeps_damaging_after_the_hit() {
    let mut app = sim_app(false);
    let dummy = spawn_dummy(&mut app, Vec3::new(0.0, DUMMY_CENTER_Y, -10.0));
    settle(&mut app);
    let shooter = spawn_shooter(
        &mut app,
        BallisticProfile::pulse_assault_rifle(),
        Vec3::new(0.0, 1.1, -10.0),
    );
    set_trigger(&mut app, shooter, true);
    let hits = tick(&mut app);
    set_trigger(&mut app, shooter, false);
    assert_eq!(hits.len(), 1);
    let after_hit = app
        .world()
        .get::<CombatDummy>(dummy)
        .expect("dummy")
        .total_damage_taken;

    let mut burn_ticks = 0;
    for _ in 0..(64 * 5) {
        app.update();
        burn_ticks += app
            .world_mut()
            .resource_mut::<Events<StatusEffectTickEvent>>()
            .drain()
            .filter(|e| e.target == dummy && e.tick.effect_type == StatusEffectType::ThermalBurn)
            .count();
    }
    assert_eq!(burn_ticks, 8, "4 s burn at 0.5 s intervals");
    let total = app
        .world()
        .get::<CombatDummy>(dummy)
        .expect("dummy")
        .total_damage_taken;
    assert!(
        (total - after_hit - 8.0 * 6.0).abs() < 1e-3,
        "burn dealt {}",
        total - after_hit
    );
    assert!(app
        .world()
        .get::<ActiveStatusEffects>(dummy)
        .expect("effects")
        .effects
        .is_empty());
}

#[test]
fn cryo_glitch_slows_a_running_parkour_character() {
    let mut app = sim_app(false);
    app.add_plugins(ParkourPlugin);
    app.world_mut().spawn((
        Transform::from_xyz(0.0, -0.1, 0.0),
        RigidBody::Static,
        Collider::cuboid(200.0, 0.2, 200.0),
    ));
    settle(&mut app);

    // Runner 15 m ahead, sprinting away along -Z.
    let runner = app
        .world_mut()
        .spawn((
            ParkourController::default(),
            MeleeCombatState::default(),
            Transform::from_xyz(0.0, 0.93, -15.0),
            RigidBody::Kinematic,
            Collider::capsule(0.4, 1.0),
        ))
        .id();
    let run = ParkourInput {
        move_direction: Vec2::new(0.0, -1.0),
        sprint: true,
        ..Default::default()
    };
    for _ in 0..32 {
        *app.world_mut()
            .get_mut::<ParkourInput>(runner)
            .expect("input") = run;
        app.update();
    }
    let speed = |app: &App| {
        app.world()
            .get::<KinematicVelocity>(runner)
            .expect("v")
            .planar_speed()
    };
    assert!((speed(&app) - 12.8).abs() < 0.05);

    let runner_z = app
        .world()
        .get::<Transform>(runner)
        .expect("t")
        .translation
        .z;
    let shooter = spawn_shooter(
        &mut app,
        BallisticProfile::tech_sniper_rifle(),
        Vec3::new(0.0, 1.1, runner_z - 0.2),
    );
    set_trigger(&mut app, shooter, true);
    let hits = tick(&mut app);
    set_trigger(&mut app, shooter, false);
    assert_eq!(hits.len(), 1, "sniper hit the runner");

    for _ in 0..32 {
        *app.world_mut()
            .get_mut::<ParkourInput>(runner)
            .expect("input") = run;
        app.update();
    }
    assert_eq!(
        app.world()
            .get::<MovementSpeedMultiplier>(runner)
            .map(|m| m.0),
        Some(0.55)
    );
    assert!(
        (speed(&app) - 12.8 * 0.55).abs() < 0.05,
        "slowed to {}",
        speed(&app)
    );

    for _ in 0..(64 * 4) {
        *app.world_mut()
            .get_mut::<ParkourInput>(runner)
            .expect("input") = run;
        app.update();
    }
    assert_eq!(
        app.world()
            .get::<MovementSpeedMultiplier>(runner)
            .map(|m| m.0),
        Some(1.0)
    );
    assert!(
        (speed(&app) - 12.8).abs() < 0.05,
        "back to full speed after 3.5 s"
    );
}
