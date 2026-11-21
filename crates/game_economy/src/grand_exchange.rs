//! Grand Exchange: escrowed buy/sell orders matched per item with
//! price-time priority, a 2% tax burned on every fill, and proceeds
//! delivered to collection boxes.
//!
//! Locking: each item has its own order book behind a `Mutex`, so trades on
//! different items run in parallel while every operation on one item is
//! serialized. Lock order is always book -> order index -> collection boxes,
//! and the item map and order index are never held while a book is being
//! acquired, so no cycle (deadlock) is possible.

use crate::error::EconomyError;
use crate::inventory::{CollectionBox, PlayerInventory};
use crate::item_catalog::{find_item, ItemDefinition};
use crate::wallet::Wallet;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use uuid::Uuid;

pub const TAX_RATE: f64 = 0.02;
const TAX_BASIS_POINTS: i128 = 200;

/// 2% of `trade_value`, rounded half up (matches `(v * 0.02).round()` for
/// non-negative values without floating point).
pub fn market_tax(trade_value: i64) -> i64 {
    ((trade_value as i128 * TAX_BASIS_POINTS + 5_000) / 10_000) as i64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    Active,
    PartiallyFilled,
    Filled,
    Cancelled,
}

impl OrderStatus {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Active | Self::PartiallyFilled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeOrder {
    pub order_id: Uuid,
    pub player_id: Uuid,
    pub item_id: u32,
    pub order_type: OrderType,
    pub unit_price: i64,
    pub quantity_total: u32,
    pub quantity_filled: u32,
    pub status: OrderStatus,
    /// Credits still held for the unfilled part of a buy order.
    pub escrow_credits: i64,
    /// Items still held for the unfilled part of a sell order.
    pub escrow_items: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Submission order; breaks price ties (earlier first).
    pub sequence: u64,
}

impl GeOrder {
    pub fn remaining_quantity(&self) -> u32 {
        self.quantity_total - self.quantity_filled
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeRecord {
    pub trade_id: Uuid,
    pub buy_order_id: Uuid,
    pub sell_order_id: Uuid,
    pub item_id: u32,
    pub executed_price: i64,
    pub quantity: u32,
    pub market_tax_collected: i64,
    pub executed_at: DateTime<Utc>,
}

/// Result of placing an order: its id and any fills it caused immediately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderReceipt {
    pub order_id: Uuid,
    pub trades: Vec<TradeRecord>,
}

#[derive(Debug, Default)]
struct OrderBook {
    orders: HashMap<Uuid, GeOrder>,
    trades: Vec<TradeRecord>,
}

impl OrderBook {
    fn best_open(&self, order_type: OrderType) -> Option<&GeOrder> {
        let open = self
            .orders
            .values()
            .filter(move |o| o.order_type == order_type && o.status.is_open());
        match order_type {
            // Highest bid, earliest first on ties.
            OrderType::Buy => open.max_by(|a, b| {
                a.unit_price
                    .cmp(&b.unit_price)
                    .then(b.sequence.cmp(&a.sequence))
            }),
            // Lowest ask, earliest first on ties.
            OrderType::Sell => open.min_by(|a, b| {
                a.unit_price
                    .cmp(&b.unit_price)
                    .then(a.sequence.cmp(&b.sequence))
            }),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Default)]
pub struct GrandExchangeEngine {
    books: RwLock<HashMap<u32, Arc<Mutex<OrderBook>>>>,
    order_items: RwLock<HashMap<Uuid, u32>>,
    collection_boxes: Mutex<HashMap<Uuid, CollectionBox>>,
    lifetime_burned_tax: AtomicI64,
    next_sequence: AtomicU64,
}

impl GrandExchangeEngine {
    pub fn new() -> Self {
        Self::default()
    }

    fn book(&self, item_id: u32) -> Arc<Mutex<OrderBook>> {
        if let Some(book) = self
            .books
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&item_id)
        {
            return Arc::clone(book);
        }
        let mut books = self.books.write().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(books.entry(item_id).or_default())
    }

    fn existing_book(&self, item_id: u32) -> Option<Arc<Mutex<OrderBook>>> {
        self.books
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&item_id)
            .cloned()
    }

    fn validate(
        item_id: u32,
        unit_price: i64,
        quantity: u32,
    ) -> Result<&'static ItemDefinition, EconomyError> {
        let item = find_item(item_id).ok_or(EconomyError::ItemNotFound(item_id))?;
        if !item.is_tradable {
            return Err(EconomyError::UntradableItem(item_id));
        }
        if unit_price <= 0 {
            return Err(EconomyError::InvalidUnitPrice(unit_price));
        }
        if quantity == 0 {
            return Err(EconomyError::InvalidQuantity(quantity));
        }
        Ok(item)
    }

    pub fn submit_buy_order(
        &self,
        player_id: Uuid,
        item_id: u32,
        unit_price: i64,
        quantity: u32,
        wallet: &mut Wallet,
    ) -> Result<Uuid, EconomyError> {
        self.place_buy_order(player_id, item_id, unit_price, quantity, wallet)
            .map(|receipt| receipt.order_id)
    }

    /// Escrows `unit_price * quantity` from `wallet`, then matches.
    pub fn place_buy_order(
        &self,
        player_id: Uuid,
        item_id: u32,
        unit_price: i64,
        quantity: u32,
        wallet: &mut Wallet,
    ) -> Result<OrderReceipt, EconomyError> {
        Self::validate(item_id, unit_price, quantity)?;
        let total_cost = unit_price
            .checked_mul(i64::from(quantity))
            .ok_or(EconomyError::ValueOverflow)?;
        wallet.withdraw(total_cost)?;
        Ok(self.open_order(
            player_id,
            item_id,
            OrderType::Buy,
            unit_price,
            quantity,
            total_cost,
            0,
        ))
    }

    pub fn submit_sell_order(
        &self,
        player_id: Uuid,
        item_id: u32,
        unit_price: i64,
        quantity: u32,
        inventory: &mut PlayerInventory,
    ) -> Result<Uuid, EconomyError> {
        self.place_sell_order(player_id, item_id, unit_price, quantity, inventory)
            .map(|receipt| receipt.order_id)
    }

    /// Escrows `quantity` items from `inventory`, then matches.
    pub fn place_sell_order(
        &self,
        player_id: Uuid,
        item_id: u32,
        unit_price: i64,
        quantity: u32,
        inventory: &mut PlayerInventory,
    ) -> Result<OrderReceipt, EconomyError> {
        Self::validate(item_id, unit_price, quantity)?;
        // A sale can be worth at most this much; reject before escrowing.
        unit_price
            .checked_mul(i64::from(quantity))
            .ok_or(EconomyError::ValueOverflow)?;
        inventory.remove_item(item_id, quantity)?;
        Ok(self.open_order(
            player_id,
            item_id,
            OrderType::Sell,
            unit_price,
            quantity,
            0,
            quantity,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn open_order(
        &self,
        player_id: Uuid,
        item_id: u32,
        order_type: OrderType,
        unit_price: i64,
        quantity: u32,
        escrow_credits: i64,
        escrow_items: u32,
    ) -> OrderReceipt {
        let now = Utc::now();
        let order = GeOrder {
            order_id: Uuid::new_v4(),
            player_id,
            item_id,
            order_type,
            unit_price,
            quantity_total: quantity,
            quantity_filled: 0,
            status: OrderStatus::Active,
            escrow_credits,
            escrow_items,
            created_at: now,
            updated_at: now,
            sequence: self.next_sequence.fetch_add(1, Ordering::Relaxed),
        };
        let order_id = order.order_id;
        let book = self.book(item_id);
        let mut book = lock(&book);
        book.orders.insert(order_id, order);
        self.order_items
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(order_id, item_id);
        let trades = self.match_book(item_id, &mut book);
        OrderReceipt { order_id, trades }
    }

    /// Matches crossing orders for `item_id` until the book no longer
    /// crosses. Normally called by order submission; safe to call any time.
    pub fn match_orders_for_item(&self, item_id: u32) -> Result<Vec<TradeRecord>, EconomyError> {
        find_item(item_id).ok_or(EconomyError::ItemNotFound(item_id))?;
        let Some(book) = self.existing_book(item_id) else {
            return Ok(Vec::new());
        };
        let mut book = lock(&book);
        Ok(self.match_book(item_id, &mut book))
    }

    fn match_book(&self, item_id: u32, book: &mut OrderBook) -> Vec<TradeRecord> {
        let mut trades = Vec::new();
        loop {
            let (Some(buy), Some(sell)) = (
                book.best_open(OrderType::Buy),
                book.best_open(OrderType::Sell),
            ) else {
                break;
            };
            if buy.unit_price < sell.unit_price {
                break;
            }
            let (buy_id, buy_price, buyer) = (buy.order_id, buy.unit_price, buy.player_id);
            let (sell_id, execution_price, seller) =
                (sell.order_id, sell.unit_price, sell.player_id);
            let quantity = buy.remaining_quantity().min(sell.remaining_quantity());
            let quantity_i64 = i64::from(quantity);

            // Both products are bounded by the buy order's escrow, which was
            // computed with checked arithmetic.
            let trade_value = execution_price * quantity_i64;
            let tax = market_tax(trade_value);
            let seller_payout = trade_value - tax;
            let buyer_refund = (buy_price - execution_price) * quantity_i64;
            let now = Utc::now();

            for (id, escrow_used) in [(buy_id, buy_price * quantity_i64), (sell_id, 0)] {
                let order = book.orders.get_mut(&id).expect("matched order is in book");
                order.quantity_filled += quantity;
                match order.order_type {
                    OrderType::Buy => order.escrow_credits -= escrow_used,
                    OrderType::Sell => order.escrow_items -= quantity,
                }
                order.status = if order.remaining_quantity() == 0 {
                    OrderStatus::Filled
                } else {
                    OrderStatus::PartiallyFilled
                };
                order.updated_at = now;
            }

            {
                let mut boxes = lock(&self.collection_boxes);
                let buyer_box = boxes.entry(buyer).or_default();
                buyer_box.deposit_item(item_id, quantity);
                buyer_box.deposit_credits(buyer_refund);
                boxes
                    .entry(seller)
                    .or_default()
                    .deposit_credits(seller_payout);
            }
            self.lifetime_burned_tax.fetch_add(tax, Ordering::Relaxed);

            let trade = TradeRecord {
                trade_id: Uuid::new_v4(),
                buy_order_id: buy_id,
                sell_order_id: sell_id,
                item_id,
                executed_price: execution_price,
                quantity,
                market_tax_collected: tax,
                executed_at: now,
            };
            book.trades.push(trade.clone());
            trades.push(trade);
        }
        trades
    }

    /// Cancels an open order owned by `player_id`, returning its remaining
    /// escrow to the player's collection box. Orders owned by someone else
    /// are reported as not found.
    pub fn cancel_order(&self, order_id: Uuid, player_id: Uuid) -> Result<(), EconomyError> {
        let item_id = self
            .order_items
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&order_id)
            .copied()
            .ok_or(EconomyError::OrderNotFound(order_id))?;
        let book = self
            .existing_book(item_id)
            .ok_or(EconomyError::OrderNotFound(order_id))?;
        let mut book = lock(&book);
        let order = book
            .orders
            .get_mut(&order_id)
            .filter(|o| o.player_id == player_id)
            .ok_or(EconomyError::OrderNotFound(order_id))?;
        if !order.status.is_open() {
            return Err(EconomyError::OrderNotActive(order_id));
        }
        let mut boxes = lock(&self.collection_boxes);
        let refund_box = boxes.entry(player_id).or_default();
        match order.order_type {
            OrderType::Buy => refund_box.deposit_credits(order.escrow_credits),
            OrderType::Sell => refund_box.deposit_item(item_id, order.escrow_items),
        }
        order.escrow_credits = 0;
        order.escrow_items = 0;
        order.status = OrderStatus::Cancelled;
        order.updated_at = Utc::now();
        Ok(())
    }

    pub fn get_order(&self, order_id: Uuid) -> Option<GeOrder> {
        let item_id = *self
            .order_items
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&order_id)?;
        let book = self.existing_book(item_id)?;
        let order = lock(&book).orders.get(&order_id).cloned();
        order
    }

    /// All of a player's orders, oldest first.
    pub fn get_player_orders(&self, player_id: Uuid) -> Vec<GeOrder> {
        let books: Vec<_> = self
            .books
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        let mut orders: Vec<GeOrder> = books
            .iter()
            .flat_map(|book| {
                lock(book)
                    .orders
                    .values()
                    .filter(|o| o.player_id == player_id)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        orders.sort_by_key(|o| o.sequence);
        orders
    }

    pub fn get_trade_history(&self, item_id: u32) -> Vec<TradeRecord> {
        self.existing_book(item_id)
            .map(|book| lock(&book).trades.clone())
            .unwrap_or_default()
    }

    pub fn total_tax_burned(&self) -> i64 {
        self.lifetime_burned_tax.load(Ordering::Relaxed)
    }

    /// Snapshot of a player's unclaimed proceeds.
    pub fn collection_box(&self, player_id: Uuid) -> CollectionBox {
        lock(&self.collection_boxes)
            .get(&player_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Moves a player's proceeds into their wallet and inventory. Items that
    /// do not fit stay in the box. Returns the credits and items moved.
    pub fn collect(
        &self,
        player_id: Uuid,
        wallet: &mut Wallet,
        inventory: &mut PlayerInventory,
    ) -> Result<(i64, HashMap<u32, u32>), EconomyError> {
        let mut boxes = lock(&self.collection_boxes);
        let Some(pending) = boxes.get_mut(&player_id) else {
            return Ok((0, HashMap::new()));
        };
        let credits = pending.claim_credits(wallet);
        let items = if pending.pending_items.is_empty() {
            HashMap::new()
        } else {
            pending.claim_items(inventory)?
        };
        if pending.is_empty() {
            boxes.remove(&player_id);
        }
        Ok((credits, items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
    use std::sync::mpsc;
    use std::time::Duration;

    const KATANA: u32 = 2001;

    struct Trader {
        id: Uuid,
        wallet: Wallet,
        inventory: PlayerInventory,
    }

    impl Trader {
        fn new(credits: i64, items: &[(u32, u32)]) -> Self {
            let mut inventory = PlayerInventory::new();
            for &(id, q) in items {
                inventory.add_item(id, q).unwrap();
            }
            Self {
                id: Uuid::new_v4(),
                wallet: Wallet::new(credits),
                inventory,
            }
        }

        fn collect(&mut self, ge: &GrandExchangeEngine) {
            ge.collect(self.id, &mut self.wallet, &mut self.inventory)
                .unwrap();
        }
    }

    #[test]
    fn tax_is_two_percent_rounded_half_up() {
        assert_eq!(market_tax(500), 10);
        assert_eq!(market_tax(800), 16);
        assert_eq!(market_tax(25), 1, "0.5 rounds up");
        assert_eq!(market_tax(24), 0);
        assert_eq!(market_tax(0), 0);
        for v in [1, 49, 50, 75, 1_234, 99_999, 1_000_000_007] {
            assert_eq!(market_tax(v), (v as f64 * TAX_RATE).round() as i64, "{v}");
        }
    }

    #[test]
    fn exact_price_match() {
        let ge = GrandExchangeEngine::new();
        let mut buyer = Trader::new(1_000, &[]);
        let mut seller = Trader::new(0, &[(KATANA, 1)]);
        let buy = ge
            .submit_buy_order(buyer.id, KATANA, 500, 1, &mut buyer.wallet)
            .unwrap();
        assert_eq!(buyer.wallet.credits(), 500, "500 escrowed");
        let receipt = ge
            .place_sell_order(seller.id, KATANA, 500, 1, &mut seller.inventory)
            .unwrap();
        assert_eq!(receipt.trades.len(), 1);
        let trade = &receipt.trades[0];
        assert_eq!(
            (
                trade.executed_price,
                trade.quantity,
                trade.market_tax_collected
            ),
            (500, 1, 10)
        );
        assert_eq!(
            (trade.buy_order_id, trade.sell_order_id),
            (buy, receipt.order_id)
        );
        assert_eq!(ge.total_tax_burned(), 10);
        assert_eq!(ge.collection_box(seller.id).pending_credits, 490);
        assert_eq!(
            ge.collection_box(buyer.id).pending_items,
            HashMap::from([(KATANA, 1)])
        );
        assert_eq!(ge.collection_box(buyer.id).pending_credits, 0, "no refund");
        buyer.collect(&ge);
        seller.collect(&ge);
        assert_eq!(buyer.inventory.item_count(KATANA), 1);
        assert_eq!(seller.wallet.credits(), 490);
        assert_eq!(ge.get_order(buy).unwrap().status, OrderStatus::Filled);
        assert_eq!(
            ge.get_order(receipt.order_id).unwrap().status,
            OrderStatus::Filled
        );
        assert_eq!(ge.get_trade_history(KATANA).len(), 1);
    }

    #[test]
    fn overbid_executes_at_ask_and_refunds_difference() {
        let ge = GrandExchangeEngine::new();
        let mut buyer = Trader::new(1_000, &[]);
        let mut seller = Trader::new(0, &[(KATANA, 1)]);
        ge.submit_sell_order(seller.id, KATANA, 800, 1, &mut seller.inventory)
            .unwrap();
        let receipt = ge
            .place_buy_order(buyer.id, KATANA, 1_000, 1, &mut buyer.wallet)
            .unwrap();
        assert_eq!(buyer.wallet.credits(), 0);
        let trade = &receipt.trades[0];
        assert_eq!(
            (trade.executed_price, trade.market_tax_collected),
            (800, 16)
        );
        let buyer_box = ge.collection_box(buyer.id);
        assert_eq!(buyer_box.pending_credits, 200, "refund of the overbid");
        assert_eq!(buyer_box.pending_items[&KATANA], 1);
        assert_eq!(ge.collection_box(seller.id).pending_credits, 784);
        assert_eq!(ge.total_tax_burned(), 16);
        buyer.collect(&ge);
        seller.collect(&ge);
        assert_eq!(
            buyer.wallet.credits() + seller.wallet.credits() + ge.total_tax_burned(),
            1_000
        );
    }

    #[test]
    fn partial_fill_leaves_remainder_open() {
        let ge = GrandExchangeEngine::new();
        let mut buyer = Trader::new(1_000, &[]);
        let mut seller = Trader::new(0, &[(1001, 4)]);
        let buy = ge
            .submit_buy_order(buyer.id, 1001, 100, 10, &mut buyer.wallet)
            .unwrap();
        ge.submit_sell_order(seller.id, 1001, 100, 4, &mut seller.inventory)
            .unwrap();
        let order = ge.get_order(buy).unwrap();
        assert_eq!(order.status, OrderStatus::PartiallyFilled);
        assert_eq!((order.quantity_filled, order.remaining_quantity()), (4, 6));
        assert_eq!(order.escrow_credits, 600);
        assert_eq!(ge.collection_box(seller.id).pending_credits, 392);
        assert_eq!(ge.total_tax_burned(), 8);
    }

    #[test]
    fn cancelling_returns_exactly_the_unspent_escrow() {
        let ge = GrandExchangeEngine::new();
        let mut buyer = Trader::new(1_000, &[]);
        let mut seller = Trader::new(0, &[(1001, 9)]);
        let buy = ge
            .submit_buy_order(buyer.id, 1001, 100, 10, &mut buyer.wallet)
            .unwrap();
        ge.submit_sell_order(seller.id, 1001, 100, 4, &mut seller.inventory)
            .unwrap();
        ge.cancel_order(buy, buyer.id).unwrap();
        assert_eq!(ge.get_order(buy).unwrap().status, OrderStatus::Cancelled);
        assert_eq!(ge.get_order(buy).unwrap().escrow_credits, 0);
        assert_eq!(ge.collection_box(buyer.id).pending_credits, 600);
        assert_eq!(
            ge.cancel_order(buy, buyer.id),
            Err(EconomyError::OrderNotActive(buy)),
            "no double refund"
        );
        assert_eq!(ge.collection_box(buyer.id).pending_credits, 600);

        let sell = ge
            .submit_sell_order(seller.id, 1001, 150, 5, &mut seller.inventory)
            .unwrap();
        assert_eq!(seller.inventory.item_count(1001), 0);
        assert_eq!(
            ge.cancel_order(sell, buyer.id),
            Err(EconomyError::OrderNotFound(sell)),
            "only the owner can cancel"
        );
        ge.cancel_order(sell, seller.id).unwrap();
        buyer.collect(&ge);
        seller.collect(&ge);
        assert_eq!(seller.inventory.item_count(1001), 5);
        assert_eq!(buyer.inventory.item_count(1001), 4);
        assert_eq!(buyer.wallet.credits(), 600);
        assert_eq!(seller.wallet.credits(), 392);
        assert_eq!(600 + 392 + ge.total_tax_burned(), 1_000);
        let ghost = Uuid::new_v4();
        assert_eq!(
            ge.cancel_order(ghost, buyer.id),
            Err(EconomyError::OrderNotFound(ghost))
        );
    }

    #[test]
    fn price_then_time_priority() {
        let ge = GrandExchangeEngine::new();
        let mut early = Trader::new(0, &[(1001, 5)]);
        let mut late = Trader::new(0, &[(1001, 5)]);
        let mut cheap = Trader::new(0, &[(1001, 5)]);
        let mut buyer = Trader::new(10_000, &[]);
        let first = ge
            .submit_sell_order(early.id, 1001, 60, 5, &mut early.inventory)
            .unwrap();
        let second = ge
            .submit_sell_order(late.id, 1001, 60, 5, &mut late.inventory)
            .unwrap();
        let lowest = ge
            .submit_sell_order(cheap.id, 1001, 55, 5, &mut cheap.inventory)
            .unwrap();
        let receipt = ge
            .place_buy_order(buyer.id, 1001, 70, 8, &mut buyer.wallet)
            .unwrap();
        let fills: Vec<(Uuid, i64, u32)> = receipt
            .trades
            .iter()
            .map(|t| (t.sell_order_id, t.executed_price, t.quantity))
            .collect();
        assert_eq!(fills, vec![(lowest, 55, 5), (first, 60, 3)]);
        assert_eq!(ge.get_order(second).unwrap().quantity_filled, 0);
        // Refund: 5 * (70 - 55) + 3 * (70 - 60).
        assert_eq!(ge.collection_box(buyer.id).pending_credits, 105);
    }

    #[test]
    fn orders_that_do_not_cross_rest() {
        let ge = GrandExchangeEngine::new();
        let mut buyer = Trader::new(1_000, &[]);
        let mut seller = Trader::new(0, &[(1001, 1)]);
        let buy = ge
            .submit_buy_order(buyer.id, 1001, 99, 1, &mut buyer.wallet)
            .unwrap();
        let sell = ge
            .submit_sell_order(seller.id, 1001, 100, 1, &mut seller.inventory)
            .unwrap();
        assert!(ge.match_orders_for_item(1001).unwrap().is_empty());
        assert_eq!(ge.get_order(buy).unwrap().status, OrderStatus::Active);
        assert_eq!(ge.get_order(sell).unwrap().status, OrderStatus::Active);
        assert_eq!(ge.get_player_orders(buyer.id).len(), 1);
        assert!(ge.get_trade_history(1001).is_empty());
        assert!(ge.match_orders_for_item(1002).unwrap().is_empty());
        assert_eq!(
            ge.match_orders_for_item(7),
            Err(EconomyError::ItemNotFound(7))
        );
    }

    #[test]
    fn validation_rejects_without_side_effects() {
        let ge = GrandExchangeEngine::new();
        let mut t = Trader::new(1_000, &[(1001, 5), (2030, 1)]);
        let id = t.id;
        assert_eq!(
            ge.submit_buy_order(id, 2030, 10, 1, &mut t.wallet),
            Err(EconomyError::UntradableItem(2030))
        );
        assert_eq!(
            ge.submit_sell_order(id, 2030, 10, 1, &mut t.inventory),
            Err(EconomyError::UntradableItem(2030))
        );
        assert_eq!(
            ge.submit_buy_order(id, 31_337, 10, 1, &mut t.wallet),
            Err(EconomyError::ItemNotFound(31_337))
        );
        assert_eq!(
            ge.submit_buy_order(id, 1001, 0, 1, &mut t.wallet),
            Err(EconomyError::InvalidUnitPrice(0))
        );
        assert_eq!(
            ge.submit_sell_order(id, 1001, -5, 1, &mut t.inventory),
            Err(EconomyError::InvalidUnitPrice(-5))
        );
        assert_eq!(
            ge.submit_buy_order(id, 1001, 10, 0, &mut t.wallet),
            Err(EconomyError::InvalidQuantity(0))
        );
        assert_eq!(
            ge.submit_buy_order(id, 1001, 101, 10, &mut t.wallet),
            Err(EconomyError::InsufficientCredits {
                required: 1_010,
                available: 1_000
            })
        );
        assert_eq!(
            ge.submit_sell_order(id, 1001, 10, 6, &mut t.inventory),
            Err(EconomyError::InsufficientItems {
                required: 6,
                available: 5
            })
        );
        assert_eq!(
            ge.submit_buy_order(id, 1001, i64::MAX, 2, &mut t.wallet),
            Err(EconomyError::ValueOverflow)
        );
        assert_eq!(t.wallet.credits(), 1_000);
        assert_eq!(t.inventory.item_count(1001), 5);
        assert!(ge.get_player_orders(id).is_empty());
    }

    #[test]
    fn concurrent_trading_conserves_credits_and_items() {
        const THREADS: usize = 20;
        const OPS: usize = 300;
        const START_CREDITS: i64 = 200_000;
        const ITEMS: [u32; 4] = [1001, 1003, 1014, 2001];
        const START_STOCK: u32 = 150;

        let ge = Arc::new(GrandExchangeEngine::new());
        let (done_tx, done_rx) = mpsc::channel();
        let engine = Arc::clone(&ge);
        std::thread::spawn(move || {
            let handles: Vec<_> = (0..THREADS)
                .map(|t| {
                    let ge = Arc::clone(&engine);
                    std::thread::spawn(move || {
                        let mut trader =
                            Trader::new(START_CREDITS, &ITEMS.map(|id| (id, START_STOCK)));
                        let mut rng = StdRng::seed_from_u64(t as u64);
                        for op in 0..OPS {
                            let item = ITEMS[rng.gen_range(0..ITEMS.len())];
                            let price = rng.gen_range(80..=120);
                            let quantity = rng.gen_range(1..=10);
                            let _ = if rng.gen_bool(0.5) {
                                ge.submit_buy_order(
                                    trader.id,
                                    item,
                                    price,
                                    quantity,
                                    &mut trader.wallet,
                                )
                            } else {
                                ge.submit_sell_order(
                                    trader.id,
                                    item,
                                    price,
                                    quantity,
                                    &mut trader.inventory,
                                )
                            };
                            if op % 25 == 0 {
                                if let Some(order) = ge
                                    .get_player_orders(trader.id)
                                    .into_iter()
                                    .find(|o| o.status.is_open())
                                {
                                    let _ = ge.cancel_order(order.order_id, trader.id);
                                }
                                trader.collect(&ge);
                            }
                        }
                        trader
                    })
                })
                .collect();
            let traders: Vec<Trader> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            done_tx.send(traders).unwrap();
        });
        let mut traders = done_rx
            .recv_timeout(Duration::from_secs(120))
            .expect("trading finished (no deadlock)");

        // The books must be uncrossed once every submission has matched.
        for item in ITEMS {
            let open: Vec<GeOrder> = traders
                .iter()
                .flat_map(|t| ge.get_player_orders(t.id))
                .filter(|o| o.item_id == item && o.status.is_open())
                .collect();
            let best_bid = open
                .iter()
                .filter(|o| o.order_type == OrderType::Buy)
                .map(|o| o.unit_price)
                .max();
            let best_ask = open
                .iter()
                .filter(|o| o.order_type == OrderType::Sell)
                .map(|o| o.unit_price)
                .min();
            if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
                assert!(bid < ask, "item {item} crossed: {bid} >= {ask}");
            }
        }

        let trades: Vec<TradeRecord> = ITEMS
            .iter()
            .flat_map(|&i| ge.get_trade_history(i))
            .collect();
        assert!(
            trades.len() > 100,
            "the run actually traded ({} fills)",
            trades.len()
        );
        assert_eq!(
            trades.iter().map(|t| t.market_tax_collected).sum::<i64>(),
            ge.total_tax_burned()
        );

        // Escrow + collection boxes + holdings + burned tax == starting supply.
        let escrowed_credits: i64 = traders
            .iter()
            .flat_map(|t| ge.get_player_orders(t.id))
            .map(|o| o.escrow_credits)
            .sum();
        let boxed_credits: i64 = traders
            .iter()
            .map(|t| ge.collection_box(t.id).pending_credits)
            .sum();
        let held: i64 = traders.iter().map(|t| t.wallet.credits()).sum();
        assert_eq!(
            held + escrowed_credits + boxed_credits + ge.total_tax_burned(),
            START_CREDITS * THREADS as i64
        );

        for trader in &mut traders {
            for order in ge.get_player_orders(trader.id) {
                if order.status.is_open() {
                    ge.cancel_order(order.order_id, trader.id).unwrap();
                }
            }
            trader.collect(&ge);
            assert!(ge.collection_box(trader.id).is_empty());
            assert!(ge
                .get_player_orders(trader.id)
                .iter()
                .all(|o| !o.status.is_open() && o.escrow_credits == 0 && o.escrow_items == 0));
        }
        let credits: i64 = traders.iter().map(|t| t.wallet.credits()).sum();
        assert_eq!(
            credits + ge.total_tax_burned(),
            START_CREDITS * THREADS as i64
        );
        for item in ITEMS {
            let total: u32 = traders.iter().map(|t| t.inventory.item_count(item)).sum();
            assert_eq!(total, START_STOCK * THREADS as u32, "item {item} conserved");
        }
        println!(
            "{} fills, {} credits burned as tax",
            trades.len(),
            ge.total_tax_burned()
        );
    }
}
