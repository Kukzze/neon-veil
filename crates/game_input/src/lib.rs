//! Unified keyboard, mouse and gamepad input abstraction.
//!
//! Pipeline per frame / tick:
//! 1. `InputManagerPlugin` updates `ActionState<PlayerAction>` (PreUpdate).
//! 2. [`latch_action_presses`] and [`apply_look_input`] record taps and turn
//!    look input into `ViewAngles` (PreUpdate, after leafwing).
//! 3. [`build_input_packets`] packs a `PlayerInputPacket` into
//!    `PlayerInputState` every fixed tick (FixedPreUpdate), ready for the
//!    simulation or the network.

pub mod actions;
pub mod aim_assist;
pub mod bindings_io;
pub mod packet_builder;

pub use actions::{
    default_gamepad_input_map, default_input_map, default_kbm_input_map, PlayerAction,
};
pub use aim_assist::{AimAssistSettings, AimAssistTarget};
pub use packet_builder::{
    apply_look_input, build_input_packets, latch_action_presses, ActionPressLatch,
    GameplayInputGate, LocalPlayerInput, LookSettings,
};

use bevy::prelude::*;
use game_core::CoreGameplayPlugin;
use leafwing_input_manager::plugin::InputManagerSystem;
use leafwing_input_manager::prelude::*;

/// Components for a locally controlled player using `map`.
pub fn local_player_input_bundle(map: InputMap<PlayerAction>) -> impl Bundle {
    (LocalPlayerInput, InputManagerBundle::with_map(map))
}

/// Registers leafwing for [`PlayerAction`], look and aim-assist settings and
/// the look / packet systems. Adds [`CoreGameplayPlugin`] if missing (packets
/// are stamped with its `CurrentNetworkTick`).
pub struct GameInputPlugin;

impl Plugin for GameInputPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<InputManagerPlugin<PlayerAction>>() {
            app.add_plugins(InputManagerPlugin::<PlayerAction>::default());
        }
        if !app.is_plugin_added::<CoreGameplayPlugin>() {
            app.add_plugins(CoreGameplayPlugin);
        }
        app.init_resource::<AimAssistSettings>()
            .init_resource::<LookSettings>()
            .init_resource::<GameplayInputGate>()
            .register_type::<GameplayInputGate>()
            .register_type::<AimAssistSettings>()
            .register_type::<LookSettings>()
            .register_type::<AimAssistTarget>()
            .register_type::<LocalPlayerInput>()
            .register_type::<ActionPressLatch>()
            .add_systems(
                PreUpdate,
                (latch_action_presses, apply_look_input).after(InputManagerSystem::ManualControl),
            )
            .add_systems(FixedPreUpdate, build_input_packets);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;
    use game_core::input::{PlayerInputState, ViewAngles};
    use game_core::protocol::InputActions;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin, GameInputPlugin));
        app
    }

    fn spawn_player(app: &mut App) -> Entity {
        app.world_mut()
            .spawn(local_player_input_bundle(default_input_map()))
            .id()
    }

    fn action_state(app: &App, e: Entity) -> &ActionState<PlayerAction> {
        app.world()
            .get::<ActionState<PlayerAction>>(e)
            .expect("action state")
    }

    #[test]
    fn keyboard_presses_trigger_actions() {
        let mut app = app();
        let player = spawn_player(&mut app);
        app.update();

        KeyCode::Space.press(app.world_mut());
        KeyCode::ShiftLeft.press(app.world_mut());
        KeyCode::KeyW.press(app.world_mut());
        KeyCode::KeyC.press(app.world_mut());
        app.update();

        let s = action_state(&app, player);
        assert!(s.just_pressed(&PlayerAction::Jump));
        assert!(s.pressed(&PlayerAction::Sprint));
        assert!(s.pressed(&PlayerAction::Slide) && s.pressed(&PlayerAction::Crouch));
        assert_eq!(s.axis_pair(&PlayerAction::Move), Vec2::new(0.0, 1.0));
        assert!(!s.pressed(&PlayerAction::FireWeapon));
    }

    #[test]
    fn mouse_and_toggle_bindings_trigger_actions() {
        let mut app = app();
        let player = spawn_player(&mut app);
        app.update();

        MouseButton::Left.press(app.world_mut());
        KeyCode::KeyV.press(app.world_mut());
        app.update();
        let s = action_state(&app, player);
        assert!(s.pressed(&PlayerAction::FireWeapon) && s.pressed(&PlayerAction::MeleeLight));
        assert!(s.just_pressed(&PlayerAction::ToggleCameraPerspective));
    }

    #[test]
    fn mouse_motion_turns_the_view() {
        let mut app = app();
        let player = spawn_player(&mut app);
        app.update();
        MouseMove::default().set_axis_pair(app.world_mut(), Vec2::new(50.0, 0.0));
        app.update();
        let view = app.world().get::<ViewAngles>(player).expect("view");
        assert!(
            view.yaw < 0.0,
            "moving the mouse right turns right: {}",
            view.yaw
        );
    }

    #[test]
    fn local_player_bundle_requires_input_components() {
        let mut app = app();
        let player = spawn_player(&mut app);
        let e = app.world().entity(player);
        assert!(e.contains::<ViewAngles>());
        assert!(e.contains::<PlayerInputState>());
        assert!(e.contains::<ActionPressLatch>());
        assert!(e.contains::<InputMap<PlayerAction>>());
    }

    #[test]
    fn fixed_tick_pushes_packets_with_held_actions() {
        let mut app = app();
        let player = spawn_player(&mut app);
        KeyCode::Space.press(app.world_mut());
        app.update();
        app.world_mut().run_schedule(FixedPreUpdate);

        let mut state = app
            .world()
            .get::<PlayerInputState>(player)
            .expect("input state")
            .clone();
        let (packet, _) = state.take_fresh().expect("packet");
        assert!(packet.has_action(InputActions::JUMP));
    }
}
