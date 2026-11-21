//! Drop tables for the 15 enemy archetypes. Weights are out of 10,000.

use crate::loot_engine::{LootDropEntry, LootTable};

const fn drop(item_id: u32, min_quantity: u32, max_quantity: u32, weight: u32) -> LootDropEntry {
    LootDropEntry {
        item_id,
        min_quantity,
        max_quantity,
        weight,
    }
}

pub static SWARM_DRONE_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1001, 2, 5, 6000),
        drop(1003, 1, 2, 3000),
        drop(1004, 1, 1, 1000),
    ],
);

pub static STREET_GRUNT_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1001, 4, 8, 5000),
        drop(1002, 2, 6, 3000),
        drop(1021, 1, 2, 1500),
        drop(2002, 1, 1, 500),
    ],
);

pub static CYBER_NINJA_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1003, 4, 10, 4000),
        drop(1007, 1, 3, 3000),
        drop(1025, 1, 2, 2000),
        drop(2004, 1, 1, 1000),
    ],
);

pub static SHIELDED_HEAVY_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1014, 6, 12, 4500),
        drop(1008, 2, 5, 3500),
        drop(1024, 1, 2, 1500),
        drop(2008, 1, 1, 500),
    ],
);

pub static NETRUNNER_SNIPER_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1004, 5, 10, 4000),
        drop(1009, 1, 2, 3000),
        drop(1017, 1, 2, 2000),
        drop(2011, 1, 1, 1000),
    ],
);

pub static MECHA_ENFORCER_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1014, 15, 30, 4000),
        drop(1008, 8, 15, 3000),
        drop(1013, 4, 8, 2000),
        drop(2010, 1, 1, 1000),
    ],
);

pub static CORPO_SECURITY_OFFICER_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1002, 4, 10, 4500),
        drop(1004, 2, 5, 3000),
        drop(1022, 1, 2, 2000),
        drop(2006, 1, 1, 500),
    ],
);

pub static SCAVENGER_RAIDER_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1001, 5, 12, 5000),
        drop(1006, 2, 5, 3000),
        drop(1028, 1, 3, 1500),
        drop(2003, 1, 1, 500),
    ],
);

pub static CYBER_HOUND_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1002, 3, 8, 6000),
        drop(1016, 1, 2, 3000),
        drop(1025, 1, 1, 1000),
    ],
);

pub static ROOFTOP_ASSASSIN_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1012, 4, 10, 4000),
        drop(1007, 1, 3, 3000),
        drop(1032, 1, 2, 2000),
        drop(2009, 1, 1, 1000),
    ],
);

pub static BIO_SYNTH_BERSERKER_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1015, 2, 6, 4000),
        drop(1016, 1, 4, 3500),
        drop(1034, 1, 2, 1500),
        drop(2013, 1, 1, 1000),
    ],
);

pub static COMBAT_MEDIC_DRONE_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1011, 3, 8, 4500),
        drop(1023, 1, 2, 3500),
        drop(1024, 1, 2, 1500),
        drop(1012, 2, 6, 500),
    ],
);

pub static PLASMA_SENTINEL_TURRET_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1015, 3, 8, 4500),
        drop(1013, 2, 5, 3500),
        drop(2007, 1, 1, 1500),
        drop(1014, 6, 12, 500),
    ],
);

pub static SUBWAY_GHOUL_DROP_TABLE: LootTable = LootTable::new(
    &[],
    &[
        drop(1001, 4, 10, 4000),
        drop(1019, 3, 8, 3500),
        drop(1031, 1, 2, 2000),
        drop(2012, 1, 1, 500),
    ],
);

pub static APEX_ENFORCER_GORGON_BOSS_DROP_TABLE: LootTable = LootTable::new(
    &[(1010, 1), (1009, 3)],
    &[
        drop(2020, 1, 1, 2500),
        drop(2021, 1, 1, 2500),
        drop(1018, 1, 2, 3000),
        drop(2039, 1, 1, 2000),
    ],
);

/// Every archetype's table, keyed by snake_case name.
pub static ENEMY_LOOT_TABLES: &[(&str, &LootTable)] = &[
    ("swarm_drone", &SWARM_DRONE_DROP_TABLE),
    ("street_grunt", &STREET_GRUNT_DROP_TABLE),
    ("cyber_ninja", &CYBER_NINJA_DROP_TABLE),
    ("shielded_heavy", &SHIELDED_HEAVY_DROP_TABLE),
    ("netrunner_sniper", &NETRUNNER_SNIPER_DROP_TABLE),
    ("mecha_enforcer", &MECHA_ENFORCER_DROP_TABLE),
    ("corpo_security_officer", &CORPO_SECURITY_OFFICER_DROP_TABLE),
    ("scavenger_raider", &SCAVENGER_RAIDER_DROP_TABLE),
    ("cyber_hound", &CYBER_HOUND_DROP_TABLE),
    ("rooftop_assassin", &ROOFTOP_ASSASSIN_DROP_TABLE),
    ("bio_synth_berserker", &BIO_SYNTH_BERSERKER_DROP_TABLE),
    ("combat_medic_drone", &COMBAT_MEDIC_DRONE_DROP_TABLE),
    ("plasma_sentinel_turret", &PLASMA_SENTINEL_TURRET_DROP_TABLE),
    ("subway_ghoul", &SUBWAY_GHOUL_DROP_TABLE),
    (
        "apex_enforcer_gorgon",
        &APEX_ENFORCER_GORGON_BOSS_DROP_TABLE,
    ),
];

/// Accepts any casing with spaces, hyphens or underscores
/// ("Swarm Drone", "swarm-drone", "SWARM_DRONE").
pub fn get_enemy_loot_table(enemy_type: &str) -> Option<&'static LootTable> {
    let key: String = enemy_type
        .trim()
        .chars()
        .map(|c| match c {
            ' ' | '-' => '_',
            c => c.to_ascii_lowercase(),
        })
        .collect();
    ENEMY_LOOT_TABLES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, table)| *table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item_catalog::find_item;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashMap;

    #[test]
    fn fifteen_tables_registered() {
        assert_eq!(ENEMY_LOOT_TABLES.len(), 15);
        for (name, _) in ENEMY_LOOT_TABLES {
            assert!(get_enemy_loot_table(name).is_some(), "{name}");
        }
        assert!(std::ptr::eq(
            get_enemy_loot_table("Swarm Drone").unwrap(),
            &SWARM_DRONE_DROP_TABLE
        ));
        assert!(get_enemy_loot_table("APEX-ENFORCER-GORGON").is_some());
        assert!(get_enemy_loot_table("void_dragon").is_none());
    }

    #[test]
    fn tables_are_well_formed() {
        for (name, table) in ENEMY_LOOT_TABLES {
            assert_eq!(table.total_weight, 10_000, "{name}");
            for entry in table.weighted_entries {
                assert!(
                    find_item(entry.item_id).is_some(),
                    "{name}: {}",
                    entry.item_id
                );
                assert!(entry.min_quantity >= 1 && entry.min_quantity <= entry.max_quantity);
                assert!(entry.weight > 0);
            }
            for &(id, q) in table.guaranteed_drops {
                assert!(find_item(id).is_some() && q > 0, "{name}: {id}");
            }
        }
    }

    #[test]
    fn swarm_drone_monte_carlo_matches_weights() {
        const ROLLS: u32 = 10_000;
        let mut rng = StdRng::seed_from_u64(0x5EED);
        let mut hits: HashMap<u32, u32> = HashMap::new();
        for _ in 0..ROLLS {
            let drops = SWARM_DRONE_DROP_TABLE.roll(&mut rng, 1.0);
            assert_eq!(drops.len(), 1, "no guaranteed drops, one pick");
            *hits.entry(drops[0].0).or_default() += 1;
        }
        for (item, expected) in [(1001, 0.60), (1003, 0.30), (1004, 0.10)] {
            let observed = hits[&item] as f64 / ROLLS as f64;
            println!("swarm drone {item}: {observed:.4} (expected {expected:.2})");
            assert!((observed - expected).abs() <= 0.025, "{item}: {observed}");
        }
    }

    #[test]
    fn gorgon_guaranteed_drops_every_roll() {
        let mut rng = StdRng::seed_from_u64(31);
        for _ in 0..1_000 {
            let drops = APEX_ENFORCER_GORGON_BOSS_DROP_TABLE.roll(&mut rng, 1.0);
            assert_eq!(&drops[..2], &[(1010, 1), (1009, 3)]);
            assert_eq!(drops.len(), 3);
            assert!([2020, 2021, 1018, 2039].contains(&drops[2].0));
        }
    }
}
