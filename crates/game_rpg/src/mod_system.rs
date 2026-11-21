//! Cyber-mod cards and capacity-limited loadouts.
//!
//! A card's capacity drain is `base_cost + current_rank`, halved (rounded up)
//! in a slot of matching polarity and raised by 25 % (rounded up) in a slot
//! of a different polarity. Bonuses scale as `value * (rank + 1)` and stack
//! additively across cards.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MOD_SLOT_COUNT: usize = 8;
pub const BASE_MOD_CAPACITY: u32 = 30;
/// Capacity with the Cyber-Core Overcharge installed.
pub const OVERCHARGED_MOD_CAPACITY: u32 = 60;

/// Mod and slot polarities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Debug, PartialEq, Hash)]
pub enum ModPolarity {
    /// Madurai `V`.
    Attack,
    /// Vazarin `D`.
    Defense,
    /// Naramon `-`.
    Utility,
    /// Zenurik `=`.
    Augment,
    /// Matches every polarity.
    Universal,
}

impl ModPolarity {
    pub fn matches(self, other: ModPolarity) -> bool {
        self == other || self == Self::Universal || other == Self::Universal
    }
}

/// One mod card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CyberModCard {
    pub id: u32,
    pub name: &'static str,
    pub polarity: ModPolarity,
    pub base_cost: u32,
    pub current_rank: u32,
    pub max_rank: u32,
    /// `(stat, bonus per rank step)` pairs.
    pub stat_multipliers: &'static [(&'static str, f32)],
}

impl CyberModCard {
    /// Capacity drain before polarity: `base_cost + current_rank`.
    pub fn drain_cost(&self) -> u32 {
        self.base_cost + self.current_rank
    }

    /// The card at `rank` (clamped to `max_rank`).
    pub fn with_rank(mut self, rank: u32) -> Self {
        self.current_rank = rank.min(self.max_rank);
        self
    }

    /// Total bonus this card gives `stat` at its current rank.
    pub fn stat_bonus(&self, stat: &str) -> f32 {
        let steps = (self.current_rank + 1) as f32;
        self.stat_multipliers
            .iter()
            .filter(|(name, _)| *name == stat)
            .map(|(_, per_rank)| per_rank * steps)
            .sum()
    }
}

/// Drain of `card` in a slot with `slot_polarity`.
pub fn slot_drain(card: &CyberModCard, slot_polarity: Option<ModPolarity>) -> u32 {
    let drain = card.drain_cost();
    match slot_polarity {
        None => drain,
        Some(polarity) if polarity.matches(card.polarity) => drain.div_ceil(2),
        Some(_) => drain + (drain * 25).div_ceil(100),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ModLoadoutError {
    #[error("slot {0} does not exist")]
    InvalidSlot(usize),
    #[error("card {0} is already installed")]
    Duplicate(u32),
    #[error("needs {needed} capacity, only {available} available")]
    OverCapacity { needed: u32, available: u32 },
}

/// The mods installed on a character.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ModLoadout {
    pub capacity_max: u32,
    pub slots: [Option<CyberModCard>; MOD_SLOT_COUNT],
    pub slot_polarities: [Option<ModPolarity>; MOD_SLOT_COUNT],
}

impl Default for ModLoadout {
    fn default() -> Self {
        Self {
            capacity_max: BASE_MOD_CAPACITY,
            slots: [None; MOD_SLOT_COUNT],
            slot_polarities: [None; MOD_SLOT_COUNT],
        }
    }
}

impl ModLoadout {
    /// Doubles capacity to [`OVERCHARGED_MOD_CAPACITY`].
    pub fn install_cyber_core_overcharge(&mut self) {
        self.capacity_max = self.capacity_max.max(OVERCHARGED_MOD_CAPACITY);
    }

    pub fn total_drain(&self) -> u32 {
        self.slots
            .iter()
            .zip(self.slot_polarities)
            .filter_map(|(card, polarity)| card.as_ref().map(|c| slot_drain(c, polarity)))
            .sum()
    }

    pub fn capacity_remaining(&self) -> i64 {
        self.capacity_max as i64 - self.total_drain() as i64
    }

    fn has_duplicates(&self) -> bool {
        let ids: Vec<u32> = self.slots.iter().flatten().map(|c| c.id).collect();
        ids.iter()
            .enumerate()
            .any(|(i, id)| ids[i + 1..].contains(id))
    }

    /// Within capacity and free of duplicate cards.
    pub fn is_valid_loadout(&self) -> bool {
        self.total_drain() <= self.capacity_max && !self.has_duplicates()
    }

    /// Installs `card` in `slot` if the result stays valid; the previous
    /// card in that slot is replaced.
    pub fn try_install(&mut self, slot: usize, card: CyberModCard) -> Result<(), ModLoadoutError> {
        if slot >= MOD_SLOT_COUNT {
            return Err(ModLoadoutError::InvalidSlot(slot));
        }
        let duplicate = self
            .slots
            .iter()
            .enumerate()
            .any(|(i, c)| i != slot && c.is_some_and(|c| c.id == card.id));
        if duplicate {
            return Err(ModLoadoutError::Duplicate(card.id));
        }
        let previous = self.slots[slot].replace(card);
        if self.total_drain() > self.capacity_max {
            let needed = slot_drain(&card, self.slot_polarities[slot]);
            self.slots[slot] = previous;
            let available = (self.capacity_remaining()
                + previous.map_or(0, |p| slot_drain(&p, self.slot_polarities[slot])) as i64)
                .max(0) as u32;
            return Err(ModLoadoutError::OverCapacity { needed, available });
        }
        Ok(())
    }

    pub fn remove(&mut self, slot: usize) -> Option<CyberModCard> {
        self.slots.get_mut(slot).and_then(Option::take)
    }

    /// `1 + sum of bonuses` to `stat` across installed cards.
    pub fn aggregate_stat_multiplier(&self, stat: &str) -> f32 {
        1.0 + self
            .slots
            .iter()
            .flatten()
            .map(|c| c.stat_bonus(stat))
            .sum::<f32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn card(id: u32, polarity: ModPolarity, base_cost: u32) -> CyberModCard {
        CyberModCard {
            id,
            name: "Test",
            polarity,
            base_cost,
            current_rank: 0,
            max_rank: 10,
            stat_multipliers: &[("base_damage", 0.15)],
        }
    }

    #[test]
    fn drain_is_base_cost_plus_rank() {
        let c = card(1, ModPolarity::Attack, 4);
        assert_eq!(c.drain_cost(), 4);
        assert_eq!(c.with_rank(6).drain_cost(), 10);
        assert_eq!(c.with_rank(99).current_rank, 10, "rank clamped");
    }

    #[test]
    fn matching_polarity_halves_drain() {
        let ten = card(1, ModPolarity::Attack, 10);
        assert_eq!(slot_drain(&ten, Some(ModPolarity::Attack)), 5);
        assert_eq!(
            slot_drain(&card(1, ModPolarity::Attack, 9), Some(ModPolarity::Attack)),
            5,
            "rounded up"
        );
        assert_eq!(slot_drain(&ten, Some(ModPolarity::Universal)), 5);
        assert_eq!(
            slot_drain(
                &card(1, ModPolarity::Universal, 10),
                Some(ModPolarity::Defense)
            ),
            5
        );
    }

    #[test]
    fn mismatched_polarity_adds_a_quarter() {
        let ten = card(1, ModPolarity::Attack, 10);
        assert_eq!(
            slot_drain(&ten, Some(ModPolarity::Defense)),
            13,
            "10 + ceil(2.5)"
        );
        assert_eq!(slot_drain(&ten, None), 10, "no polarity: normal drain");
        assert_eq!(
            slot_drain(&card(1, ModPolarity::Attack, 4), Some(ModPolarity::Utility)),
            5
        );
    }

    #[test]
    fn loadout_rejects_overcapacity_and_duplicates() {
        let mut loadout = ModLoadout::default();
        loadout.slot_polarities[0] = Some(ModPolarity::Attack);
        loadout
            .try_install(0, card(1, ModPolarity::Attack, 10))
            .expect("5 drain");
        loadout
            .try_install(1, card(2, ModPolarity::Attack, 10))
            .expect("10 drain");
        loadout
            .try_install(2, card(3, ModPolarity::Attack, 10))
            .expect("10 drain");
        assert_eq!(loadout.total_drain(), 25);
        assert_eq!(
            loadout.try_install(3, card(4, ModPolarity::Attack, 10)),
            Err(ModLoadoutError::OverCapacity {
                needed: 10,
                available: 5
            })
        );
        assert!(loadout.slots[3].is_none(), "rejected card not installed");
        assert_eq!(
            loadout.try_install(4, card(2, ModPolarity::Attack, 1)),
            Err(ModLoadoutError::Duplicate(2))
        );
        assert_eq!(
            loadout.try_install(9, card(9, ModPolarity::Attack, 1)),
            Err(ModLoadoutError::InvalidSlot(9))
        );
        assert!(loadout.is_valid_loadout());

        loadout.slots[3] = Some(card(4, ModPolarity::Attack, 10));
        assert!(!loadout.is_valid_loadout(), "35 > 30");
        loadout.install_cyber_core_overcharge();
        assert_eq!(loadout.capacity_max, 60);
        assert!(loadout.is_valid_loadout());
        loadout.slots[5] = Some(card(1, ModPolarity::Attack, 1));
        assert!(!loadout.is_valid_loadout(), "duplicate placed directly");
    }

    #[test]
    fn replacing_a_slot_frees_its_capacity() {
        let mut loadout = ModLoadout::default();
        loadout
            .try_install(0, card(1, ModPolarity::Attack, 20))
            .expect("20");
        loadout
            .try_install(1, card(2, ModPolarity::Attack, 10))
            .expect("30");
        loadout
            .try_install(0, card(3, ModPolarity::Attack, 15))
            .expect("swap 20 for 15");
        assert_eq!(loadout.total_drain(), 25);
        assert_eq!(loadout.remove(1).map(|c| c.id), Some(2));
        assert_eq!(loadout.total_drain(), 15);
    }

    #[test]
    fn bonuses_scale_with_rank_and_stack() {
        let mut loadout = ModLoadout::default();
        assert_eq!(loadout.aggregate_stat_multiplier("base_damage"), 1.0);
        loadout.slots[0] = Some(card(1, ModPolarity::Attack, 4).with_rank(10));
        assert!(
            (loadout.aggregate_stat_multiplier("base_damage") - 2.65).abs() < 1e-5,
            "+165 %"
        );
        loadout.slots[1] = Some(card(2, ModPolarity::Attack, 4));
        assert!(
            (loadout.aggregate_stat_multiplier("base_damage") - 2.80).abs() < 1e-5,
            "additive"
        );
        assert_eq!(loadout.aggregate_stat_multiplier("max_health"), 1.0);
    }
}
