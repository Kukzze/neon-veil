//! The Neural Lattice: a passive tree of permanent upgrades. Nodes radiate
//! from a central origin in three branches (Blade, Ghost, Bulwark) joined by
//! an outer ring. A node can be taken when it touches one already owned;
//! each branch ends in a keystone that trades a weakness for a strength.

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
    /// Flat armour.
    Armor,
    /// Percent melee damage.
    MeleeDamage,
    /// Flat stamina.
    Stamina,
    /// Health restored on each kill.
    HealOnKill,
}

impl PassiveStat {
    pub fn describe(self, value: f32) -> String {
        let sign = if value >= 0.0 { "+" } else { "-" };
        let v = value.abs();
        match self {
            Self::MaxHealth => format!("{sign}{v:.0} maximum health"),
            Self::MaxHealthPercent => format!("{sign}{v:.0}% maximum health"),
            Self::MaxShields => format!("{sign}{v:.0} maximum shields"),
            Self::MaxShieldsPercent => format!("{sign}{v:.0}% maximum shields"),
            Self::ShieldRecharge => format!("{sign}{v:.0}% shield recharge rate"),
            Self::Armor => format!("{sign}{v:.0} armour"),
            Self::MeleeDamage => format!("{sign}{v:.0}% melee damage"),
            Self::Stamina => format!("{sign}{v:.0} stamina"),
            Self::HealOnKill => format!("{sign}{v:.0} health on kill"),
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
        let mut nodes = vec![PassiveNode {
            id: ORIGIN_ID.into(),
            name: "Neural Origin".into(),
            kind: NodeKind::Origin,
            branch: Branch::Core,
            position: Vec2::ZERO,
            stats: Vec::new(),
        }];
        let mut links = Vec::new();
        let add = |nodes: &mut Vec<PassiveNode>,
                   id: String,
                   name: &str,
                   kind: NodeKind,
                   branch: Branch,
                   position: Vec2,
                   stats: Vec<(PassiveStat, f32)>| {
            nodes.push(PassiveNode {
                id,
                name: name.into(),
                kind,
                branch,
                position,
                stats,
            });
            nodes.len() - 1
        };
        use PassiveStat::*;
        // Each branch: a spine of eight steps out from the origin, with a
        // notable side cluster at steps 3 and 6 and a keystone at the end.
        let branches: [(Branch, f32, &str, [(&str, Vec<(PassiveStat, f32)>); 4]); 3] = [
            (
                Branch::Blade,
                90.0,
                "blade",
                [
                    ("Honed Edge", vec![(MeleeDamage, 5.0)]),
                    ("Severing Arc", vec![(MeleeDamage, 14.0), (Stamina, 10.0)]),
                    (
                        "Executioner's Rhythm",
                        vec![(MeleeDamage, 18.0), (HealOnKill, 4.0)],
                    ),
                    (
                        "Glass Circuit",
                        vec![(MeleeDamage, 45.0), (MaxHealthPercent, -30.0)],
                    ),
                ],
            ),
            (
                Branch::Ghost,
                210.0,
                "ghost",
                [
                    ("Light Step", vec![(Stamina, 8.0), (ShieldRecharge, 4.0)]),
                    ("Second Wind", vec![(Stamina, 20.0), (ShieldRecharge, 12.0)]),
                    ("Blood Siphon", vec![(HealOnKill, 8.0), (MeleeDamage, 6.0)]),
                    (
                        "Vampiric Protocol",
                        vec![(HealOnKill, 20.0), (MaxShieldsPercent, -35.0)],
                    ),
                ],
            ),
            (
                Branch::Bulwark,
                330.0,
                "bulwark",
                [
                    ("Reinforced Frame", vec![(MaxHealth, 8.0), (Armor, 3.0)]),
                    ("Kinetic Plating", vec![(MaxShields, 20.0), (Armor, 8.0)]),
                    (
                        "Unbroken",
                        vec![(MaxHealthPercent, 12.0), (ShieldRecharge, 15.0)],
                    ),
                    (
                        "Iron Lung",
                        vec![(MaxShieldsPercent, 60.0), (ShieldRecharge, -30.0)],
                    ),
                ],
            ),
        ];
        let mut ring_anchor = Vec::new();
        for (branch, degrees, key, [small, notable_a, notable_b, keystone]) in branches {
            let dir = Vec2::from_angle(degrees.to_radians());
            let side = Vec2::new(-dir.y, dir.x);
            let mut previous = 0;
            for step in 1..=8 {
                let at = dir * (step as f32 * 70.0 + 40.0);
                let (kind, name, stats) = match step {
                    8 => (NodeKind::Keystone, keystone.0, keystone.1.clone()),
                    _ => (NodeKind::Small, small.0, small.1.clone()),
                };
                let id = format!("{key}_{step}");
                let node = add(&mut nodes, id, name, kind, branch, at, stats);
                links.push((previous, node));
                previous = node;
                if step == 4 {
                    ring_anchor.push((node, at));
                }
                // Side clusters: two smalls leading to a notable.
                if step == 3 || step == 6 {
                    let (notable_name, notable_stats) = if step == 3 {
                        (notable_a.0, notable_a.1.clone())
                    } else {
                        (notable_b.0, notable_b.1.clone())
                    };
                    let flip = if step == 3 { 1.0 } else { -1.0 };
                    let mut from = node;
                    for k in 1..=2 {
                        let p = at + side * flip * (k as f32 * 55.0) + dir * (k as f32 * 18.0);
                        let id = format!("{key}_{step}_{k}");
                        let n = add(
                            &mut nodes,
                            id,
                            small.0,
                            NodeKind::Small,
                            branch,
                            p,
                            small.1.clone(),
                        );
                        links.push((from, n));
                        from = n;
                    }
                    let p = at + side * flip * 165.0 + dir * 50.0;
                    let id = format!("{key}_{step}_notable");
                    let n = add(
                        &mut nodes,
                        id,
                        notable_name,
                        NodeKind::Notable,
                        branch,
                        p,
                        notable_stats,
                    );
                    links.push((from, n));
                }
            }
        }
        // The outer ring: two connectors between each pair of branches, so
        // a build can cross over without walking back through the origin.
        for i in 0..ring_anchor.len() {
            let (a, pa) = ring_anchor[i];
            let (b, pb) = ring_anchor[(i + 1) % ring_anchor.len()];
            let mut from = a;
            for k in 1..=2 {
                let t = k as f32 / 3.0;
                let radius = pa.length() * 1.05;
                let p = pa.lerp(pb, t).normalize_or_zero() * radius;
                let id = format!("ring_{i}_{k}");
                let n = add(
                    &mut nodes,
                    id,
                    "Synaptic Bridge",
                    NodeKind::Small,
                    Branch::Core,
                    p,
                    vec![(MaxHealth, 5.0), (MaxShields, 5.0)],
                );
                links.push((from, n));
                from = n;
            }
            links.push((from, b));
        }
        Self::from_parts(nodes, links)
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
            xp: 50_000,
            allocated: Vec::new(),
        }
    }

    #[test]
    fn lattice_is_connected_with_unique_ids() {
        let tree = PassiveTree::lattice();
        assert!(tree.nodes.len() >= 40, "{}", tree.nodes.len());
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
        let keystones = tree
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Keystone)
            .count();
        assert_eq!(keystones, 3);
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
        let mut p = PassiveProgress::default();
        assert_eq!(p.allocate(&tree, "blade_1"), Err(AllocateError::NoPoints));
        let mut p2 = rich();
        assert_eq!(
            p2.allocate(&tree, "blade_2"),
            Err(AllocateError::NotConnected)
        );
        assert_eq!(p2.allocate(&tree, "blade_1"), Ok(()));
        assert_eq!(
            p2.allocate(&tree, "blade_1"),
            Err(AllocateError::AlreadyOwned)
        );
        assert_eq!(p2.allocate(&tree, "blade_2"), Ok(()));
        assert_eq!(p2.allocate(&tree, "nope"), Err(AllocateError::UnknownNode));
        p.xp = xp_for_next_level(1);
        assert_eq!(p.allocate(&tree, "ghost_1"), Ok(()));
        assert_eq!(p.allocate(&tree, "ghost_2"), Err(AllocateError::NoPoints));
    }

    #[test]
    fn refunds_keep_the_tree_connected() {
        let tree = PassiveTree::lattice();
        let mut p = rich();
        for id in ["bulwark_1", "bulwark_2", "bulwark_3"] {
            p.allocate(&tree, id).unwrap();
        }
        assert_eq!(
            p.refund(&tree, "bulwark_2"),
            Err(AllocateError::WouldDisconnect)
        );
        assert_eq!(p.refund(&tree, "bulwark_3"), Ok(()));
        assert_eq!(p.refund(&tree, "bulwark_2"), Ok(()));
        assert_eq!(p.refund(&tree, ORIGIN_ID), Err(AllocateError::Origin));
        assert_eq!(p.refund(&tree, "bulwark_3"), Err(AllocateError::NotOwned));
    }

    #[test]
    fn bonuses_sum_and_keystones_trade_off() {
        let tree = PassiveTree::lattice();
        let mut p = rich();
        for step in 1..=8 {
            p.allocate(&tree, &format!("blade_{step}")).unwrap();
        }
        let b = p.bonuses(&tree);
        assert!((b.get(PassiveStat::MeleeDamage) - (7.0 * 5.0 + 45.0)).abs() < 1e-4);
        assert!(
            b.scale(PassiveStat::MaxHealthPercent) < 1.0,
            "glass circuit costs health"
        );
    }

    #[test]
    fn sanitizing_drops_junk_and_orphans() {
        let tree = PassiveTree::lattice();
        let saved = PassiveProgress {
            xp: xp_for_next_level(1) + xp_for_next_level(2),
            allocated: vec![
                "blade_2".into(),
                "blade_1".into(),
                "blade_1".into(),
                "ghost_5".into(),
                "../hack".into(),
                ORIGIN_ID.into(),
            ],
        };
        let clean = saved.sanitized(&tree);
        assert_eq!(
            clean.allocated,
            vec!["blade_1".to_string(), "blade_2".to_string()]
        );
    }

    #[test]
    fn tougher_enemies_give_more_xp() {
        assert!(kill_xp(400.0) > kill_xp(60.0));
        assert!(kill_xp(60.0) >= 20);
    }
}
