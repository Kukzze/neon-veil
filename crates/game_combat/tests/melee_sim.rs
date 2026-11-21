//! Headless melee simulations against real Avian colliders at 64 Hz.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use game_combat::combat_dummy::{calculate_dps, CombatDummy, CombatDummyType, FloatingCombatText};
use game_combat::{
    AttackPhase, CombatDummyPlugin, CombatPlugin, MeleeCombatState, MeleeHitEvent, MeleeInput,
    ParryResult,
};
use game_parkour::{KinematicVelocity, ParkourState};
use std::f32::consts::PI;
use std::time::Duration;

fn sim_app() -> App {
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

fn spawn_fighter(app: &mut App, position: Vec3, yaw: f32) -> Entity {
    app.world_mut()
        .spawn((
            MeleeCombatState::default(),
            Transform::from_translation(position).with_rotation(Quat::from_rotation_y(yaw)),
            RigidBody::Kinematic,
            Collider::capsule(0.4, 1.0),
        ))
        .id()
}

fn input(app: &mut App, entity: Entity) -> Mut<'_, MeleeInput> {
    app.world_mut()
        .get_mut::<MeleeInput>(entity)
        .expect("melee input")
}

fn melee(app: &App, entity: Entity) -> &MeleeCombatState {
    app.world()
        .get::<MeleeCombatState>(entity)
        .expect("melee state")
}

/// One update (= one fixed tick); returns the hits it produced.
fn tick(app: &mut App) -> Vec<MeleeHitEvent> {
    app.update();
    app.world_mut()
        .resource_mut::<Events<MeleeHitEvent>>()
        .drain()
        .collect()
}

#[test]
fn katana_combo_damages_dummy_with_floating_text_and_dps() {
    let mut app = sim_app();
    app.init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .add_plugins(CombatDummyPlugin);
    let dummy = spawn_dummy(&mut app, Vec3::new(0.0, 0.9, -1.2));
    settle(&mut app);
    let player = spawn_fighter(&mut app, Vec3::new(0.0, 0.9, 0.0), 0.0);

    let mut hits = Vec::new();
    let mut presses = 0;
    let mut max_floating_text = 0;
    for _ in 0..200 {
        let state = melee(&app, player);
        let ready = state.current_attack.is_none() || state.can_chain_combo();
        if ready && presses < 3 && state.queued_attack.is_none() {
            input(&mut app, player).light_pressed = true;
            presses += 1;
        }
        hits.extend(tick(&mut app));
        let world = app.world_mut();
        let texts = world.query::<&FloatingCombatText>().iter(world).count();
        max_floating_text = max_floating_text.max(texts);
        if hits.len() == 3 && melee(&app, player).current_attack.is_none() {
            break;
        }
    }

    let names: Vec<_> = hits.iter().map(|h| h.attack_name).collect();
    assert_eq!(
        names,
        ["Katana_Light_1", "Katana_Light_2", "Katana_Heavy_Finisher"]
    );
    assert!(hits
        .iter()
        .all(|h| h.target == dummy && h.parry_result == ParryResult::Hit));

    let d = app.world().get::<CombatDummy>(dummy).expect("dummy");
    assert_eq!(
        d.total_damage_taken,
        48.0 + 56.0 + 135.0,
        "each swing hit exactly once"
    );
    // 22 + 26 + 65 = 113 posture: the finisher breaks the dummy's guard.
    assert_eq!(d.posture_current, d.posture_max);
    assert!(d.posture_broken);
    let dps = calculate_dps(d);
    assert!(dps > 100.0, "dps {dps}");
    assert!(max_floating_text >= 2, "floating text spawned per hit");
}

#[test]
fn whiff_outside_reach_deals_nothing() {
    let mut app = sim_app();
    let dummy = spawn_dummy(&mut app, Vec3::new(0.0, 0.9, -3.5));
    settle(&mut app);
    let player = spawn_fighter(&mut app, Vec3::new(0.0, 0.9, 0.0), 0.0);
    input(&mut app, player).light_pressed = true;
    let mut hits = Vec::new();
    for _ in 0..40 {
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
}

/// Attacker swings Katana_Light_1 at a defender who raises the guard on
/// `parry_tick` (attack starts on tick 0; its hitbox goes live on tick 8).
fn duel(
    parry_tick: usize,
    defender_yaw: f32,
) -> (MeleeHitEvent, MeleeCombatState, MeleeCombatState) {
    let mut app = sim_app();
    settle(&mut app);
    let attacker = spawn_fighter(&mut app, Vec3::new(0.0, 0.9, 0.0), 0.0);
    let defender = spawn_fighter(&mut app, Vec3::new(0.0, 0.9, -1.2), defender_yaw);
    app.update();

    let mut hits = Vec::new();
    for t in 0..30 {
        if t == 0 {
            input(&mut app, attacker).light_pressed = true;
        }
        if t >= parry_tick {
            input(&mut app, defender).parry_held = true;
        }
        hits.extend(tick(&mut app));
        if !hits.is_empty() {
            break;
        }
    }
    assert_eq!(hits.len(), 1, "exactly one hit");
    (
        hits[0],
        melee(&app, attacker).clone(),
        melee(&app, defender).clone(),
    )
}

#[test]
fn perfect_parry_negates_and_staggers_the_attacker() {
    // Guard up on tick 1, hit on tick 8: 7 ticks = 109 ms <= 120 ms.
    let (hit, attacker, defender) = duel(1, PI);
    assert_eq!(hit.parry_result, ParryResult::PerfectParry);
    assert_eq!(hit.damage, 0.0);
    assert_eq!(hit.knockback, Vec3::ZERO);
    assert_eq!(attacker.posture_current, 40.0);
    assert_eq!(attacker.attack_phase, AttackPhase::Idle, "swing deflected");
    assert_eq!(defender.posture_current, 0.0);
}

#[test]
fn late_parry_blocks_seventy_percent() {
    // Guard up on tick 0: 8 ticks = 125 ms > 120 ms.
    let (hit, attacker, defender) = duel(0, PI);
    assert_eq!(hit.parry_result, ParryResult::Blocked);
    assert!(
        (hit.damage - 48.0 * 0.3).abs() < 1e-4,
        "damage {}",
        hit.damage
    );
    assert!((defender.posture_current - 11.0).abs() < 1e-4);
    assert_eq!(attacker.posture_current, 0.0);
}

#[test]
fn guard_facing_away_does_not_protect() {
    let (hit, _, defender) = duel(1, 0.0);
    assert_eq!(hit.parry_result, ParryResult::Hit);
    assert_eq!(hit.damage, 48.0);
    assert_eq!(defender.posture_current, 22.0);
}

#[test]
fn wall_run_plunge_shockwave_is_radius_limited() {
    let mut app = sim_app();
    let near = spawn_dummy(&mut app, Vec3::new(2.5, 0.9, -1.5));
    let corner = spawn_dummy(&mut app, Vec3::new(3.5, 0.9, -4.0));
    settle(&mut app);
    // Attacker running along a wall 2 m above the ground.
    let player = spawn_fighter(&mut app, Vec3::new(0.0, 2.9, 0.0), 0.0);
    app.world_mut()
        .entity_mut(player)
        .insert(ParkourState::WallRunRight);
    input(&mut app, player).light_pressed = true;

    let mut hits = Vec::new();
    for _ in 0..30 {
        hits.extend(tick(&mut app));
    }
    assert_eq!(hits.len(), 1, "only the dummy within 4 m is hit: {hits:?}");
    assert_eq!(hits[0].target, near);
    assert_eq!(hits[0].attack_name, "Wall_Run_Plunge");
    assert_eq!(hits[0].damage, 90.0);
    assert!(hits[0].knockback.x > 0.0, "pushed outward from the impact");
    assert_eq!(
        app.world()
            .get::<CombatDummy>(corner)
            .expect("dummy")
            .total_damage_taken,
        0.0
    );
}

#[test]
fn aerial_slam_dives_and_breaks_posture() {
    let mut app = sim_app();
    settle(&mut app);
    let attacker = spawn_fighter(&mut app, Vec3::new(0.0, 0.9, 0.0), 0.0);
    app.world_mut().entity_mut(attacker).insert((
        ParkourState::JumpDescent,
        KinematicVelocity(Vec3::new(0.0, -4.0, -6.0)),
    ));
    let victim = spawn_fighter(&mut app, Vec3::new(0.0, 0.9, -0.8), 0.0);
    app.update();

    input(&mut app, attacker).heavy_pressed = true;
    let mut hits = tick(&mut app);
    let v = app
        .world()
        .get::<KinematicVelocity>(attacker)
        .expect("velocity")
        .0;
    assert_eq!(v.y, -35.0, "driven straight down");
    assert_eq!(v.z, -6.0, "planar momentum kept");
    for _ in 0..20 {
        hits.extend(tick(&mut app));
    }
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].attack_name, "Aerial_Slam");
    assert!(
        melee(&app, victim).is_posture_broken,
        "100 posture damage shatters the guard"
    );
}
