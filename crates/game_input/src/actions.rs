//! Player actions and default keyboard/mouse and gamepad bindings.

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use serde::{Deserialize, Serialize};

/// Everything a player can do, independent of the physical device.
#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect, Serialize, Deserialize)]
pub enum PlayerAction {
    // Locomotion
    /// Planar movement intent: `x` = strafe right, `y` = forward.
    #[actionlike(DualAxis)]
    Move,
    /// Per-frame look delta in pixels (mouse).
    #[actionlike(DualAxis)]
    Look,
    /// Look rate in `[-1, 1]` (stick); scaled by frame time, unlike `Look`.
    #[actionlike(DualAxis)]
    LookRate,
    Sprint,
    Crouch,
    Slide,
    Jump,
    Mantle,
    DodgeRoll,
    // Combat
    MeleeLight,
    MeleeHeavy,
    AimBlock,
    FireWeapon,
    Reload,
    SwapWeapon,
    QuickCyberAbility1,
    QuickCyberAbility2,
    QuickCyberAbility3,
    // UI / meta
    Interact,
    ToggleCameraPerspective,
    OpenInventory,
    OpenSkills,
    OpenMap,
    OpenDevMenu,
}

impl PlayerAction {
    /// Every action, in declaration order.
    pub const ALL: [PlayerAction; 24] = [
        Self::Move,
        Self::Look,
        Self::LookRate,
        Self::Sprint,
        Self::Crouch,
        Self::Slide,
        Self::Jump,
        Self::Mantle,
        Self::DodgeRoll,
        Self::MeleeLight,
        Self::MeleeHeavy,
        Self::AimBlock,
        Self::FireWeapon,
        Self::Reload,
        Self::SwapWeapon,
        Self::QuickCyberAbility1,
        Self::QuickCyberAbility2,
        Self::QuickCyberAbility3,
        Self::Interact,
        Self::ToggleCameraPerspective,
        Self::OpenInventory,
        Self::OpenSkills,
        Self::OpenMap,
        Self::OpenDevMenu,
    ];
}

/// Default keyboard and mouse bindings.
pub fn default_kbm_input_map() -> InputMap<PlayerAction> {
    use PlayerAction::*;
    let mut map = InputMap::default();
    map.insert_dual_axis(Move, VirtualDPad::wasd())
        .insert_dual_axis(Look, MouseMove::default())
        .insert(Sprint, KeyCode::ShiftLeft)
        .insert(Jump, KeyCode::Space)
        .insert(Mantle, KeyCode::Space)
        .insert(Crouch, KeyCode::KeyC)
        .insert(Crouch, ModifierKey::Control)
        .insert(Slide, KeyCode::KeyC)
        .insert(Slide, ModifierKey::Control)
        .insert(DodgeRoll, KeyCode::AltLeft)
        .insert(MeleeLight, MouseButton::Left)
        .insert(FireWeapon, MouseButton::Left)
        .insert(MeleeHeavy, MouseButton::Middle)
        .insert(AimBlock, MouseButton::Right)
        .insert(Reload, KeyCode::KeyR)
        .insert(SwapWeapon, KeyCode::KeyQ)
        .insert(QuickCyberAbility1, KeyCode::Digit1)
        .insert(QuickCyberAbility2, KeyCode::Digit2)
        .insert(QuickCyberAbility3, KeyCode::Digit3)
        .insert(Interact, KeyCode::KeyF)
        .insert(ToggleCameraPerspective, KeyCode::KeyV)
        .insert(OpenInventory, KeyCode::Tab)
        .insert(OpenSkills, KeyCode::KeyK)
        .insert(OpenMap, KeyCode::KeyM)
        .insert(OpenDevMenu, KeyCode::F1);
    map
}

/// Default gamepad bindings.
pub fn default_gamepad_input_map() -> InputMap<PlayerAction> {
    use PlayerAction::*;
    let mut map = InputMap::default();
    map.insert_dual_axis(Move, GamepadStick::LEFT)
        .insert_dual_axis(LookRate, GamepadStick::RIGHT)
        .insert(Jump, GamepadButton::South)
        .insert(Mantle, GamepadButton::South)
        .insert(Crouch, GamepadButton::East)
        .insert(Slide, GamepadButton::East)
        .insert(Interact, GamepadButton::West)
        .insert(SwapWeapon, GamepadButton::North)
        .insert(Sprint, GamepadButton::LeftThumb)
        .insert(DodgeRoll, GamepadButton::RightThumb)
        .insert(ToggleCameraPerspective, GamepadButton::DPadDown)
        .insert(FireWeapon, GamepadButton::RightTrigger2)
        .insert(MeleeLight, GamepadButton::RightTrigger2)
        .insert(MeleeHeavy, GamepadButton::RightTrigger)
        .insert(AimBlock, GamepadButton::LeftTrigger2)
        .insert(QuickCyberAbility1, GamepadButton::LeftTrigger)
        .insert(QuickCyberAbility2, GamepadButton::DPadUp)
        .insert(QuickCyberAbility3, GamepadButton::DPadRight)
        .insert(Reload, GamepadButton::DPadLeft)
        .insert(OpenInventory, GamepadButton::Select)
        .insert(OpenMap, GamepadButton::Start);
    map
}

/// Keyboard/mouse and gamepad bindings together, so either device works.
pub fn default_input_map() -> InputMap<PlayerAction> {
    let mut map = default_kbm_input_map();
    map.merge(&default_gamepad_input_map());
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_lists_every_action_once() {
        let unique: HashSet<_> = PlayerAction::ALL.iter().collect();
        assert_eq!(unique.len(), PlayerAction::ALL.len());
    }

    #[test]
    fn axis_actions_are_dual_axis() {
        use leafwing_input_manager::InputControlKind;
        for action in [
            PlayerAction::Move,
            PlayerAction::Look,
            PlayerAction::LookRate,
        ] {
            assert_eq!(action.input_control_kind(), InputControlKind::DualAxis);
        }
        assert_eq!(
            PlayerAction::Jump.input_control_kind(),
            InputControlKind::Button
        );
    }

    #[test]
    fn every_action_has_a_keyboard_or_mouse_binding() {
        let map = default_kbm_input_map();
        for action in PlayerAction::ALL {
            if action == PlayerAction::LookRate {
                continue; // stick-only by design
            }
            let bound = map.get_buttonlike(&action).is_some_and(|b| !b.is_empty())
                || map
                    .get_dual_axislike(&action)
                    .is_some_and(|b| !b.is_empty());
            assert!(bound, "{action:?} has no KBM binding");
        }
    }

    #[test]
    fn gamepad_covers_core_actions() {
        let map = default_gamepad_input_map();
        for action in [
            PlayerAction::Jump,
            PlayerAction::Slide,
            PlayerAction::Crouch,
            PlayerAction::Sprint,
            PlayerAction::Interact,
            PlayerAction::SwapWeapon,
            PlayerAction::ToggleCameraPerspective,
            PlayerAction::FireWeapon,
            PlayerAction::AimBlock,
        ] {
            assert!(map.get_buttonlike(&action).is_some(), "{action:?}");
        }
        assert!(map.get_dual_axislike(&PlayerAction::Move).is_some());
        assert!(map.get_dual_axislike(&PlayerAction::LookRate).is_some());
    }

    #[test]
    fn merged_map_keeps_both_devices() {
        let map = default_input_map();
        assert_eq!(
            map.get_buttonlike(&PlayerAction::Jump).map(Vec::len),
            Some(2)
        );
        assert_eq!(
            map.get_dual_axislike(&PlayerAction::Move).map(Vec::len),
            Some(2)
        );
    }
}
