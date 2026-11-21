//! Master item catalog: materials, consumables, weapons, armor and
//! cyberware, quest curios and mastery capes.

use serde::Serialize;

pub const ITEM_CATEGORIES: &[&str] = &[
    "Material",
    "Consumable",
    "Weapon",
    "Armor",
    "Cyberware",
    "Cape",
    "Curio",
];

/// Ordered from most to least common.
pub const ITEM_RARITIES: &[&str] = &["Common", "Uncommon", "Rare", "Epic", "Legendary", "Exotic"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemDefinition {
    pub id: u32,
    pub name: &'static str,
    pub category: &'static str,
    pub rarity: &'static str,
    pub is_tradable: bool,
    pub high_alch_value: u64,
    pub store_price: u64,
}

#[rustfmt::skip]
pub static MASTER_ITEM_CATALOG: &[ItemDefinition] = &[
    // --- RAW MATERIALS (1001 - 1020) ---
    ItemDefinition { id: 1001, name: "Nanite Alloy Ingot", category: "Material", rarity: "Common", is_tradable: true, high_alch_value: 30, store_price: 50 },
    ItemDefinition { id: 1002, name: "Synthetic Polymer Sheet", category: "Material", rarity: "Common", is_tradable: true, high_alch_value: 20, store_price: 35 },
    ItemDefinition { id: 1003, name: "Optical Fiber Bundle", category: "Material", rarity: "Common", is_tradable: true, high_alch_value: 45, store_price: 80 },
    ItemDefinition { id: 1004, name: "Micro-Processing Chip", category: "Material", rarity: "Uncommon", is_tradable: true, high_alch_value: 120, store_price: 200 },
    ItemDefinition { id: 1005, name: "Pneumatic Pressure Valve", category: "Material", rarity: "Uncommon", is_tradable: true, high_alch_value: 150, store_price: 250 },
    ItemDefinition { id: 1006, name: "Volatile Electrolyte Fluid", category: "Material", rarity: "Uncommon", is_tradable: true, high_alch_value: 180, store_price: 300 },
    ItemDefinition { id: 1007, name: "Diamond Crystal Lattice", category: "Material", rarity: "Rare", is_tradable: true, high_alch_value: 850, store_price: 1500 },
    ItemDefinition { id: 1008, name: "Superconducting Magnetic Coil", category: "Material", rarity: "Rare", is_tradable: true, high_alch_value: 1200, store_price: 2200 },
    ItemDefinition { id: 1009, name: "Quantum Data Bus", category: "Material", rarity: "Epic", is_tradable: true, high_alch_value: 4500, store_price: 8000 },
    ItemDefinition { id: 1010, name: "Darknet Void Core", category: "Material", rarity: "Legendary", is_tradable: true, high_alch_value: 25000, store_price: 50000 },
    ItemDefinition { id: 1011, name: "Cryo-Coolant Canister", category: "Material", rarity: "Uncommon", is_tradable: true, high_alch_value: 110, store_price: 190 },
    ItemDefinition { id: 1012, name: "Graphene Weave Fabric", category: "Material", rarity: "Uncommon", is_tradable: true, high_alch_value: 140, store_price: 240 },
    ItemDefinition { id: 1013, name: "High-Density Capacitance Cell", category: "Material", rarity: "Rare", is_tradable: true, high_alch_value: 950, store_price: 1700 },
    ItemDefinition { id: 1014, name: "Titanium Alloy Rod", category: "Material", rarity: "Uncommon", is_tradable: true, high_alch_value: 160, store_price: 280 },
    ItemDefinition { id: 1015, name: "Refined Plasma Gel", category: "Material", rarity: "Rare", is_tradable: true, high_alch_value: 1400, store_price: 2500 },
    ItemDefinition { id: 1016, name: "Bionic Nerve Tendril", category: "Material", rarity: "Rare", is_tradable: true, high_alch_value: 1800, store_price: 3200 },
    ItemDefinition { id: 1017, name: "Photonic Prism Lens", category: "Material", rarity: "Epic", is_tradable: true, high_alch_value: 5200, store_price: 9500 },
    ItemDefinition { id: 1018, name: "Tachyon Frequency Emitter", category: "Material", rarity: "Legendary", is_tradable: true, high_alch_value: 30000, store_price: 60000 },
    ItemDefinition { id: 1019, name: "Sub-Zero Liquid Nitrogen", category: "Material", rarity: "Common", is_tradable: true, high_alch_value: 25, store_price: 40 },
    ItemDefinition { id: 1020, name: "Nanite Swarm Canister", category: "Material", rarity: "Epic", is_tradable: true, high_alch_value: 6000, store_price: 11000 },

    // --- CONSUMABLES (1021 - 1040) ---
    ItemDefinition { id: 1021, name: "Nano-Med Hypo I", category: "Consumable", rarity: "Common", is_tradable: true, high_alch_value: 15, store_price: 25 },
    ItemDefinition { id: 1022, name: "Nano-Med Hypo II", category: "Consumable", rarity: "Uncommon", is_tradable: true, high_alch_value: 50, store_price: 90 },
    ItemDefinition { id: 1023, name: "Nano-Med Hypo III", category: "Consumable", rarity: "Rare", is_tradable: true, high_alch_value: 180, store_price: 320 },
    ItemDefinition { id: 1024, name: "Shield Overcharge Cell", category: "Consumable", rarity: "Uncommon", is_tradable: true, high_alch_value: 60, store_price: 110 },
    ItemDefinition { id: 1025, name: "Adrenaline Combat Stim", category: "Consumable", rarity: "Rare", is_tradable: true, high_alch_value: 220, store_price: 400 },
    ItemDefinition { id: 1026, name: "Neural Ram Expander Pill", category: "Consumable", rarity: "Uncommon", is_tradable: true, high_alch_value: 80, store_price: 140 },
    ItemDefinition { id: 1027, name: "Cyberware Degreaser Spray", category: "Consumable", rarity: "Common", is_tradable: true, high_alch_value: 10, store_price: 20 },
    ItemDefinition { id: 1028, name: "Smoke Screen Capsule", category: "Consumable", rarity: "Common", is_tradable: true, high_alch_value: 20, store_price: 35 },
    ItemDefinition { id: 1029, name: "EMP Detonator Pin", category: "Consumable", rarity: "Uncommon", is_tradable: true, high_alch_value: 95, store_price: 160 },
    ItemDefinition { id: 1030, name: "Thermal Grenade", category: "Consumable", rarity: "Uncommon", is_tradable: true, high_alch_value: 95, store_price: 160 },
    ItemDefinition { id: 1031, name: "Cryo Flashbang", category: "Consumable", rarity: "Uncommon", is_tradable: true, high_alch_value: 95, store_price: 160 },
    ItemDefinition { id: 1032, name: "Acid Slime Flask", category: "Consumable", rarity: "Uncommon", is_tradable: true, high_alch_value: 110, store_price: 190 },
    ItemDefinition { id: 1033, name: "Velocity Booster Drink", category: "Consumable", rarity: "Common", is_tradable: true, high_alch_value: 12, store_price: 25 },
    ItemDefinition { id: 1034, name: "Focus Neuro-Noötropic", category: "Consumable", rarity: "Rare", is_tradable: true, high_alch_value: 250, store_price: 450 },
    ItemDefinition { id: 1035, name: "Overclock Liquid Coolant", category: "Consumable", rarity: "Rare", is_tradable: true, high_alch_value: 300, store_price: 550 },
    ItemDefinition { id: 1036, name: "Emergency Teleport Transponder", category: "Consumable", rarity: "Epic", is_tradable: true, high_alch_value: 1500, store_price: 3000 },
    ItemDefinition { id: 1037, name: "Syndicate Bribery Voucher", category: "Consumable", rarity: "Rare", is_tradable: true, high_alch_value: 600, store_price: 1000 },
    ItemDefinition { id: 1038, name: "Synthetic Blood Pouch", category: "Consumable", rarity: "Common", is_tradable: true, high_alch_value: 35, store_price: 60 },
    ItemDefinition { id: 1039, name: "Nanite Armor Patch Kit", category: "Consumable", rarity: "Uncommon", is_tradable: true, high_alch_value: 130, store_price: 220 },
    ItemDefinition { id: 1040, name: "Elixir of Agility", category: "Consumable", rarity: "Epic", is_tradable: true, high_alch_value: 2000, store_price: 4000 },

    // --- WEAPONS (2001 - 2030) ---
    ItemDefinition { id: 2001, name: "Carbon-Nanotube Katana", category: "Weapon", rarity: "Common", is_tradable: true, high_alch_value: 300, store_price: 600 },
    ItemDefinition { id: 2002, name: "Smart Pistol Apex-9", category: "Weapon", rarity: "Common", is_tradable: true, high_alch_value: 350, store_price: 700 },
    ItemDefinition { id: 2003, name: "Shock Baton Stunner", category: "Weapon", rarity: "Uncommon", is_tradable: true, high_alch_value: 750, store_price: 1500 },
    ItemDefinition { id: 2004, name: "Mantis Dual Blades", category: "Weapon", rarity: "Uncommon", is_tradable: true, high_alch_value: 900, store_price: 1800 },
    ItemDefinition { id: 2005, name: "Smart SMG Vector-X", category: "Weapon", rarity: "Uncommon", is_tradable: true, high_alch_value: 1100, store_price: 2200 },
    ItemDefinition { id: 2006, name: "Pulse Assault Rifle Viper", category: "Weapon", rarity: "Rare", is_tradable: true, high_alch_value: 2400, store_price: 4800 },
    ItemDefinition { id: 2007, name: "Plasma Shotgun Breaker", category: "Weapon", rarity: "Rare", is_tradable: true, high_alch_value: 2800, store_price: 5500 },
    ItemDefinition { id: 2008, name: "Kinetic Heavy Gauntlet", category: "Weapon", rarity: "Rare", is_tradable: true, high_alch_value: 3200, store_price: 6400 },
    ItemDefinition { id: 2009, name: "Monofilament Whip Razor", category: "Weapon", rarity: "Rare", is_tradable: true, high_alch_value: 3600, store_price: 7200 },
    ItemDefinition { id: 2010, name: "Charge Railgun Cataclysm", category: "Weapon", rarity: "Epic", is_tradable: true, high_alch_value: 8500, store_price: 17000 },
    ItemDefinition { id: 2011, name: "Tech Sniper Rifle Ghost", category: "Weapon", rarity: "Epic", is_tradable: true, high_alch_value: 9200, store_price: 18500 },
    ItemDefinition { id: 2012, name: "Muramasa High-Frequency Katana", category: "Weapon", rarity: "Epic", is_tradable: true, high_alch_value: 11000, store_price: 22000 },
    ItemDefinition { id: 2013, name: "Thermal Claymore Dragon", category: "Weapon", rarity: "Epic", is_tradable: true, high_alch_value: 12500, store_price: 25000 },
    ItemDefinition { id: 2014, name: "Cryo Beam Projector", category: "Weapon", rarity: "Epic", is_tradable: true, high_alch_value: 9800, store_price: 19500 },
    ItemDefinition { id: 2015, name: "Acid Dart Pistol", category: "Weapon", rarity: "Uncommon", is_tradable: true, high_alch_value: 1000, store_price: 2000 },
    ItemDefinition { id: 2016, name: "Heavy Autocannon Barrage", category: "Weapon", rarity: "Epic", is_tradable: true, high_alch_value: 13500, store_price: 27000 },
    ItemDefinition { id: 2017, name: "EMP Scattergun Pulsar", category: "Weapon", rarity: "Rare", is_tradable: true, high_alch_value: 3000, store_price: 6000 },
    ItemDefinition { id: 2018, name: "Dual Particle SMGs Gemini", category: "Weapon", rarity: "Rare", is_tradable: true, high_alch_value: 3400, store_price: 6800 },
    ItemDefinition { id: 2019, name: "Arc Thrower Tesla", category: "Weapon", rarity: "Rare", is_tradable: true, high_alch_value: 3800, store_price: 7600 },
    ItemDefinition { id: 2020, name: "Singularity Blade Eclipse", category: "Weapon", rarity: "Exotic", is_tradable: true, high_alch_value: 75000, store_price: 150000 },
    ItemDefinition { id: 2021, name: "Heavy Railgun Cataclysm-X", category: "Weapon", rarity: "Exotic", is_tradable: true, high_alch_value: 88000, store_price: 175000 },
    ItemDefinition { id: 2022, name: "Overclocked Plasma Carbine", category: "Weapon", rarity: "Epic", is_tradable: true, high_alch_value: 14000, store_price: 28000 },
    ItemDefinition { id: 2023, name: "Nanite Swarm Launcher", category: "Weapon", rarity: "Legendary", is_tradable: true, high_alch_value: 52000, store_price: 104000 },
    ItemDefinition { id: 2024, name: "Tachyon Lance", category: "Weapon", rarity: "Exotic", is_tradable: true, high_alch_value: 80000, store_price: 160000 },
    ItemDefinition { id: 2025, name: "Void Reaver Scythe", category: "Weapon", rarity: "Exotic", is_tradable: true, high_alch_value: 95000, store_price: 190000 },
    ItemDefinition { id: 2026, name: "Graviton Hammer Goliath", category: "Weapon", rarity: "Exotic", is_tradable: true, high_alch_value: 90000, store_price: 180000 },
    ItemDefinition { id: 2027, name: "Hyper-Velocity Gauss Rifle", category: "Weapon", rarity: "Exotic", is_tradable: true, high_alch_value: 92000, store_price: 184000 },
    ItemDefinition { id: 2028, name: "Glitch SMG Mirage", category: "Weapon", rarity: "Epic", is_tradable: true, high_alch_value: 10000, store_price: 20000 },
    ItemDefinition { id: 2029, name: "Hellfire Flamethrower", category: "Weapon", rarity: "Epic", is_tradable: true, high_alch_value: 12000, store_price: 24000 },
    ItemDefinition { id: 2030, name: "Cosmic Void Cyber-Katana", category: "Weapon", rarity: "Exotic", is_tradable: false, high_alch_value: 250000, store_price: 500000 },

    // --- ARMOR & CYBERWARE (2031 - 2060) ---
    ItemDefinition { id: 2031, name: "Pneumatic Acrobat Boots", category: "Armor", rarity: "Uncommon", is_tradable: true, high_alch_value: 600, store_price: 1200 },
    ItemDefinition { id: 2032, name: "Kinetic Overdrive Boots", category: "Armor", rarity: "Rare", is_tradable: true, high_alch_value: 2200, store_price: 4400 },
    ItemDefinition { id: 2033, name: "Stalker Trenchcoat", category: "Armor", rarity: "Common", is_tradable: true, high_alch_value: 200, store_price: 400 },
    ItemDefinition { id: 2034, name: "Optical Dampener Cloak", category: "Armor", rarity: "Rare", is_tradable: true, high_alch_value: 3500, store_price: 7000 },
    ItemDefinition { id: 2035, name: "Sub-Dermal Ballistic Weave", category: "Cyberware", rarity: "Uncommon", is_tradable: true, high_alch_value: 1200, store_price: 2400 },
    ItemDefinition { id: 2036, name: "Titanium Cranial Plating", category: "Cyberware", rarity: "Rare", is_tradable: true, high_alch_value: 4000, store_price: 8000 },
    ItemDefinition { id: 2037, name: "Nyx Cyber-Deck v1", category: "Cyberware", rarity: "Common", is_tradable: true, high_alch_value: 500, store_price: 1000 },
    ItemDefinition { id: 2038, name: "Nyx Cyber-Deck v2", category: "Cyberware", rarity: "Rare", is_tradable: true, high_alch_value: 4500, store_price: 9000 },
    ItemDefinition { id: 2039, name: "Black Ice Cyberdeck", category: "Cyberware", rarity: "Legendary", is_tradable: true, high_alch_value: 35000, store_price: 70000 },
    ItemDefinition { id: 2040, name: "Hydraulic Grappling Hook", category: "Cyberware", rarity: "Rare", is_tradable: true, high_alch_value: 3000, store_price: 6000 },

    ItemDefinition { id: 2041, name: "Reflex Booster Node", category: "Cyberware", rarity: "Rare", is_tradable: true, high_alch_value: 2600, store_price: 5200 },
    ItemDefinition { id: 2042, name: "Dermal Plating Alpha", category: "Cyberware", rarity: "Uncommon", is_tradable: true, high_alch_value: 900, store_price: 1800 },
    ItemDefinition { id: 2043, name: "Synaptic Relay", category: "Cyberware", rarity: "Uncommon", is_tradable: true, high_alch_value: 1000, store_price: 2000 },
    ItemDefinition { id: 2044, name: "Adrenal Pump", category: "Cyberware", rarity: "Rare", is_tradable: true, high_alch_value: 2800, store_price: 5600 },
    ItemDefinition { id: 2045, name: "Retinal HUD Upgrade", category: "Cyberware", rarity: "Uncommon", is_tradable: true, high_alch_value: 800, store_price: 1600 },
    ItemDefinition { id: 2046, name: "Thermal Dissipator Vest", category: "Armor", rarity: "Rare", is_tradable: true, high_alch_value: 2400, store_price: 4800 },
    ItemDefinition { id: 2047, name: "Impact Absorber Greaves", category: "Armor", rarity: "Rare", is_tradable: true, high_alch_value: 2500, store_price: 5000 },
    ItemDefinition { id: 2048, name: "Void Shield Emitter", category: "Cyberware", rarity: "Epic", is_tradable: true, high_alch_value: 15000, store_price: 30000 },
    ItemDefinition { id: 2049, name: "Graphene Exo-Skeleton", category: "Armor", rarity: "Epic", is_tradable: true, high_alch_value: 16000, store_price: 32000 },
    ItemDefinition { id: 2050, name: "Cyber-Heart Reactor", category: "Cyberware", rarity: "Legendary", is_tradable: true, high_alch_value: 40000, store_price: 80000 },
    ItemDefinition { id: 2051, name: "Overclock Heatsink", category: "Cyberware", rarity: "Rare", is_tradable: true, high_alch_value: 3000, store_price: 6000 },
    ItemDefinition { id: 2052, name: "Neural Accelerator", category: "Cyberware", rarity: "Epic", is_tradable: true, high_alch_value: 17000, store_price: 34000 },
    ItemDefinition { id: 2053, name: "Optical Camouflage Mesh", category: "Armor", rarity: "Epic", is_tradable: true, high_alch_value: 14000, store_price: 28000 },
    ItemDefinition { id: 2054, name: "Reinforced Spine Strut", category: "Cyberware", rarity: "Rare", is_tradable: true, high_alch_value: 3300, store_price: 6600 },
    ItemDefinition { id: 2055, name: "Micro-Thruster Soles", category: "Armor", rarity: "Rare", is_tradable: true, high_alch_value: 3100, store_price: 6200 },
    ItemDefinition { id: 2056, name: "Biometric Recoil Stabilizer", category: "Cyberware", rarity: "Rare", is_tradable: true, high_alch_value: 2900, store_price: 5800 },
    ItemDefinition { id: 2057, name: "Smart-Link Neuro-Grip", category: "Cyberware", rarity: "Uncommon", is_tradable: true, high_alch_value: 1100, store_price: 2200 },
    ItemDefinition { id: 2058, name: "Audio Dampener Bionics", category: "Cyberware", rarity: "Uncommon", is_tradable: true, high_alch_value: 950, store_price: 1900 },
    ItemDefinition { id: 2059, name: "EMP Hardened Chassis", category: "Cyberware", rarity: "Epic", is_tradable: true, high_alch_value: 13000, store_price: 26000 },
    ItemDefinition { id: 2060, name: "Chrono-Dialator Implant", category: "Cyberware", rarity: "Exotic", is_tradable: true, high_alch_value: 85000, store_price: 170000 },

    // --- QUEST CURIOS & LORE RELICS (2061 - 2070), bound to the player ---
    ItemDefinition { id: 2061, name: "Encrypted Datashard", category: "Curio", rarity: "Uncommon", is_tradable: false, high_alch_value: 100, store_price: 200 },
    ItemDefinition { id: 2062, name: "Syndicate Black Ledger", category: "Curio", rarity: "Rare", is_tradable: false, high_alch_value: 500, store_price: 1000 },
    ItemDefinition { id: 2063, name: "Prototype AI Core", category: "Curio", rarity: "Epic", is_tradable: false, high_alch_value: 2500, store_price: 5000 },
    ItemDefinition { id: 2064, name: "Zero Protocol Keycard", category: "Curio", rarity: "Rare", is_tradable: false, high_alch_value: 400, store_price: 800 },
    ItemDefinition { id: 2065, name: "Council Security Token", category: "Curio", rarity: "Epic", is_tradable: false, high_alch_value: 3000, store_price: 6000 },
    ItemDefinition { id: 2066, name: "Corrupted Firmware Drive", category: "Curio", rarity: "Uncommon", is_tradable: false, high_alch_value: 150, store_price: 300 },
    ItemDefinition { id: 2067, name: "Neon Spire Access Chip", category: "Curio", rarity: "Rare", is_tradable: false, high_alch_value: 800, store_price: 1600 },
    ItemDefinition { id: 2068, name: "Decontaminated Sample 99", category: "Curio", rarity: "Rare", is_tradable: false, high_alch_value: 600, store_price: 1200 },
    ItemDefinition { id: 2069, name: "Cyber-Relic of Olympus", category: "Curio", rarity: "Legendary", is_tradable: false, high_alch_value: 20000, store_price: 40000 },
    ItemDefinition { id: 2070, name: "Void Matrix Conduit", category: "Curio", rarity: "Legendary", is_tradable: false, high_alch_value: 25000, store_price: 50000 },

    // --- MASTERY CAPES (2091 - 2099) ---
    ItemDefinition { id: 2091, name: "Cape of Agility Mastery", category: "Cape", rarity: "Exotic", is_tradable: false, high_alch_value: 99000, store_price: 99000 },
    ItemDefinition { id: 2092, name: "Cape of Cyber-Melee Mastery", category: "Cape", rarity: "Exotic", is_tradable: false, high_alch_value: 99000, store_price: 99000 },
    ItemDefinition { id: 2093, name: "Cape of Ballistics Mastery", category: "Cape", rarity: "Exotic", is_tradable: false, high_alch_value: 99000, store_price: 99000 },
    ItemDefinition { id: 2094, name: "Cape of Netrunning Mastery", category: "Cape", rarity: "Exotic", is_tradable: false, high_alch_value: 99000, store_price: 99000 },
    ItemDefinition { id: 2095, name: "Cape of Engineering Mastery", category: "Cape", rarity: "Exotic", is_tradable: false, high_alch_value: 99000, store_price: 99000 },
    ItemDefinition { id: 2096, name: "Cape of Shadow Mastery", category: "Cape", rarity: "Exotic", is_tradable: false, high_alch_value: 99000, store_price: 99000 },
    ItemDefinition { id: 2097, name: "Cape of Endurance Mastery", category: "Cape", rarity: "Exotic", is_tradable: false, high_alch_value: 99000, store_price: 99000 },
    ItemDefinition { id: 2098, name: "Cape of Street-Credit Mastery", category: "Cape", rarity: "Exotic", is_tradable: false, high_alch_value: 99000, store_price: 99000 },
    ItemDefinition { id: 2099, name: "Cape of the Council Destroyer", category: "Cape", rarity: "Exotic", is_tradable: false, high_alch_value: 500000, store_price: 1000000 },
];

/// Number of distinct items the brief enumerates across all ranges.
pub const ITEM_COUNT: usize = 119;

pub fn find_item(id: u32) -> Option<&'static ItemDefinition> {
    MASTER_ITEM_CATALOG.iter().find(|item| item.id == id)
}

/// Case-insensitive substring match on item names.
pub fn search_items_by_name(query: &str) -> Vec<&'static ItemDefinition> {
    let query = query.to_lowercase();
    MASTER_ITEM_CATALOG
        .iter()
        .filter(|item| item.name.to_lowercase().contains(&query))
        .collect()
}

pub fn filter_by_category(category: &str) -> Vec<&'static ItemDefinition> {
    MASTER_ITEM_CATALOG
        .iter()
        .filter(|item| item.category.eq_ignore_ascii_case(category))
        .collect()
}

pub fn filter_by_rarity(rarity: &str) -> Vec<&'static ItemDefinition> {
    MASTER_ITEM_CATALOG
        .iter()
        .filter(|item| item.rarity.eq_ignore_ascii_case(rarity))
        .collect()
}

pub fn catalog_len() -> usize {
    debug_assert_eq!(MASTER_ITEM_CATALOG.len(), ITEM_COUNT);
    MASTER_ITEM_CATALOG.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn count_in(range: std::ops::RangeInclusive<u32>) -> usize {
        MASTER_ITEM_CATALOG
            .iter()
            .filter(|i| range.contains(&i.id))
            .count()
    }

    #[test]
    fn every_enumerated_range_is_complete() {
        assert_eq!(catalog_len(), ITEM_COUNT);
        assert_eq!(count_in(1001..=1020), 20, "raw materials");
        assert_eq!(count_in(1021..=1040), 20, "consumables");
        assert_eq!(count_in(2001..=2030), 30, "weapons");
        assert_eq!(count_in(2031..=2060), 30, "armor and cyberware");
        assert_eq!(count_in(2061..=2070), 10, "curios");
        assert_eq!(count_in(2091..=2099), 9, "capes");
        assert_eq!(20 + 20 + 30 + 30 + 10 + 9, ITEM_COUNT);
    }

    #[test]
    fn ids_and_names_are_unique_and_sorted() {
        let ids: Vec<u32> = MASTER_ITEM_CATALOG.iter().map(|i| i.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids, sorted, "ascending, no duplicates");
        let names: HashSet<_> = MASTER_ITEM_CATALOG.iter().map(|i| i.name).collect();
        assert_eq!(names.len(), ITEM_COUNT);
    }

    #[test]
    fn every_item_is_well_formed() {
        for item in MASTER_ITEM_CATALOG {
            assert!(!item.name.trim().is_empty(), "{}", item.id);
            assert!(
                ITEM_CATEGORIES.contains(&item.category),
                "{} category",
                item.id
            );
            assert!(ITEM_RARITIES.contains(&item.rarity), "{} rarity", item.id);
            assert!(
                item.store_price >= item.high_alch_value,
                "{} prices",
                item.id
            );
            assert!(item.high_alch_value > 0, "{} value", item.id);
        }
    }

    #[test]
    fn bound_items_are_untradable() {
        assert!(
            !find_item(2030).unwrap().is_tradable,
            "Cosmic Void Cyber-Katana"
        );
        for item in MASTER_ITEM_CATALOG {
            if matches!(item.category, "Cape" | "Curio") {
                assert!(!item.is_tradable, "{}", item.name);
            }
        }
        assert!(find_item(1001).unwrap().is_tradable);
    }

    #[test]
    fn spec_spot_checks() {
        let ingot = find_item(1001).unwrap();
        assert_eq!(
            (ingot.name, ingot.high_alch_value, ingot.store_price),
            ("Nanite Alloy Ingot", 30, 50)
        );
        let cape = find_item(2099).unwrap();
        assert_eq!(
            (cape.name, cape.store_price),
            ("Cape of the Council Destroyer", 1_000_000)
        );
        assert_eq!(find_item(2039).unwrap().rarity, "Legendary");
        assert!(find_item(2071).is_none());
        assert!(find_item(0).is_none());
    }

    #[test]
    fn search_and_filters() {
        let katanas = search_items_by_name("katana");
        let ids: Vec<u32> = katanas.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![2001, 2012, 2030]);
        assert_eq!(search_items_by_name("NANO-MED").len(), 3);
        assert!(search_items_by_name("no such thing").is_empty());

        assert_eq!(filter_by_category("Material").len(), 20);
        assert_eq!(filter_by_category("consumable").len(), 20);
        assert_eq!(filter_by_category("Weapon").len(), 30);
        assert_eq!(filter_by_category("Cape").len(), 9);
        assert_eq!(filter_by_category("Curio").len(), 10);
        assert_eq!(
            filter_by_category("Armor").len() + filter_by_category("Cyberware").len(),
            30
        );
        let total: usize = ITEM_RARITIES
            .iter()
            .map(|r| filter_by_rarity(r).len())
            .sum();
        assert_eq!(total, ITEM_COUNT, "every item has exactly one known rarity");
        assert!(filter_by_rarity("Exotic").iter().any(|i| i.id == 2099));
    }
}
