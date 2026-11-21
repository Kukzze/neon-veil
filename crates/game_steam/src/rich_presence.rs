//! Rich presence: total level, district and what the player is doing.

use crate::PlatformServiceResource;
use bevy::prelude::*;
use game_core::identifiers::PlayerCharacterMarker;
use game_parkour::ParkourState;
use game_rpg::CharacterSkills;
use game_world::district_manifest::get_district_at_position;
use std::fmt::Write;

/// Human wording for a traversal state.
pub fn traversal_label(state: ParkourState) -> &'static str {
    use ParkourState::*;
    match state {
        Idle => "Idle",
        Walking => "Walking",
        Sprinting => "Sprinting",
        Crouching => "Sneaking",
        Sliding => "Sliding",
        JumpAscent | JumpApex | JumpDescent => "Airborne",
        WallRunLeft | WallRunRight => "Wall-Running",
        WallClimb => "Wall-Climbing",
        WallKick => "Wall-Kicking",
        LedgeGrab | LedgeMantle => "Mantling",
        VaultLow | VaultMid | VaultHigh => "Vaulting",
        ZiplineTraverse => "Ziplining",
        GrapplePull => "Grappling",
        HardLanding | LandingRoll => "Landing",
        Ragdoll => "Down",
    }
}

/// `Level 152 // Kuroshio Commercial Hub // Wall-Running`.
pub fn write_presence(
    out: &mut String,
    total_level: u16,
    district: Option<&str>,
    state: Option<ParkourState>,
) {
    out.clear();
    let _ = write!(
        out,
        "Level {total_level} // {}",
        district.unwrap_or("Neo-Veridia")
    );
    if let Some(state) = state {
        let _ = write!(out, " // {}", traversal_label(state));
    }
}

/// Pushes rich presence when its text changes.
pub fn update_steam_rich_presence(
    platform: Option<Res<PlatformServiceResource>>,
    players: Query<
        (&CharacterSkills, &Transform, Option<&ParkourState>),
        With<PlayerCharacterMarker>,
    >,
    mut last: Local<String>,
    mut scratch: Local<String>,
) {
    let Some(platform) = platform else {
        return;
    };
    let Some((skills, transform, state)) = players.iter().next() else {
        return;
    };
    let district = get_district_at_position(transform.translation).map(|d| d.name);
    write_presence(
        &mut scratch,
        skills.get_total_level(),
        district,
        state.copied(),
    );
    if *scratch != *last {
        platform.0.update_rich_presence(&scratch);
        std::mem::swap(&mut *last, &mut *scratch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_text() {
        let mut out = String::new();
        write_presence(
            &mut out,
            152,
            Some("Kuroshio Commercial Hub (Sector 2)"),
            Some(ParkourState::WallRunLeft),
        );
        assert_eq!(
            out,
            "Level 152 // Kuroshio Commercial Hub (Sector 2) // Wall-Running"
        );
        write_presence(&mut out, 8, None, None);
        assert_eq!(out, "Level 8 // Neo-Veridia");
    }

    #[test]
    fn every_state_has_a_label() {
        for state in ParkourState::ALL {
            assert!(!traversal_label(state).is_empty(), "{state:?}");
        }
    }
}
