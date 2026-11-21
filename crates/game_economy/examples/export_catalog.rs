//! Writes the item catalog as JSON for the web companion portal.
//!
//! `cargo run -p game_economy --example export_catalog > web/companion/src/data/item-catalog.json`

fn main() {
    println!("{}", game_economy::item_catalog::catalog_json());
}
