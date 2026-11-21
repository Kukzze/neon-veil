//! The 40 cyber-mod cards.
//!
//! Cards 501-534 are the design catalog. Cards 509-510, 516-519, 526-529,
//! 535-537 and the Universal 541-545 complete the 40-card set, covering
//! status resistances, movement and economy stats used elsewhere in the game.
//! Bonuses are per rank step: a card at rank `r` gives `value * (r + 1)`.

use crate::mod_system::{CyberModCard, ModPolarity};

const fn card(
    id: u32,
    name: &'static str,
    polarity: ModPolarity,
    base_cost: u32,
    max_rank: u32,
    stat_multipliers: &'static [(&'static str, f32)],
) -> CyberModCard {
    CyberModCard {
        id,
        name,
        polarity,
        base_cost,
        current_rank: 0,
        max_rank,
        stat_multipliers,
    }
}

use ModPolarity::{Attack, Augment, Defense, Universal, Utility};

pub static MOD_CATALOG: [CyberModCard; 40] = [
    // Attack (Madurai V)
    card(
        501,
        "Serrated Edge",
        Attack,
        4,
        10,
        &[("base_damage", 0.15)],
    ),
    card(
        502,
        "Thermal Ignition",
        Attack,
        6,
        5,
        &[("fire_damage", 0.15), ("status_chance", 0.10)],
    ),
    card(
        503,
        "Point Strike",
        Attack,
        4,
        5,
        &[("critical_chance", 0.25)],
    ),
    card(
        504,
        "Vital Sense",
        Attack,
        6,
        5,
        &[("critical_damage", 0.20)],
    ),
    card(
        505,
        "Pressure Point",
        Attack,
        4,
        5,
        &[("melee_damage", 0.20)],
    ),
    card(
        506,
        "Blood Rush",
        Attack,
        7,
        10,
        &[("combo_crit_scaling", 0.04)],
    ),
    card(
        507,
        "Condition Overload",
        Attack,
        8,
        5,
        &[("status_damage_multiplier", 0.16)],
    ),
    card(
        508,
        "Heavy Caliber",
        Attack,
        6,
        10,
        &[("base_damage", 0.15), ("accuracy_penalty", -0.05)],
    ),
    card(509, "Split Chamber", Attack, 10, 5, &[("multishot", 0.15)]),
    card(510, "Impact Driver", Attack, 4, 5, &[("knockback", 0.10)]),
    // Defense (Vazarin D)
    card(511, "Redirection", Defense, 4, 10, &[("max_shields", 0.40)]),
    card(512, "Vitality", Defense, 4, 10, &[("max_health", 0.40)]),
    card(
        513,
        "Steel Fiber",
        Defense,
        4,
        10,
        &[("armor_rating", 0.10)],
    ),
    card(
        514,
        "Adaptation",
        Defense,
        8,
        10,
        &[("damage_resistance_stack", 0.01)],
    ),
    card(
        515,
        "Fast Deflection",
        Defense,
        4,
        5,
        &[("shield_recharge_rate", 0.15)],
    ),
    card(
        516,
        "Heat Sink Weave",
        Defense,
        4,
        5,
        &[("thermal_resistance", 0.15)],
    ),
    card(
        517,
        "Acid Ward",
        Defense,
        4,
        5,
        &[("corrosive_resistance", 0.15)],
    ),
    card(
        518,
        "Cryo Insulation",
        Defense,
        4,
        5,
        &[("cryo_resistance", 0.15)],
    ),
    card(
        519,
        "Grounding Coil",
        Defense,
        4,
        5,
        &[("emp_resistance", 0.15)],
    ),
    // Utility (Naramon -)
    card(521, "Rush", Utility, 4, 5, &[("sprint_speed", 0.05)]),
    card(
        522,
        "Streamline",
        Utility,
        4,
        5,
        &[("stamina_efficiency", 0.06)],
    ),
    card(523, "Fast Hands", Utility, 2, 5, &[("reload_speed", 0.06)]),
    card(
        524,
        "Maglev",
        Utility,
        4,
        5,
        &[("slide_friction_reduction", 0.08)],
    ),
    card(525, "Mobilize", Utility, 2, 3, &[("wall_run_speed", 0.07)]),
    card(
        526,
        "Steady Hands",
        Utility,
        4,
        5,
        &[("recoil_reduction", 0.08)],
    ),
    card(
        527,
        "Muffled Soles",
        Utility,
        4,
        5,
        &[("footstep_noise_reduction", 0.10)],
    ),
    card(528, "Vigor Coil", Utility, 4, 5, &[("stamina_max", 0.08)]),
    card(
        529,
        "Spring Tendons",
        Utility,
        4,
        5,
        &[("jump_height", 0.05)],
    ),
    // Augment (Zenurik =)
    card(
        531,
        "Kinetic Siphon",
        Augment,
        6,
        5,
        &[("sprint_shield_recharge", 0.10)],
    ),
    card(
        532,
        "Acrobatic Ricochet",
        Augment,
        7,
        5,
        &[("wall_kick_bullet_crit", 0.35)],
    ),
    card(
        533,
        "Ghost Frequency",
        Augment,
        8,
        5,
        &[("slide_radar_invisibility", 1.0)],
    ),
    card(
        534,
        "Overdrive Capacitor",
        Augment,
        9,
        5,
        &[("flow_decay_reduction", 0.20)],
    ),
    card(
        535,
        "Parry Resonator",
        Augment,
        7,
        5,
        &[("perfect_parry_window", 0.05)],
    ),
    card(
        536,
        "Momentum Battery",
        Augment,
        6,
        5,
        &[("slide_speed", 0.06)],
    ),
    card(
        537,
        "Netlink Overclock",
        Augment,
        6,
        5,
        &[("hack_speed", 0.10)],
    ),
    // Universal
    card(541, "Cortex Uplink", Universal, 6, 5, &[("xp_gain", 0.03)]),
    card(
        542,
        "Adaptive Plating",
        Universal,
        6,
        5,
        &[("armor_rating", 0.05)],
    ),
    card(
        543,
        "Reflex Booster",
        Universal,
        6,
        5,
        &[("fire_rate", 0.05)],
    ),
    card(
        544,
        "Street Rapport",
        Universal,
        4,
        5,
        &[("vendor_discount", 0.02)],
    ),
    card(
        545,
        "Cyberware Stabilizer",
        Universal,
        6,
        5,
        &[("cyberware_tolerance", 0.05)],
    ),
];

pub fn find_mod(id: u32) -> Option<CyberModCard> {
    MOD_CATALOG.iter().find(|c| c.id == id).copied()
}

pub fn find_mod_by_name(name: &str) -> Option<CyberModCard> {
    MOD_CATALOG
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_system::ModLoadout;
    use std::collections::HashSet;

    #[test]
    fn forty_unique_cards() {
        assert_eq!(MOD_CATALOG.len(), 40);
        let ids: HashSet<_> = MOD_CATALOG.iter().map(|c| c.id).collect();
        let names: HashSet<_> = MOD_CATALOG.iter().map(|c| c.name).collect();
        assert_eq!((ids.len(), names.len()), (40, 40));
        for c in &MOD_CATALOG {
            assert_eq!(c.current_rank, 0, "{}", c.name);
            assert!(
                c.max_rank > 0 && c.base_cost > 0 && !c.stat_multipliers.is_empty(),
                "{}",
                c.name
            );
        }
    }

    #[test]
    fn every_polarity_is_represented() {
        for polarity in [Attack, Defense, Utility, Augment, Universal] {
            assert!(
                MOD_CATALOG.iter().any(|c| c.polarity == polarity),
                "{polarity:?}"
            );
        }
    }

    #[test]
    fn spec_cards_keep_their_values() {
        let serrated = find_mod(501).expect("Serrated Edge");
        assert_eq!((serrated.base_cost, serrated.max_rank), (4, 10));
        assert!(
            (serrated.with_rank(10).stat_bonus("base_damage") - 1.65).abs() < 1e-5,
            "+165 % at max"
        );
        let redirection = find_mod_by_name("redirection").expect("Redirection");
        assert!(
            (redirection.with_rank(10).stat_bonus("max_shields") - 4.40).abs() < 1e-5,
            "+440 % at max"
        );
        let heavy = find_mod_by_name("Heavy Caliber").expect("Heavy Caliber");
        assert!(heavy.stat_bonus("accuracy_penalty") < 0.0);
        assert!(find_mod(999).is_none());
    }

    #[test]
    fn redirection_and_vitality_boost_shields_and_health() {
        let mut loadout = ModLoadout::default();
        loadout.slot_polarities[0] = Some(Defense);
        loadout.slot_polarities[1] = Some(Defense);
        let redirection = find_mod_by_name("Redirection").expect("card").with_rank(4);
        let vitality = find_mod_by_name("Vitality").expect("card").with_rank(2);
        loadout.try_install(0, redirection).expect("fits");
        loadout.try_install(1, vitality).expect("fits");
        assert_eq!(loadout.total_drain(), 4 + 3, "ceil(8/2) + ceil(6/2)");
        assert!(
            (loadout.aggregate_stat_multiplier("max_shields") - 3.0).abs() < 1e-5,
            "+200 %"
        );
        assert!(
            (loadout.aggregate_stat_multiplier("max_health") - 2.2).abs() < 1e-5,
            "+120 %"
        );
        assert_eq!(loadout.aggregate_stat_multiplier("armor_rating"), 1.0);
    }
}
