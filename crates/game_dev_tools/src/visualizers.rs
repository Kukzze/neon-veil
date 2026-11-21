//! Gizmo overlays for hitboxes and spatial queries.
//!
//! Gameplay systems submit the casts they perform to
//! [`DebugShapecastQueue`] (defined in `game_core` so producers need not
//! depend on this crate); [`draw_debug_gizmos`] renders them as green (clear)
//! or red (contact) rays. The queue is cleared once per fixed tick by
//! `CoreGameplayPlugin`, so the latest casts remain visible between ticks.

use bevy::math::Isometry3d;
use bevy::prelude::*;
use game_combat::combat_dummy::{CombatDummy, DUMMY_RADIUS, DUMMY_SEGMENT_LENGTH};

pub use game_core::debug_draw::{DebugShapecast, DebugShapecastQueue, MAX_QUEUED_SHAPECASTS};

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

/// Mirrors the console's overlay toggles into [`DebugVisualizerConfig`] and
/// tells producers whether to record casts.
pub fn sync_visualizer_config(
    state: Res<crate::physics_editor::DevToolsState>,
    mut config: ResMut<DebugVisualizerConfig>,
    mut queue: ResMut<DebugShapecastQueue>,
) {
    config.draw_hitboxes = state.show_hitboxes;
    config.draw_shapecasts = state.show_shapecasts;
    if queue.is_recording() != state.show_shapecasts {
        queue.set_recording(state.show_shapecasts);
    }
}

/// Draws enabled overlays.
pub fn draw_debug_gizmos(
    mut gizmos: Gizmos,
    config: Res<DebugVisualizerConfig>,
    queue: Res<DebugShapecastQueue>,
    dummies: Query<&GlobalTransform, With<CombatDummy>>,
) {
    if config.draw_shapecasts {
        for cast in queue.casts() {
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
    fn config_and_recording_follow_console_toggles() {
        let mut app = App::new();
        app.init_resource::<DevToolsState>()
            .init_resource::<DebugVisualizerConfig>()
            .init_resource::<DebugShapecastQueue>()
            .add_systems(Update, sync_visualizer_config);

        {
            let mut state = app.world_mut().resource_mut::<DevToolsState>();
            state.show_hitboxes = true;
            state.show_shapecasts = true;
        }
        app.update();
        let config = app.world().resource::<DebugVisualizerConfig>();
        assert!(config.draw_hitboxes && config.draw_shapecasts);
        assert!(app.world().resource::<DebugShapecastQueue>().is_recording());

        app.world_mut()
            .resource_mut::<DevToolsState>()
            .show_shapecasts = false;
        app.update();
        let config = app.world().resource::<DebugVisualizerConfig>();
        assert!(config.draw_hitboxes && !config.draw_shapecasts);
        assert!(!app.world().resource::<DebugShapecastQueue>().is_recording());
    }
}
