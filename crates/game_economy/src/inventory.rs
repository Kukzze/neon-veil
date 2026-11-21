//! 28-slot stacking inventory and the exchange collection box.

use crate::error::EconomyError;
use crate::wallet::Wallet;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MAX_INVENTORY_SLOTS: usize = 28;

/// Every item stacks, so a slot is one distinct item id.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInventory {
    slots: HashMap<u32, u32>,
}

impl PlayerInventory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn free_slots(&self) -> usize {
        MAX_INVENTORY_SLOTS - self.slots.len()
    }

    pub fn can_fit(&self, item_id: u32, quantity: u32) -> bool {
        match self.slots.get(&item_id) {
            Some(&held) => held.checked_add(quantity).is_some(),
            None => self.slots.len() < MAX_INVENTORY_SLOTS,
        }
    }

    pub fn add_item(&mut self, item_id: u32, quantity: u32) -> Result<(), EconomyError> {
        if quantity == 0 {
            return Err(EconomyError::InvalidQuantity(0));
        }
        if let Some(held) = self.slots.get_mut(&item_id) {
            *held = held
                .checked_add(quantity)
                .ok_or(EconomyError::ValueOverflow)?;
            return Ok(());
        }
        if self.slots.len() >= MAX_INVENTORY_SLOTS {
            return Err(EconomyError::InventoryFull {
                max: MAX_INVENTORY_SLOTS,
            });
        }
        self.slots.insert(item_id, quantity);
        Ok(())
    }

    /// Removing the whole stack frees its slot.
    pub fn remove_item(&mut self, item_id: u32, quantity: u32) -> Result<(), EconomyError> {
        if quantity == 0 {
            return Err(EconomyError::InvalidQuantity(0));
        }
        let available = self.item_count(item_id);
        if available < quantity {
            return Err(EconomyError::InsufficientItems {
                required: quantity,
                available,
            });
        }
        if available == quantity {
            self.slots.remove(&item_id);
        } else {
            self.slots.insert(item_id, available - quantity);
        }
        Ok(())
    }

    pub fn has_item(&self, item_id: u32, quantity: u32) -> bool {
        self.item_count(item_id) >= quantity
    }

    pub fn item_count(&self, item_id: u32) -> u32 {
        self.slots.get(&item_id).copied().unwrap_or(0)
    }

    pub fn items(&self) -> &HashMap<u32, u32> {
        &self.slots
    }
}

/// Proceeds waiting to be claimed: bought items, sale payouts, refunds.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionBox {
    pub pending_credits: i64,
    pub pending_items: HashMap<u32, u32>,
}

impl CollectionBox {
    pub fn is_empty(&self) -> bool {
        self.pending_credits == 0 && self.pending_items.is_empty()
    }

    /// Non-positive amounts are ignored.
    pub fn deposit_credits(&mut self, amount: i64) {
        if amount > 0 {
            self.pending_credits = self.pending_credits.saturating_add(amount);
        }
    }

    pub fn deposit_item(&mut self, item_id: u32, quantity: u32) {
        if quantity > 0 {
            let held = self.pending_items.entry(item_id).or_insert(0);
            *held = held.saturating_add(quantity);
        }
    }

    /// Moves all pending credits into `wallet`; returns the amount moved.
    pub fn claim_credits(&mut self, wallet: &mut Wallet) -> i64 {
        let amount = self.pending_credits;
        if amount > 0 && wallet.deposit(amount).is_ok() {
            self.pending_credits = 0;
            amount
        } else {
            0
        }
    }

    /// Moves every stack that fits into `inventory`; the rest stays boxed.
    /// Errors only when items are pending and none of them fit.
    pub fn claim_items(
        &mut self,
        inventory: &mut PlayerInventory,
    ) -> Result<HashMap<u32, u32>, EconomyError> {
        let mut ids: Vec<u32> = self.pending_items.keys().copied().collect();
        // Stacks already held first: they never need a new slot.
        ids.sort_by_key(|id| (inventory.item_count(*id) == 0, *id));
        let mut claimed = HashMap::new();
        for id in ids {
            let quantity = self.pending_items[&id];
            if inventory.add_item(id, quantity).is_ok() {
                self.pending_items.remove(&id);
                claimed.insert(id, quantity);
            }
        }
        if claimed.is_empty() && !self.pending_items.is_empty() {
            return Err(EconomyError::InventoryFull {
                max: MAX_INVENTORY_SLOTS,
            });
        }
        Ok(claimed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacks_share_a_slot() {
        let mut inv = PlayerInventory::new();
        inv.add_item(1001, 5).unwrap();
        inv.add_item(1001, 7).unwrap();
        assert_eq!(inv.item_count(1001), 12);
        assert_eq!(inv.slot_count(), 1);
        assert!(inv.has_item(1001, 12) && !inv.has_item(1001, 13));
    }

    #[test]
    fn twenty_eight_slots_maximum() {
        let mut inv = PlayerInventory::new();
        for id in 0..28 {
            inv.add_item(1000 + id, 1).unwrap();
        }
        assert_eq!(inv.slot_count(), MAX_INVENTORY_SLOTS);
        assert_eq!(inv.free_slots(), 0);
        assert!(!inv.can_fit(5000, 1));
        assert_eq!(
            inv.add_item(5000, 1),
            Err(EconomyError::InventoryFull { max: 28 })
        );
        assert!(inv.can_fit(1000, 99), "existing stacks still grow");
        inv.add_item(1000, 99).unwrap();
        inv.remove_item(1027, 1).unwrap();
        assert_eq!(inv.slot_count(), 27, "emptied stack frees its slot");
        inv.add_item(5000, 1).unwrap();
    }

    #[test]
    fn removal_checks_quantity() {
        let mut inv = PlayerInventory::new();
        inv.add_item(1001, 3).unwrap();
        assert_eq!(
            inv.remove_item(1001, 4),
            Err(EconomyError::InsufficientItems {
                required: 4,
                available: 3
            })
        );
        assert_eq!(
            inv.remove_item(1002, 1),
            Err(EconomyError::InsufficientItems {
                required: 1,
                available: 0
            })
        );
        assert_eq!(inv.add_item(1001, 0), Err(EconomyError::InvalidQuantity(0)));
        inv.remove_item(1001, 2).unwrap();
        assert_eq!(inv.item_count(1001), 1);
    }

    #[test]
    fn collection_box_claims() {
        let mut cb = CollectionBox::default();
        cb.deposit_credits(490);
        cb.deposit_credits(-10);
        cb.deposit_item(2001, 1);
        cb.deposit_item(2001, 2);
        let mut wallet = Wallet::new(0);
        let mut inv = PlayerInventory::new();
        assert_eq!(cb.claim_credits(&mut wallet), 490);
        assert_eq!(wallet.credits(), 490);
        assert_eq!(cb.claim_credits(&mut wallet), 0);
        let claimed = cb.claim_items(&mut inv).unwrap();
        assert_eq!(claimed, HashMap::from([(2001, 3)]));
        assert_eq!(inv.item_count(2001), 3);
        assert!(cb.is_empty());
    }

    #[test]
    fn collection_box_keeps_what_does_not_fit() {
        let mut inv = PlayerInventory::new();
        for id in 0..28 {
            inv.add_item(1000 + id, 1).unwrap();
        }
        let mut cb = CollectionBox::default();
        cb.deposit_item(9999, 1);
        assert_eq!(
            cb.claim_items(&mut inv),
            Err(EconomyError::InventoryFull { max: 28 })
        );
        cb.deposit_item(1000, 4);
        let claimed = cb.claim_items(&mut inv).unwrap();
        assert_eq!(claimed, HashMap::from([(1000, 4)]));
        assert_eq!(cb.pending_items, HashMap::from([(9999, 1)]));
    }
}
