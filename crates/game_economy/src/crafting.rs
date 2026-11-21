//! Engineering recipes, fabrication and salvage.

use crate::error::EconomyError;
use crate::inventory::PlayerInventory;
use crate::item_catalog::find_item;

#[derive(Debug, Clone, PartialEq)]
pub struct CraftingRecipe {
    pub recipe_id: u32,
    pub output_item_id: u32,
    pub output_quantity: u32,
    /// Engineering level required.
    pub required_skill_level: u8,
    pub crafting_duration_secs: f32,
    pub required_materials: &'static [(u32, u32)],
    /// Materials returned when one output item is broken down. Empty means
    /// the default: half of each material per item, rounded to nearest.
    pub salvage_yield: &'static [(u32, u32)],
}

const fn recipe(
    recipe_id: u32,
    output_item_id: u32,
    output_quantity: u32,
    required_skill_level: u8,
    crafting_duration_secs: f32,
    required_materials: &'static [(u32, u32)],
) -> CraftingRecipe {
    CraftingRecipe {
        recipe_id,
        output_item_id,
        output_quantity,
        required_skill_level,
        crafting_duration_secs,
        required_materials,
        salvage_yield: &[],
    }
}

#[rustfmt::skip]
pub static MASTER_RECIPE_CATALOG: &[CraftingRecipe] = &[
    // Tier 1 (levels 1-25)
    recipe(1, 2001, 1, 5, 15.0, &[(1001, 12), (1002, 6)]),
    recipe(2, 2002, 1, 8, 20.0, &[(1001, 16), (1003, 4), (1004, 2)]),
    recipe(3, 2003, 1, 12, 25.0, &[(1001, 14), (1006, 8), (1004, 3)]),
    recipe(4, 2004, 1, 15, 30.0, &[(1001, 20), (1007, 2), (1004, 4)]),
    recipe(5, 2031, 1, 18, 35.0, &[(1002, 24), (1005, 8)]),
    recipe(6, 1021, 5, 3, 8.0, &[(1002, 4), (1006, 2)]),
    recipe(7, 1022, 3, 15, 12.0, &[(1002, 8), (1006, 6)]),
    recipe(8, 1028, 3, 6, 10.0, &[(1002, 6), (1006, 4)]),
    recipe(9, 1029, 3, 14, 15.0, &[(1001, 8), (1006, 8)]),
    recipe(10, 1030, 3, 16, 15.0, &[(1001, 8), (1015, 4)]),
    recipe(11, 1031, 3, 17, 15.0, &[(1002, 8), (1019, 6)]),
    recipe(12, 1032, 3, 20, 15.0, &[(1002, 6), (1006, 8)]),
    // Tier 2 (levels 25-55)
    recipe(13, 2005, 1, 28, 45.0, &[(1001, 30), (1004, 10), (1003, 12)]),
    recipe(14, 2006, 1, 35, 60.0, &[(1001, 35), (1004, 12), (1008, 4)]),
    recipe(15, 2007, 1, 38, 65.0, &[(1001, 40), (1015, 10), (1005, 8)]),
    recipe(16, 2008, 1, 42, 70.0, &[(1014, 25), (1005, 12), (1008, 6)]),
    recipe(17, 2009, 1, 45, 80.0, &[(1003, 30), (1007, 10)]),
    recipe(18, 2038, 1, 42, 90.0, &[(1004, 20), (1009, 6), (1003, 16)]),
    recipe(19, 2040, 1, 48, 75.0, &[(1001, 40), (1005, 14), (1007, 4)]),
    recipe(20, 1023, 2, 35, 20.0, &[(1002, 12), (1006, 12), (1015, 4)]),
    recipe(21, 1024, 2, 40, 25.0, &[(1013, 4), (1008, 2)]),
    recipe(22, 1025, 2, 45, 25.0, &[(1006, 16), (1016, 2)]),
    recipe(23, 2019, 1, 50, 85.0, &[(1014, 20), (1008, 8), (1004, 10)]),
    recipe(24, 2041, 1, 30, 60.0, &[(1004, 15), (1016, 4)]),
    recipe(25, 1039, 2, 26, 20.0, &[(1001, 20), (1012, 10)]),
    recipe(26, 2035, 1, 52, 90.0, &[(1012, 30), (1001, 25)]),
    // Tier 3 (levels 55-85)
    recipe(27, 2010, 1, 60, 120.0, &[(1014, 50), (1008, 16), (1009, 8)]),
    recipe(28, 2011, 1, 65, 140.0, &[(1014, 45), (1017, 4), (1004, 30)]),
    recipe(29, 2012, 1, 70, 180.0, &[(1007, 20), (1014, 40), (1013, 8)]),
    recipe(30, 2013, 1, 75, 200.0, &[(1015, 30), (1014, 60), (1008, 12)]),
    recipe(31, 2032, 1, 62, 110.0, &[(1012, 40), (1005, 20), (1008, 8)]),
    recipe(32, 2034, 1, 68, 150.0, &[(1012, 50), (1017, 6), (1003, 40)]),
    recipe(33, 2036, 1, 72, 160.0, &[(1014, 40), (1016, 12), (1013, 10)]),
    recipe(34, 1035, 1, 58, 45.0, &[(1011, 8), (1019, 12)]),
    recipe(35, 1036, 1, 80, 90.0, &[(1018, 1), (1009, 10)]),
    recipe(36, 1040, 1, 78, 80.0, &[(1016, 6), (1020, 2)]),
    recipe(37, 2014, 1, 64, 130.0, &[(1011, 12), (1008, 10), (1017, 3)]),
    recipe(38, 2016, 1, 76, 210.0, &[(1014, 60), (1005, 24), (1001, 80)]),
    recipe(39, 2048, 1, 82, 190.0, &[(1013, 15), (1009, 6), (1008, 12)]),
    recipe(40, 2052, 1, 84, 200.0, &[(1016, 16), (1004, 40), (1017, 4)]),
    // Tier 4 (levels 85-99)
    recipe(41, 2020, 1, 85, 300.0, &[(1010, 5), (1009, 15), (1007, 25), (1001, 150)]),
    recipe(42, 2021, 1, 92, 420.0, &[(1010, 8), (1008, 30), (1001, 200)]),
    recipe(43, 2039, 1, 88, 360.0, &[(1010, 6), (1009, 20), (1017, 10)]),
    recipe(44, 2024, 1, 90, 380.0, &[(1018, 4), (1010, 4), (1017, 8)]),
    recipe(45, 2026, 1, 93, 440.0, &[(1010, 5), (1014, 120), (1008, 25)]),
    recipe(46, 2027, 1, 94, 450.0, &[(1009, 18), (1018, 2), (1014, 100)]),
    recipe(47, 2060, 1, 96, 480.0, &[(1018, 5), (1016, 20), (1010, 3)]),
    recipe(48, 2095, 1, 99, 600.0, &[(1010, 20), (1007, 50), (1002, 500)]),
    recipe(49, 2023, 1, 89, 340.0, &[(1020, 10), (1009, 12), (1014, 80)]),
    recipe(50, 2025, 1, 97, 500.0, &[(1010, 7), (1007, 40), (1012, 60)]),
];

pub fn find_recipe(recipe_id: u32) -> Option<&'static CraftingRecipe> {
    MASTER_RECIPE_CATALOG
        .iter()
        .find(|r| r.recipe_id == recipe_id)
}

pub fn find_recipe_for_output(item_id: u32) -> Option<&'static CraftingRecipe> {
    MASTER_RECIPE_CATALOG
        .iter()
        .find(|r| r.output_item_id == item_id)
}

/// Checks level, then materials, then that the output will have a slot
/// once the materials are consumed.
pub fn can_craft(
    recipe: &CraftingRecipe,
    engineering_level: u8,
    inventory: &PlayerInventory,
) -> Result<(), EconomyError> {
    preview_craft(recipe, engineering_level, inventory).map(|_| ())
}

fn preview_craft(
    recipe: &CraftingRecipe,
    engineering_level: u8,
    inventory: &PlayerInventory,
) -> Result<PlayerInventory, EconomyError> {
    if engineering_level < recipe.required_skill_level {
        return Err(EconomyError::InsufficientSkillLevel {
            required: recipe.required_skill_level,
            current: engineering_level,
        });
    }
    let mut next = inventory.clone();
    for &(item_id, quantity) in recipe.required_materials {
        next.remove_item(item_id, quantity)?;
    }
    next.add_item(recipe.output_item_id, recipe.output_quantity)?;
    Ok(next)
}

/// Consumes the materials and adds the output in one step; on error the
/// inventory is untouched. Returns the quantity produced.
pub fn craft_item(
    recipe: &CraftingRecipe,
    engineering_level: u8,
    inventory: &mut PlayerInventory,
) -> Result<u32, EconomyError> {
    *inventory = preview_craft(recipe, engineering_level, inventory)?;
    Ok(recipe.output_quantity)
}

/// What breaking down one `item_id` returns.
pub fn salvage_yield_for(item_id: u32) -> Result<Vec<(u32, u32)>, EconomyError> {
    find_item(item_id).ok_or(EconomyError::ItemNotFound(item_id))?;
    let recipe = find_recipe_for_output(item_id).ok_or(EconomyError::NotSalvageable(item_id))?;
    let materials: Vec<(u32, u32)> = if recipe.salvage_yield.is_empty() {
        let per_batch = 2 * recipe.output_quantity;
        recipe
            .required_materials
            .iter()
            .map(|&(id, q)| (id, (q + recipe.output_quantity) / per_batch))
            .filter(|&(_, q)| q > 0)
            .collect()
    } else {
        recipe.salvage_yield.to_vec()
    };
    if materials.is_empty() {
        return Err(EconomyError::NotSalvageable(item_id));
    }
    Ok(materials)
}

/// Breaks down one `item_id` into materials; atomic like `craft_item`.
pub fn salvage_item(
    item_id: u32,
    inventory: &mut PlayerInventory,
) -> Result<Vec<(u32, u32)>, EconomyError> {
    let materials = salvage_yield_for(item_id)?;
    let mut next = inventory.clone();
    next.remove_item(item_id, 1)?;
    for &(id, quantity) in &materials {
        next.add_item(id, quantity)?;
    }
    *inventory = next;
    Ok(materials)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn stocked(materials: &[(u32, u32)]) -> PlayerInventory {
        let mut inv = PlayerInventory::new();
        for &(id, q) in materials {
            inv.add_item(id, q).unwrap();
        }
        inv
    }

    #[test]
    fn fifty_recipes_numbered_1_to_50() {
        assert_eq!(MASTER_RECIPE_CATALOG.len(), 50);
        let ids: Vec<u32> = MASTER_RECIPE_CATALOG.iter().map(|r| r.recipe_id).collect();
        assert_eq!(ids, (1..=50).collect::<Vec<_>>());
        let outputs: HashSet<u32> = MASTER_RECIPE_CATALOG
            .iter()
            .map(|r| r.output_item_id)
            .collect();
        assert_eq!(outputs.len(), 50, "one recipe per output item");
    }

    #[test]
    fn every_referenced_item_exists() {
        for r in MASTER_RECIPE_CATALOG {
            assert!(
                find_item(r.output_item_id).is_some(),
                "recipe {} output",
                r.recipe_id
            );
            assert!(!r.required_materials.is_empty(), "recipe {}", r.recipe_id);
            for &(id, q) in r.required_materials.iter().chain(r.salvage_yield) {
                let item =
                    find_item(id).unwrap_or_else(|| panic!("recipe {} uses {id}", r.recipe_id));
                assert_eq!(
                    item.category, "Material",
                    "recipe {} input {id}",
                    r.recipe_id
                );
                assert!(q > 0);
            }
            assert!(r.output_quantity > 0 && r.crafting_duration_secs > 0.0);
        }
    }

    #[test]
    fn levels_fall_inside_their_tier() {
        for r in MASTER_RECIPE_CATALOG {
            let band = match r.recipe_id {
                1..=12 => 1..=25,
                13..=26 => 25..=55,
                27..=40 => 55..=85,
                _ => 85..=99,
            };
            assert!(
                band.contains(&r.required_skill_level),
                "recipe {}",
                r.recipe_id
            );
        }
    }

    #[test]
    fn level_gate() {
        let katana = find_recipe(1).unwrap();
        let inv = stocked(katana.required_materials);
        assert_eq!(
            can_craft(katana, 4, &inv),
            Err(EconomyError::InsufficientSkillLevel {
                required: 5,
                current: 4
            })
        );
        assert_eq!(can_craft(katana, 5, &inv), Ok(()));
        let cape = find_recipe(48).unwrap();
        assert!(matches!(
            can_craft(cape, 98, &stocked(cape.required_materials)),
            Err(EconomyError::InsufficientSkillLevel { required: 99, .. })
        ));
    }

    #[test]
    fn crafting_consumes_materials_and_adds_output() {
        let hypo = find_recipe(6).unwrap();
        let mut inv = stocked(&[(1002, 10), (1006, 2)]);
        assert_eq!(craft_item(hypo, 3, &mut inv), Ok(5));
        assert_eq!(inv.item_count(1002), 6);
        assert_eq!(inv.item_count(1006), 0);
        assert_eq!(inv.item_count(1021), 5);
        assert_eq!(inv.slot_count(), 2, "spent electrolyte freed its slot");
        assert_eq!(
            craft_item(hypo, 3, &mut inv),
            Err(EconomyError::InsufficientItems {
                required: 2,
                available: 0
            })
        );
        assert_eq!(inv.item_count(1002), 6, "failed craft consumes nothing");
    }

    #[test]
    fn full_inventory_blocks_crafting_atomically() {
        let katana = find_recipe(1).unwrap();
        let mut inv = stocked(&[(1001, 13), (1002, 7)]);
        for id in 0..26 {
            inv.add_item(5000 + id, 1).unwrap();
        }
        let before = inv.clone();
        assert_eq!(
            craft_item(katana, 99, &mut inv),
            Err(EconomyError::InventoryFull { max: 28 })
        );
        assert_eq!(inv, before);
        // Using up a whole stack frees the slot the output needs.
        let mut exact = stocked(katana.required_materials);
        for id in 0..26 {
            exact.add_item(5000 + id, 1).unwrap();
        }
        assert_eq!(craft_item(katana, 99, &mut exact), Ok(1));
        assert_eq!(exact.item_count(2001), 1);
    }

    #[test]
    fn salvage_returns_half_the_materials() {
        let mut inv = stocked(&[(2001, 1)]);
        let returned = salvage_item(2001, &mut inv).unwrap();
        assert_eq!(returned, vec![(1001, 6), (1002, 3)]);
        assert_eq!(inv.item_count(2001), 0);
        assert_eq!((inv.item_count(1001), inv.item_count(1002)), (6, 3));
        // 16 / 4 / 2 halved, rounded to nearest: 8, 2, 1.
        assert_eq!(
            salvage_yield_for(2002),
            Ok(vec![(1001, 8), (1003, 2), (1004, 1)])
        );
    }

    #[test]
    fn salvage_rejections() {
        let mut inv = PlayerInventory::new();
        assert_eq!(
            salvage_item(2001, &mut inv),
            Err(EconomyError::InsufficientItems {
                required: 1,
                available: 0
            })
        );
        assert_eq!(
            salvage_item(1001, &mut inv),
            Err(EconomyError::NotSalvageable(1001))
        );
        assert_eq!(
            salvage_item(424_242, &mut inv),
            Err(EconomyError::ItemNotFound(424_242))
        );
        // A single Hypo I is worth less than one of any material.
        assert_eq!(
            salvage_yield_for(1021),
            Err(EconomyError::NotSalvageable(1021))
        );
    }

    #[test]
    fn craft_then_salvage_never_creates_materials() {
        for r in MASTER_RECIPE_CATALOG {
            let Ok(back) = salvage_yield_for(r.output_item_id) else {
                continue;
            };
            for (id, q) in back {
                let spent = r.required_materials.iter().find(|m| m.0 == id).unwrap().1;
                assert!(
                    q * r.output_quantity <= spent,
                    "recipe {} material {id}",
                    r.recipe_id
                );
            }
        }
    }
}
