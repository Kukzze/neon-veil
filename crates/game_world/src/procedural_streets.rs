//! Procedural traversal infrastructure: skybridges, bounce awnings, steam
//! vent geysers and conveyor belts.
//!
//! Interactive props carry a [`TraversalVolume`] (an oriented box in the
//! prop's local space) and act on parkour characters whose capsule center
//! is inside it. They are root entities, so their `Transform` is their
//! world placement. Launches run before the parkour controller step, belt
//! carry runs after it.

use avian3d::prelude::*;
use bevy::prelude::*;
use game_parkour::{KinematicVelocity, ParkourController, ParkourState};

pub const SKYBRIDGE_DECK_THICKNESS: f32 = 0.4;
pub const AWNING_THICKNESS: f32 = 0.25;
pub const BELT_THICKNESS: f32 = 0.3;
/// How far above a walkable prop surface its volume reaches. Covers the
/// standing capsule center (~1 m) with room to spare.
pub const SURFACE_VOLUME_HEIGHT: f32 = 1.6;
pub const STEAM_VENT_LIFT_SPEED: f32 = 20.0;

/// Oriented trigger box: `half_extents` around `offset`, in local space.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Debug, PartialEq)]
pub struct TraversalVolume {
    pub offset: Vec3,
    pub half_extents: Vec3,
}

impl TraversalVolume {
    /// The slab above a surface whose top is at local `surface_top`, slightly
    /// overlapping it so a resting capsule is always inside.
    pub fn above_surface(size_x: f32, size_z: f32, surface_top: f32) -> Self {
        let height = SURFACE_VOLUME_HEIGHT + 0.1;
        Self {
            offset: Vec3::new(0.0, surface_top - 0.1 + height * 0.5, 0.0),
            half_extents: Vec3::new(size_x * 0.5, height * 0.5, size_z * 0.5),
        }
    }

    pub fn contains(&self, transform: &Transform, point: Vec3) -> bool {
        let local = transform.rotation.inverse() * (point - transform.translation) - self.offset;
        local.abs().cmple(self.half_extents).all()
    }
}

/// Placement of a straight skybridge deck between two points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkybridgeLayout {
    pub length: f32,
    pub midpoint: Vec3,
    /// Local +Z runs along the span; local +X stays horizontal so inclined
    /// decks never roll.
    pub rotation: Quat,
    pub width: f32,
}

impl SkybridgeLayout {
    /// `None` when the endpoints coincide or the width is not positive.
    pub fn new(start: Vec3, end: Vec3, width: f32) -> Option<Self> {
        let span = end - start;
        let length = span.length();
        if length < 1e-3 || width <= 0.0 || !span.is_finite() {
            return None;
        }
        let dir = span / length;
        let rotation = if dir.y.abs() > 0.999 {
            Quat::from_rotation_arc(Vec3::Z, dir)
        } else {
            let x_axis = Vec3::Y.cross(dir).normalize();
            let y_axis = dir.cross(x_axis);
            Quat::from_mat3(&Mat3::from_cols(x_axis, y_axis, dir))
        };
        Some(Self {
            length,
            midpoint: (start + end) * 0.5,
            rotation,
            width,
        })
    }

    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.midpoint).with_rotation(self.rotation)
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Debug, PartialEq)]
pub struct Skybridge {
    pub start: Vec3,
    pub end: Vec3,
    pub width: f32,
}

/// Elastic storefront awning: landing on it springs the player back up.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Debug, PartialEq)]
pub struct BounceAwning {
    /// Fraction of the landing speed returned upward.
    pub restitution: f32,
    pub min_rebound_speed: f32,
    pub max_rebound_speed: f32,
}

impl Default for BounceAwning {
    fn default() -> Self {
        Self {
            restitution: 0.85,
            min_rebound_speed: 9.0,
            max_rebound_speed: 22.0,
        }
    }
}

impl BounceAwning {
    /// Velocity after landing with `velocity`, or `None` when not falling.
    pub fn rebound(&self, velocity: Vec3) -> Option<Vec3> {
        (velocity.y < -0.5).then(|| {
            let up = (-velocity.y * self.restitution)
                .clamp(self.min_rebound_speed, self.max_rebound_speed);
            Vec3::new(velocity.x, up, velocity.z)
        })
    }
}

/// Geothermal vent that erupts on a cycle and launches anyone above it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Debug, PartialEq)]
pub struct SteamVentGeyser {
    /// Vertical speed a launch guarantees, m/s.
    pub lift_speed: f32,
    pub erupt_secs: f32,
    pub dormant_secs: f32,
    /// Position in the cycle, s; eruption first.
    pub cycle_time: f32,
}

impl Default for SteamVentGeyser {
    fn default() -> Self {
        Self {
            lift_speed: STEAM_VENT_LIFT_SPEED,
            erupt_secs: 1.5,
            dormant_secs: 2.5,
            cycle_time: 0.0,
        }
    }
}

impl SteamVentGeyser {
    /// A vent that never goes dormant.
    pub fn continuous() -> Self {
        Self {
            dormant_secs: 0.0,
            ..default()
        }
    }

    pub fn period(&self) -> f32 {
        self.erupt_secs + self.dormant_secs
    }

    pub fn is_erupting(&self) -> bool {
        self.cycle_time < self.erupt_secs
    }

    pub fn advance(&mut self, dt: f32) {
        let period = self.period();
        if period > 0.0 {
            self.cycle_time = (self.cycle_time + dt).rem_euclid(period);
        }
    }

    /// Raises vertical speed to at least `lift_speed` while erupting. The
    /// result does not depend on how long the player has been in the column,
    /// so it is applied every tick without stacking.
    pub fn launch(&self, velocity: Vec3) -> Option<Vec3> {
        (self.is_erupting() && velocity.y < self.lift_speed)
            .then(|| Vec3::new(velocity.x, self.lift_speed, velocity.z))
    }
}

/// Moving belt along its local +Z; negative speed runs it backwards.
/// Grounded characters on it are carried, so running with the belt is
/// faster and against it slower.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Debug, PartialEq)]
pub struct ConveyorBelt {
    pub speed: f32,
}

impl ConveyorBelt {
    pub fn belt_velocity(&self, transform: &Transform) -> Vec3 {
        transform.rotation * Vec3::Z * self.speed
    }
}

fn can_be_launched(state: ParkourState) -> bool {
    state.is_grounded() || state.is_airborne()
}

pub fn advance_steam_vents(time: Res<Time>, mut vents: Query<&mut SteamVentGeyser>) {
    let dt = time.delta_secs();
    for mut vent in &mut vents {
        vent.advance(dt);
    }
}

type LaunchCharacter<'a> = (
    &'a Transform,
    &'a mut KinematicVelocity,
    &'a mut ParkourState,
    &'a mut ParkourController,
);

/// Bounce awnings and erupting vents throw characters into a jump ascent.
pub fn apply_launch_surfaces(
    mut characters: Query<LaunchCharacter, Without<TraversalVolume>>,
    awnings: Query<(&Transform, &TraversalVolume, &BounceAwning)>,
    vents: Query<(&Transform, &TraversalVolume, &SteamVentGeyser)>,
) {
    for (transform, mut velocity, mut state, mut controller) in &mut characters {
        if !can_be_launched(*state) {
            continue;
        }
        let at = transform.translation;
        let launched = awnings
            .iter()
            .filter(|(t, volume, _)| volume.contains(t, at))
            .find_map(|(_, _, awning)| awning.rebound(velocity.0))
            .or_else(|| {
                vents
                    .iter()
                    .filter(|(t, volume, _)| volume.contains(t, at))
                    .find_map(|(_, _, vent)| vent.launch(velocity.0))
            });
        if let Some(v) = launched {
            velocity.0 = v;
            *state = ParkourState::JumpAscent;
            controller.state_time = 0.0;
            controller.airborne_vertical_velocity = v.y;
        }
    }
}

/// Carries grounded characters standing on belts.
pub fn apply_conveyor_belts(
    time: Res<Time>,
    mut characters: Query<
        (&mut Transform, &ParkourState),
        (With<ParkourController>, Without<ConveyorBelt>),
    >,
    belts: Query<(&Transform, &TraversalVolume, &ConveyorBelt)>,
) {
    let dt = time.delta_secs();
    for (mut transform, state) in &mut characters {
        if !state.is_grounded() {
            continue;
        }
        let at = transform.translation;
        if let Some((belt_transform, _, belt)) = belts.iter().find(|(t, v, _)| v.contains(t, at)) {
            transform.translation += belt.belt_velocity(belt_transform) * dt;
        }
    }
}

fn surface_material(
    materials: &mut Assets<StandardMaterial>,
    color: Color,
    emissive: LinearRgba,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        emissive,
        metallic: 0.6,
        perceptual_roughness: 0.5,
        ..default()
    })
}

/// Static deck from `start` to `end`. `None` for a degenerate span.
pub fn spawn_skybridge(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    width: f32,
) -> Option<Entity> {
    let layout = SkybridgeLayout::new(start, end, width)?;
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.2, 0.24),
        metallic: 0.8,
        perceptual_roughness: 0.35,
        ..default()
    });
    let entity = commands
        .spawn((
            Name::new("Skybridge"),
            Skybridge { start, end, width },
            Mesh3d(meshes.add(Cuboid::new(width, SKYBRIDGE_DECK_THICKNESS, layout.length))),
            MeshMaterial3d(material),
            layout.transform(),
            RigidBody::Static,
            Collider::cuboid(width, SKYBRIDGE_DECK_THICKNESS, layout.length),
        ))
        .id();
    Some(entity)
}

/// Awning slab centered at `transform`, `size` = (width x, depth z).
pub fn spawn_bounce_awning(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    transform: Transform,
    size: Vec2,
    awning: BounceAwning,
) -> Entity {
    let material = surface_material(
        materials,
        Color::srgb(0.75, 0.12, 0.42),
        LinearRgba::rgb(1.6, 0.2, 0.7),
    );
    commands
        .spawn((
            Name::new("Bounce Awning"),
            awning,
            TraversalVolume::above_surface(size.x, size.y, AWNING_THICKNESS * 0.5),
            Mesh3d(meshes.add(Cuboid::new(size.x, AWNING_THICKNESS, size.y))),
            MeshMaterial3d(material),
            transform,
            RigidBody::Static,
            Collider::cuboid(size.x, AWNING_THICKNESS, size.y),
        ))
        .id()
}

/// Flush vent grate at `position` (grate top) with a launch column above.
pub fn spawn_steam_vent(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    position: Vec3,
    radius: f32,
    geyser: SteamVentGeyser,
) -> Entity {
    let material = surface_material(
        materials,
        Color::srgb(0.18, 0.16, 0.14),
        LinearRgba::rgb(0.9, 0.35, 0.05),
    );
    let column_height = 3.0;
    commands
        .spawn((
            Name::new("Steam Vent"),
            geyser,
            TraversalVolume {
                offset: Vec3::new(0.0, column_height * 0.5 - 0.1, 0.0),
                half_extents: Vec3::new(radius, column_height * 0.5, radius),
            },
            Mesh3d(meshes.add(Cylinder::new(radius, 0.1))),
            MeshMaterial3d(material),
            Transform::from_translation(position - Vec3::Y * 0.05),
        ))
        .id()
}

/// Belt slab centered at `transform`, running along its local +Z.
pub fn spawn_conveyor_belt(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    transform: Transform,
    length: f32,
    width: f32,
    belt: ConveyorBelt,
) -> Entity {
    let material = surface_material(
        materials,
        Color::srgb(0.08, 0.08, 0.09),
        LinearRgba::rgb(0.0, 0.25, 0.3),
    );
    commands
        .spawn((
            Name::new("Conveyor Belt"),
            belt,
            TraversalVolume::above_surface(width, length, BELT_THICKNESS * 0.5),
            Mesh3d(meshes.add(Cuboid::new(width, BELT_THICKNESS, length))),
            MeshMaterial3d(material),
            transform,
            RigidBody::Static,
            Collider::cuboid(width, BELT_THICKNESS, length),
        ))
        .id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn approx(a: Vec3, b: Vec3) -> bool {
        a.distance(b) < 1e-4
    }

    #[test]
    fn skybridge_layout() {
        let start = Vec3::new(0.0, 100.0, 0.0);
        let end = Vec3::new(30.0, 100.0, 40.0);
        let layout = SkybridgeLayout::new(start, end, 4.0).unwrap();
        assert!((layout.length - 50.0).abs() < 1e-4);
        assert!(approx(layout.midpoint, Vec3::new(15.0, 100.0, 20.0)));
        let dir = (end - start).normalize();
        assert!(approx(layout.rotation * Vec3::Z, dir));
        assert!(
            approx(layout.rotation * Vec3::Y, Vec3::Y),
            "flat deck faces up"
        );
        // Horizontal spans agree with the shortest-arc rotation.
        let arc = Quat::from_rotation_arc(Vec3::Z, dir);
        assert!(layout.rotation.angle_between(arc) < 1e-4);
        // Deck ends land on the endpoints.
        let t = layout.transform();
        assert!(approx(
            t.transform_point(Vec3::Z * layout.length * 0.5),
            end
        ));
        assert!(approx(
            t.transform_point(-Vec3::Z * layout.length * 0.5),
            start
        ));
    }

    #[test]
    fn inclined_skybridge_does_not_roll() {
        let start = Vec3::ZERO;
        let end = Vec3::new(20.0, 15.0, 20.0);
        let layout = SkybridgeLayout::new(start, end, 3.0).unwrap();
        assert!(approx(layout.rotation * Vec3::Z, (end - start).normalize()));
        assert!(
            (layout.rotation * Vec3::X).y.abs() < 1e-5,
            "deck stays level side to side"
        );
        assert!((layout.rotation * Vec3::Y).y > 0.0, "deck top faces up");
        let vertical = SkybridgeLayout::new(start, Vec3::Y * 10.0, 2.0).unwrap();
        assert!(approx(vertical.rotation * Vec3::Z, Vec3::Y));
        assert!(SkybridgeLayout::new(start, start, 3.0).is_none());
        assert!(SkybridgeLayout::new(start, end, 0.0).is_none());
    }

    #[test]
    fn volume_is_oriented() {
        let t = Transform::from_xyz(10.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(FRAC_PI_2));
        let volume = TraversalVolume {
            offset: Vec3::ZERO,
            half_extents: Vec3::new(1.0, 1.0, 5.0),
        };
        // Local +Z is world +X after the quarter turn.
        assert!(volume.contains(&t, Vec3::new(14.0, 0.0, 0.0)));
        assert!(!volume.contains(&t, Vec3::new(10.0, 0.0, 4.0)));
        let slab = TraversalVolume::above_surface(4.0, 4.0, 0.125);
        let at = Transform::from_xyz(0.0, 5.0, 0.0);
        assert!(
            slab.contains(&at, Vec3::new(0.0, 6.0, 0.0)),
            "standing capsule center"
        );
        assert!(slab.contains(&at, Vec3::new(1.9, 6.6, -1.9)));
        assert!(!slab.contains(&at, Vec3::new(0.0, 6.9, 0.0)));
        assert!(
            !slab.contains(&at, Vec3::new(0.0, 4.8, 0.0)),
            "below the slab"
        );
    }

    #[test]
    fn awning_rebound() {
        let awning = BounceAwning::default();
        let v = awning.rebound(Vec3::new(3.0, -12.0, 1.0)).unwrap();
        assert!(approx(v, Vec3::new(3.0, 10.2, 1.0)));
        assert_eq!(
            awning.rebound(Vec3::new(0.0, -2.0, 0.0)).unwrap().y,
            9.0,
            "minimum spring"
        );
        assert_eq!(
            awning.rebound(Vec3::new(0.0, -60.0, 0.0)).unwrap().y,
            22.0,
            "capped"
        );
        assert!(
            awning.rebound(Vec3::new(5.0, 0.0, 0.0)).is_none(),
            "walking across"
        );
        assert!(awning.rebound(Vec3::Y * 4.0).is_none(), "rising through");
    }

    #[test]
    fn geyser_cycle_and_launch() {
        let mut vent = SteamVentGeyser::default();
        assert!(vent.is_erupting());
        assert_eq!(
            vent.launch(Vec3::new(2.0, -5.0, 0.0)),
            Some(Vec3::new(2.0, 20.0, 0.0))
        );
        assert_eq!(vent.launch(Vec3::Y * 25.0), None, "already faster");
        vent.advance(1.6);
        assert!(!vent.is_erupting());
        assert_eq!(vent.launch(Vec3::ZERO), None);
        vent.advance(2.5);
        assert!(vent.is_erupting(), "wrapped into the next eruption");
        assert!((vent.cycle_time - 0.1).abs() < 1e-4);
        let mut always = SteamVentGeyser::continuous();
        always.advance(100.0);
        assert!(always.is_erupting());
    }

    #[test]
    fn belt_direction_follows_rotation() {
        let belt = ConveyorBelt { speed: 4.0 };
        let t = Transform::from_rotation(Quat::from_rotation_y(FRAC_PI_2));
        assert!(approx(belt.belt_velocity(&t), Vec3::X * 4.0));
        let reverse = ConveyorBelt { speed: -2.0 };
        assert!(approx(
            reverse.belt_velocity(&Transform::IDENTITY),
            -Vec3::Z * 2.0
        ));
    }

    fn prop_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(
            Update,
            (
                advance_steam_vents,
                apply_launch_surfaces,
                apply_conveyor_belts,
            )
                .chain(),
        );
        app
    }

    fn character(app: &mut App, at: Vec3, velocity: Vec3, state: ParkourState) -> Entity {
        app.world_mut()
            .spawn((
                ParkourController::default(),
                Transform::from_translation(at),
                KinematicVelocity(velocity),
                state,
            ))
            .id()
    }

    #[test]
    fn landing_on_an_awning_bounces_the_character() {
        let mut app = prop_app();
        let surface = Transform::from_xyz(0.0, 10.0, 0.0);
        app.world_mut().spawn((
            surface,
            BounceAwning::default(),
            TraversalVolume::above_surface(4.0, 4.0, AWNING_THICKNESS * 0.5),
        ));
        let faller = character(
            &mut app,
            Vec3::new(0.0, 11.2, 0.0),
            Vec3::new(1.0, -14.0, 0.0),
            ParkourState::JumpDescent,
        );
        let walker = character(
            &mut app,
            Vec3::new(1.0, 11.0, 0.0),
            Vec3::X * 3.0,
            ParkourState::Walking,
        );
        let vaulter = character(
            &mut app,
            Vec3::new(-1.0, 11.0, 0.0),
            Vec3::NEG_Y * 9.0,
            ParkourState::VaultMid,
        );
        app.update();
        let world = app.world();
        assert_eq!(
            *world.get::<ParkourState>(faller).unwrap(),
            ParkourState::JumpAscent
        );
        let v = world.get::<KinematicVelocity>(faller).unwrap().0;
        assert!(approx(v, Vec3::new(1.0, 11.9, 0.0)), "{v}");
        assert_eq!(
            world
                .get::<ParkourController>(faller)
                .unwrap()
                .airborne_vertical_velocity,
            v.y
        );
        assert_eq!(
            *world.get::<ParkourState>(walker).unwrap(),
            ParkourState::Walking
        );
        assert_eq!(
            *world.get::<ParkourState>(vaulter).unwrap(),
            ParkourState::VaultMid,
            "scripted moves are left alone"
        );
    }

    #[test]
    fn vent_launches_only_while_erupting() {
        let mut app = prop_app();
        app.world_mut().spawn((
            Transform::IDENTITY,
            SteamVentGeyser {
                erupt_secs: 1.0,
                dormant_secs: 1.0,
                cycle_time: 1.5,
                ..default()
            },
            TraversalVolume {
                offset: Vec3::Y * 1.4,
                half_extents: Vec3::new(1.0, 1.5, 1.0),
            },
        ));
        let idle = character(&mut app, Vec3::Y, Vec3::ZERO, ParkourState::Idle);
        app.update();
        assert_eq!(
            *app.world().get::<ParkourState>(idle).unwrap(),
            ParkourState::Idle,
            "dormant"
        );
        let mut vent_q = app.world_mut().query::<&mut SteamVentGeyser>();
        vent_q.single_mut(app.world_mut()).cycle_time = 0.0;
        app.update();
        assert_eq!(
            *app.world().get::<ParkourState>(idle).unwrap(),
            ParkourState::JumpAscent
        );
        assert_eq!(
            app.world().get::<KinematicVelocity>(idle).unwrap().0.y,
            STEAM_VENT_LIFT_SPEED
        );
    }

    #[test]
    fn belts_carry_grounded_characters_only() {
        let mut app = prop_app();
        app.world_mut().spawn((
            Transform::from_rotation(Quat::from_rotation_y(FRAC_PI_2)),
            ConveyorBelt { speed: 5.0 },
            TraversalVolume::above_surface(2.0, 20.0, BELT_THICKNESS * 0.5),
        ));
        let rider = character(&mut app, Vec3::Y, Vec3::ZERO, ParkourState::Idle);
        let jumper = character(
            &mut app,
            Vec3::new(2.0, 1.0, 0.0),
            Vec3::Y,
            ParkourState::JumpAscent,
        );
        let off = character(
            &mut app,
            Vec3::new(0.0, 1.0, 5.0),
            Vec3::ZERO,
            ParkourState::Idle,
        );
        app.update();
        app.update();
        let x_of = |e: Entity| app.world().get::<Transform>(e).unwrap().translation.x;
        assert!(x_of(rider) > 0.0, "carried along world +X");
        assert_eq!(x_of(jumper), 2.0);
        assert_eq!(x_of(off), 0.0);
    }

    #[test]
    fn spawners_build_static_colliders() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(
                Startup,
                |mut commands: Commands,
                 mut meshes: ResMut<Assets<Mesh>>,
                 mut materials: ResMut<Assets<StandardMaterial>>| {
                    let (m, mat) = (meshes.as_mut(), materials.as_mut());
                    spawn_skybridge(
                        &mut commands,
                        m,
                        mat,
                        Vec3::ZERO,
                        Vec3::new(0.0, 0.0, 30.0),
                        4.0,
                    );
                    assert!(
                        spawn_skybridge(&mut commands, m, mat, Vec3::ONE, Vec3::ONE, 4.0).is_none()
                    );
                    spawn_bounce_awning(
                        &mut commands,
                        m,
                        mat,
                        Transform::default(),
                        Vec2::new(4.0, 2.0),
                        BounceAwning::default(),
                    );
                    spawn_steam_vent(
                        &mut commands,
                        m,
                        mat,
                        Vec3::X * 5.0,
                        1.0,
                        SteamVentGeyser::default(),
                    );
                    spawn_conveyor_belt(
                        &mut commands,
                        m,
                        mat,
                        Transform::default(),
                        12.0,
                        2.0,
                        ConveyorBelt { speed: 3.0 },
                    );
                },
            );
        app.update();
        let world = app.world_mut();
        let statics = world
            .query_filtered::<&RigidBody, With<Collider>>()
            .iter(world)
            .filter(|b| **b == RigidBody::Static)
            .count();
        assert_eq!(
            statics, 3,
            "bridge, awning and belt are solid; the vent is a flush grate"
        );
        assert_eq!(world.query::<&TraversalVolume>().iter(world).count(), 3);
        let bridge = world.query::<(&Skybridge, &Transform)>().single(world);
        assert_eq!(bridge.1.translation, Vec3::new(0.0, 0.0, 15.0));
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 4);
    }
}
