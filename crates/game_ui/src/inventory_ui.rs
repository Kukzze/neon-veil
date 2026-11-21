//! The 28-slot inventory panel (hotkey Tab).
//!
//! `InventoryGrid` is the player's slot arrangement over the economy's
//! `PlayerInventory` (which only knows item counts). Rearranging slots never
//! changes what the player owns; syncing keeps the arrangement while counts
//! change, puts new items in the first free slot and drops sold-out ones.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use game_core::identifiers::PlayerCharacterMarker;
use game_economy::item_catalog::{find_item, ItemDefinition};
use game_economy::{PlayerInventory, MAX_INVENTORY_SLOTS};

pub const GRID_COLUMNS: usize = 4;
pub const GRID_ROWS: usize = 7;
pub const INVENTORY_SLOT_COUNT: usize = GRID_COLUMNS * GRID_ROWS;

const _: () = assert!(INVENTORY_SLOT_COUNT == MAX_INVENTORY_SLOTS);

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct InventoryModalState {
    pub is_open: bool,
    /// Slot picked up by the first click of a move.
    pub dragging_slot_index: Option<usize>,
    pub selected_slot: Option<usize>,
}

impl InventoryModalState {
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
        if !self.is_open {
            self.dragging_slot_index = None;
        }
    }

    /// Click-to-move: the first click on an occupied slot picks it up, the
    /// second click drops it (swapping with whatever is there). Clicking
    /// the picked-up slot again cancels. Returns the swap performed.
    pub fn click_slot(&mut self, grid: &mut InventoryGrid, index: usize) -> Option<(usize, usize)> {
        if index >= INVENTORY_SLOT_COUNT {
            return None;
        }
        self.selected_slot = Some(index);
        match self.dragging_slot_index.take() {
            Some(from) if from == index => None,
            Some(from) => grid.swap_slots(from, index).then_some((from, index)),
            None => {
                if grid.slots[index].is_some() {
                    self.dragging_slot_index = Some(index);
                }
                None
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventorySlotData {
    pub item_id: u32,
    pub quantity: u32,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct InventoryGrid {
    pub slots: [Option<InventorySlotData>; INVENTORY_SLOT_COUNT],
}

impl Default for InventoryGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl InventoryGrid {
    pub fn new() -> Self {
        Self {
            slots: [None; INVENTORY_SLOT_COUNT],
        }
    }

    /// Swaps two slots (either may be empty). `false` if out of range.
    pub fn swap_slots(&mut self, a: usize, b: usize) -> bool {
        if a >= INVENTORY_SLOT_COUNT || b >= INVENTORY_SLOT_COUNT {
            return false;
        }
        self.slots.swap(a, b);
        true
    }

    /// `false` if out of range or `quantity` is zero.
    pub fn set_slot(&mut self, index: usize, item_id: u32, quantity: u32) -> bool {
        if index >= INVENTORY_SLOT_COUNT || quantity == 0 {
            return false;
        }
        self.slots[index] = Some(InventorySlotData { item_id, quantity });
        true
    }

    pub fn clear_slot(&mut self, index: usize) -> Option<InventorySlotData> {
        self.slots.get_mut(index)?.take()
    }

    pub fn total_item_count(&self) -> u32 {
        self.slots
            .iter()
            .flatten()
            .fold(0u32, |sum, s| sum.saturating_add(s.quantity))
    }

    pub fn used_slots(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn slot_of(&self, item_id: u32) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.is_some_and(|s| s.item_id == item_id))
    }

    /// Mirrors `inventory`'s counts while keeping the current arrangement.
    pub fn sync_with(&mut self, inventory: &PlayerInventory) {
        for slot in &mut self.slots {
            if let Some(data) = slot {
                match inventory.item_count(data.item_id) {
                    0 => *slot = None,
                    quantity => data.quantity = quantity,
                }
            }
        }
        let mut new_items: Vec<(u32, u32)> = inventory
            .items()
            .iter()
            .filter(|(id, _)| self.slot_of(**id).is_none())
            .map(|(id, q)| (*id, *q))
            .collect();
        new_items.sort_unstable();
        for (item_id, quantity) in new_items {
            if let Some(free) = self.slots.iter().position(Option::is_none) {
                self.slots[free] = Some(InventorySlotData { item_id, quantity });
            }
        }
    }

    pub fn from_inventory(inventory: &PlayerInventory) -> Self {
        let mut grid = Self::new();
        grid.sync_with(inventory);
        grid
    }
}

/// Keeps each player's grid in step with their inventory.
pub fn sync_inventory_grids(
    mut commands: Commands,
    mut players: Query<
        (Entity, &PlayerInventory, Option<&mut InventoryGrid>),
        Changed<PlayerInventory>,
    >,
) {
    for (entity, inventory, grid) in &mut players {
        match grid {
            Some(mut grid) => grid.sync_with(inventory),
            None => {
                commands
                    .entity(entity)
                    .insert(InventoryGrid::from_inventory(inventory));
            }
        }
    }
}

pub fn rarity_color(rarity: &str) -> egui::Color32 {
    match rarity {
        "Uncommon" => egui::Color32::from_rgb(46, 204, 113),
        "Rare" => egui::Color32::from_rgb(52, 152, 219),
        "Epic" => egui::Color32::from_rgb(171, 99, 230),
        "Legendary" => egui::Color32::from_rgb(255, 170, 0),
        "Exotic" => egui::Color32::from_rgb(0, 229, 255),
        _ => egui::Color32::from_gray(190),
    }
}

fn item_tooltip(ui: &mut egui::Ui, item: &ItemDefinition, quantity: u32) {
    ui.colored_label(
        rarity_color(item.rarity),
        egui::RichText::new(item.name).strong(),
    );
    ui.label(format!("{} // {}", item.rarity, item.category));
    ui.label(format!("Quantity: {quantity}"));
    ui.label(format!("Store price: {} cr", item.store_price));
    ui.label(format!("High alch: {} cr", item.high_alch_value));
    if !item.is_tradable {
        ui.colored_label(egui::Color32::from_rgb(255, 120, 90), "Untradable");
    }
}

pub fn render_inventory_egui(
    mut contexts: EguiContexts,
    mut state: ResMut<InventoryModalState>,
    mut players: Query<&mut InventoryGrid, With<PlayerCharacterMarker>>,
) {
    if !state.is_open {
        return;
    }
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let mut open = true;
    egui::Window::new("TACTICAL INVENTORY (28 SLOTS)")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            let Some(mut grid) = players.iter_mut().next() else {
                ui.label("No inventory.");
                return;
            };
            ui.label(format!(
                "{} / {INVENTORY_SLOT_COUNT} slots // {} items",
                grid.used_slots(),
                grid.total_item_count()
            ));
            egui::Grid::new("inventory_grid")
                .num_columns(GRID_COLUMNS)
                .spacing([6.0, 6.0])
                .show(ui, |ui| {
                    for index in 0..INVENTORY_SLOT_COUNT {
                        let slot = grid.slots[index];
                        let item = slot.and_then(|s| find_item(s.item_id));
                        let label = match (slot, item) {
                            (Some(s), Some(def)) => {
                                egui::RichText::new(format!("{}\nx{}", def.name, s.quantity))
                                    .color(rarity_color(def.rarity))
                            }
                            (Some(s), None) => {
                                egui::RichText::new(format!("#{}\nx{}", s.item_id, s.quantity))
                            }
                            (None, _) => egui::RichText::new("[Empty]").weak(),
                        };
                        let picked = state.dragging_slot_index == Some(index);
                        let button = egui::Button::new(label)
                            .min_size(egui::vec2(84.0, 48.0))
                            .wrap()
                            .selected(picked);
                        let mut response = ui.add(button);
                        if let (Some(s), Some(def)) = (slot, item) {
                            response = response.on_hover_ui(|ui| item_tooltip(ui, def, s.quantity));
                        }
                        if response.clicked() {
                            state.click_slot(&mut grid, index);
                        }
                        if (index + 1) % GRID_COLUMNS == 0 {
                            ui.end_row();
                        }
                    }
                });
            if state.dragging_slot_index.is_some() {
                ui.colored_label(
                    egui::Color32::from_rgb(0, 229, 255),
                    "Moving item: pick a destination slot",
                );
            }
        });
    if !open {
        state.is_open = false;
        state.dragging_slot_index = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_clears_a_pending_move() {
        let mut state = InventoryModalState::default();
        state.toggle();
        assert!(state.is_open);
        state.dragging_slot_index = Some(3);
        state.toggle();
        assert!(!state.is_open);
        assert_eq!(state.dragging_slot_index, None);
    }

    #[test]
    fn empty_grid() {
        let grid = InventoryGrid::new();
        assert_eq!(grid.total_item_count(), 0);
        assert_eq!(grid.used_slots(), 0);
        assert_eq!(grid.slots.iter().filter(|s| s.is_none()).count(), 28);
    }

    #[test]
    fn set_swap_clear() {
        let mut grid = InventoryGrid::new();
        assert!(grid.set_slot(0, 1001, 10));
        assert_eq!(
            grid.slots[0],
            Some(InventorySlotData {
                item_id: 1001,
                quantity: 10
            })
        );
        assert!(grid.swap_slots(0, 5));
        assert_eq!(grid.slots[0], None);
        assert_eq!(grid.slots[5].unwrap().item_id, 1001);
        assert!(!grid.swap_slots(5, 30), "out of range");
        assert!(!grid.swap_slots(28, 0));
        assert_eq!(
            grid.slots[5].unwrap().quantity,
            10,
            "failed swap changes nothing"
        );
        assert!(!grid.set_slot(28, 1002, 1));
        assert!(!grid.set_slot(1, 1002, 0));
        assert!(grid.set_slot(1, 1002, 7));
        assert_eq!((grid.total_item_count(), grid.used_slots()), (17, 2));
        assert_eq!(
            grid.clear_slot(1),
            Some(InventorySlotData {
                item_id: 1002,
                quantity: 7
            })
        );
        assert_eq!(grid.clear_slot(1), None);
        assert_eq!(grid.clear_slot(99), None);
        assert_eq!((grid.total_item_count(), grid.used_slots()), (10, 1));
    }

    #[test]
    fn click_to_move_swaps_and_cancels() {
        let mut grid = InventoryGrid::new();
        grid.set_slot(0, 1001, 10);
        grid.set_slot(2, 2001, 1);
        let mut state = InventoryModalState::default();
        assert_eq!(
            state.click_slot(&mut grid, 3),
            None,
            "empty slot picks up nothing"
        );
        assert_eq!(state.dragging_slot_index, None);
        assert_eq!(state.click_slot(&mut grid, 0), None);
        assert_eq!(state.dragging_slot_index, Some(0));
        assert_eq!(
            state.click_slot(&mut grid, 2),
            Some((0, 2)),
            "drop onto an occupied slot swaps"
        );
        assert_eq!(grid.slots[2].unwrap().item_id, 1001);
        assert_eq!(grid.slots[0].unwrap().item_id, 2001);
        state.click_slot(&mut grid, 0);
        assert_eq!(state.click_slot(&mut grid, 0), None, "same slot cancels");
        assert_eq!(state.dragging_slot_index, None);
        assert_eq!(grid.slots[0].unwrap().item_id, 2001);
        assert_eq!(state.click_slot(&mut grid, 40), None);
    }

    #[test]
    fn sync_keeps_the_arrangement() {
        let mut inventory = PlayerInventory::new();
        inventory.add_item(1001, 10).unwrap();
        inventory.add_item(2001, 1).unwrap();
        let mut grid = InventoryGrid::from_inventory(&inventory);
        assert_eq!(grid.slot_of(1001), Some(0));
        assert_eq!(grid.slot_of(2001), Some(1));
        grid.swap_slots(0, 10);
        inventory.add_item(1001, 5).unwrap();
        inventory.remove_item(2001, 1).unwrap();
        inventory.add_item(1021, 3).unwrap();
        grid.sync_with(&inventory);
        assert_eq!(
            grid.slots[10],
            Some(InventorySlotData {
                item_id: 1001,
                quantity: 15
            }),
            "moved stack stays put"
        );
        assert_eq!(grid.slot_of(2001), None, "sold out");
        assert_eq!(
            grid.slot_of(1021),
            Some(0),
            "new item takes the first free slot"
        );
        assert_eq!(grid.total_item_count(), 18);
    }

    #[test]
    fn system_attaches_and_syncs_grids() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_inventory_grids);
        let mut inventory = PlayerInventory::new();
        inventory.add_item(1001, 4).unwrap();
        let player = app.world_mut().spawn(inventory).id();
        app.update();
        assert_eq!(
            app.world()
                .get::<InventoryGrid>(player)
                .unwrap()
                .slot_of(1001),
            Some(0)
        );
        app.world_mut()
            .get_mut::<PlayerInventory>(player)
            .unwrap()
            .add_item(1002, 2)
            .unwrap();
        app.update();
        let grid = app.world().get::<InventoryGrid>(player).unwrap();
        assert_eq!((grid.used_slots(), grid.total_item_count()), (2, 6));
    }
}
