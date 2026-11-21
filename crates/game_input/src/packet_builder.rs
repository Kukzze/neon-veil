//! Turns device input into look angles and network input packets.
//!
//! Every fixed tick the local player's `ActionState` and [`ViewAngles`] are
//! packed into a [`PlayerInputPacket`] and pushed into its
//! [`PlayerInputState`]. The simulation consumes that component the same way
//! for local and remote players.

use crate::actions::PlayerAction;
use crate::aim_assist::{apply_magnetism, evaluate_aim_assist, AimAssistSettings, AimAssistTarget};
use bevy::prelude::*;
use game_core::input::{PlayerInputState, ViewAngles};
use game_core::math::sanitize_f32;
use game_core::protocol::{InputActions, NetworkTick, PlayerInputPacket};
use game_core::CurrentNetworkTick;
use leafwing_input_manager::prelude::ActionState;

/// Eye height above the character's capsule center, used as the aim-assist
/// ray origin (eye at 1.68 m over a 0.9 m center).
pub const AIM_EYE_OFFSET: Vec3 = Vec3::new(0.0, 0.78, 0.0);

/// Marks the locally controlled player and pulls in the input components.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[reflect(Component, Default, Debug)]
#[require(ViewAngles, PlayerInputState, ActionPressLatch)]
pub struct LocalPlayerInput;

/// Look sensitivity.
#[derive(Resource, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource, Default, Debug)]
pub struct LookSettings {
    /// Radians per pixel of mouse motion.
    pub mouse_sensitivity: f32,
    /// Radians per second at full stick deflection.
    pub stick_sensitivity: f32,
    pub invert_y: bool,
}

impl Default for LookSettings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 0.0022,
            stick_sensitivity: 3.2,
            invert_y: false,
        }
    }
}

/// Packet action bits pressed since the last packet was built. Keeps a tap
/// shorter than one fixed tick from being lost between ticks.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[reflect(Component, Default, Debug)]
pub struct ActionPressLatch {
    pub bits: u32,
}

/// Maps each packet bit to the actions that set it.
const ACTION_BITS: [(u32, &[PlayerAction]); 7] = [
    (
        InputActions::JUMP,
        &[PlayerAction::Jump, PlayerAction::Mantle],
    ),
    (
        InputActions::SLIDE,
        &[PlayerAction::Slide, PlayerAction::Crouch],
    ),
    // Holding jump signals wall-run intent; the controller attaches
    // automatically, so this bit is advisory.
    (InputActions::WALL_RUN, &[PlayerAction::Jump]),
    (InputActions::SPRINT, &[PlayerAction::Sprint]),
    (
        InputActions::FIRE,
        &[PlayerAction::FireWeapon, PlayerAction::MeleeLight],
    ),
    (InputActions::PARRY, &[PlayerAction::AimBlock]),
    (InputActions::MELEE_HEAVY, &[PlayerAction::MeleeHeavy]),
];

fn bits_where(test: impl Fn(&PlayerAction) -> bool) -> u32 {
    ACTION_BITS
        .iter()
        .filter(|(_, actions)| actions.iter().any(&test))
        .fold(0, |bits, (bit, _)| bits | bit)
}

/// Packet bits for actions held right now.
pub fn held_action_bits(state: &ActionState<PlayerAction>) -> u32 {
    bits_where(|a| state.pressed(a))
}

/// Packet bits for actions pressed this frame.
pub fn just_pressed_action_bits(state: &ActionState<PlayerAction>) -> u32 {
    bits_where(|a| state.just_pressed(a))
}

/// Builds a sanitized packet from the current action state.
pub fn build_input_packet(
    state: &ActionState<PlayerAction>,
    view: ViewAngles,
    tick: NetworkTick,
    latched_bits: u32,
) -> PlayerInputPacket {
    let movement = state.axis_pair(&PlayerAction::Move);
    PlayerInputPacket {
        tick,
        movement_vector: Vec2::new(sanitize_f32(movement.x), sanitize_f32(movement.y))
            .clamp_length_max(1.0),
        pitch: view.pitch,
        yaw: view.yaw,
        actions: held_action_bits(state) | latched_bits,
    }
    .sanitized()
}

/// Records presses each frame so short taps reach the next packet.
pub fn latch_action_presses(
    mut players: Query<(&ActionState<PlayerAction>, &mut ActionPressLatch), With<LocalPlayerInput>>,
) {
    for (state, mut latch) in &mut players {
        let pressed = just_pressed_action_bits(state);
        if pressed != 0 {
            latch.bits |= pressed;
        }
    }
}

/// Applies mouse and stick look to [`ViewAngles`], with aim assist on the
/// stick only.
pub fn apply_look_input(
    time: Res<Time>,
    look: Res<LookSettings>,
    assist: Res<AimAssistSettings>,
    targets: Query<(&GlobalTransform, &AimAssistTarget)>,
    mut players: Query<
        (
            &ActionState<PlayerAction>,
            &mut ViewAngles,
            Option<&GlobalTransform>,
        ),
        With<LocalPlayerInput>,
    >,
) {
    let dt = time.delta_secs();
    for (state, mut view, transform) in &mut players {
        let mouse = state.axis_pair(&PlayerAction::Look);
        let stick = state.clamped_axis_pair(&PlayerAction::LookRate);
        let moving = state.axis_pair(&PlayerAction::Move) != Vec2::ZERO;
        let y_sign = if look.invert_y { -1.0 } else { 1.0 };

        let result = match transform {
            Some(transform) if stick != Vec2::ZERO || moving => evaluate_aim_assist(
                transform.translation() + AIM_EYE_OFFSET,
                view.forward(),
                targets
                    .iter()
                    .map(|(t, target)| t.translation() + target.offset),
                &assist,
            ),
            _ => crate::aim_assist::AimAssistResult::NONE,
        };

        let stick_rate = look.stick_sensitivity * dt * result.sensitivity_scale;
        let delta_yaw = -mouse.x * look.mouse_sensitivity - stick.x * stick_rate;
        let delta_pitch = y_sign * (-mouse.y * look.mouse_sensitivity + stick.y * stick_rate);
        if delta_yaw != 0.0 || delta_pitch != 0.0 {
            view.rotate(delta_yaw, delta_pitch);
        }
        if let Some(target) = result.magnetism_target {
            apply_magnetism(&mut view, target, assist.magnetism_strength, dt);
        }
    }
}

/// Builds this tick's packet for every local player. Runs in
/// `FixedPreUpdate`, before the simulation reads `PlayerInputState`.
pub fn build_input_packets(
    tick: Res<CurrentNetworkTick>,
    mut players: Query<
        (
            &ActionState<PlayerAction>,
            &ViewAngles,
            &mut ActionPressLatch,
            &mut PlayerInputState,
        ),
        With<LocalPlayerInput>,
    >,
) {
    for (state, view, mut latch, mut input) in &mut players {
        let packet = build_input_packet(state, *view, tick.0, latch.bits);
        latch.bits = 0;
        input.push(packet);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ActionState<PlayerAction> {
        ActionState::default()
    }

    #[test]
    fn packet_maps_movement_view_and_actions() {
        let mut s = state();
        s.set_axis_pair(&PlayerAction::Move, Vec2::new(0.0, 1.0));
        s.press(&PlayerAction::Jump);
        s.press(&PlayerAction::Sprint);
        s.press(&PlayerAction::AimBlock);
        let view = ViewAngles::new(0.5, -0.2);
        let p = build_input_packet(&s, view, NetworkTick(42), 0);

        assert_eq!(p.tick, NetworkTick(42));
        assert_eq!(p.movement_vector, Vec2::new(0.0, 1.0));
        assert!((p.yaw - 0.5).abs() < 1e-6 && (p.pitch - -0.2).abs() < 1e-6);
        for bit in [
            InputActions::JUMP,
            InputActions::WALL_RUN,
            InputActions::SPRINT,
            InputActions::PARRY,
        ] {
            assert!(p.has_action(bit), "missing bit {bit:#b}");
        }
        assert!(!p.has_action(InputActions::SLIDE));
        assert!(!p.has_action(InputActions::FIRE));
    }

    #[test]
    fn diagonal_movement_is_clamped_to_unit_length() {
        let mut s = state();
        s.set_axis_pair(&PlayerAction::Move, Vec2::new(1.0, 1.0));
        let p = build_input_packet(&s, ViewAngles::default(), NetworkTick(0), 0);
        assert!((p.movement_vector.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn crouch_slide_and_fire_aliases() {
        let mut s = state();
        s.press(&PlayerAction::Crouch);
        s.press(&PlayerAction::MeleeLight);
        let p = build_input_packet(&s, ViewAngles::default(), NetworkTick(0), 0);
        assert!(p.has_action(InputActions::SLIDE) && p.has_action(InputActions::FIRE));
    }

    #[test]
    fn latched_tap_survives_release_before_tick() {
        let s = state();
        let p = build_input_packet(
            &s,
            ViewAngles::default(),
            NetworkTick(0),
            InputActions::JUMP,
        );
        assert!(p.has_action(InputActions::JUMP));
    }

    #[test]
    fn idle_state_produces_empty_packet() {
        let p = build_input_packet(&state(), ViewAngles::default(), NetworkTick(3), 0);
        assert_eq!(p.actions, 0);
        assert_eq!(p.movement_vector, Vec2::ZERO);
    }

    fn input_app() -> App {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<LookSettings>()
            .init_resource::<AimAssistSettings>()
            .init_resource::<CurrentNetworkTick>()
            .add_systems(Update, (latch_action_presses, apply_look_input))
            .add_systems(PostUpdate, build_input_packets);
        app
    }

    #[test]
    fn systems_latch_look_and_push_packets() {
        let mut app = input_app();
        let mut s = state();
        s.press(&PlayerAction::Jump);
        s.set_axis_pair(&PlayerAction::Look, Vec2::new(100.0, 50.0));
        let player = app.world_mut().spawn((LocalPlayerInput, s)).id();
        app.update();

        let world = app.world();
        let view = world.get::<ViewAngles>(player).expect("view");
        assert!(
            (view.yaw - -0.22).abs() < 1e-5,
            "mouse right turns right: {}",
            view.yaw
        );
        assert!(
            (view.pitch - -0.11).abs() < 1e-5,
            "mouse down looks down: {}",
            view.pitch
        );

        let mut input = world
            .get::<PlayerInputState>(player)
            .expect("input")
            .clone();
        let (packet, previous) = input.take_fresh().expect("packet pushed");
        assert!(packet.has_action(InputActions::JUMP));
        assert!(previous.is_none());
        assert_eq!(
            world.get::<ActionPressLatch>(player).map(|l| l.bits),
            Some(0)
        );
    }

    #[test]
    fn invert_y_flips_pitch() {
        let mut app = input_app();
        app.insert_resource(LookSettings {
            invert_y: true,
            ..Default::default()
        });
        let mut s = state();
        s.set_axis_pair(&PlayerAction::Look, Vec2::new(0.0, 50.0));
        let player = app.world_mut().spawn((LocalPlayerInput, s)).id();
        app.update();
        let pitch = app.world().get::<ViewAngles>(player).expect("view").pitch;
        assert!(pitch > 0.0);
    }
}
