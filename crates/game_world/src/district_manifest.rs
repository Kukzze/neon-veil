//! The four districts of Neo-Veridia and their spatial extents.
//!
//! Bounds are half-open (`min <= p < max`), so districts that share a face
//! (Kuroshio's ceiling is the Spire's floor at 480 m) never both claim a
//! point. The one volume where two districts genuinely overlap, the strip
//! where the Foundry rises into Kuroshio's lowest level (y 220-250,
//! z 1000-1200), resolves by catalog order: the Foundry.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum DistrictId {
    /// Sector 0: ground level, heavy rain, toxic alleys.
    UndercitySlums,
    /// Sector 4: +50 m, molten slag and steam geysers.
    IndustrialFoundry,
    /// Sector 2: +250 m, neon avenues and bounce awnings.
    KuroshioCommercial,
    /// Sector 1: +800 m, corporate monoliths and skybridges.
    SpireMegatowers,
}

impl DistrictId {
    pub const ALL: [Self; 4] = [
        Self::UndercitySlums,
        Self::IndustrialFoundry,
        Self::KuroshioCommercial,
        Self::SpireMegatowers,
    ];

    pub const fn sector(self) -> u8 {
        match self {
            Self::UndercitySlums => 0,
            Self::SpireMegatowers => 1,
            Self::KuroshioCommercial => 2,
            Self::IndustrialFoundry => 4,
        }
    }

    pub fn zone(self) -> &'static DistrictZone {
        DISTRICT_CATALOG
            .iter()
            .find(|zone| zone.id == self)
            .expect("every district has a catalog entry")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DistrictZone {
    pub id: DistrictId,
    pub name: &'static str,
    pub min_bounds: Vec3,
    pub max_bounds: Vec3,
    pub ambient_color: Color,
    pub fog_density: f32,
    pub rain_intensity: f32,
    pub background_ambience_path: &'static str,
}

impl DistrictZone {
    /// Half-open containment: `min <= pos < max` on every axis.
    pub fn contains(&self, pos: Vec3) -> bool {
        pos.cmpge(self.min_bounds).all() && pos.cmplt(self.max_bounds).all()
    }

    pub fn center(&self) -> Vec3 {
        (self.min_bounds + self.max_bounds) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max_bounds - self.min_bounds
    }
}

/// Lookup priority is catalog order.
pub static DISTRICT_CATALOG: &[DistrictZone] = &[
    DistrictZone {
        id: DistrictId::UndercitySlums,
        name: "The Undercity Slums (Sector 0)",
        min_bounds: Vec3::new(-1000.0, -50.0, -1000.0),
        max_bounds: Vec3::new(1000.0, 80.0, 1000.0),
        ambient_color: Color::srgb(0.04, 0.08, 0.06),
        fog_density: 0.045,
        rain_intensity: 0.85,
        background_ambience_path: "audio/ambience/undercity_rain.ogg",
    },
    DistrictZone {
        id: DistrictId::IndustrialFoundry,
        name: "Industrial Foundry Sector (Sector 4)",
        min_bounds: Vec3::new(-1500.0, 50.0, 1000.0),
        max_bounds: Vec3::new(1500.0, 250.0, 2500.0),
        ambient_color: Color::srgb(0.12, 0.05, 0.02),
        fog_density: 0.035,
        rain_intensity: 0.20,
        background_ambience_path: "audio/ambience/foundry_furnace.ogg",
    },
    DistrictZone {
        id: DistrictId::KuroshioCommercial,
        name: "Kuroshio Commercial Hub (Sector 2)",
        min_bounds: Vec3::new(-1200.0, 220.0, -1200.0),
        max_bounds: Vec3::new(1200.0, 480.0, 1200.0),
        ambient_color: Color::srgb(0.02, 0.09, 0.14),
        fog_density: 0.020,
        rain_intensity: 0.60,
        background_ambience_path: "audio/ambience/kuroshio_neon.ogg",
    },
    DistrictZone {
        id: DistrictId::SpireMegatowers,
        name: "The Spire Megatowers (Sector 1)",
        min_bounds: Vec3::new(-800.0, 480.0, -800.0),
        max_bounds: Vec3::new(800.0, 1200.0, 800.0),
        ambient_color: Color::srgb(0.01, 0.04, 0.08),
        fog_density: 0.012,
        rain_intensity: 0.40,
        background_ambience_path: "audio/ambience/spire_highwind.ogg",
    },
];

/// The district enclosing `pos`, or `None` in the open air between them.
pub fn get_district_at_position(pos: Vec3) -> Option<&'static DistrictZone> {
    DISTRICT_CATALOG.iter().find(|zone| zone.contains(pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_at(x: f32, y: f32, z: f32) -> Option<DistrictId> {
        get_district_at_position(Vec3::new(x, y, z)).map(|zone| zone.id)
    }

    #[test]
    fn four_districts_each_listed_once() {
        assert_eq!(DISTRICT_CATALOG.len(), 4);
        for id in DistrictId::ALL {
            assert_eq!(DISTRICT_CATALOG.iter().filter(|z| z.id == id).count(), 1);
            assert_eq!(id.zone().id, id);
        }
        let sectors: Vec<u8> = DistrictId::ALL.iter().map(|d| d.sector()).collect();
        assert_eq!(sectors, vec![0, 4, 2, 1]);
    }

    #[test]
    fn catalog_values_match_the_design() {
        let slums = DistrictId::UndercitySlums.zone();
        assert_eq!(slums.min_bounds, Vec3::new(-1000.0, -50.0, -1000.0));
        assert_eq!(slums.max_bounds, Vec3::new(1000.0, 80.0, 1000.0));
        assert_eq!((slums.fog_density, slums.rain_intensity), (0.045, 0.85));
        assert_eq!(slums.ambient_color, Color::srgb(0.04, 0.08, 0.06));
        let spire = DistrictId::SpireMegatowers.zone();
        assert_eq!(
            spire.background_ambience_path,
            "audio/ambience/spire_highwind.ogg"
        );
        for zone in DISTRICT_CATALOG {
            assert!(zone.size().min_element() > 0.0, "{}", zone.name);
            assert!((0.0..=1.0).contains(&zone.rain_intensity));
            assert!(zone.fog_density > 0.0);
            assert!(zone.background_ambience_path.ends_with(".ogg"));
        }
    }

    #[test]
    fn point_queries() {
        assert_eq!(id_at(0.0, 10.0, 0.0), Some(DistrictId::UndercitySlums));
        assert_eq!(id_at(0.0, 300.0, 0.0), Some(DistrictId::KuroshioCommercial));
        assert_eq!(id_at(0.0, 600.0, 0.0), Some(DistrictId::SpireMegatowers));
        assert_eq!(
            id_at(500.0, 150.0, 1500.0),
            Some(DistrictId::IndustrialFoundry)
        );
        assert_eq!(id_at(0.0, 150.0, 0.0), None, "open air between levels");
        assert_eq!(id_at(0.0, 1200.0, 0.0), None, "above the Spire");
        assert_eq!(id_at(5000.0, 10.0, 0.0), None);
    }

    #[test]
    fn shared_faces_belong_to_exactly_one_district() {
        assert_eq!(id_at(0.0, 480.0, 0.0), Some(DistrictId::SpireMegatowers));
        assert_eq!(id_at(0.0, 479.9, 0.0), Some(DistrictId::KuroshioCommercial));
        assert_eq!(
            id_at(0.0, 60.0, 1000.0),
            Some(DistrictId::IndustrialFoundry)
        );
        assert_eq!(id_at(0.0, 60.0, 999.9), Some(DistrictId::UndercitySlums));
    }

    #[test]
    fn only_foundry_and_kuroshio_overlap_and_foundry_wins() {
        let mut overlapping = Vec::new();
        for (i, a) in DISTRICT_CATALOG.iter().enumerate() {
            for b in &DISTRICT_CATALOG[i + 1..] {
                let lo = a.min_bounds.max(b.min_bounds);
                let hi = a.max_bounds.min(b.max_bounds);
                if lo.cmplt(hi).all() {
                    overlapping.push((a.id, b.id, lo, hi));
                }
            }
        }
        assert_eq!(overlapping.len(), 1, "{overlapping:?}");
        let (a, b, lo, hi) = overlapping[0];
        assert_eq!(
            (a, b),
            (
                DistrictId::IndustrialFoundry,
                DistrictId::KuroshioCommercial
            )
        );
        assert_eq!(
            (lo, hi),
            (
                Vec3::new(-1200.0, 220.0, 1000.0),
                Vec3::new(1200.0, 250.0, 1200.0)
            )
        );
        assert_eq!(
            id_at(0.0, 235.0, 1100.0),
            Some(DistrictId::IndustrialFoundry)
        );
    }
}
