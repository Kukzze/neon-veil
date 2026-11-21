//! Parkour finite state machine states and classification predicates.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Locomotion state of a parkour character, evaluated every fixed tick.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default, Debug, PartialEq, Hash)]
pub enum ParkourState {
    #[default]
    Idle,
    Walking,
    Sprinting,
    Crouching,
    Sliding,
    JumpAscent,
    JumpApex,
    JumpDescent,
    WallRunLeft,
    WallRunRight,
    WallClimb,
    WallKick,
    LedgeGrab,
    LedgeMantle,
    VaultLow,
    VaultMid,
    VaultHigh,
    ZiplineTraverse,
    GrapplePull,
    HardLanding,
    LandingRoll,
    Ragdoll,
}

impl ParkourState {
    /// Every state, in declaration order.
    pub const ALL: [ParkourState; 22] = [
        Self::Idle,
        Self::Walking,
        Self::Sprinting,
        Self::Crouching,
        Self::Sliding,
        Self::JumpAscent,
        Self::JumpApex,
        Self::JumpDescent,
        Self::WallRunLeft,
        Self::WallRunRight,
        Self::WallClimb,
        Self::WallKick,
        Self::LedgeGrab,
        Self::LedgeMantle,
        Self::VaultLow,
        Self::VaultMid,
        Self::VaultHigh,
        Self::ZiplineTraverse,
        Self::GrapplePull,
        Self::HardLanding,
        Self::LandingRoll,
        Self::Ragdoll,
    ];

    /// Standing on walkable ground (refreshes coyote time).
    #[inline]
    pub const fn is_grounded(&self) -> bool {
        matches!(
            self,
            Self::Idle
                | Self::Walking
                | Self::Sprinting
                | Self::Crouching
                | Self::Sliding
                | Self::HardLanding
                | Self::LandingRoll
        )
    }

    /// In free ballistic flight from a jump or fall.
    #[inline]
    pub const fn is_airborne(&self) -> bool {
        matches!(self, Self::JumpAscent | Self::JumpApex | Self::JumpDescent)
    }

    /// Attached to, or pushing off, a wall.
    #[inline]
    pub const fn is_wall_interacting(&self) -> bool {
        matches!(
            self,
            Self::WallRunLeft | Self::WallRunRight | Self::WallClimb | Self::WallKick
        )
    }

    /// Performing a scripted vault over an obstacle.
    #[inline]
    pub const fn is_vaulting(&self) -> bool {
        matches!(self, Self::VaultLow | Self::VaultMid | Self::VaultHigh)
    }

    /// Whether this state may transition into a slide. Only a grounded sprint
    /// qualifies; the caller must also check that the character is moving at
    /// slide speed (see `kinematics::can_begin_slide`).
    #[inline]
    pub const fn can_slide(&self) -> bool {
        matches!(self, Self::Sprinting)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_lists_every_variant_once() {
        let unique: HashSet<_> = ParkourState::ALL.iter().collect();
        assert_eq!(unique.len(), ParkourState::ALL.len());
        assert_eq!(ParkourState::default(), ParkourState::Idle);
    }

    #[test]
    fn classification_for_every_variant() {
        use ParkourState::*;
        for state in ParkourState::ALL {
            let grounded = matches!(
                state,
                Idle | Walking | Sprinting | Crouching | Sliding | HardLanding | LandingRoll
            );
            let airborne = matches!(state, JumpAscent | JumpApex | JumpDescent);
            let wall = matches!(state, WallRunLeft | WallRunRight | WallClimb | WallKick);
            let vault = matches!(state, VaultLow | VaultMid | VaultHigh);

            assert_eq!(state.is_grounded(), grounded, "{state:?} grounded");
            assert_eq!(state.is_airborne(), airborne, "{state:?} airborne");
            assert_eq!(state.is_wall_interacting(), wall, "{state:?} wall");
            assert_eq!(state.is_vaulting(), vault, "{state:?} vault");
            assert_eq!(state.can_slide(), state == Sprinting, "{state:?} slide");
        }
    }

    #[test]
    fn categories_are_mutually_exclusive() {
        for state in ParkourState::ALL {
            let count = [
                state.is_grounded(),
                state.is_airborne(),
                state.is_wall_interacting(),
                state.is_vaulting(),
            ]
            .into_iter()
            .filter(|&b| b)
            .count();
            assert!(count <= 1, "{state:?} is in {count} categories");
        }
    }

    #[test]
    fn serde_roundtrip_every_variant() {
        for state in ParkourState::ALL {
            let json = serde_json::to_string(&state).expect("encode");
            let decoded: ParkourState = serde_json::from_str(&json).expect("decode");
            assert_eq!(decoded, state);
        }
    }

    #[test]
    fn reflection_round_trips_through_registry() {
        use bevy::reflect::{FromReflect, Reflect, TypeRegistry};
        let mut registry = TypeRegistry::default();
        registry.register::<ParkourState>();
        let registration = registry
            .get(std::any::TypeId::of::<ParkourState>())
            .expect("registered");
        assert!(registration.data::<ReflectComponent>().is_some());

        let reflected: Box<dyn Reflect> = Box::new(ParkourState::WallRunLeft);
        let restored =
            ParkourState::from_reflect(reflected.as_partial_reflect()).expect("from_reflect");
        assert_eq!(restored, ParkourState::WallRunLeft);
    }
}
