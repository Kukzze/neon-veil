//! The 60 engine sound cues: parkour, melee, ranged, UI, music stems and
//! ambience.

use crate::audio_bus::AudioChannelBus::{
    self, Ambience, CombatFoley, DynamicMusic, ParkourMovement, UserInterface,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundCueDefinition {
    pub id: &'static str,
    pub bus: AudioChannelBus,
    pub default_volume: f32,
    pub base_pitch: f32,
    pub frequency_profile: &'static str,
    pub trigger: &'static str,
}

const fn cue(
    id: &'static str,
    bus: AudioChannelBus,
    default_volume: f32,
    base_pitch: f32,
    frequency_profile: &'static str,
    trigger: &'static str,
) -> SoundCueDefinition {
    SoundCueDefinition {
        id,
        bus,
        default_volume,
        base_pitch,
        frequency_profile,
        trigger,
    }
}

pub const AUDIO_CUE_COUNT: usize = 60;

#[rustfmt::skip]
pub static MASTER_AUDIO_CATALOG: &[SoundCueDefinition] = &[
    // --- Parkour movement ---
    cue("sfx_footstep_concrete", ParkourMovement, 0.55, 1.0, "450 Hz - 2.5 kHz", "Foot touches pavement"),
    cue("sfx_footstep_metal", ParkourMovement, 0.60, 1.0, "800 Hz - 6.0 kHz", "Foot touches grating"),
    cue("sfx_footstep_wood", ParkourMovement, 0.50, 1.0, "250 Hz - 3.0 kHz", "Foot touches planks"),
    cue("sfx_slide_loop", ParkourMovement, 0.65, 1.0, "300 Hz - 4.0 kHz", "Sliding on a surface"),
    cue("sfx_wallrun_loop", ParkourMovement, 0.60, 1.0, "1.2 kHz - 8.0 kHz", "Wall-run contact"),
    cue("sfx_wallkick_impulse", ParkourMovement, 0.75, 1.0, "80 Hz sub + transient", "Jump off a wall"),
    cue("sfx_mantle_grab", ParkourMovement, 0.70, 1.0, "500 Hz thud", "Hand touches a ledge"),
    cue("sfx_zipline_whine", ParkourMovement, 0.55, 1.0, "3.5 kHz whine", "Zipline trolley"),
    cue("sfx_jump_takeoff", ParkourMovement, 0.50, 1.0, "200 Hz - 2.0 kHz push", "Ground jump"),
    cue("sfx_land_soft", ParkourMovement, 0.55, 1.0, "150 Hz - 1.5 kHz thump", "Normal landing"),
    cue("sfx_land_hard", ParkourMovement, 0.80, 0.95, "60 Hz sub impact", "Hard landing lock"),
    cue("sfx_landing_roll", ParkourMovement, 0.60, 1.0, "300 Hz - 3.0 kHz cloth roll", "Landing roll"),
    cue("sfx_vault_hand_plant", ParkourMovement, 0.60, 1.0, "700 Hz slap", "Vault over an obstacle"),
    cue("sfx_grapple_fire", ParkourMovement, 0.75, 1.0, "2.0 kHz pneumatic snap", "Grappling hook fired"),
    cue("sfx_wind_rush_loop", ParkourMovement, 0.80, 1.0, "Broadband rush", "Horizontal speed 8-25 m/s"),
    // --- Melee combat ---
    cue("sfx_katana_swing_light", CombatFoley, 0.70, 1.0, "1.5 kHz whoosh", "Light attack 1/2"),
    cue("sfx_katana_swing_heavy", CombatFoley, 0.80, 0.9, "800 Hz deep swish", "Heavy finisher"),
    cue("sfx_parry_spark", CombatFoley, 0.90, 1.0, "4.5 kHz chime", "Perfect parry (120 ms)"),
    cue("sfx_posture_shatter", CombatFoley, 0.90, 1.0, "60 Hz sub drop", "Posture breaks"),
    cue("sfx_flesh_slice", CombatFoley, 0.70, 1.0, "1.2 kHz impact", "Melee hit, unarmored"),
    cue("sfx_armor_clink", CombatFoley, 0.65, 1.0, "5.0 kHz deflect", "Melee hit against armor"),
    cue("sfx_block_guard", CombatFoley, 0.65, 1.0, "2.5 kHz metal ring", "Hit taken on guard"),
    cue("sfx_dodge_whoosh", CombatFoley, 0.50, 1.1, "1.0 kHz air", "Dodge step"),
    cue("sfx_finisher_impact", CombatFoley, 0.90, 0.9, "50 Hz + 3 kHz crack", "Execution finisher"),
    // --- Ranged combat ---
    cue("sfx_smart_pistol_shot", CombatFoley, 0.75, 1.0, "2.2 kHz pop", "Pistol fire"),
    cue("sfx_smg_shot_loop", CombatFoley, 0.70, 1.0, "1.8 kHz staccato", "Vector-X fire"),
    cue("sfx_railgun_charge", CombatFoley, 0.75, 1.0, "200 Hz -> 8 kHz sweep", "Charging the railgun"),
    cue("sfx_railgun_discharge", CombatFoley, 0.90, 1.0, "45 Hz heavy crack", "Railgun fired"),
    cue("sfx_shotgun_blast", CombatFoley, 0.90, 1.0, "90 Hz boom", "Plasma shotgun"),
    cue("sfx_emp_detonation", CombatFoley, 0.85, 1.0, "120 Hz electric", "EMP grenade pop"),
    cue("sfx_thermal_burn_tick", CombatFoley, 0.45, 1.0, "800 Hz sizzle", "Burn damage tick"),
    cue("sfx_cryo_freeze", CombatFoley, 0.70, 1.0, "6.0 kHz shatter", "Slow applied"),
    cue("sfx_acid_hiss", CombatFoley, 0.55, 1.0, "3.0 kHz hiss", "Acid armor shred"),
    cue("sfx_reload_pistol", CombatFoley, 0.55, 1.0, "1.5 kHz mechanical clicks", "Pistol reload"),
    cue("sfx_reload_heavy", CombatFoley, 0.60, 0.9, "600 Hz magazine slam", "Rifle and shotgun reload"),
    cue("sfx_dry_fire_click", CombatFoley, 0.45, 1.0, "3.0 kHz click", "Trigger on an empty magazine"),
    cue("sfx_bullet_ricochet", CombatFoley, 0.55, 1.0, "4.0 kHz zing", "Round hits hard cover"),
    cue("sfx_bullet_whizby", CombatFoley, 0.60, 1.0, "2.5 kHz doppler", "Near miss on the player"),
    cue("sfx_drone_kamikaze_beep", CombatFoley, 0.85, 1.0, "3.2 kHz accelerating beep", "Swarm drone arming"),
    cue("sfx_explosion_large", CombatFoley, 0.90, 1.0, "35 Hz rumble + debris", "Drone or ordnance blast"),
    cue("sfx_gorgon_laser_charge", CombatFoley, 0.85, 1.0, "150 Hz -> 5 kHz hum", "Gorgon laser grid telegraph"),
    cue("sfx_gorgon_airstrike_siren", CombatFoley, 0.85, 1.0, "700 Hz rising siren", "Gorgon airstrike telegraph"),
    // --- User interface ---
    cue("sfx_hit_marker_body", UserInterface, 0.70, 1.0, "1.0 kHz click", "Shot confirmed on a target"),
    cue("sfx_hit_marker_crit", UserInterface, 0.80, 1.0, "2.2 kHz ding", "Critical hit confirmed"),
    cue("sfx_level_up_fanfare", UserInterface, 1.00, 1.0, "Brass cyber chime", "Skill level-up"),
    cue("sfx_quest_completed", UserInterface, 1.00, 1.0, "Triumphant fanfare", "Quest completed"),
    cue("sfx_ge_trade_filled", UserInterface, 0.85, 1.0, "Cash register chime", "Exchange trade executed"),
    cue("sfx_quest_started", UserInterface, 0.80, 1.0, "Rising synth stab", "Quest accepted"),
    cue("sfx_xp_drop_tick", UserInterface, 0.40, 1.2, "4.0 kHz tick", "Experience gained"),
    cue("sfx_item_pickup", UserInterface, 0.60, 1.0, "1.8 kHz blip", "Item added to inventory"),
    cue("sfx_menu_navigate", UserInterface, 0.40, 1.0, "2.0 kHz tick", "Menu focus moves"),
    cue("sfx_menu_confirm", UserInterface, 0.60, 1.0, "1.2 kHz double blip", "Menu selection"),
    cue("sfx_error_buzz", UserInterface, 0.60, 1.0, "180 Hz buzz", "Action refused"),
    // --- Music stems ---
    cue("mus_stem_ambient", DynamicMusic, 1.00, 1.0, "Low pad, 60 BPM", "Resting or exploring"),
    cue("mus_stem_stealth", DynamicMusic, 1.00, 1.0, "Ticking synth", "Crouching in shadow"),
    cue("mus_stem_parkour_flow", DynamicMusic, 1.00, 1.0, "160 BPM breakbeat", "High flow or wall movement"),
    cue("mus_stem_combat_intense", DynamicMusic, 1.00, 1.0, "Distorted 808s", "Enemy in combat"),
    // --- Ambience ---
    cue("amb_undercity_rain_loop", Ambience, 0.70, 1.0, "Broadband rain + drips", "Undercity Slums bed"),
    cue("amb_foundry_furnace_loop", Ambience, 0.70, 1.0, "40 Hz roar + clank", "Industrial Foundry bed"),
    cue("amb_steam_vent_hiss", Ambience, 0.60, 1.0, "5.0 kHz hiss", "Steam vent geyser"),
];

pub fn find_sound_cue(id: &str) -> Option<&'static SoundCueDefinition> {
    MASTER_AUDIO_CATALOG.iter().find(|c| c.id == id)
}

pub fn cues_on_bus(bus: AudioChannelBus) -> impl Iterator<Item = &'static SoundCueDefinition> {
    MASTER_AUDIO_CATALOG.iter().filter(move |c| c.bus == bus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The 30 cues listed in the design table.
    const DESIGN_CUES: [&str; 30] = [
        "sfx_footstep_concrete",
        "sfx_footstep_metal",
        "sfx_slide_loop",
        "sfx_wallrun_loop",
        "sfx_wallkick_impulse",
        "sfx_mantle_grab",
        "sfx_zipline_whine",
        "sfx_katana_swing_light",
        "sfx_katana_swing_heavy",
        "sfx_parry_spark",
        "sfx_posture_shatter",
        "sfx_flesh_slice",
        "sfx_armor_clink",
        "sfx_smart_pistol_shot",
        "sfx_smg_shot_loop",
        "sfx_railgun_charge",
        "sfx_railgun_discharge",
        "sfx_shotgun_blast",
        "sfx_emp_detonation",
        "sfx_thermal_burn_tick",
        "sfx_cryo_freeze",
        "sfx_hit_marker_body",
        "sfx_hit_marker_crit",
        "sfx_level_up_fanfare",
        "sfx_quest_completed",
        "sfx_ge_trade_filled",
        "mus_stem_ambient",
        "mus_stem_stealth",
        "mus_stem_parkour_flow",
        "mus_stem_combat_intense",
    ];

    #[test]
    fn sixty_unique_cues() {
        assert_eq!(MASTER_AUDIO_CATALOG.len(), AUDIO_CUE_COUNT);
        let ids: HashSet<&str> = MASTER_AUDIO_CATALOG.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), AUDIO_CUE_COUNT);
        for id in DESIGN_CUES {
            assert!(find_sound_cue(id).is_some(), "{id}");
        }
        assert!(find_sound_cue("sfx_does_not_exist").is_none());
    }

    #[test]
    fn cues_are_well_formed() {
        for c in MASTER_AUDIO_CATALOG {
            let prefix_ok = match c.bus {
                DynamicMusic => c.id.starts_with("mus_"),
                Ambience => c.id.starts_with("amb_"),
                _ => c.id.starts_with("sfx_"),
            };
            assert!(prefix_ok, "{} prefix does not match {:?}", c.id, c.bus);
            assert!(
                c.default_volume > 0.0 && c.default_volume <= 1.0,
                "{}",
                c.id
            );
            assert!((0.5..=2.0).contains(&c.base_pitch), "{}", c.id);
            assert!(!c.frequency_profile.is_empty() && !c.trigger.is_empty());
            assert!(c.bus != AudioChannelBus::Master && c.bus != AudioChannelBus::Voiceover);
        }
    }

    #[test]
    fn four_music_stems_and_bus_counts() {
        let stems: Vec<&str> = cues_on_bus(DynamicMusic).map(|c| c.id).collect();
        assert_eq!(
            stems,
            vec![
                "mus_stem_ambient",
                "mus_stem_stealth",
                "mus_stem_parkour_flow",
                "mus_stem_combat_intense"
            ]
        );
        assert_eq!(cues_on_bus(ParkourMovement).count(), 15);
        assert_eq!(cues_on_bus(CombatFoley).count(), 27);
        assert_eq!(cues_on_bus(UserInterface).count(), 11);
        assert_eq!(cues_on_bus(Ambience).count(), 3);
    }

    #[test]
    fn fanfares_sit_above_combat() {
        let loudest_combat = cues_on_bus(CombatFoley)
            .map(|c| c.default_volume)
            .fold(0.0, f32::max);
        let fanfare = find_sound_cue("sfx_level_up_fanfare")
            .unwrap()
            .default_volume;
        assert!(fanfare > loudest_combat);
    }
}
