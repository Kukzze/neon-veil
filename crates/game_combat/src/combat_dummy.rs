//! Combat target dummies, live DPS metering and floating combat text.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::Stopwatch;
use std::time::Duration;

/// Capsule radius of a dummy.
pub const DUMMY_RADIUS: f32 = 0.4;
/// Total standing height of a dummy, hemispheres included.
pub const DUMMY_HEIGHT: f32 = 1.8;
/// Length of the capsule's cylindrical section (height minus both caps).
pub const DUMMY_SEGMENT_LENGTH: f32 = DUMMY_HEIGHT - 2.0 * DUMMY_RADIUS;

pub const DUMMY_MAX_HEALTH: f32 = 100_000.0;
pub const DUMMY_POSTURE_MAX: f32 = 100.0;
/// Seconds without taking damage before a dummy fully resets.
pub const DUMMY_RESET_SECONDS: f32 = 5.0;
/// Period of the parry dummy's attack telegraph cycle.
pub const PARRY_TELEGRAPH_PERIOD_SECONDS: f32 = 2.5;
/// Remaining time in the cycle at which the yellow wind-up begins.
pub const PARRY_WINDUP_SECONDS: f32 = 0.30;
/// Remaining time in the cycle at which the red parry window opens.
pub const PARRY_WINDOW_SECONDS: f32 = 0.12;
/// DPS is averaged over at least this long so a single hit does not read as
/// an enormous spike.
pub const MIN_DPS_WINDOW_SECONDS: f32 = 1.0;

pub const STRAFE_AMPLITUDE_METERS: f32 = 4.0;
pub const STRAFE_ANGULAR_SPEED: f32 = 2.0;

pub const FLOATING_TEXT_LIFETIME_SECONDS: f32 = 1.0;
pub const FLOATING_TEXT_RISE_SPEED: f32 = 1.2;
/// Spawn height of floating text above the dummy's center.
pub const FLOATING_TEXT_SPAWN_OFFSET: f32 = 1.3;

pub const STATIC_DUMMY_POSITION: Vec3 = Vec3::new(-15.0, 0.9, -15.0);
pub const STRAFING_DUMMY_POSITION: Vec3 = Vec3::new(-25.0, 0.9, -15.0);
pub const PARRY_DUMMY_POSITION: Vec3 = Vec3::new(-35.0, 0.9, -15.0);

/// Behaviour profile of a dummy.
#[derive(Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[reflect(Debug, PartialEq, Hash)]
pub enum CombatDummyType {
    Static,
    Strafing,
    ParryTelegraph,
}

/// Phase of the parry dummy's attack telegraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParryTelegraphPhase {
    Idle,
    /// Yellow wind-up warning.
    Windup,
    /// Red window in which a parry succeeds.
    ParryWindow,
}

/// A damageable training target that never dies and resets when left alone.
#[derive(Component, Debug, Clone)]
pub struct CombatDummy {
    pub max_health: f32,
    pub current_health: f32,
    pub posture_max: f32,
    pub posture_current: f32,
    pub posture_broken: bool,
    pub total_damage_taken: f32,
    /// Time since the last hit; when it finishes the dummy resets.
    pub combat_timer: Timer,
    /// Length of the current engagement, used for DPS.
    pub combat_duration: Stopwatch,
    pub is_strafing: bool,
    pub parry_telegraph_timer: Timer,
    pub dummy_type: CombatDummyType,
    /// Anchor of the strafing motion.
    pub home_position: Vec3,
}

impl CombatDummy {
    pub fn new(dummy_type: CombatDummyType, home_position: Vec3) -> Self {
        let mut combat_timer = Timer::from_seconds(DUMMY_RESET_SECONDS, TimerMode::Once);
        // Start finished: a fresh dummy is out of combat.
        combat_timer.tick(combat_timer.duration());
        Self {
            max_health: DUMMY_MAX_HEALTH,
            current_health: DUMMY_MAX_HEALTH,
            posture_max: DUMMY_POSTURE_MAX,
            posture_current: 0.0,
            posture_broken: false,
            total_damage_taken: 0.0,
            combat_timer,
            combat_duration: Stopwatch::new(),
            is_strafing: dummy_type == CombatDummyType::Strafing,
            parry_telegraph_timer: Timer::from_seconds(
                PARRY_TELEGRAPH_PERIOD_SECONDS,
                TimerMode::Repeating,
            ),
            dummy_type,
            home_position,
        }
    }

    pub fn in_combat(&self) -> bool {
        self.total_damage_taken > 0.0
    }

    /// Restores full health and posture and clears the engagement.
    pub fn reset(&mut self) {
        self.current_health = self.max_health;
        self.posture_current = 0.0;
        self.posture_broken = false;
        self.total_damage_taken = 0.0;
        self.combat_duration.reset();
    }
}

/// Result of a single hit, used to drive feedback such as floating text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DummyHitOutcome {
    pub damage: f32,
    pub is_critical: bool,
    /// True only on the hit that broke posture.
    pub broke_posture: bool,
}

/// Requests damage on a dummy entity.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct DummyDamageEvent {
    pub target: Entity,
    pub damage: f32,
    pub posture_damage: f32,
    pub is_critical: bool,
}

/// A rising damage number that despawns when its lifetime ends. Rendering is
/// handled by the HUD; this component carries the data and motion.
#[derive(Component, Debug, Clone)]
pub struct FloatingCombatText {
    pub damage_value: f32,
    pub is_critical: bool,
    pub lifetime: Timer,
    pub initial_position: Vec3,
}

impl FloatingCombatText {
    pub fn new(damage_value: f32, is_critical: bool, initial_position: Vec3) -> Self {
        Self {
            damage_value,
            is_critical,
            lifetime: Timer::from_seconds(FLOATING_TEXT_LIFETIME_SECONDS, TimerMode::Once),
            initial_position,
        }
    }

    /// Current world position after rising for the elapsed lifetime.
    pub fn current_position(&self) -> Vec3 {
        self.initial_position + Vec3::Y * (FLOATING_TEXT_RISE_SPEED * self.lifetime.elapsed_secs())
    }
}

/// Materials used by dummies, swapped to show the parry telegraph.
#[derive(Resource, Debug, Clone)]
pub struct DummyMaterials {
    pub base: Handle<StandardMaterial>,
    pub windup: Handle<StandardMaterial>,
    pub parry_window: Handle<StandardMaterial>,
}

impl DummyMaterials {
    pub fn for_phase(&self, phase: ParryTelegraphPhase) -> &Handle<StandardMaterial> {
        match phase {
            ParryTelegraphPhase::Idle => &self.base,
            ParryTelegraphPhase::Windup => &self.windup,
            ParryTelegraphPhase::ParryWindow => &self.parry_window,
        }
    }
}

fn sanitize_amount(x: f32) -> f32 {
    if x.is_finite() {
        x.max(0.0)
    } else {
        0.0
    }
}

/// Applies one hit. Health never drops below zero; posture breaks when it
/// reaches `posture_max` and stays broken until the dummy resets. Negative or
/// non-finite amounts are treated as zero.
pub fn apply_dummy_damage(
    dummy: &mut CombatDummy,
    damage: f32,
    is_crit: bool,
    posture_damage: f32,
) -> DummyHitOutcome {
    let damage = sanitize_amount(damage);
    let posture_damage = sanitize_amount(posture_damage);

    dummy.current_health = (dummy.current_health - damage).max(0.0);
    dummy.total_damage_taken += damage;
    dummy.combat_timer.reset();

    let was_broken = dummy.posture_broken;
    dummy.posture_current = (dummy.posture_current + posture_damage).min(dummy.posture_max);
    if dummy.posture_current >= dummy.posture_max {
        dummy.posture_broken = true;
    }

    DummyHitOutcome {
        damage,
        is_critical: is_crit,
        broke_posture: dummy.posture_broken && !was_broken,
    }
}

/// Damage per second over the current engagement; `0.0` out of combat.
pub fn calculate_dps(dummy: &CombatDummy) -> f32 {
    if !dummy.in_combat() {
        return 0.0;
    }
    let window = dummy
        .combat_duration
        .elapsed_secs()
        .max(MIN_DPS_WINDOW_SECONDS);
    dummy.total_damage_taken / window
}

/// Telegraph phase derived from the time left in the current cycle.
pub fn parry_telegraph_phase(timer: &Timer) -> ParryTelegraphPhase {
    let remaining = timer.remaining_secs();
    if remaining <= PARRY_WINDOW_SECONDS {
        ParryTelegraphPhase::ParryWindow
    } else if remaining <= PARRY_WINDUP_SECONDS {
        ParryTelegraphPhase::Windup
    } else {
        ParryTelegraphPhase::Idle
    }
}

/// Horizontal strafing offset at `elapsed_secs`.
pub fn strafe_offset(elapsed_secs: f32) -> f32 {
    (elapsed_secs * STRAFE_ANGULAR_SPEED).sin() * STRAFE_AMPLITUDE_METERS
}

/// Advances a dummy's timers by `delta` and resets it once it has gone
/// [`DUMMY_RESET_SECONDS`] without being hit.
pub fn tick_dummy(dummy: &mut CombatDummy, delta: Duration) {
    dummy.combat_timer.tick(delta);
    dummy.parry_telegraph_timer.tick(delta);
    if dummy.in_combat() {
        dummy.combat_duration.tick(delta);
        if dummy.combat_timer.finished() {
            dummy.reset();
        }
    }
}

fn dummy_bundle(
    dummy_type: CombatDummyType,
    position: Vec3,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
) -> impl Bundle {
    let name = match dummy_type {
        CombatDummyType::Static => "combat_dummy_static",
        CombatDummyType::Strafing => "combat_dummy_strafing",
        CombatDummyType::ParryTelegraph => "combat_dummy_parry",
    };
    (
        Name::new(name),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(position),
        RigidBody::Kinematic,
        Collider::capsule(DUMMY_RADIUS, DUMMY_SEGMENT_LENGTH),
        CombatDummy::new(dummy_type, position),
    )
}

/// Spawns the static, strafing and parry-telegraph dummies.
pub fn spawn_combat_dummy_arena(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dummy_materials = DummyMaterials {
        base: materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.3, 0.35),
            perceptual_roughness: 0.5,
            metallic: 0.8,
            ..default()
        }),
        windup: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.1),
            emissive: LinearRgba::rgb(4.0, 3.2, 0.2),
            ..default()
        }),
        parry_window: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.1, 0.1),
            emissive: LinearRgba::rgb(6.0, 0.2, 0.2),
            ..default()
        }),
    };
    let mesh = meshes.add(Capsule3d::new(DUMMY_RADIUS, DUMMY_SEGMENT_LENGTH));

    for (dummy_type, position) in [
        (CombatDummyType::Static, STATIC_DUMMY_POSITION),
        (CombatDummyType::Strafing, STRAFING_DUMMY_POSITION),
        (CombatDummyType::ParryTelegraph, PARRY_DUMMY_POSITION),
    ] {
        commands.spawn(dummy_bundle(
            dummy_type,
            position,
            mesh.clone(),
            dummy_materials.base.clone(),
        ));
    }

    commands.insert_resource(dummy_materials);
}

/// Spawns a floating damage number above a target centered at `target_center`.
pub fn spawn_floating_combat_text(
    commands: &mut Commands,
    target_center: Vec3,
    damage: f32,
    is_critical: bool,
) {
    let origin = target_center + Vec3::Y * FLOATING_TEXT_SPAWN_OFFSET;
    commands.spawn((
        Name::new("floating_combat_text"),
        Transform::from_translation(origin),
        FloatingCombatText::new(damage, is_critical, origin),
    ));
}

/// Applies queued [`DummyDamageEvent`]s and spawns floating combat text.
pub fn apply_dummy_damage_events(
    mut commands: Commands,
    mut events: EventReader<DummyDamageEvent>,
    mut dummies: Query<(&mut CombatDummy, &Transform)>,
) {
    for event in events.read() {
        let Ok((mut dummy, transform)) = dummies.get_mut(event.target) else {
            warn!(
                "DummyDamageEvent targets {:?}, which is not a combat dummy",
                event.target
            );
            continue;
        };
        let outcome = apply_dummy_damage(
            &mut dummy,
            event.damage,
            event.is_critical,
            event.posture_damage,
        );
        spawn_floating_combat_text(
            &mut commands,
            transform.translation,
            outcome.damage,
            outcome.is_critical,
        );
    }
}

/// Ticks dummy timers, drives strafing motion and auto-resets idle dummies.
pub fn update_combat_dummies(
    time: Res<Time>,
    mut query: Query<(&mut CombatDummy, &mut Transform)>,
) {
    let offset = strafe_offset(time.elapsed_secs());
    for (mut dummy, mut transform) in &mut query {
        tick_dummy(&mut dummy, time.delta());
        if dummy.is_strafing {
            transform.translation.x = dummy.home_position.x + offset;
        }
    }
}

/// Swaps the parry dummy's material to show the wind-up and parry window.
pub fn update_parry_telegraph_materials(
    materials: Option<Res<DummyMaterials>>,
    mut query: Query<(&CombatDummy, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let Some(materials) = materials else {
        return;
    };
    for (dummy, mut material) in &mut query {
        if dummy.dummy_type != CombatDummyType::ParryTelegraph {
            continue;
        }
        let wanted = materials.for_phase(parry_telegraph_phase(&dummy.parry_telegraph_timer));
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

/// Rises floating combat text and despawns it when its lifetime ends.
pub fn update_floating_combat_text(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut FloatingCombatText, &mut Transform)>,
) {
    for (entity, mut text, mut transform) in &mut query {
        text.lifetime.tick(time.delta());
        if text.lifetime.finished() {
            commands.entity(entity).despawn();
        } else {
            transform.translation = text.current_position();
        }
    }
}

/// Spawns the dummy arena and runs dummy simulation. Add only in greybox mode.
pub struct CombatDummyPlugin;

impl Plugin for CombatDummyPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CombatDummyType>()
            .add_event::<DummyDamageEvent>()
            .add_systems(Startup, spawn_combat_dummy_arena)
            .add_systems(
                Update,
                (
                    apply_dummy_damage_events,
                    update_combat_dummies,
                    update_parry_telegraph_materials,
                    update_floating_combat_text,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-3;

    fn dummy() -> CombatDummy {
        CombatDummy::new(CombatDummyType::Static, STATIC_DUMMY_POSITION)
    }

    #[test]
    fn new_dummy_matches_spec_defaults() {
        let d = dummy();
        assert_eq!(d.max_health, 100_000.0);
        assert_eq!(d.current_health, 100_000.0);
        assert_eq!(d.posture_max, 100.0);
        assert_eq!(d.posture_current, 0.0);
        assert!(!d.in_combat());
        assert!(!d.is_strafing);
        assert!(CombatDummy::new(CombatDummyType::Strafing, Vec3::ZERO).is_strafing);
        assert_eq!(d.parry_telegraph_timer.mode(), TimerMode::Repeating);
        assert!((d.parry_telegraph_timer.duration().as_secs_f32() - 2.5).abs() < EPS);
    }

    #[test]
    fn capsule_stands_exactly_on_ground() {
        let total = DUMMY_SEGMENT_LENGTH + 2.0 * DUMMY_RADIUS;
        assert!((total - DUMMY_HEIGHT).abs() < EPS);
        for pos in [
            STATIC_DUMMY_POSITION,
            STRAFING_DUMMY_POSITION,
            PARRY_DUMMY_POSITION,
        ] {
            assert!((pos.y - DUMMY_HEIGHT * 0.5).abs() < EPS);
        }
    }

    #[test]
    fn damage_accumulates() {
        let mut d = dummy();
        apply_dummy_damage(&mut d, 1_200.0, false, 0.0);
        apply_dummy_damage(&mut d, 800.0, false, 0.0);
        assert_eq!(d.total_damage_taken, 2_000.0);
        assert_eq!(d.current_health, 98_000.0);
        assert!(d.in_combat());
    }

    #[test]
    fn critical_hits_are_reported() {
        let mut d = dummy();
        let crit = apply_dummy_damage(&mut d, 5_000.0, true, 0.0);
        assert!(crit.is_critical);
        assert_eq!(crit.damage, 5_000.0);
        let normal = apply_dummy_damage(&mut d, 100.0, false, 0.0);
        assert!(!normal.is_critical);
    }

    #[test]
    fn health_never_goes_negative_but_overkill_still_counts() {
        let mut d = dummy();
        apply_dummy_damage(&mut d, 250_000.0, false, 0.0);
        assert_eq!(d.current_health, 0.0);
        assert_eq!(d.total_damage_taken, 250_000.0);
    }

    #[test]
    fn hostile_amounts_are_ignored() {
        let mut d = dummy();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -500.0] {
            let outcome = apply_dummy_damage(&mut d, bad, false, bad);
            assert_eq!(outcome.damage, 0.0);
        }
        assert_eq!(d.current_health, d.max_health);
        assert_eq!(d.posture_current, 0.0);
        assert_eq!(d.total_damage_taken, 0.0);
    }

    #[test]
    fn posture_breaks_once_at_threshold() {
        let mut d = dummy();
        let first = apply_dummy_damage(&mut d, 10.0, false, 60.0);
        assert!(!first.broke_posture && !d.posture_broken);

        let second = apply_dummy_damage(&mut d, 10.0, false, 39.0);
        assert!(!second.broke_posture);
        assert!((d.posture_current - 99.0).abs() < EPS);

        let breaking = apply_dummy_damage(&mut d, 10.0, false, 1.0);
        assert!(breaking.broke_posture && d.posture_broken);
        assert_eq!(d.posture_current, d.posture_max);

        let after = apply_dummy_damage(&mut d, 10.0, false, 50.0);
        assert!(!after.broke_posture, "break is reported only once");
        assert_eq!(d.posture_current, d.posture_max, "posture is clamped");
    }

    #[test]
    fn dps_over_engagement() {
        let mut d = dummy();
        assert_eq!(calculate_dps(&d), 0.0);

        apply_dummy_damage(&mut d, 1_000.0, false, 0.0);
        assert!(
            (calculate_dps(&d) - 1_000.0).abs() < EPS,
            "uses minimum window"
        );

        tick_dummy(&mut d, Duration::from_secs(2));
        apply_dummy_damage(&mut d, 3_000.0, false, 0.0);
        tick_dummy(&mut d, Duration::from_secs(2));
        assert!((calculate_dps(&d) - 1_000.0).abs() < EPS);
    }

    #[test]
    fn dummy_resets_after_five_idle_seconds() {
        let mut d = dummy();
        apply_dummy_damage(&mut d, 4_000.0, false, 100.0);
        assert!(d.posture_broken);

        tick_dummy(&mut d, Duration::from_secs_f32(4.9));
        assert!(d.in_combat(), "still in combat before the timeout");

        tick_dummy(&mut d, Duration::from_secs_f32(0.2));
        assert!(!d.in_combat());
        assert_eq!(d.current_health, d.max_health);
        assert_eq!(d.total_damage_taken, 0.0);
        assert_eq!(d.posture_current, 0.0);
        assert!(!d.posture_broken);
        assert_eq!(d.combat_duration.elapsed(), Duration::ZERO);
        assert_eq!(calculate_dps(&d), 0.0);
    }

    #[test]
    fn hits_postpone_the_reset() {
        let mut d = dummy();
        apply_dummy_damage(&mut d, 100.0, false, 0.0);
        tick_dummy(&mut d, Duration::from_secs(4));
        apply_dummy_damage(&mut d, 100.0, false, 0.0);
        tick_dummy(&mut d, Duration::from_secs(4));
        assert!(d.in_combat());
        assert_eq!(d.total_damage_taken, 200.0);
    }

    #[test]
    fn telegraph_phases_follow_cycle() {
        let mut timer = Timer::from_seconds(PARRY_TELEGRAPH_PERIOD_SECONDS, TimerMode::Repeating);
        assert_eq!(parry_telegraph_phase(&timer), ParryTelegraphPhase::Idle);
        timer.tick(Duration::from_secs_f32(2.3));
        assert_eq!(parry_telegraph_phase(&timer), ParryTelegraphPhase::Windup);
        timer.tick(Duration::from_secs_f32(0.1));
        assert_eq!(
            parry_telegraph_phase(&timer),
            ParryTelegraphPhase::ParryWindow
        );
        timer.tick(Duration::from_secs_f32(0.2));
        assert_eq!(parry_telegraph_phase(&timer), ParryTelegraphPhase::Idle);
    }

    #[test]
    fn strafe_offset_is_bounded_sine() {
        assert_eq!(strafe_offset(0.0), 0.0);
        let quarter_period = std::f32::consts::FRAC_PI_2 / STRAFE_ANGULAR_SPEED;
        assert!((strafe_offset(quarter_period) - 4.0).abs() < EPS);
        for i in 0..200 {
            assert!(strafe_offset(i as f32 * 0.05).abs() <= STRAFE_AMPLITUDE_METERS + EPS);
        }
    }

    fn arena_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Time>()
            .add_plugins(CombatDummyPlugin);
        app.update();
        app
    }

    fn find_dummy(app: &mut App, dummy_type: CombatDummyType) -> Entity {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &CombatDummy)>();
        q.iter(world)
            .find(|(_, d)| d.dummy_type == dummy_type)
            .map(|(e, _)| e)
            .expect("dummy spawned")
    }

    #[test]
    fn arena_spawns_three_dummies_with_capsule_colliders() {
        let mut app = arena_app();
        let world = app.world_mut();
        let mut q = world.query::<(&CombatDummy, &Transform, &Collider, &RigidBody)>();
        let dummies: Vec<_> = q.iter(world).collect();
        assert_eq!(dummies.len(), 3);
        for (dummy, transform, collider, body) in dummies {
            assert_eq!(*body, RigidBody::Kinematic);
            assert_eq!(transform.translation, dummy.home_position);
            let capsule = collider.shape().as_capsule().expect("capsule collider");
            assert!((capsule.radius - DUMMY_RADIUS).abs() < EPS);
            assert!((capsule.height() - DUMMY_SEGMENT_LENGTH).abs() < EPS);
        }
        assert!(world.contains_resource::<DummyMaterials>());
    }

    #[test]
    fn damage_event_updates_dummy_and_spawns_floating_text() {
        let mut app = arena_app();
        let target = find_dummy(&mut app, CombatDummyType::Static);
        app.world_mut().send_event(DummyDamageEvent {
            target,
            damage: 2_500.0,
            posture_damage: 10.0,
            is_critical: true,
        });
        app.update();

        let dummy = app.world().get::<CombatDummy>(target).expect("dummy");
        assert_eq!(dummy.total_damage_taken, 2_500.0);
        assert_eq!(dummy.posture_current, 10.0);

        let world = app.world_mut();
        let mut q = world.query::<&FloatingCombatText>();
        let texts: Vec<_> = q.iter(world).collect();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].damage_value, 2_500.0);
        assert!(texts[0].is_critical);
    }

    #[test]
    fn floating_text_despawns_after_lifetime() {
        let mut app = arena_app();
        let target = find_dummy(&mut app, CombatDummyType::Static);
        app.world_mut().send_event(DummyDamageEvent {
            target,
            damage: 1.0,
            posture_damage: 0.0,
            is_critical: false,
        });
        app.update();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(
                FLOATING_TEXT_LIFETIME_SECONDS + 0.1,
            ));
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&FloatingCombatText>().iter(world).count(), 0);
    }

    #[test]
    fn strafing_dummy_moves_around_home() {
        let mut app = arena_app();
        let target = find_dummy(&mut app, CombatDummyType::Strafing);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.5));
        app.update();
        let x = app
            .world()
            .get::<Transform>(target)
            .expect("transform")
            .translation
            .x;
        let expected = STRAFING_DUMMY_POSITION.x + strafe_offset(0.5);
        assert!((x - expected).abs() < EPS, "x {x} expected {expected}");
    }

    #[test]
    fn damage_event_for_unknown_entity_is_ignored() {
        let mut app = arena_app();
        let stranger = app.world_mut().spawn_empty().id();
        app.world_mut().send_event(DummyDamageEvent {
            target: stranger,
            damage: 10.0,
            posture_damage: 0.0,
            is_critical: false,
        });
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&FloatingCombatText>().iter(world).count(), 0);
    }
}
