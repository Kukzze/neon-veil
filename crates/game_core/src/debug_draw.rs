//! Shared debug-draw queue for spatial queries.
//!
//! Lives in `game_core` so any gameplay crate can submit casts without
//! depending on the developer tools crate that renders them. Producers check
//! [`DebugShapecastQueue::is_recording`] first so no work is done while the
//! overlay is off. The queue is cleared once per fixed tick (see
//! [`clear_debug_shapecasts`]), so the latest tick's casts stay visible on
//! every rendered frame until the next tick replaces them.

use crate::math::safe_normalize;
use bevy::prelude::*;

/// Upper bound on queued casts, protecting memory if producers run away.
pub const MAX_QUEUED_SHAPECASTS: usize = 4_096;

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

/// Casts submitted since the last fixed tick.
#[derive(Resource, Debug, Default)]
pub struct DebugShapecastQueue {
    casts: Vec<DebugShapecast>,
    recording: bool,
}

impl DebugShapecastQueue {
    /// Whether an overlay is consuming casts; producers should skip
    /// submitting when this is false.
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
        if !recording {
            self.casts.clear();
        }
    }

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

    pub fn casts(&self) -> &[DebugShapecast] {
        &self.casts
    }

    pub fn len(&self) -> usize {
        self.casts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.casts.is_empty()
    }

    pub fn clear(&mut self) {
        self.casts.clear();
    }

    pub fn drain(&mut self) -> std::vec::Drain<'_, DebugShapecast> {
        self.casts.drain(..)
    }
}

/// Clears last tick's casts. Runs in `FixedFirst`.
pub fn clear_debug_shapecasts(mut queue: ResMut<DebugShapecastQueue>) {
    if !queue.is_empty() {
        queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn disabling_recording_discards_pending_casts() {
        let mut queue = DebugShapecastQueue::default();
        assert!(!queue.is_recording());
        queue.set_recording(true);
        queue.push(Vec3::ZERO, Vec3::X, 1.0, 0.0, None);
        queue.set_recording(false);
        assert!(queue.is_empty());
    }

    #[test]
    fn clear_system_empties_queue() {
        let mut app = App::new();
        app.init_resource::<DebugShapecastQueue>()
            .add_systems(Update, clear_debug_shapecasts);
        app.world_mut().resource_mut::<DebugShapecastQueue>().push(
            Vec3::ZERO,
            Vec3::X,
            1.0,
            0.0,
            None,
        );
        app.update();
        assert!(app.world().resource::<DebugShapecastQueue>().is_empty());
    }
}
