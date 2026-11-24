//! The arsenal: elements, 500 weapon bases and a few thousand weapon
//! mods, all generated from fixed tables so every machine agrees.
//!
//! - Ten weapon classes, each with fifty bases (ten manufacturers, five
//!   models). A base has damage, speed, a tier and an elemental affinity.
//! - Mods come in families (added fire damage, status chance, critical
//!   strikes, sustain...) with ten tiers, and every tier exists once per
//!   weapon class with values tuned to it.
//! - A dropped weapon rolls a rarity, then that many mods from the table,
//!   each with a roll inside its tier's range.

use serde::{Deserialize, Serialize};

/// Damage elements and what they do on hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Element {
    Physical,
    /// Ignites: damage over time.
    Fire,
    /// Chills: slows.
    Frost,
    /// Overloads: stuns cybernetics.
    Shock,
    /// Corrodes: strips armour.
    Toxin,
    /// Pierces: ignores armour.
    Void,
}

impl Element {
    pub const ALL: [Self; 6] = [
        Self::Physical,
        Self::Fire,
        Self::Frost,
        Self::Shock,
        Self::Toxin,
        Self::Void,
    ];
    /// The five non-physical elements.
    pub const ELEMENTAL: [Self; 5] = [
        Self::Fire,
        Self::Frost,
        Self::Shock,
        Self::Toxin,
        Self::Void,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Physical => "Physical",
            Self::Fire => "Fire",
            Self::Frost => "Frost",
            Self::Shock => "Shock",
            Self::Toxin => "Toxin",
            Self::Void => "Void",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Display colour (sRGB).
    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Physical => [220, 224, 230],
            Self::Fire => [255, 120, 50],
            Self::Frost => [110, 200, 255],
            Self::Shock => [250, 230, 80],
            Self::Toxin => [140, 230, 70],
            Self::Void => [180, 110, 255],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponClass {
    Katana,
    DualBlades,
    Baton,
    Gauntlet,
    Whip,
    Claymore,
    Spear,
    Scythe,
    Axe,
    Hammer,
}

impl WeaponClass {
    pub const ALL: [Self; 10] = [
        Self::Katana,
        Self::DualBlades,
        Self::Baton,
        Self::Gauntlet,
        Self::Whip,
        Self::Claymore,
        Self::Spear,
        Self::Scythe,
        Self::Axe,
        Self::Hammer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Katana => "Katana",
            Self::DualBlades => "Dual Blades",
            Self::Baton => "Baton",
            Self::Gauntlet => "Gauntlet",
            Self::Whip => "Mono-Whip",
            Self::Claymore => "Claymore",
            Self::Spear => "Spear",
            Self::Scythe => "Scythe",
            Self::Axe => "Axe",
            Self::Hammer => "Hammer",
        }
    }

    /// Damage per hit relative to a katana, and swings per second.
    pub fn profile(self) -> (f32, f32) {
        match self {
            Self::Katana => (1.0, 1.0),
            Self::DualBlades => (0.62, 1.55),
            Self::Baton => (0.85, 1.15),
            Self::Gauntlet => (1.2, 0.85),
            Self::Whip => (0.8, 1.2),
            Self::Claymore => (1.55, 0.7),
            Self::Spear => (1.1, 0.95),
            Self::Scythe => (1.3, 0.8),
            Self::Axe => (1.35, 0.78),
            Self::Hammer => (1.7, 0.62),
        }
    }

    /// Model names, five per class.
    fn models(self) -> [&'static str; 5] {
        match self {
            Self::Katana => ["Shiden", "Kagerou", "Tsukikage", "Raimei", "Hototogisu"],
            Self::DualBlades => ["Mantis", "Twin Fang", "Sickle Pair", "Scissor", "Wasp"],
            Self::Baton => [
                "Riot Rod",
                "Lineman",
                "Stun Pike",
                "Night Stick",
                "Arc Mace",
            ],
            Self::Gauntlet => [
                "Piston Fist",
                "Breaker",
                "Knuckle Drive",
                "Ram",
                "Jackhammer",
            ],
            Self::Whip => ["Monofilament", "Lash", "Garrote", "Coil", "Razor Line"],
            Self::Claymore => [
                "Greatblade",
                "Executioner",
                "Bulkhead",
                "Cleaver",
                "Longsword",
            ],
            Self::Spear => ["Lance", "Harpoon", "Pike", "Glaive", "Javelin"],
            Self::Scythe => ["Reaper", "Harvester", "Crescent", "Mourner", "Grim Arc"],
            Self::Axe => ["Hatchet", "Bearded Axe", "Splitter", "Breacher", "Tomahawk"],
            Self::Hammer => ["Maul", "Sledge", "Pile Driver", "Anvil", "Wrecker"],
        }
    }
}

/// Weapon makers: a name and an elemental affinity.
const MAKERS: [(&str, Element); 10] = [
    ("Hoshida", Element::Physical),
    ("Vantek", Element::Shock),
    ("Kurogane Works", Element::Fire),
    ("Sable Arms", Element::Void),
    ("Neuroforge", Element::Shock),
    ("Tessera", Element::Frost),
    ("Okami Dynamics", Element::Fire),
    ("Rook & Vale", Element::Physical),
    ("Helix Armory", Element::Toxin),
    ("Mirrorline", Element::Frost),
];

/// A weapon base.
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponBase {
    pub id: u16,
    pub name: String,
    pub class: WeaponClass,
    /// 1 (street) to 10 (prototype).
    pub tier: u8,
    /// Damage multiplier on the combo's base hit.
    pub damage: f32,
    pub attack_speed: f32,
    pub element: Element,
    /// Share of the hit converted to `element` (0 for physical makers).
    pub element_share: f32,
}

/// Every weapon base (500).
pub fn weapon_bases() -> &'static [WeaponBase] {
    static BASES: std::sync::OnceLock<Vec<WeaponBase>> = std::sync::OnceLock::new();
    BASES.get_or_init(|| {
        let mut out = Vec::with_capacity(500);
        for class in WeaponClass::ALL {
            let (damage, speed) = class.profile();
            for (m, &(maker, element)) in MAKERS.iter().enumerate() {
                for (k, model) in class.models().iter().enumerate() {
                    let tier = ((m + k * 2) % 10 + 1) as u8;
                    let scale = 1.0 + (tier as f32 - 1.0) * 0.12;
                    let jitter = (mix((out.len() as u64) << 3) % 100) as f32 / 1000.0;
                    out.push(WeaponBase {
                        id: out.len() as u16,
                        name: format!("{maker} {model}"),
                        class,
                        tier,
                        damage: damage * scale * (0.95 + jitter),
                        attack_speed: speed * (0.97 + jitter * 0.6),
                        element,
                        element_share: if element == Element::Physical {
                            0.0
                        } else {
                            0.2 + (tier as f32) * 0.03
                        },
                    });
                }
            }
        }
        out
    })
}

/// What a mod does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModEffect {
    /// Percent physical damage.
    PhysicalDamage,
    /// Flat damage of an element added to each hit.
    AddedDamage(Element),
    /// Percent damage of an element.
    ElementDamage(Element),
    /// Percent damage of every element.
    ElementalDamage,
    /// Percent chance to apply an element's status on hit.
    StatusChance(Element),
    /// Percent stronger status effects of an element.
    StatusPotency(Element),
    CritChance,
    CritMultiplier,
    LifeOnHit,
    HealOnKill,
    ShieldOnKill,
    /// Percent more damage to bosses.
    BossDamage,
    /// Percent more damage to enemies below a third of their health.
    ExecuteDamage,
    XpGain,
    /// Percent of armour ignored.
    ArmorPierce,
}

impl ModEffect {
    /// Every family, in table order.
    pub fn families() -> Vec<Self> {
        let mut out = vec![Self::PhysicalDamage];
        for e in Element::ELEMENTAL {
            out.push(Self::AddedDamage(e));
            out.push(Self::ElementDamage(e));
        }
        out.push(Self::ElementalDamage);
        for e in [
            Element::Fire,
            Element::Frost,
            Element::Shock,
            Element::Toxin,
        ] {
            out.push(Self::StatusChance(e));
            out.push(Self::StatusPotency(e));
        }
        out.extend([
            Self::CritChance,
            Self::CritMultiplier,
            Self::LifeOnHit,
            Self::HealOnKill,
            Self::ShieldOnKill,
            Self::BossDamage,
            Self::ExecuteDamage,
            Self::XpGain,
            Self::ArmorPierce,
        ]);
        out
    }

    /// Value range of tier 1 (weakest); higher tiers scale it.
    fn base_range(self) -> (f32, f32) {
        match self {
            Self::PhysicalDamage => (6.0, 10.0),
            Self::AddedDamage(_) => (2.0, 4.0),
            Self::ElementDamage(_) => (6.0, 10.0),
            Self::ElementalDamage => (4.0, 7.0),
            Self::StatusChance(_) => (3.0, 5.0),
            Self::StatusPotency(_) => (6.0, 10.0),
            Self::CritChance => (1.0, 2.0),
            Self::CritMultiplier => (8.0, 12.0),
            Self::LifeOnHit => (0.4, 0.8),
            Self::HealOnKill => (2.0, 4.0),
            Self::ShieldOnKill => (2.0, 4.0),
            Self::BossDamage => (4.0, 7.0),
            Self::ExecuteDamage => (6.0, 10.0),
            Self::XpGain => (2.0, 4.0),
            Self::ArmorPierce => (4.0, 7.0),
        }
    }

    /// Prefixes add damage; suffixes do the rest.
    pub fn is_prefix(self) -> bool {
        matches!(
            self,
            Self::PhysicalDamage
                | Self::AddedDamage(_)
                | Self::ElementDamage(_)
                | Self::ElementalDamage
        )
    }

    pub fn describe(self, value: f32) -> String {
        match self {
            Self::PhysicalDamage => format!("+{value:.0}% physical damage"),
            Self::AddedDamage(e) => format!("Adds {value:.0} {} damage", e.label().to_lowercase()),
            Self::ElementDamage(e) => format!("+{value:.0}% {} damage", e.label().to_lowercase()),
            Self::ElementalDamage => format!("+{value:.0}% elemental damage"),
            Self::StatusChance(e) => format!("{value:.0}% chance to {}", status_verb(e)),
            Self::StatusPotency(e) => format!("+{value:.0}% {} effect", status_noun(e)),
            Self::CritChance => format!("+{value:.1}% critical strike chance"),
            Self::CritMultiplier => format!("+{value:.0}% critical strike damage"),
            Self::LifeOnHit => format!("+{value:.1} health on hit"),
            Self::HealOnKill => format!("+{value:.0} health on kill"),
            Self::ShieldOnKill => format!("+{value:.0} shields on kill"),
            Self::BossDamage => format!("+{value:.0}% damage to bosses"),
            Self::ExecuteDamage => format!("+{value:.0}% damage to wounded enemies"),
            Self::XpGain => format!("+{value:.0}% experience gained"),
            Self::ArmorPierce => format!("Ignores {value:.0}% of armour"),
        }
    }

    /// Tier names, weakest first; prefixes read "<name> Katana", suffixes
    /// "Katana <name>".
    fn tier_name(self, tier: u8) -> String {
        let t = usize::from(tier.saturating_sub(1)).min(9);
        let words: [&str; 10] = match self {
            Self::PhysicalDamage => [
                "Sharp",
                "Honed",
                "Keen",
                "Tempered",
                "Brutal",
                "Vicious",
                "Savage",
                "Merciless",
                "Butchering",
                "Annihilating",
            ],
            Self::AddedDamage(Element::Fire) | Self::ElementDamage(Element::Fire) => [
                "Warm",
                "Smouldering",
                "Burning",
                "Scorching",
                "Searing",
                "Blazing",
                "Infernal",
                "Solar",
                "Plasma",
                "Stellar",
            ],
            Self::AddedDamage(Element::Frost) | Self::ElementDamage(Element::Frost) => [
                "Cool",
                "Chilled",
                "Frosted",
                "Icy",
                "Frozen",
                "Glacial",
                "Polar",
                "Arctic",
                "Absolute",
                "Cryogenic",
            ],
            Self::AddedDamage(Element::Shock) | Self::ElementDamage(Element::Shock) => [
                "Static",
                "Humming",
                "Sparking",
                "Charged",
                "Crackling",
                "Arcing",
                "Thundering",
                "Overloaded",
                "Tesla",
                "Lightning",
            ],
            Self::AddedDamage(Element::Toxin) | Self::ElementDamage(Element::Toxin) => [
                "Tainted",
                "Sour",
                "Noxious",
                "Septic",
                "Venomous",
                "Caustic",
                "Virulent",
                "Corrosive",
                "Plague",
                "Necrotic",
            ],
            Self::AddedDamage(_) | Self::ElementDamage(_) => [
                "Hollow",
                "Dim",
                "Shaded",
                "Umbral",
                "Null",
                "Abyssal",
                "Eclipsed",
                "Voidborn",
                "Singular",
                "Event-Horizon",
            ],
            Self::ElementalDamage => [
                "Tinted",
                "Hued",
                "Prismatic",
                "Spectral",
                "Chromatic",
                "Iridescent",
                "Radiant",
                "Resplendent",
                "Kaleidoscopic",
                "Rainbow-Forged",
            ],
            _ => [
                "of the Street",
                "of the Alley",
                "of the Block",
                "of the Ward",
                "of the District",
                "of the Syndicate",
                "of the Spire",
                "of the Skyline",
                "of the Grid",
                "of Zero Protocol",
            ],
        };
        let family = match self {
            Self::StatusChance(e) => format!("{} ", status_noun(e)),
            Self::StatusPotency(e) => format!("Deep {} ", status_noun(e)),
            Self::CritChance => "Precision ".into(),
            Self::CritMultiplier => "Lethality ".into(),
            Self::LifeOnHit => "Leech ".into(),
            Self::HealOnKill => "Harvest ".into(),
            Self::ShieldOnKill => "Spark ".into(),
            Self::BossDamage => "Slayer ".into(),
            Self::ExecuteDamage => "Executioner ".into(),
            Self::XpGain => "Scholar ".into(),
            Self::ArmorPierce => "Piercer ".into(),
            _ => String::new(),
        };
        format!("{family}{}", words[t])
    }
}

fn status_verb(e: Element) -> &'static str {
    match e {
        Element::Fire => "ignite",
        Element::Frost => "chill",
        Element::Shock => "overload",
        Element::Toxin => "corrode",
        _ => "pierce",
    }
}

fn status_noun(e: Element) -> &'static str {
    match e {
        Element::Fire => "Ignite",
        Element::Frost => "Chill",
        Element::Shock => "Overload",
        Element::Toxin => "Corrosion",
        _ => "Pierce",
    }
}

/// One mod tier for one weapon class.
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponMod {
    pub id: u32,
    pub name: String,
    pub effect: ModEffect,
    pub class: WeaponClass,
    pub tier: u8,
    pub min: f32,
    pub max: f32,
}

impl WeaponMod {
    pub fn value(&self, roll: f32) -> f32 {
        let v = self.min + (self.max - self.min) * roll.clamp(0.0, 1.0);
        (v * 10.0).round() / 10.0
    }
}

/// Every weapon mod (families x 10 tiers x 10 classes).
pub fn weapon_mods() -> &'static [WeaponMod] {
    static MODS: std::sync::OnceLock<Vec<WeaponMod>> = std::sync::OnceLock::new();
    MODS.get_or_init(|| {
        let mut out = Vec::new();
        for class in WeaponClass::ALL {
            let (damage, speed) = class.profile();
            for effect in ModEffect::families() {
                for tier in 1..=10u8 {
                    let (lo, hi) = effect.base_range();
                    let growth = 1.0 + (tier as f32 - 1.0) * 0.55;
                    // Slow, heavy weapons get bigger flat damage per hit;
                    // fast ones bigger on-hit effects per swing feel.
                    let class_scale = match effect {
                        ModEffect::AddedDamage(_) => damage,
                        ModEffect::LifeOnHit => 1.0 / speed.max(0.5),
                        _ => 1.0,
                    };
                    out.push(WeaponMod {
                        id: out.len() as u32,
                        name: effect.tier_name(tier),
                        effect,
                        class,
                        tier,
                        min: lo * growth * class_scale,
                        max: hi * growth * class_scale,
                    });
                }
            }
        }
        out
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Enhanced,
    Rare,
    Prototype,
}

impl Rarity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Common => "Common",
            Self::Enhanced => "Enhanced",
            Self::Rare => "Rare",
            Self::Prototype => "Prototype",
        }
    }

    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Common => [200, 206, 214],
            Self::Enhanced => [90, 170, 255],
            Self::Rare => [255, 210, 70],
            Self::Prototype => [255, 110, 60],
        }
    }

    /// Mods a weapon of this rarity rolls.
    fn mod_count(self, roll: u64) -> usize {
        match self {
            Self::Common => 0,
            Self::Enhanced => 1 + (roll % 2) as usize,
            Self::Rare => 3 + (roll % 2) as usize,
            Self::Prototype => 5 + (roll % 2) as usize,
        }
    }
}

/// A weapon the player owns: a base, a rarity and rolled mods.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponItem {
    pub base: u16,
    pub rarity: Rarity,
    /// Mod ids and their rolls (0..1).
    pub mods: Vec<(u32, f32)>,
}

/// SplitMix64.
pub fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl WeaponItem {
    /// The starter: a common Hoshida Shiden katana.
    pub fn starter() -> Self {
        Self {
            base: 0,
            rarity: Rarity::Common,
            mods: Vec::new(),
        }
    }

    pub fn base(&self) -> &'static WeaponBase {
        let bases = weapon_bases();
        &bases[usize::from(self.base).min(bases.len() - 1)]
    }

    /// Rolls a drop. `level` (1..) pushes tiers and rarity up; `bonus`
    /// raises the rarity floor (bosses).
    pub fn roll(seed: u64, level: u32, bonus: Rarity) -> Self {
        let mut s = mix(seed);
        let mut next = || {
            s = mix(s);
            s
        };
        let bases = weapon_bases();
        // Tiers open up with level: a level-1 drop is tier 1-3.
        let max_tier = (3 + level / 2).min(10) as u8;
        let candidates: Vec<&WeaponBase> = bases.iter().filter(|b| b.tier <= max_tier).collect();
        let base = candidates[(next() % candidates.len() as u64) as usize];
        let r = next() % 1000;
        let luck = (level.min(30) * 6) as u64;
        let rarity = if r < 15 + luck / 3 {
            Rarity::Prototype
        } else if r < 120 + luck {
            Rarity::Rare
        } else if r < 420 + luck {
            Rarity::Enhanced
        } else {
            Rarity::Common
        }
        .max(bonus);
        let count = rarity.mod_count(next());
        let pool: Vec<&WeaponMod> = weapon_mods()
            .iter()
            .filter(|m| m.class == base.class && m.tier <= max_tier)
            .collect();
        let mut mods: Vec<(u32, f32)> = Vec::new();
        let mut prefixes = 0;
        while mods.len() < count {
            let m = pool[(next() % pool.len() as u64) as usize];
            // One mod per family, at most three prefixes.
            let taken = mods
                .iter()
                .any(|(id, _)| weapon_mods()[*id as usize].effect == m.effect);
            if taken || (m.effect.is_prefix() && prefixes >= 3) {
                continue;
            }
            prefixes += usize::from(m.effect.is_prefix());
            let roll = (next() % 1000) as f32 / 999.0;
            mods.push((m.id, roll));
        }
        Self {
            base: base.id,
            rarity,
            mods,
        }
    }

    /// Display name: commons use the base name; enhanced weapons wear their
    /// best prefix and suffix; rares and prototypes get a coined name.
    pub fn name(&self) -> String {
        let base = &self.base().name;
        let mods = weapon_mods();
        match self.rarity {
            Rarity::Common => base.clone(),
            Rarity::Enhanced => {
                let prefix = self
                    .mods
                    .iter()
                    .map(|(id, _)| &mods[*id as usize])
                    .find(|m| m.effect.is_prefix());
                let suffix = self
                    .mods
                    .iter()
                    .map(|(id, _)| &mods[*id as usize])
                    .find(|m| !m.effect.is_prefix());
                match (prefix, suffix) {
                    (Some(p), Some(s)) => format!("{} {base} ({})", p.name, s.name),
                    (Some(p), None) => format!("{} {base}", p.name),
                    (None, Some(s)) => format!("{base} ({})", s.name),
                    (None, None) => base.clone(),
                }
            }
            Rarity::Rare | Rarity::Prototype => {
                const FIRST: [&str; 12] = [
                    "Ghost", "Neon", "Iron", "Blood", "Storm", "Ash", "Glass", "Night", "Static",
                    "Hollow", "Crimson", "Silent",
                ];
                const SECOND: [&str; 12] = [
                    "Verdict", "Requiem", "Oath", "Hunger", "Bloom", "Signal", "Fang", "Debt",
                    "Choir", "Tide", "Halo", "Ledger",
                ];
                let h = mix(self.mods.iter().fold(u64::from(self.base), |a, (id, _)| {
                    a.wrapping_mul(31).wrapping_add(u64::from(*id))
                }));
                format!(
                    "{} {}  //  {base}",
                    FIRST[(h % 12) as usize],
                    SECOND[((h >> 8) % 12) as usize]
                )
            }
        }
    }

    /// The weapon's effect on combat.
    pub fn stats(&self) -> WeaponStats {
        let base = self.base();
        let mut s = WeaponStats {
            damage: base.damage,
            attack_speed: base.attack_speed,
            element: base.element,
            element_share: base.element_share,
            crit_chance: 5.0,
            crit_multiplier: 150.0,
            ..Default::default()
        };
        for &(id, roll) in &self.mods {
            let Some(m) = weapon_mods().get(id as usize) else {
                continue;
            };
            let v = m.value(roll);
            match m.effect {
                ModEffect::PhysicalDamage => s.physical_pct += v,
                ModEffect::AddedDamage(e) => s.added[e.index()] += v,
                ModEffect::ElementDamage(e) => s.element_pct[e.index()] += v,
                ModEffect::ElementalDamage => s.elemental_pct += v,
                ModEffect::StatusChance(e) => s.status_chance[e.index()] += v,
                ModEffect::StatusPotency(e) => s.status_potency[e.index()] += v,
                ModEffect::CritChance => s.crit_chance += v,
                ModEffect::CritMultiplier => s.crit_multiplier += v,
                ModEffect::LifeOnHit => s.life_on_hit += v,
                ModEffect::HealOnKill => s.heal_on_kill += v,
                ModEffect::ShieldOnKill => s.shield_on_kill += v,
                ModEffect::BossDamage => s.boss_pct += v,
                ModEffect::ExecuteDamage => s.execute_pct += v,
                ModEffect::XpGain => s.xp_pct += v,
                ModEffect::ArmorPierce => s.armor_pierce += v,
            }
        }
        // Elemental weapons always have a base chance to apply their status.
        if base.element != Element::Physical {
            s.status_chance[base.element.index()] += 10.0;
        }
        s
    }

    /// Mod lines for tooltips.
    pub fn mod_lines(&self) -> Vec<(String, bool)> {
        self.mods
            .iter()
            .filter_map(|&(id, roll)| {
                let m = weapon_mods().get(id as usize)?;
                Some((
                    format!("{}  [T{}]", m.effect.describe(m.value(roll)), m.tier),
                    m.effect.is_prefix(),
                ))
            })
            .collect()
    }
}

/// Combat numbers of an equipped weapon.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WeaponStats {
    /// Multiplier on the combo's base hit.
    pub damage: f32,
    pub attack_speed: f32,
    pub element: Element,
    pub element_share: f32,
    pub physical_pct: f32,
    /// Flat damage added per hit, by element index.
    pub added: [f32; 6],
    pub element_pct: [f32; 6],
    pub elemental_pct: f32,
    /// Percent chance per hit to apply each element's status.
    pub status_chance: [f32; 6],
    pub status_potency: [f32; 6],
    pub crit_chance: f32,
    pub crit_multiplier: f32,
    pub life_on_hit: f32,
    pub heal_on_kill: f32,
    pub shield_on_kill: f32,
    pub boss_pct: f32,
    pub execute_pct: f32,
    pub xp_pct: f32,
    pub armor_pierce: f32,
}

impl Default for Element {
    fn default() -> Self {
        Self::Physical
    }
}

/// One hit's damage split by element.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HitDamage {
    pub by_element: [f32; 6],
    pub critical: bool,
}

impl HitDamage {
    pub fn total(&self) -> f32 {
        self.by_element.iter().sum()
    }
}

impl WeaponStats {
    /// Damage of one hit whose combo base is `base`, with the wielder's
    /// extra percent per element (`bonus_pct`, from the passive tree).
    /// `crit_roll` is uniform 0..100.
    pub fn hit(&self, base: f32, bonus_pct: &[f32; 6], crit_roll: f32) -> HitDamage {
        let mut out = HitDamage::default();
        let raw = base * self.damage;
        let elemental = raw * self.element_share;
        out.by_element[Element::Physical.index()] =
            (raw - elemental) * (1.0 + self.physical_pct / 100.0);
        out.by_element[self.element.index()] += elemental;
        for e in Element::ELEMENTAL {
            let i = e.index();
            out.by_element[i] += self.added[i];
            out.by_element[i] *=
                1.0 + (self.element_pct[i] + self.elemental_pct + bonus_pct[i]) / 100.0;
        }
        out.by_element[0] *= 1.0 + bonus_pct[0] / 100.0;
        if crit_roll < self.crit_chance {
            out.critical = true;
            let m = self.crit_multiplier / 100.0;
            for v in &mut out.by_element {
                *v *= m;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn five_hundred_unique_bases_across_ten_classes() {
        let bases = weapon_bases();
        assert_eq!(bases.len(), 500);
        let names: HashSet<(&str, WeaponClass)> =
            bases.iter().map(|b| (b.name.as_str(), b.class)).collect();
        assert_eq!(names.len(), 500);
        for class in WeaponClass::ALL {
            assert_eq!(bases.iter().filter(|b| b.class == class).count(), 50);
        }
        assert!(bases.iter().all(|b| (1..=10).contains(&b.tier)));
        assert!(Element::ALL
            .iter()
            .all(|e| bases.iter().any(|b| b.element == *e)));
    }

    #[test]
    fn thousands_of_mods_with_rising_tiers() {
        let mods = weapon_mods();
        assert!(mods.len() >= 2000, "{}", mods.len());
        for pair in mods.windows(2) {
            if pair[0].effect == pair[1].effect && pair[0].class == pair[1].class {
                assert!(pair[1].min > pair[0].min, "{:?}", pair[1]);
            }
        }
        assert!(mods
            .iter()
            .all(|m| m.min > 0.0 && m.max > m.min && !m.name.is_empty()));
    }

    #[test]
    fn drops_follow_rarity_and_mod_rules() {
        let mut seen = HashSet::new();
        for seed in 0..2000u64 {
            let w = WeaponItem::roll(seed, 12, Rarity::Common);
            seen.insert(w.rarity);
            let class = w.base().class;
            let families: HashSet<ModEffect> = w
                .mods
                .iter()
                .map(|(id, _)| weapon_mods()[*id as usize].effect)
                .collect();
            assert_eq!(families.len(), w.mods.len(), "one mod per family");
            assert!(w.mods.iter().all(|(id, r)| {
                weapon_mods()[*id as usize].class == class && (0.0..=1.0).contains(r)
            }));
            let prefixes = families.iter().filter(|e| e.is_prefix()).count();
            assert!(prefixes <= 3);
            assert!(!w.name().is_empty());
        }
        assert_eq!(seen.len(), 4, "every rarity drops");
        let boss = WeaponItem::roll(7, 1, Rarity::Rare);
        assert!(boss.rarity >= Rarity::Rare);
        assert_eq!(
            WeaponItem::roll(42, 5, Rarity::Common),
            WeaponItem::roll(42, 5, Rarity::Common)
        );
    }

    #[test]
    fn hits_split_by_element_and_crit() {
        let fire = weapon_bases()
            .iter()
            .find(|b| b.element == Element::Fire)
            .expect("a fire base");
        let item = WeaponItem {
            base: fire.id,
            rarity: Rarity::Common,
            mods: Vec::new(),
        };
        let s = item.stats();
        let hit = s.hit(100.0, &[0.0; 6], 99.0);
        assert!(!hit.critical);
        assert!(hit.by_element[Element::Fire.index()] > 0.0);
        assert!(hit.by_element[Element::Physical.index()] > 0.0);
        let crit = s.hit(100.0, &[0.0; 6], 0.0);
        assert!(crit.critical);
        assert!((crit.total() - hit.total() * 1.5).abs() < 1e-3);
        assert!(s.status_chance[Element::Fire.index()] >= 10.0);
        let starter = WeaponItem::starter();
        assert_eq!(starter.base().class, WeaponClass::Katana);
        assert!(
            (starter.stats().hit(10.0, &[0.0; 6], 99.0).total() - 10.0 * starter.base().damage)
                .abs()
                < 1e-4
        );
    }
}
