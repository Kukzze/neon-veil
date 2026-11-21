//! Player credits. The balance can never go negative.

use crate::error::EconomyError;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const STARTING_CREDITS: i64 = 1_000;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wallet {
    credits: i64,
}

impl Default for Wallet {
    fn default() -> Self {
        Self::new(STARTING_CREDITS)
    }
}

impl Wallet {
    /// Negative starting balances are clamped to zero.
    pub fn new(starting_credits: i64) -> Self {
        Self {
            credits: starting_credits.max(0),
        }
    }

    pub fn credits(&self) -> i64 {
        self.credits
    }

    pub fn can_afford(&self, amount: i64) -> bool {
        amount >= 0 && self.credits >= amount
    }

    /// Zero is accepted as a no-op; negative amounts are rejected.
    pub fn deposit(&mut self, amount: i64) -> Result<(), EconomyError> {
        if amount < 0 {
            return Err(EconomyError::InvalidAmount(amount));
        }
        self.credits = self
            .credits
            .checked_add(amount)
            .ok_or(EconomyError::ValueOverflow)?;
        Ok(())
    }

    pub fn withdraw(&mut self, amount: i64) -> Result<(), EconomyError> {
        if amount <= 0 {
            return Err(EconomyError::InvalidAmount(amount));
        }
        if self.credits < amount {
            return Err(EconomyError::InsufficientCredits {
                required: amount,
                available: self.credits,
            });
        }
        self.credits -= amount;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_1000_credits() {
        assert_eq!(Wallet::default().credits(), 1_000);
        assert_eq!(Wallet::new(-50).credits(), 0, "never negative");
    }

    #[test]
    fn deposit_and_withdraw() {
        let mut w = Wallet::default();
        w.deposit(500).unwrap();
        assert_eq!(w.credits(), 1_500);
        assert!(w.can_afford(1_500) && !w.can_afford(1_501));
        w.withdraw(1_500).unwrap();
        assert_eq!(w.credits(), 0);
    }

    #[test]
    fn rejects_bad_amounts_and_overdrafts() {
        let mut w = Wallet::default();
        assert_eq!(w.deposit(-1), Err(EconomyError::InvalidAmount(-1)));
        assert_eq!(w.withdraw(0), Err(EconomyError::InvalidAmount(0)));
        assert_eq!(w.withdraw(-5), Err(EconomyError::InvalidAmount(-5)));
        assert_eq!(
            w.withdraw(1_001),
            Err(EconomyError::InsufficientCredits {
                required: 1_001,
                available: 1_000
            })
        );
        assert_eq!(w.credits(), 1_000, "failed calls change nothing");
        let mut rich = Wallet::new(i64::MAX);
        assert_eq!(rich.deposit(1), Err(EconomyError::ValueOverflow));
    }
}
