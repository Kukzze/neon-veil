//! Errors shared by the wallet, inventory, exchange and crafting code.

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EconomyError {
    #[error("Insufficient funds. Required: {required}, Available: {available}")]
    InsufficientCredits { required: i64, available: i64 },
    #[error("Insufficient item quantity. Required: {required}, Available: {available}")]
    InsufficientItems { required: u32, available: u32 },
    #[error("Inventory is full (maximum {max} slots reached)")]
    InventoryFull { max: usize },
    #[error("Order has already been completed or cancelled: {0}")]
    OrderNotActive(Uuid),
    #[error("Order not found: {0}")]
    OrderNotFound(Uuid),
    #[error("Invalid unit price: {0}. Price must be greater than zero.")]
    InvalidUnitPrice(i64),
    #[error("Invalid quantity: {0}. Quantity must be greater than zero.")]
    InvalidQuantity(u32),
    #[error("Item with ID {0} is untradable")]
    UntradableItem(u32),
    #[error("Item with ID {0} does not exist in master catalog")]
    ItemNotFound(u32),
    #[error("Crafting level requirement not met: required {required}, player has {current}")]
    InsufficientSkillLevel { required: u8, current: u8 },
    #[error("Crafting recipe {0} not found")]
    RecipeNotFound(u32),
    #[error("Invalid credit amount: {0}")]
    InvalidAmount(i64),
    #[error("Credit or quantity arithmetic overflowed")]
    ValueOverflow,
    #[error("Item with ID {0} cannot be salvaged")]
    NotSalvageable(u32),
}
