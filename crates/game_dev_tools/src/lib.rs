//! Developer greybox tooling, runtime profiler and debug visualizers.

pub mod physics_editor;
pub mod visualizers;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use physics_editor::{
    apply_tuning_to_parkour, parse_and_execute_console_command, render_dev_gui,
    seed_tuning_from_parkour, toggle_dev_console, ConsoleCommandEvent, ConsoleSubmitEvent,
    DevToolsState, LivePhysicsTuning,
};
use visualizers::{
    draw_debug_gizmos, sync_visualizer_config, DebugShapecastQueue, DebugVisualizerConfig,
};

/// Developer console, live physics tuning and gizmo overlays.
///
/// Requires a windowed app (`DefaultPlugins`); adds `EguiPlugin` and
/// `FrameTimeDiagnosticsPlugin` when they are not already present. When
/// `ParkourPhysicsConstants` exists, the tuning sliders mirror and edit it.
pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EguiPlugin>() {
            app.add_plugins(EguiPlugin);
        }
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin);
        }

        app.register_type::<LivePhysicsTuning>()
            .init_resource::<LivePhysicsTuning>()
            .init_resource::<DevToolsState>()
            .init_resource::<DebugVisualizerConfig>()
            .init_resource::<DebugShapecastQueue>()
            .add_event::<ConsoleSubmitEvent>()
            .add_event::<ConsoleCommandEvent>()
            .add_systems(Startup, seed_tuning_from_parkour)
            .add_systems(
                Update,
                (
                    toggle_dev_console.run_if(resource_exists::<ButtonInput<KeyCode>>),
                    render_dev_gui,
                    apply_tuning_to_parkour.run_if(resource_changed::<LivePhysicsTuning>),
                    parse_and_execute_console_command,
                    sync_visualizer_config.run_if(resource_changed::<DevToolsState>),
                )
                    .chain(),
            )
            .add_systems(PostUpdate, draw_debug_gizmos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_combat::combat_dummy::{CombatDummy, CombatDummyType};
    use physics_editor::ConsoleSubmitEvent;

    /// Builds the full plugin headlessly and runs frames, so every system's
    /// parameters are validated against the resources the plugin provides.
    #[test]
    fn plugin_runs_headless_with_overlays_enabled() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            bevy::input::InputPlugin,
            bevy::window::WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                close_when_requested: false,
            },
        ));
        // Provided by `RenderPlugin` in a real client; GizmoPlugin loads its
        // line shaders into this asset collection.
        app.init_asset::<bevy::render::render_resource::Shader>()
            .init_asset::<Image>();
        app.add_plugins((bevy::gizmos::GizmoPlugin, DevToolsPlugin));
        app.world_mut().spawn((
            CombatDummy::new(CombatDummyType::Static, Vec3::ZERO),
            Transform::default(),
            GlobalTransform::default(),
        ));
        {
            let mut state = app.world_mut().resource_mut::<DevToolsState>();
            state.show_hitboxes = true;
            state.show_shapecasts = true;
            state.show_profiler = true;
        }
        app.world_mut().resource_mut::<DebugShapecastQueue>().push(
            Vec3::ZERO,
            Vec3::NEG_Y,
            2.0,
            0.3,
            Some(1.0),
        );
        app.world_mut()
            .send_event(ConsoleSubmitEvent("/godmode".to_owned()));

        for _ in 0..3 {
            app.update();
        }

        // Drawing does not consume casts; CoreGameplayPlugin clears them per
        // fixed tick (not added here), so the cast stays visible.
        let queue = app.world().resource::<DebugShapecastQueue>();
        assert_eq!(queue.len(), 1);
        assert!(queue.is_recording());
        assert!(
            app.world()
                .resource::<DebugVisualizerConfig>()
                .draw_hitboxes
        );
        assert!(app.world().resource::<DevToolsState>().god_mode);
    }
}
