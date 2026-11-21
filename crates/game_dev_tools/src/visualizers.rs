//! Gizmo overlays for hitboxes and spatial queries.
//!
//! Gameplay systems submit the casts they perform to [`DebugShapecastQueue`];
//! [`draw_debug_gizmos`] renders them as green (clear) or red (contact) rays
//! and drains the queue every frame.

use bevy::math::Isometry3d;
use bevy::prelude::*;
use game_combat::combat_dummy::{CombatDummy, DUMMY_RADIUS, DUMMY_SEGMENT_LENGTH};
use game_core::math::safe_normalize;

/// Upper bound on queued casts per frame, protecting memory if a producer
/// runs away while overlays are disabled.
pub const MAX_QUEUED_SHAPECASTS: usize = 4_096;

/// Which overlays to draw and in which colors.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct DebugVisualizerConfig {
    pub draw_hitboxes: bool,
    pub draw_shapecasts: bool,
    pub clear_color: Color,
    pub contact_color: Color,
    pub hitbox_color: Color,
}

impl Default for DebugVisualizerConfig {
    fn default() -> Self {
        Self {
            draw_hitboxes: false,
            draw_shapecasts: false,
            clear_color: Color::srgb(0.1, 1.0, 0.3),
            contact_color: Color::srgb(1.0, 0.15, 0.1),
            hitbox_color: Color::srgb(1.0, 0.8, 0.0),
        }
    }
}

/// One ray (`radius == 0`) or sphere cast to visualize.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugShapecast {
    pub origin: Vec3,
    /// Unit direction of travel.
    pub direction: Vec3,
    pub max_distance: f32,
    pub radius: f32,
    /// Distance to the first contact, if any.
    pub hit_distance: Option<f32>,
}

impl DebugShapecast {
    pub fn is_contact(&self) -> bool {
        self.hit_distance.is_some()
    }

    /// Where the cast stopped: the contact point, or its full length.
    pub fn end_point(&self) -> Vec3 {
        let distance = self
            .hit_distance
            .unwrap_or(self.max_distance)
            .clamp(0.0, self.max_distance);
        self.origin + self.direction * distance
    }
}

/// Casts submitted this frame for drawing.
#[derive(Resource, Debug, Default)]
pub struct DebugShapecastQueue {
    casts: Vec<DebugShapecast>,
}

impl DebugShapecastQueue {
    /// Queues a cast. Casts with non-finite input, a zero direction or a
    /// non-positive length are dropped, as are casts beyond
    /// [`MAX_QUEUED_SHAPECASTS`]. Returns whether the cast was queued.
    pub fn push(
        &mut self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        radius: f32,
        hit_distance: Option<f32>,
    ) -> bool {
        let direction = safe_normalize(direction, Vec3::ZERO);
        let valid = origin.is_finite()
            && direction != Vec3::ZERO
            && max_distance.is_finite()
            && max_distance > 0.0
            && radius.is_finite()
            && radius >= 0.0
            && hit_distance.map_or(true, f32::is_finite);
        if !valid || self.casts.len() >= MAX_QUEUED_SHAPECASTS {
            return false;
        }
        self.casts.push(DebugShapecast {
            origin,
            direction,
            max_distance,
            radius,
            hit_distance,
        });
        true
    }

    pub fn len(&self) -> usize {
        self.casts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.casts.is_empty()
    }

    pub fn drain(&mut self) -> std::vec::Drain<'_, DebugShapecast> {
        self.casts.drain(..)
    }
}

/// Mirrors the console's overlay toggles into [`DebugVisualizerConfig`].
pub fn sync_visualizer_config(
    state: Res<crate::physics_editor::DevToolsState>,
    mut config: ResMut<DebugVisualizerConfig>,
) {
    config.draw_hitboxes = state.show_hitboxes;
    config.draw_shapecasts = state.show_shapecasts;
}

/// Draws enabled overlays and drains the shapecast queue.
pub fn draw_debug_gizmos(
    mut gizmos: Gizmos,
    config: Res<DebugVisualizerConfig>,
    mut queue: ResMut<DebugShapecastQueue>,
    dummies: Query<&GlobalTransform, With<CombatDummy>>,
) {
    let draw_casts = config.draw_shapecasts;
    for cast in queue.drain() {
        if !draw_casts {
            continue;
        }
        let color = if cast.is_contact() {
            config.contact_color
        } else {
            config.clear_color
        };
        let end = cast.end_point();
        gizmos.line(cast.origin, end, color);
        if cast.radius > 0.0 {
            gizmos.sphere(Isometry3d::from_translation(end), cast.radius, color);
        }
    }

    if config.draw_hitboxes {
        let hurtbox = Capsule3d::new(DUMMY_RADIUS, DUMMY_SEGMENT_LENGTH);
        for transform in &dummies {
            let (_, rotation, translation) = transform.to_scale_rotation_translation();
            gizmos.primitive_3d(
                &hurtbox,
                Isometry3d::new(translation, rotation),
                config.hitbox_color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics_editor::DevToolsState;

    #[test]
    fn end_point_stops_at_contact() {
        let mut queue = DebugShapecastQueue::default();
        assert!(queue.push(Vec3::ZERO, Vec3::new(0.0, 0.0, -5.0), 10.0, 0.0, Some(4.0)));
        assert!(queue.push(Vec3::Y, Vec3::X, 2.0, 0.25, None));
        let casts: Vec<_> = queue.drain().collect();
        assert!(queue.is_empty());

        assert!(casts[0].is_contact());
        assert_eq!(casts[0].direction, Vec3::NEG_Z);
        assert_eq!(casts[0].end_point(), Vec3::new(0.0, 0.0, -4.0));
        assert!(!casts[1].is_contact());
        assert_eq!(casts[1].end_point(), Vec3::new(2.0, 1.0, 0.0));
    }

    #[test]
    fn contact_beyond_range_is_clamped() {
        let cast = DebugShapecast {
            origin: Vec3::ZERO,
            direction: Vec3::X,
            max_distance: 3.0,
            radius: 0.0,
            hit_distance: Some(50.0),
        };
        assert_eq!(cast.end_point(), Vec3::new(3.0, 0.0, 0.0));
    }

    #[test]
    fn invalid_casts_are_rejected() {
        let mut queue = DebugShapecastQueue::default();
        assert!(!queue.push(Vec3::NAN, Vec3::X, 1.0, 0.0, None));
        assert!(!queue.push(Vec3::ZERO, Vec3::ZERO, 1.0, 0.0, None));
        assert!(!queue.push(Vec3::ZERO, Vec3::INFINITY, 1.0, 0.0, None));
        assert!(!queue.push(Vec3::ZERO, Vec3::X, 0.0, 0.0, None));
        assert!(!queue.push(Vec3::ZERO, Vec3::X, f32::INFINITY, 0.0, None));
        assert!(!queue.push(Vec3::ZERO, Vec3::X, 1.0, -1.0, None));
        assert!(!queue.push(Vec3::ZERO, Vec3::X, 1.0, 0.0, Some(f32::NAN)));
        assert!(queue.is_empty());
    }

    #[test]
    fn queue_is_bounded() {
        let mut queue = DebugShapecastQueue::default();
        for _ in 0..MAX_QUEUED_SHAPECASTS {
            assert!(queue.push(Vec3::ZERO, Vec3::X, 1.0, 0.0, None));
        }
        assert!(!queue.push(Vec3::ZERO, Vec3::X, 1.0, 0.0, None));
        assert_eq!(queue.len(), MAX_QUEUED_SHAPECASTS);
    }

    #[test]
    fn config_follows_console_toggles() {
        let mut app = App::new();
        app.init_resource::<DevToolsState>()
            .init_resource::<DebugVisualizerConfig>()
            .add_systems(Update, sync_visualizer_config);

        {
            let mut state = app.world_mut().resource_mut::<DevToolsState>();
            state.show_hitboxes = true;
            state.show_shapecasts = true;
        }
        app.update();
        let config = app.world().resource::<DebugVisualizerConfig>();
        assert!(config.draw_hitboxes && config.draw_shapecasts);

        app.world_mut()
            .resource_mut::<DevToolsState>()
            .show_shapecasts = false;
        app.update();
        let config = app.world().resource::<DebugVisualizerConfig>();
        assert!(config.draw_hitboxes && !config.draw_shapecasts);
    }
}
