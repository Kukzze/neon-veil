//! The Neural Lattice: a passive tree of several thousand permanent
//! upgrades. Six sectors radiate from a central origin (Blade, Ghost,
//! Bulwark, Aegis, Reaver, Pulse). Each sector opens at a gateway, then
//! spreads over ten rings of clusters: a named notable inside a ring of
//! small nodes. Paths join each cluster to the ring before it and to its
//! neighbours, bridges cross between sectors, and each sector ends in two
//! keystones that trade a weakness for a strength.
//!
//! The tree is generated from fixed tables with an integer hash, so it is
//! identical on every machine and node ids stay stable for saves.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// What a node changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PassiveStat {
    /// Flat maximum health.
    MaxHealth,
    /// Percent maximum health.
    MaxHealthPercent,
    /// Flat maximum shields.
    MaxShields,
    /// Percent maximum shields.
    MaxShieldsPercent,
    /// Percent shield recharge rate.
    ShieldRecharge,
    /// Percent shorter delay before shields recharge.
    ShieldDelay,
    /// Percent less damage taken.
    DamageReduction,
    /// Percent melee damage.
    MeleeDamage,
    /// Flat stamina.
    Stamina,
    /// Health restored on each kill.
    HealOnKill,
    /// Shields restored on each kill.
    ShieldOnKill,
    /// Health restored on each melee hit.
    LifeOnHit,
    /// Health regenerated per second.
    HealthRegen,
    /// Percent more experience.
    XpGain,
    /// Percent fire damage.
    FireDamage,
    /// Percent frost damage.
    FrostDamage,
    /// Percent shock damage.
    ShockDamage,
    /// Percent toxin damage.
    ToxinDamage,
    /// Percent void damage.
    VoidDamage,
    /// Percent damage of every element.
    ElementalDamage,
}

impl PassiveStat {
    pub fn describe(self, value: f32) -> String {
        let sign = if value >= 0.0 { "+" } else { "-" };
        let v = value.abs();
        let pct = |what: &str| format!("{sign}{v:.0}% {what}");
        let flat = |what: &str| format!("{sign}{v:.0} {what}");
        match self {
            Self::MaxHealth => flat("maximum health"),
            Self::MaxHealthPercent => pct("maximum health"),
            Self::MaxShields => flat("maximum shields"),
            Self::MaxShieldsPercent => pct("maximum shields"),
            Self::ShieldRecharge => pct("shield recharge rate"),
            Self::ShieldDelay => pct("faster shield recharge start"),
            Self::DamageReduction => pct("less damage taken"),
            Self::MeleeDamage => pct("melee damage"),
            Self::Stamina => flat("stamina"),
            Self::HealOnKill => flat("health on kill"),
            Self::ShieldOnKill => flat("shields on kill"),
            Self::LifeOnHit => format!("{sign}{v:.1} health on hit"),
            Self::HealthRegen => format!("{sign}{v:.1} health regenerated per second"),
            Self::XpGain => pct("experience gained"),
            Self::FireDamage => pct("fire damage"),
            Self::FrostDamage => pct("frost damage"),
            Self::ShockDamage => pct("shock damage"),
            Self::ToxinDamage => pct("toxin damage"),
            Self::VoidDamage => pct("void damage"),
            Self::ElementalDamage => pct("elemental damage"),
        }
    }

    /// Name of a small node that grants this stat.
    fn small_name(self) -> &'static str {
        match self {
            Self::MaxHealth => "Dense Tissue",
            Self::MaxHealthPercent => "Hardened Frame",
            Self::MaxShields => "Field Cell",
            Self::MaxShieldsPercent => "Field Amplifier",
            Self::ShieldRecharge => "Fast Charge",
            Self::ShieldDelay => "Quick Cycle",
            Self::DamageReduction => "Plated Weave",
            Self::MeleeDamage => "Edge Graft",
            Self::Stamina => "Lung Mod",
            Self::HealOnKill => "Harvester",
            Self::ShieldOnKill => "Spark Tap",
            Self::LifeOnHit => "Siphon Node",
            Self::HealthRegen => "Nano Swarm",
            Self::XpGain => "Pattern Cache",
            Self::FireDamage => "Thermal Coil",
            Self::FrostDamage => "Cryo Vein",
            Self::ShockDamage => "Arc Relay",
            Self::ToxinDamage => "Venom Gland",
            Self::VoidDamage => "Null Shard",
            Self::ElementalDamage => "Prism Core",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Origin,
    Small,
    Notable,
    Keystone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Branch {
    Core,
    Blade,
    Ghost,
    Bulwark,
    Aegis,
    Reaver,
    Pulse,
}

impl Branch {
    pub const SECTORS: [Self; 6] = [
        Self::Blade,
        Self::Ghost,
        Self::Bulwark,
        Self::Aegis,
        Self::Reaver,
        Self::Pulse,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Core => "Core",
            Self::Blade => "Blade",
            Self::Ghost => "Ghost",
            Self::Bulwark => "Bulwark",
            Self::Aegis => "Aegis",
            Self::Reaver => "Reaver",
            Self::Pulse => "Pulse",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PassiveNode {
    /// Stable id, saved with progress.
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    pub branch: Branch,
    /// Layout position, in tree units (the origin at 0, 0).
    pub position: Vec2,
    pub stats: Vec<(PassiveStat, f32)>,
}

/// The tree's nodes and links.
#[derive(Resource, Debug, Clone)]
pub struct PassiveTree {
    pub nodes: Vec<PassiveNode>,
    pub links: Vec<(usize, usize)>,
    index: HashMap<String, usize>,
    neighbours: Vec<Vec<usize>>,
}

pub const ORIGIN_ID: &str = "origin";

/// Rings of clusters per sector, and the radius step between them.
const RINGS: usize = 12;
const FIRST_RING: f32 = 420.0;
const RING_STEP: f32 = 240.0;
/// Half the angle a sector spans, degrees.
const SECTOR_SPREAD: f32 = 25.0;
const CLUSTER_RADIUS: f32 = 64.0;

/// Per sector: its stat pool with small-node values, and the words its
/// notables are named from.
struct SectorTable {
    branch: Branch,
    pool: &'static [(PassiveStat, f32)],
    adjectives: [&'static str; 10],
    nouns: [&'static str; 10],
    keystones: [(&'static str, &'static [(PassiveStat, f32)]); 2],
}

const SECTORS: [SectorTable; 6] = [
    SectorTable {
        branch: Branch::Blade,
        pool: &[
            (PassiveStat::MeleeDamage, 4.0),
            (PassiveStat::LifeOnHit, 0.8),
            (PassiveStat::FireDamage, 6.0),
        ],
        adjectives: [
            "Crimson", "Honed", "Severing", "Red", "Keen", "Bleeding", "Last", "Clean", "Hundred",
            "Iron",
        ],
        nouns: [
            "Arc",
            "Edge",
            "Rhythm",
            "Draw",
            "Choir",
            "Rain",
            "Cut",
            "Kill",
            "Oath",
            "Whetstone",
        ],
        keystones: [
            (
                "Glass Circuit",
                &[
                    (PassiveStat::MeleeDamage, 45.0),
                    (PassiveStat::MaxHealthPercent, -30.0),
                ],
            ),
            (
                "Berserk Firmware",
                &[
                    (PassiveStat::LifeOnHit, 5.0),
                    (PassiveStat::MaxShieldsPercent, -40.0),
                ],
            ),
        ],
    },
    SectorTable {
        branch: Branch::Ghost,
        pool: &[
            (PassiveStat::Stamina, 6.0),
            (PassiveStat::ShieldDelay, 4.0),
            (PassiveStat::ShockDamage, 6.0),
            (PassiveStat::XpGain, 3.0),
        ],
        adjectives: [
            "Quiet", "Phantom", "Static", "Neon", "Dead", "Zero", "Glass", "Second", "Silent",
            "Pale",
        ],
        nouns: [
            "Step", "Image", "Wind", "Stream", "Air", "Weight", "Lungs", "Gait", "Shadow", "Calm",
        ],
        keystones: [
            (
                "Phase Walker",
                &[
                    (PassiveStat::ShieldDelay, 50.0),
                    (PassiveStat::MaxHealthPercent, -20.0),
                ],
            ),
            (
                "Storm Conduit",
                &[
                    (PassiveStat::ShockDamage, 60.0),
                    (PassiveStat::DamageReduction, -15.0),
                ],
            ),
        ],
    },
    SectorTable {
        branch: Branch::Bulwark,
        pool: &[
            (PassiveStat::MaxHealth, 8.0),
            (PassiveStat::MaxHealthPercent, 3.0),
            (PassiveStat::DamageReduction, 1.5),
            (PassiveStat::FrostDamage, 6.0),
        ],
        adjectives: [
            "Iron",
            "Scarred",
            "Deep",
            "Stone",
            "Riot",
            "Unbent",
            "Weathered",
            "Load",
            "Granite",
            "Old",
        ],
        nouns: [
            "Spine", "Bastion", "Hide", "Roots", "Heart", "Frame", "Line", "Wall", "Anchor", "Keel",
        ],
        keystones: [
            (
                "Juggernaut Core",
                &[
                    (PassiveStat::DamageReduction, 25.0),
                    (PassiveStat::Stamina, -50.0),
                ],
            ),
            (
                "Unyielding",
                &[
                    (PassiveStat::MaxHealthPercent, 60.0),
                    (PassiveStat::MaxShieldsPercent, -50.0),
                ],
            ),
        ],
    },
    SectorTable {
        branch: Branch::Aegis,
        pool: &[
            (PassiveStat::MaxShields, 8.0),
            (PassiveStat::MaxShieldsPercent, 3.0),
            (PassiveStat::ShieldRecharge, 5.0),
            (PassiveStat::VoidDamage, 6.0),
        ],
        adjectives: [
            "Halo",
            "Mirror",
            "Ion",
            "Prism",
            "Bright",
            "Flux",
            "Lattice",
            "Feedback",
            "Capacitor",
            "Silver",
        ],
        nouns: [
            "Array", "Field", "Skin", "Bank", "Charge", "Loop", "Ward", "Shell", "Barrier", "Veil",
        ],
        keystones: [
            (
                "Iron Lung",
                &[
                    (PassiveStat::MaxShieldsPercent, 60.0),
                    (PassiveStat::ShieldRecharge, -30.0),
                ],
            ),
            (
                "Overclocked Halo",
                &[
                    (PassiveStat::ShieldRecharge, 80.0),
                    (PassiveStat::MaxShieldsPercent, -25.0),
                ],
            ),
        ],
    },
    SectorTable {
        branch: Branch::Reaver,
        pool: &[
            (PassiveStat::HealOnKill, 2.5),
            (PassiveStat::ShieldOnKill, 3.0),
            (PassiveStat::ToxinDamage, 6.0),
            (PassiveStat::MeleeDamage, 3.0),
        ],
        adjectives: [
            "Blood", "Grave", "Carrion", "Marrow", "Black", "Red", "Hungry", "Leech", "Debt", "Rot",
        ],
        nouns: [
            "Tithe",
            "Harvest",
            "Ledger",
            "Code",
            "Tap",
            "Feed",
            "Collector",
            "Market",
            "Protocol",
            "Hunger",
        ],
        keystones: [
            (
                "Vampiric Protocol",
                &[
                    (PassiveStat::HealOnKill, 20.0),
                    (PassiveStat::MaxShieldsPercent, -35.0),
                ],
            ),
            (
                "Blood Pact",
                &[
                    (PassiveStat::LifeOnHit, 4.0),
                    (PassiveStat::MaxHealthPercent, -20.0),
                ],
            ),
        ],
    },
    SectorTable {
        branch: Branch::Pulse,
        pool: &[
            (PassiveStat::HealthRegen, 0.6),
            (PassiveStat::ShieldRecharge, 4.0),
            (PassiveStat::ElementalDamage, 4.0),
            (PassiveStat::XpGain, 3.0),
        ],
        adjectives: [
            "Steady", "Nano", "Clean", "Adrenal", "Cold", "Green", "Second", "Patched", "Living",
            "Bright",
        ],
        nouns: [
            "Pulse",
            "Mending",
            "Signal",
            "Drip",
            "Boot",
            "Light",
            "Heartbeat",
            "Rebuild",
            "Mode",
            "Bloom",
        ],
        keystones: [
            (
                "Perpetual Engine",
                &[
                    (PassiveStat::HealthRegen, 4.0),
                    (PassiveStat::MeleeDamage, -20.0),
                ],
            ),
            (
                "Mind Over Matter",
                &[
                    (PassiveStat::XpGain, 25.0),
                    (PassiveStat::MaxHealthPercent, -15.0),
                ],
            ),
        ],
    },
];

/// Integer hash for stable choices (SplitMix64).
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// One cluster: its notable and the ring of small nodes around it.
struct Cluster {
    center: Vec2,
    ring: Vec<usize>,
}

struct Builder {
    nodes: Vec<PassiveNode>,
    links: Vec<(usize, usize)>,
}

impl Builder {
    fn add(
        &mut self,
        name: String,
        kind: NodeKind,
        branch: Branch,
        position: Vec2,
        stats: Vec<(PassiveStat, f32)>,
    ) -> usize {
        let id = format!("n{}", self.nodes.len());
        self.nodes.push(PassiveNode {
            id,
            name,
            kind,
            branch,
            position,
            stats,
        });
        self.nodes.len() - 1
    }

    fn link(&mut self, a: usize, b: usize) {
        if a != b && !self.links.contains(&(a, b)) && !self.links.contains(&(b, a)) {
            self.links.push((a, b));
        }
    }

    /// A small node with one stat from `pool`, scaled by `power`.
    fn small(
        &mut self,
        branch: Branch,
        position: Vec2,
        pool: &[(PassiveStat, f32)],
        pick: u64,
        power: f32,
    ) -> usize {
        let (stat, value) = pool[(pick % pool.len() as u64) as usize];
        self.add(
            stat.small_name().into(),
            NodeKind::Small,
            branch,
            position,
            vec![(stat, round_stat(value * power))],
        )
    }

    /// Joins `from` to `to` with small nodes spaced about 90 units apart.
    fn path(
        &mut self,
        from: usize,
        to: usize,
        branch: Branch,
        pool: &[(PassiveStat, f32)],
        seed: u64,
        power: f32,
    ) {
        let (a, b) = (self.nodes[from].position, self.nodes[to].position);
        let steps = ((a.distance(b) / 90.0).round() as usize).clamp(1, 6);
        let mut previous = from;
        for k in 1..steps {
            let at = a.lerp(b, k as f32 / steps as f32);
            let node = self.small(branch, at, pool, mix(seed ^ k as u64), power);
            self.link(previous, node);
            previous = node;
        }
        self.link(previous, to);
    }

    fn nearest(&self, among: &[usize], to: Vec2) -> usize {
        *among
            .iter()
            .min_by(|&&a, &&b| {
                self.nodes[a]
                    .position
                    .distance_squared(to)
                    .total_cmp(&self.nodes[b].position.distance_squared(to))
            })
            .expect("non-empty")
    }
}

/// Rounds a stat to one decimal so tooltips and sums stay tidy.
fn round_stat(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

impl PassiveTree {
    fn from_parts(nodes: Vec<PassiveNode>, links: Vec<(usize, usize)>) -> Self {
        let index = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let mut neighbours = vec![Vec::new(); nodes.len()];
        for &(a, b) in &links {
            neighbours[a].push(b);
            neighbours[b].push(a);
        }
        Self {
            nodes,
            links,
            index,
            neighbours,
        }
    }

    pub fn find(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    pub fn neighbours(&self, node: usize) -> &[usize] {
        &self.neighbours[node]
    }

    /// The Neural Lattice.
    pub fn lattice() -> Self {
        let mut b = Builder {
            nodes: Vec::new(),
            links: Vec::new(),
        };
        let origin = b.add(
            "Neural Origin".into(),
            NodeKind::Origin,
            Branch::Core,
            Vec2::ZERO,
            Vec::new(),
        );
        b.nodes[origin].id = ORIGIN_ID.into();
        // Edge clusters of every sector, per ring, for the bridges.
        let mut edges: Vec<Vec<(usize, usize)>> = Vec::new();
        for (s, table) in SECTORS.iter().enumerate() {
            let branch = table.branch;
            let pool = table.pool;
            let seed = mix(0xA11C_E000 ^ s as u64);
            let theta = (90.0 + s as f32 * 60.0).to_radians();
            let dir = Vec2::from_angle(theta);
            // Direction of increasing angle: towards the next sector.
            let tangent = Vec2::new(-dir.y, dir.x);
            // Origin -> gateway.
            let gate_at = dir * 260.0;
            let gate = b.add(
                format!("{} Gateway", branch.label()),
                NodeKind::Notable,
                branch,
                gate_at,
                pool.iter()
                    .take(2)
                    .map(|&(st, v)| (st, round_stat(v * 2.0)))
                    .collect(),
            );
            b.path(origin, gate, branch, pool, seed, 1.0);

            let mut previous: Vec<Cluster> = vec![Cluster {
                center: gate_at,
                ring: vec![gate],
            }];
            let mut name_counter = 0usize;
            let mut sector_edges = Vec::new();
            for ring in 0..RINGS {
                let radius = FIRST_RING + ring as f32 * RING_STEP;
                let count = 2 + ring;
                let power = 1.0 + ring as f32 * 0.06;
                let mut clusters: Vec<Cluster> = Vec::new();
                for k in 0..count {
                    let t = (k as f32 + 0.5) / count as f32 * 2.0 - 1.0;
                    let angle = theta + (t * SECTOR_SPREAD).to_radians();
                    let center = Vec2::from_angle(angle) * radius;
                    let h = mix(seed ^ ((ring * 64 + k) as u64 + 1));
                    // The notable: two stats at about three small nodes' worth.
                    let first = (h % pool.len() as u64) as usize;
                    let second = (first + 1 + (h >> 8) as usize % (pool.len() - 1)) % pool.len();
                    let name = format!(
                        "{} {}",
                        table.adjectives[name_counter % 10],
                        table.nouns[(name_counter / 10 + name_counter) % 10]
                    );
                    name_counter += 1;
                    let notable = b.add(
                        name,
                        NodeKind::Notable,
                        branch,
                        center,
                        vec![
                            (pool[first].0, round_stat(pool[first].1 * 3.0 * power)),
                            (pool[second].0, round_stat(pool[second].1 * 2.0 * power)),
                        ],
                    );
                    // The ring: 5 to 8 small nodes looping around it.
                    let size = 5 + (h >> 16) as usize % 4;
                    let theme = h >> 24;
                    let mut ring_nodes = Vec::with_capacity(size);
                    for j in 0..size {
                        let a = j as f32 / size as f32 * std::f32::consts::TAU + theta;
                        let at = center + Vec2::from_angle(a) * CLUSTER_RADIUS;
                        // Mostly the cluster's theme stat, sometimes another.
                        let pick = if j % 3 == 2 { mix(h ^ j as u64) } else { theme };
                        ring_nodes.push(b.small(branch, at, pool, pick, power));
                    }
                    for j in 0..size {
                        b.link(ring_nodes[j], ring_nodes[(j + 1) % size]);
                    }
                    let inner = b.nearest(&ring_nodes, Vec2::ZERO);
                    b.link(inner, notable);
                    // Join to the nearest cluster of the ring before.
                    let parent = previous
                        .iter()
                        .min_by(|a, c| {
                            a.center
                                .distance_squared(center)
                                .total_cmp(&c.center.distance_squared(center))
                        })
                        .expect("a previous ring");
                    let from = b.nearest(&parent.ring, center);
                    b.path(from, inner, branch, pool, h, power);
                    // And to the neighbour in this ring.
                    if let Some(left) = clusters.last() {
                        let a = b.nearest(&left.ring, center);
                        let c = b.nearest(&ring_nodes, left.center);
                        b.path(a, c, branch, pool, h ^ 0x55, power);
                    }
                    clusters.push(Cluster {
                        center,
                        ring: ring_nodes,
                    });
                }
                let first = clusters.first().expect("clusters");
                let last = clusters.last().expect("clusters");
                sector_edges.push((
                    b.nearest(&first.ring, first.center - tangent * 200.0),
                    b.nearest(&last.ring, last.center + tangent * 200.0),
                ));
                previous = clusters;
            }
            // Two keystones past the outer ring.
            for (k, &(name, stats)) in table.keystones.iter().enumerate() {
                let side = if k == 0 { -1.0 } else { 1.0 };
                let at = Vec2::from_angle(theta + (side * 9.0f32).to_radians())
                    * (FIRST_RING + RINGS as f32 * RING_STEP + 120.0);
                let key = b.add(name.into(), NodeKind::Keystone, branch, at, stats.to_vec());
                let parent = previous
                    .iter()
                    .min_by(|a, c| {
                        a.center
                            .distance_squared(at)
                            .total_cmp(&c.center.distance_squared(at))
                    })
                    .expect("outer ring");
                let from = b.nearest(&parent.ring, at);
                b.path(
                    from,
                    key,
                    branch,
                    pool,
                    mix(seed ^ 0xC0FFEE ^ k as u64),
                    1.5,
                );
            }
            edges.push(sector_edges);
        }
        // Bridges between neighbouring sectors on every third ring.
        for s in 0..SECTORS.len() {
            let next = (s + 1) % SECTORS.len();
            for ring in (1..RINGS).step_by(3) {
                let (_, from) = edges[s][ring];
                let (to, _) = edges[next][ring];
                let pool = &[
                    (PassiveStat::MaxHealth, 5.0),
                    (PassiveStat::MaxShields, 5.0),
                ];
                b.path(
                    from,
                    to,
                    Branch::Core,
                    pool,
                    mix((s * 100 + ring) as u64),
                    1.0,
                );
            }
        }
        for node in &mut b.nodes {
            if node.kind == NodeKind::Small && node.branch == Branch::Core {
                node.name = "Synaptic Bridge".into();
            }
        }
        Self::from_parts(b.nodes, b.links)
    }
}

impl Default for PassiveTree {
    fn default() -> Self {
        Self::lattice()
    }
}

/// XP needed to go from `level` to the next.
pub fn xp_for_next_level(level: u32) -> u32 {
    80 + 45 * level
}

/// Level reached with `xp` total experience (starting at level 1).
pub fn level_for_xp(xp: u32) -> u32 {
    let mut level = 1;
    let mut left = xp;
    while left >= xp_for_next_level(level) {
        left -= xp_for_next_level(level);
        level += 1;
    }
    level
}

/// XP into the current level, and the XP that level needs.
pub fn level_progress(xp: u32) -> (u32, u32) {
    let mut level = 1;
    let mut left = xp;
    while left >= xp_for_next_level(level) {
        left -= xp_for_next_level(level);
        level += 1;
    }
    (left, xp_for_next_level(level))
}

/// The player's experience and owned nodes.
#[derive(Resource, Component, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PassiveProgress {
    pub xp: u32,
    /// Owned node ids (the origin is always owned and not listed).
    pub allocated: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocateError {
    UnknownNode,
    AlreadyOwned,
    NotConnected,
    NoPoints,
    NotOwned,
    WouldDisconnect,
    Origin,
}

impl PassiveProgress {
    pub fn level(&self) -> u32 {
        level_for_xp(self.xp)
    }

    /// One point per level gained.
    pub fn points_earned(&self) -> u32 {
        self.level() - 1
    }

    pub fn points_available(&self) -> u32 {
        self.points_earned()
            .saturating_sub(self.allocated.len() as u32)
    }

    pub fn owns(&self, tree: &PassiveTree, node: usize) -> bool {
        tree.nodes[node].kind == NodeKind::Origin
            || self.allocated.iter().any(|id| *id == tree.nodes[node].id)
    }

    fn owned_set(&self, tree: &PassiveTree) -> HashSet<usize> {
        let mut owned: HashSet<usize> = self
            .allocated
            .iter()
            .filter_map(|id| tree.find(id))
            .collect();
        if let Some(origin) = tree.find(ORIGIN_ID) {
            owned.insert(origin);
        }
        owned
    }

    /// Whether `node` can be taken now (ignoring points).
    pub fn reachable(&self, tree: &PassiveTree, node: usize) -> bool {
        !self.owns(tree, node) && tree.neighbours(node).iter().any(|&n| self.owns(tree, n))
    }

    pub fn allocate(&mut self, tree: &PassiveTree, id: &str) -> Result<(), AllocateError> {
        let node = tree.find(id).ok_or(AllocateError::UnknownNode)?;
        if self.owns(tree, node) {
            return Err(AllocateError::AlreadyOwned);
        }
        if !self.reachable(tree, node) {
            return Err(AllocateError::NotConnected);
        }
        if self.points_available() == 0 {
            return Err(AllocateError::NoPoints);
        }
        self.allocated.push(id.to_owned());
        Ok(())
    }

    /// Refunds a node, if every other owned node stays connected to the
    /// origin without it.
    pub fn refund(&mut self, tree: &PassiveTree, id: &str) -> Result<(), AllocateError> {
        let node = tree.find(id).ok_or(AllocateError::UnknownNode)?;
        if tree.nodes[node].kind == NodeKind::Origin {
            return Err(AllocateError::Origin);
        }
        if !self.owns(tree, node) {
            return Err(AllocateError::NotOwned);
        }
        let mut owned = self.owned_set(tree);
        owned.remove(&node);
        let origin = tree.find(ORIGIN_ID).ok_or(AllocateError::UnknownNode)?;
        let mut seen = HashSet::from([origin]);
        let mut queue = VecDeque::from([origin]);
        while let Some(n) = queue.pop_front() {
            for &m in tree.neighbours(n) {
                if owned.contains(&m) && seen.insert(m) {
                    queue.push_back(m);
                }
            }
        }
        if seen.len() != owned.len() {
            return Err(AllocateError::WouldDisconnect);
        }
        self.allocated.retain(|a| a != id);
        Ok(())
    }

    /// Drops unknown, duplicate, unreachable or unaffordable nodes (saved
    /// progress from an older tree, or a hand-edited file).
    pub fn sanitized(mut self, tree: &PassiveTree) -> Self {
        let wanted = std::mem::take(&mut self.allocated);
        let mut pending: Vec<String> = Vec::new();
        for id in wanted {
            if tree.find(&id).is_some() && id != ORIGIN_ID && !pending.contains(&id) {
                pending.push(id);
            }
        }
        // Re-allocate in connection order until nothing more fits.
        loop {
            let before = pending.len();
            pending.retain(|id| self.allocate(tree, id).is_err());
            if pending.len() == before {
                break;
            }
        }
        self
    }

    /// Summed stats of every owned node.
    pub fn bonuses(&self, tree: &PassiveTree) -> PassiveBonuses {
        let mut bonuses = PassiveBonuses::default();
        for node in self.owned_set(tree) {
            for &(stat, value) in &tree.nodes[node].stats {
                *bonuses.values.entry(stat).or_insert(0.0) += value;
            }
        }
        bonuses
    }
}

/// Totals of every owned node's stats.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct PassiveBonuses {
    pub values: HashMap<PassiveStat, f32>,
}

impl PassiveBonuses {
    pub fn get(&self, stat: PassiveStat) -> f32 {
        self.values.get(&stat).copied().unwrap_or(0.0)
    }

    /// Multiplier for a percent stat (never below 10%).
    pub fn scale(&self, stat: PassiveStat) -> f32 {
        (1.0 + self.get(stat) / 100.0).max(0.1)
    }
}

/// Multiplier on damage a character takes (the passive tree's damage
/// reduction on the player).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct IncomingDamageScale(pub f32);

/// Extra melee damage multiplier on an attacker (characters whose vitals
/// are not derived from [`crate::CharacterAttributes`], like the player).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct MeleeDamageScale(pub f32);

/// XP for defeating an enemy, by how dangerous it is.
pub fn kill_xp(max_health: f32) -> u32 {
    (20.0 + max_health * 0.35).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rich() -> PassiveProgress {
        PassiveProgress {
            xp: 5_000_000,
            allocated: Vec::new(),
        }
    }

    /// A chain of `n` nodes walking outward from the origin.
    fn chain(tree: &PassiveTree, n: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::from([0usize]);
        let mut at = 0;
        for _ in 0..n {
            let next = *tree
                .neighbours(at)
                .iter()
                .find(|m| !seen.contains(m))
                .expect("a way on");
            seen.insert(next);
            out.push(tree.nodes[next].id.clone());
            at = next;
        }
        out
    }

    /// The first `n` nodes in breadth-first order: each touches an earlier one.
    fn connected(tree: &PassiveTree, n: usize) -> Vec<String> {
        let mut seen = HashSet::from([0usize]);
        let mut queue = VecDeque::from([0usize]);
        let mut out = Vec::new();
        while let Some(at) = queue.pop_front() {
            for &m in tree.neighbours(at) {
                if out.len() < n && seen.insert(m) {
                    out.push(tree.nodes[m].id.clone());
                    queue.push_back(m);
                }
            }
        }
        out
    }

    #[test]
    fn lattice_is_huge_connected_and_unique() {
        let tree = PassiveTree::lattice();
        assert!(tree.nodes.len() >= 4000, "{}", tree.nodes.len());
        let ids: HashSet<&str> = tree.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), tree.nodes.len(), "ids are unique");
        let mut seen = HashSet::from([0]);
        let mut queue = VecDeque::from([0]);
        while let Some(n) = queue.pop_front() {
            for &m in tree.neighbours(n) {
                if seen.insert(m) {
                    queue.push_back(m);
                }
            }
        }
        assert_eq!(seen.len(), tree.nodes.len(), "every node reachable");
        let count = |kind| tree.nodes.iter().filter(|n| n.kind == kind).count();
        assert_eq!(count(NodeKind::Keystone), 12);
        assert!(count(NodeKind::Notable) >= 300);
        for branch in Branch::SECTORS {
            let names: Vec<&str> = tree
                .nodes
                .iter()
                .filter(|n| n.branch == branch && n.kind == NodeKind::Notable)
                .map(|n| n.name.as_str())
                .collect();
            let unique: HashSet<&str> = names.iter().copied().collect();
            assert_eq!(
                unique.len(),
                names.len(),
                "{branch:?} notables are distinct"
            );
        }
        assert!(tree
            .nodes
            .iter()
            .all(|n| n.kind == NodeKind::Origin || !n.stats.is_empty()));
    }

    #[test]
    fn lattice_is_the_same_every_time() {
        let (a, b) = (PassiveTree::lattice(), PassiveTree::lattice());
        assert_eq!(a.nodes, b.nodes);
        assert_eq!(a.links, b.links);
    }

    #[test]
    fn levels_grant_points_and_xp_curve_rises() {
        assert_eq!(level_for_xp(0), 1);
        assert_eq!(level_for_xp(xp_for_next_level(1)), 2);
        assert!(xp_for_next_level(5) > xp_for_next_level(1));
        let progress = PassiveProgress {
            xp: xp_for_next_level(1) + xp_for_next_level(2),
            allocated: Vec::new(),
        };
        assert_eq!(progress.level(), 3);
        assert_eq!(progress.points_available(), 2);
        assert_eq!(
            level_progress(xp_for_next_level(1) + 10),
            (10, xp_for_next_level(2))
        );
    }

    #[test]
    fn allocation_needs_a_connection_and_a_point() {
        let tree = PassiveTree::lattice();
        let path = chain(&tree, 3);
        let mut broke = PassiveProgress::default();
        assert_eq!(
            broke.allocate(&tree, &path[0]),
            Err(AllocateError::NoPoints)
        );
        let mut p = rich();
        assert_eq!(
            p.allocate(&tree, &path[1]),
            Err(AllocateError::NotConnected)
        );
        assert_eq!(p.allocate(&tree, &path[0]), Ok(()));
        assert_eq!(
            p.allocate(&tree, &path[0]),
            Err(AllocateError::AlreadyOwned)
        );
        assert_eq!(p.allocate(&tree, &path[1]), Ok(()));
        assert_eq!(p.allocate(&tree, "nope"), Err(AllocateError::UnknownNode));
        broke.xp = xp_for_next_level(1);
        assert_eq!(broke.allocate(&tree, &path[0]), Ok(()));
        assert_eq!(
            broke.allocate(&tree, &path[1]),
            Err(AllocateError::NoPoints)
        );
    }

    #[test]
    fn refunds_keep_the_tree_connected() {
        let tree = PassiveTree::lattice();
        let path = chain(&tree, 3);
        let mut p = rich();
        for id in &path {
            p.allocate(&tree, id).unwrap();
        }
        assert_eq!(
            p.refund(&tree, &path[1]),
            Err(AllocateError::WouldDisconnect)
        );
        assert_eq!(p.refund(&tree, &path[2]), Ok(()));
        assert_eq!(p.refund(&tree, &path[1]), Ok(()));
        assert_eq!(p.refund(&tree, ORIGIN_ID), Err(AllocateError::Origin));
        assert_eq!(p.refund(&tree, &path[2]), Err(AllocateError::NotOwned));
    }

    #[test]
    fn bonuses_sum_every_owned_node() {
        let tree = PassiveTree::lattice();
        let path = connected(&tree, 40);
        let mut p = rich();
        let mut expected: HashMap<PassiveStat, f32> = HashMap::new();
        for id in &path {
            p.allocate(&tree, id).unwrap();
            for &(stat, v) in &tree.nodes[tree.find(id).unwrap()].stats {
                *expected.entry(stat).or_default() += v;
            }
        }
        let b = p.bonuses(&tree);
        for (stat, v) in expected {
            assert!((b.get(stat) - v).abs() < 1e-3, "{stat:?}");
        }
    }

    #[test]
    fn keystones_trade_a_strength_for_a_weakness() {
        let tree = PassiveTree::lattice();
        for key in tree.nodes.iter().filter(|n| n.kind == NodeKind::Keystone) {
            assert!(key.stats.iter().any(|(_, v)| *v > 0.0), "{}", key.name);
            assert!(key.stats.iter().any(|(_, v)| *v < 0.0), "{}", key.name);
        }
    }

    #[test]
    fn sanitizing_drops_junk_and_orphans() {
        let tree = PassiveTree::lattice();
        let path = chain(&tree, 4);
        let saved = PassiveProgress {
            xp: xp_for_next_level(1) + xp_for_next_level(2),
            allocated: vec![
                path[1].clone(),
                path[0].clone(),
                path[0].clone(),
                path[3].clone(),
                "../hack".into(),
                ORIGIN_ID.into(),
            ],
        };
        let clean = saved.sanitized(&tree);
        assert_eq!(clean.allocated, vec![path[0].clone(), path[1].clone()]);
    }

    #[test]
    fn tougher_enemies_give_more_xp() {
        assert!(kill_xp(400.0) > kill_xp(60.0));
        assert!(kill_xp(60.0) >= 20);
    }
}
