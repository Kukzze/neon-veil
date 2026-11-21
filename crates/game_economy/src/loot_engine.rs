//! Weighted loot rolls.

use rand::Rng;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LootDropEntry {
    pub item_id: u32,
    pub min_quantity: u32,
    pub max_quantity: u32,
    /// Relative weight; tables use a 10,000 scale.
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LootTable {
    pub guaranteed_drops: &'static [(u32, u32)],
    pub weighted_entries: &'static [LootDropEntry],
    pub total_weight: u32,
}

impl LootTable {
    pub const fn new(guaranteed: &'static [(u32, u32)], entries: &'static [LootDropEntry]) -> Self {
        let mut total_weight = 0;
        let mut i = 0;
        while i < entries.len() {
            total_weight += entries[i].weight;
            i += 1;
        }
        Self {
            guaranteed_drops: guaranteed,
            weighted_entries: entries,
            total_weight,
        }
    }

    /// The weighted entry selected by `ticket` in `0..total_weight`.
    pub fn entry_for_ticket(&self, ticket: u32) -> Option<&LootDropEntry> {
        let mut upper = 0;
        self.weighted_entries.iter().find(|entry| {
            upper += entry.weight;
            ticket < upper
        })
    }

    /// All guaranteed drops plus one weighted pick, with its quantity drawn
    /// uniformly from `min..=max`. Every quantity, guaranteed ones included,
    /// is scaled by `drop_rate_multiplier` (values below 1.0 count as 1.0)
    /// and rounded to nearest.
    pub fn roll(&self, rng: &mut impl Rng, drop_rate_multiplier: f32) -> Vec<(u32, u32)> {
        let multiplier = if drop_rate_multiplier.is_finite() {
            drop_rate_multiplier.max(1.0)
        } else {
            1.0
        };
        let scale = |quantity: u32| ((quantity as f32 * multiplier).round() as u32).max(1);
        let mut drops: Vec<(u32, u32)> = self
            .guaranteed_drops
            .iter()
            .map(|&(id, q)| (id, scale(q)))
            .collect();
        if self.total_weight > 0 {
            let ticket = rng.gen_range(0..self.total_weight);
            if let Some(entry) = self.entry_for_ticket(ticket) {
                let low = entry.min_quantity.min(entry.max_quantity);
                let high = entry.min_quantity.max(entry.max_quantity);
                drops.push((entry.item_id, scale(rng.gen_range(low..=high))));
            }
        }
        drops
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    static ENTRIES: &[LootDropEntry] = &[
        LootDropEntry {
            item_id: 1,
            min_quantity: 2,
            max_quantity: 4,
            weight: 75,
        },
        LootDropEntry {
            item_id: 2,
            min_quantity: 1,
            max_quantity: 1,
            weight: 25,
        },
    ];
    static TABLE: LootTable = LootTable::new(&[(9, 1)], ENTRIES);

    #[test]
    fn total_weight_is_computed_at_compile_time() {
        assert_eq!(TABLE.total_weight, 100);
    }

    #[test]
    fn tickets_map_onto_cumulative_weights() {
        assert_eq!(TABLE.entry_for_ticket(0).unwrap().item_id, 1);
        assert_eq!(TABLE.entry_for_ticket(74).unwrap().item_id, 1);
        assert_eq!(TABLE.entry_for_ticket(75).unwrap().item_id, 2);
        assert_eq!(TABLE.entry_for_ticket(99).unwrap().item_id, 2);
        assert!(TABLE.entry_for_ticket(100).is_none());
    }

    #[test]
    fn quantities_stay_in_range_and_scale() {
        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..500 {
            let drops = TABLE.roll(&mut rng, 1.0);
            assert_eq!(drops[0], (9, 1), "guaranteed first");
            assert_eq!(drops.len(), 2);
            let (id, q) = drops[1];
            match id {
                1 => assert!((2..=4).contains(&q)),
                2 => assert_eq!(q, 1),
                _ => unreachable!(),
            }
        }
        for _ in 0..200 {
            let drops = TABLE.roll(&mut rng, 2.0);
            assert_eq!(drops[0], (9, 2));
            let (id, q) = drops[1];
            assert!(if id == 1 {
                (4..=8).contains(&q) && q % 2 == 0
            } else {
                q == 2
            });
        }
        assert_eq!(
            TABLE.roll(&mut rng, 0.25)[0],
            (9, 1),
            "multipliers floor at 1.0"
        );
        assert_eq!(TABLE.roll(&mut rng, f32::NAN)[0], (9, 1));
    }

    #[test]
    fn same_seed_same_loot() {
        let a: Vec<_> = (0..50)
            .map({
                let mut rng = StdRng::seed_from_u64(99);
                move |_| TABLE.roll(&mut rng, 1.0)
            })
            .collect();
        let b: Vec<_> = (0..50)
            .map({
                let mut rng = StdRng::seed_from_u64(99);
                move |_| TABLE.roll(&mut rng, 1.0)
            })
            .collect();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_table_only_yields_guaranteed() {
        let table = LootTable::new(&[(5, 3)], &[]);
        let mut rng = StdRng::seed_from_u64(1);
        assert_eq!(table.roll(&mut rng, 1.0), vec![(5, 3)]);
    }
}
