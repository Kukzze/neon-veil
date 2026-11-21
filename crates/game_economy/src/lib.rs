//! Economy, crafting and loot systems.

pub mod crafting;
pub mod enemy_loot_tables;
pub mod error;
pub mod grand_exchange;
pub mod inventory;
pub mod item_catalog;
pub mod loot_engine;
pub mod wallet;

pub use crafting::{
    can_craft, craft_item, find_recipe, salvage_item, CraftingRecipe, MASTER_RECIPE_CATALOG,
};
pub use enemy_loot_tables::{get_enemy_loot_table, ENEMY_LOOT_TABLES};
pub use error::EconomyError;
pub use grand_exchange::{
    GeOrder, GrandExchangeEngine, OrderReceipt, OrderStatus, OrderType, TradeRecord, TAX_RATE,
};
pub use inventory::{CollectionBox, PlayerInventory, MAX_INVENTORY_SLOTS};
pub use item_catalog::{find_item, ItemDefinition, MASTER_ITEM_CATALOG};
pub use loot_engine::{LootDropEntry, LootTable};
pub use wallet::Wallet;

use bevy::prelude::*;
use std::sync::Arc;
use uuid::Uuid;

/// The shared exchange. Cloning shares the same engine.
#[derive(Resource, Debug, Clone, Default)]
pub struct GrandExchangeResource(pub Arc<GrandExchangeEngine>);

#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub struct OrderPlacedEvent {
    pub order_id: Uuid,
    pub player_id: Uuid,
    pub item_id: u32,
    pub order_type: OrderType,
    pub unit_price: i64,
    pub quantity: u32,
}

#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub struct TradeExecutedEvent {
    pub trade: TradeRecord,
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderCancelledEvent {
    pub order_id: Uuid,
    pub player_id: Uuid,
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemCraftedEvent {
    pub crafter: Entity,
    pub recipe_id: u32,
    pub item_id: u32,
    pub quantity: u32,
}

#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub struct ItemSalvagedEvent {
    pub salvager: Entity,
    pub item_id: u32,
    pub materials: Vec<(u32, u32)>,
}

#[derive(Event, Debug, Clone, PartialEq)]
pub struct LootDroppedEvent {
    /// The killed enemy (may already be despawned).
    pub source: Entity,
    pub enemy_type: &'static str,
    pub position: Vec3,
    pub drops: Vec<(u32, u32)>,
}

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GrandExchangeResource>()
            .add_event::<OrderPlacedEvent>()
            .add_event::<TradeExecutedEvent>()
            .add_event::<OrderCancelledEvent>()
            .add_event::<ItemCraftedEvent>()
            .add_event::<ItemSalvagedEvent>()
            .add_event::<LootDroppedEvent>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_registers_exchange_and_events() {
        let mut app = App::new();
        app.add_plugins(EconomyPlugin);
        let exchange = app.world().resource::<GrandExchangeResource>().clone();
        assert_eq!(exchange.0.total_tax_burned(), 0);
        for event_registered in [
            app.world().contains_resource::<Events<OrderPlacedEvent>>(),
            app.world()
                .contains_resource::<Events<TradeExecutedEvent>>(),
            app.world()
                .contains_resource::<Events<OrderCancelledEvent>>(),
            app.world().contains_resource::<Events<ItemCraftedEvent>>(),
            app.world().contains_resource::<Events<ItemSalvagedEvent>>(),
            app.world().contains_resource::<Events<LootDroppedEvent>>(),
        ] {
            assert!(event_registered);
        }
    }

    #[test]
    fn resource_clones_share_one_engine() {
        let a = GrandExchangeResource::default();
        let b = a.clone();
        assert!(Arc::ptr_eq(&a.0, &b.0));
    }
}
