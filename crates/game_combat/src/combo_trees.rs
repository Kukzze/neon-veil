//! Combo catalogs for the five melee weapon classes and parkour-chained
//! attacks.
//!
//! Frame data and damage are from the combat design spec. Hitbox offsets are
//! converted to Bevy convention (forward is -Z; the spec lists forward
//! reach as +Z) and measured from the attacker's feet. Extents are full box
//! sizes.

use crate::melee::{AttackFrameData, MeleeCombatState, MeleeWeaponClass};
use bevy::prelude::*;
use game_parkour::ParkourState;

/// Forward knockback in attacker space.
const FORWARD: Vec3 = Vec3::NEG_Z;

const fn attack(
    name: &'static str,
    frames: [u32; 4],
    damage: f32,
    posture: f32,
    knockback: f32,
    extents: Vec3,
    offset: Vec3,
) -> AttackFrameData {
    AttackFrameData {
        name,
        startup_frames: frames[0],
        active_frames: frames[1],
        recovery_frames: frames[2],
        cancel_frame: frames[3],
        base_damage: damage,
        posture_damage: posture,
        knockback_impulse: knockback,
        hitbox_extents: extents,
        hitbox_offset: offset,
        knockback_direction: FORWARD,
        area_radius: 0.0,
        attacker_dive_speed: 0.0,
    }
}

pub static CYBER_KATANA_COMBO_TREE: &[AttackFrameData] = &[
    attack(
        "Katana_Light_1",
        [7, 6, 14, 20],
        48.0,
        22.0,
        3.5,
        Vec3::new(0.8, 0.4, 1.4),
        Vec3::new(0.0, 1.0, -1.0),
    ),
    attack(
        "Katana_Light_2",
        [6, 7, 12, 18],
        56.0,
        26.0,
        4.2,
        Vec3::new(1.1, 0.3, 1.5),
        Vec3::new(0.0, 0.9, -1.1),
    ),
    attack(
        "Katana_Heavy_Finisher",
        [14, 8, 22, 32],
        135.0,
        65.0,
        12.0,
        Vec3::new(1.4, 0.6, 2.0),
        Vec3::new(0.0, 1.1, -1.4),
    ),
];

pub static MANTIS_DUAL_BLADES_COMBO_TREE: &[AttackFrameData] = &[
    attack(
        "Mantis_Flurry_1",
        [4, 5, 10, 14],
        32.0,
        16.0,
        1.8,
        Vec3::new(0.6, 0.3, 1.0),
        Vec3::new(0.2, 0.9, -0.8),
    ),
    attack(
        "Mantis_Flurry_2",
        [5, 6, 11, 15],
        36.0,
        18.0,
        2.1,
        Vec3::new(0.7, 0.3, 1.1),
        Vec3::new(-0.2, 0.9, -0.8),
    ),
    attack(
        "Mantis_Cross_Slash",
        [9, 6, 18, 24],
        88.0,
        42.0,
        7.5,
        Vec3::new(1.2, 0.5, 1.3),
        Vec3::new(0.0, 1.0, -1.0),
    ),
];

pub static SHOCK_BATON_COMBO_TREE: &[AttackFrameData] = &[
    attack(
        "Baton_Strike_1",
        [8, 7, 16, 22],
        42.0,
        38.0,
        4.8,
        Vec3::new(0.7, 0.4, 1.2),
        Vec3::new(0.0, 1.0, -0.9),
    ),
    attack(
        "Baton_Overcharge_Slam",
        [16, 9, 26, 36],
        110.0,
        85.0,
        14.0,
        Vec3::new(1.6, 0.8, 1.6),
        Vec3::new(0.0, 0.5, -1.2),
    ),
];

pub static KINETIC_GAUNTLET_COMBO_TREE: &[AttackFrameData] = &[
    attack(
        "Gauntlet_Jab",
        [6, 5, 12, 16],
        52.0,
        30.0,
        5.5,
        Vec3::new(0.5, 0.5, 0.8),
        Vec3::new(0.0, 1.1, -0.7),
    ),
    attack(
        "Gauntlet_Rocket_Punch",
        [18, 8, 28, 38],
        165.0,
        95.0,
        22.0,
        Vec3::new(1.2, 0.8, 1.8),
        Vec3::new(0.0, 1.1, -1.5),
    ),
];

/// The whip sweeps a 3.5 m radius: a 7 m box limited to that radius.
pub static MONOFILAMENT_WHIP_COMBO_TREE: &[AttackFrameData] = &[AttackFrameData {
    area_radius: 3.5,
    ..attack(
        "Whip_Sweep",
        [10, 9, 20, 28],
        64.0,
        20.0,
        3.0,
        Vec3::new(7.0, 0.3, 7.0),
        Vec3::new(0.0, 0.8, -1.5),
    )
}];

/// Heavy attack out of a slide: launches the target upward.
pub const SLIDE_UPPERCUT: AttackFrameData = AttackFrameData {
    knockback_direction: Vec3::new(0.0, 1.0, -0.3),
    ..attack(
        "Slide_Uppercut",
        [5, 7, 18, 24],
        70.0,
        35.0,
        11.0,
        Vec3::new(1.0, 1.6, 1.6),
        Vec3::new(0.0, 0.8, -0.9),
    )
};

/// Light attack off a wall run: 45 degree thrust into the ground below,
/// 4 m radius shockwave for 90 damage.
pub const WALL_RUN_PLUNGE: AttackFrameData = AttackFrameData {
    area_radius: 4.0,
    ..attack(
        "Wall_Run_Plunge",
        [8, 6, 20, 26],
        90.0,
        45.0,
        8.0,
        Vec3::new(8.0, 4.0, 8.0),
        Vec3::new(0.0, -1.0, -1.5),
    )
};

/// Heavy attack while falling: dives at 35 m/s and shatters posture.
pub const AERIAL_SLAM: AttackFrameData = AttackFrameData {
    attacker_dive_speed: 35.0,
    knockback_direction: Vec3::new(0.0, -0.2, -1.0),
    ..attack(
        "Aerial_Slam",
        [4, 10, 20, 26],
        95.0,
        100.0,
        6.0,
        Vec3::new(2.5, 2.0, 2.5),
        Vec3::new(0.0, 0.0, -0.5),
    )
};

/// Light or heavy attack input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum AttackRequest {
    Light,
    Heavy,
}

/// An attack chosen for a request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackSelection {
    pub attack: AttackFrameData,
    pub combo_index: u8,
    /// Part of the combo chain (light attacks) rather than a standalone move.
    pub continues_combo: bool,
}

pub fn combo_tree_for(class: MeleeWeaponClass) -> &'static [AttackFrameData] {
    match class {
        MeleeWeaponClass::CyberKatana => CYBER_KATANA_COMBO_TREE,
        MeleeWeaponClass::MantisDualBlades => MANTIS_DUAL_BLADES_COMBO_TREE,
        MeleeWeaponClass::ShockBaton => SHOCK_BATON_COMBO_TREE,
        MeleeWeaponClass::KineticHeavyGauntlet => KINETIC_GAUNTLET_COMBO_TREE,
        MeleeWeaponClass::MonofilamentWhip => MONOFILAMENT_WHIP_COMBO_TREE,
    }
}

/// Parkour state that turns `request` into a special attack, if any.
pub fn parkour_attack_for(
    state: Option<ParkourState>,
    request: AttackRequest,
) -> Option<AttackFrameData> {
    match (state?, request) {
        (ParkourState::Sliding, AttackRequest::Heavy) => Some(SLIDE_UPPERCUT),
        (ParkourState::WallRunLeft | ParkourState::WallRunRight, AttackRequest::Light) => {
            Some(WALL_RUN_PLUNGE)
        }
        (ParkourState::JumpDescent, AttackRequest::Heavy) => Some(AERIAL_SLAM),
        _ => None,
    }
}

/// Chooses the attack for `request`:
/// - parkour-chained specials take priority (wall runs allow only the
///   plunge);
/// - light attacks continue the combo when chaining, wrapping after the last
///   hit, and otherwise start it at index 0;
/// - heavy attacks perform the weapon's finisher (last entry) directly.
pub fn select_attack(
    state: &MeleeCombatState,
    request: AttackRequest,
    parkour: Option<ParkourState>,
) -> Option<AttackSelection> {
    if let Some(attack) = parkour_attack_for(parkour, request) {
        return Some(AttackSelection {
            attack,
            combo_index: 0,
            continues_combo: false,
        });
    }
    if matches!(
        parkour,
        Some(ParkourState::WallRunLeft | ParkourState::WallRunRight)
    ) {
        return None;
    }
    let tree = combo_tree_for(state.weapon_class);
    let last = tree.len().checked_sub(1)?;
    let index = match request {
        AttackRequest::Light => {
            let chaining =
                state.in_combo && state.current_attack.is_some() && state.can_chain_combo();
            if chaining {
                (state.current_combo_index as usize + 1) % tree.len()
            } else {
                0
            }
        }
        AttackRequest::Heavy => last,
    };
    Some(AttackSelection {
        attack: tree[index],
        combo_index: index as u8,
        continues_combo: request == AttackRequest::Light,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::melee::{step_melee, AttackPhase, MeleeInput};
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    fn all_attacks() -> Vec<AttackFrameData> {
        let mut attacks: Vec<_> = [
            CYBER_KATANA_COMBO_TREE,
            MANTIS_DUAL_BLADES_COMBO_TREE,
            SHOCK_BATON_COMBO_TREE,
            KINETIC_GAUNTLET_COMBO_TREE,
            MONOFILAMENT_WHIP_COMBO_TREE,
        ]
        .iter()
        .flat_map(|tree| tree.iter().copied())
        .collect();
        attacks.extend([SLIDE_UPPERCUT, WALL_RUN_PLUNGE, AERIAL_SLAM]);
        attacks
    }

    #[test]
    fn catalog_frame_data_is_well_formed() {
        for a in all_attacks() {
            assert!(
                a.startup_frames > 0 && a.active_frames > 0 && a.recovery_frames > 0,
                "{}",
                a.name
            );
            assert!(
                a.cancel_frame >= a.active_end_frame() && a.cancel_frame < a.total_frames(),
                "{} cancel window lies in recovery",
                a.name
            );
            assert!(a.base_damage > 0.0 && a.posture_damage > 0.0, "{}", a.name);
            assert!(a.hitbox_extents.min_element() > 0.0, "{}", a.name);
            assert!(a.knockback_direction.length() > 0.0, "{}", a.name);
        }
    }

    #[test]
    fn hitboxes_reach_forward_in_bevy_space() {
        for a in all_attacks() {
            if a.name == "Wall_Run_Plunge" || a.name == "Aerial_Slam" {
                continue;
            }
            assert!(
                a.hitbox_offset.z < 0.0,
                "{} hitbox must be in front (-Z)",
                a.name
            );
        }
    }

    #[test]
    fn spec_values_for_named_attacks() {
        let k = CYBER_KATANA_COMBO_TREE;
        assert_eq!(k.len(), 3);
        assert_eq!(
            (k[0].name, k[0].base_damage, k[0].posture_damage),
            ("Katana_Light_1", 48.0, 22.0)
        );
        assert_eq!(
            (k[1].name, k[1].base_damage, k[1].knockback_impulse),
            ("Katana_Light_2", 56.0, 4.2)
        );
        assert_eq!(
            (k[2].name, k[2].base_damage, k[2].cancel_frame),
            ("Katana_Heavy_Finisher", 135.0, 32)
        );
        assert_eq!(SHOCK_BATON_COMBO_TREE[1].posture_damage, 85.0);
        assert_eq!(KINETIC_GAUNTLET_COMBO_TREE[1].knockback_impulse, 22.0);
        assert_eq!(MONOFILAMENT_WHIP_COMBO_TREE[0].area_radius, 3.5);
        assert_eq!(MANTIS_DUAL_BLADES_COMBO_TREE.len(), 3);
        assert_eq!(
            (WALL_RUN_PLUNGE.area_radius, WALL_RUN_PLUNGE.base_damage),
            (4.0, 90.0)
        );
        assert_eq!(AERIAL_SLAM.attacker_dive_speed, 35.0);
        let launch = SLIDE_UPPERCUT.knockback_direction;
        assert_eq!(
            launch,
            Vec3::new(0.0, 1.0, -0.3),
            "uppercut launches upward"
        );
    }

    /// Presses light once and then again as soon as each chain window opens.
    fn chain_lights(state: &mut MeleeCombatState, hits: usize) -> Vec<u8> {
        let mut input = MeleeInput::default();
        let mut indices = Vec::new();
        let mut presses = 0;
        for _ in 0..1000 {
            let ready = state.current_attack.is_none() || state.can_chain_combo();
            if ready && presses < hits && state.queued_attack.is_none() {
                input.light_pressed = true;
                presses += 1;
            }
            if let Some(started) = step_melee(state, &mut input, None, DT).started {
                indices.push(state.current_combo_index);
                assert_eq!(
                    started,
                    combo_tree_for(state.weapon_class)[state.current_combo_index as usize]
                );
            }
            if indices.len() == hits {
                break;
            }
        }
        indices
    }

    #[test]
    fn katana_combo_advances_and_wraps() {
        let mut s = MeleeCombatState::default();
        assert_eq!(chain_lights(&mut s, 4), vec![0, 1, 2, 0]);
    }

    #[test]
    fn combo_resets_when_left_to_finish() {
        let mut s = MeleeCombatState::default();
        assert_eq!(chain_lights(&mut s, 2), vec![0, 1]);
        while s.current_attack.is_some() {
            step_melee(&mut s, &mut MeleeInput::default(), None, DT);
        }
        assert_eq!(s.current_combo_index, 0);
        assert_eq!(chain_lights(&mut s, 1), vec![0], "restarts at the opener");
    }

    #[test]
    fn early_press_is_buffered_until_the_chain_window() {
        let mut s = MeleeCombatState::default();
        let mut input = MeleeInput {
            light_pressed: true,
            ..Default::default()
        };
        step_melee(&mut s, &mut input, None, DT);
        // Run into the active window, then press.
        while s.attack_phase != AttackPhase::Active {
            step_melee(&mut s, &mut input, None, DT);
        }
        input.light_pressed = true;
        let mut chained = false;
        for _ in 0..20 {
            if step_melee(&mut s, &mut input, None, DT).started.is_some() {
                chained = true;
                break;
            }
        }
        assert!(chained, "buffered press fired when recovery began");
        assert_eq!(s.current_combo_index, 1);
    }

    #[test]
    fn stale_buffered_press_expires() {
        let mut s = MeleeCombatState::default();
        s.start_attack(CYBER_KATANA_COMBO_TREE[2]); // long startup, 14 frames
        let mut input = MeleeInput {
            light_pressed: true,
            ..Default::default()
        };
        step_melee(&mut s, &mut input, None, DT);
        for _ in 0..14 {
            step_melee(&mut s, &mut input, None, DT);
        }
        assert!(
            s.queued_attack.is_none(),
            "0.2 s buffer elapsed before the window"
        );
    }

    #[test]
    fn heavy_goes_straight_to_the_finisher() {
        let s = MeleeCombatState::with_weapon(MeleeWeaponClass::KineticHeavyGauntlet);
        let pick = select_attack(&s, AttackRequest::Heavy, None).expect("attack");
        assert_eq!(pick.attack.name, "Gauntlet_Rocket_Punch");
        assert!(!pick.continues_combo);
    }

    #[test]
    fn parkour_chained_attacks() {
        let s = MeleeCombatState::default();
        let pick = |state, request| select_attack(&s, request, Some(state)).map(|p| p.attack.name);
        assert_eq!(
            pick(ParkourState::Sliding, AttackRequest::Heavy),
            Some("Slide_Uppercut")
        );
        assert_eq!(
            pick(ParkourState::Sliding, AttackRequest::Light),
            Some("Katana_Light_1")
        );
        assert_eq!(
            pick(ParkourState::WallRunLeft, AttackRequest::Light),
            Some("Wall_Run_Plunge")
        );
        assert_eq!(
            pick(ParkourState::WallRunRight, AttackRequest::Light),
            Some("Wall_Run_Plunge")
        );
        assert_eq!(pick(ParkourState::WallRunRight, AttackRequest::Heavy), None);
        assert_eq!(
            pick(ParkourState::JumpDescent, AttackRequest::Heavy),
            Some("Aerial_Slam")
        );
        assert_eq!(
            pick(ParkourState::JumpAscent, AttackRequest::Light),
            Some("Katana_Light_1")
        );
        assert_eq!(
            pick(ParkourState::Sprinting, AttackRequest::Heavy),
            Some("Katana_Heavy_Finisher")
        );
    }

    #[test]
    fn locked_parkour_states_block_attacks() {
        let mut s = MeleeCombatState::default();
        for state in [
            ParkourState::LedgeMantle,
            ParkourState::VaultLow,
            ParkourState::HardLanding,
            ParkourState::Ragdoll,
        ] {
            let mut input = MeleeInput {
                light_pressed: true,
                ..Default::default()
            };
            step_melee(&mut s, &mut input, Some(state), DT);
            assert!(s.current_attack.is_none(), "{state:?}");
            s.queued_attack = None;
        }
    }
}
