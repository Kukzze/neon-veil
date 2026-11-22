//! A character's look: body and face slider values plus colours, saved per
//! player and sent to other players in co-op.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const APPEARANCE_FILE: &str = "config/operative.json";

/// Skin tones offered by the creator, light to deep (sRGB).
pub const SKIN_TONES: [[f32; 3]; 8] = [
    [0.96, 0.80, 0.69],
    [0.91, 0.72, 0.58],
    [0.84, 0.64, 0.49],
    [0.76, 0.55, 0.40],
    [0.64, 0.45, 0.32],
    [0.52, 0.36, 0.25],
    [0.40, 0.27, 0.18],
    [0.29, 0.19, 0.13],
];

/// Suit and accent colours (sRGB); accents also glow.
pub const SUIT_COLORS: [[f32; 3]; 6] = [
    [0.07, 0.08, 0.10],
    [0.16, 0.17, 0.20],
    [0.10, 0.12, 0.18],
    [0.18, 0.10, 0.10],
    [0.08, 0.14, 0.12],
    [0.85, 0.86, 0.88],
];
pub const ACCENT_COLORS: [[f32; 3]; 6] = [
    [0.0, 0.9, 1.0],
    [1.0, 0.15, 0.55],
    [1.0, 0.6, 0.05],
    [0.5, 1.0, 0.2],
    [0.6, 0.35, 1.0],
    [1.0, 0.25, 0.2],
];
pub const EYE_COLORS: [[f32; 3]; 6] = [
    [0.28, 0.18, 0.10],
    [0.12, 0.35, 0.55],
    [0.20, 0.42, 0.22],
    [0.45, 0.45, 0.48],
    [0.55, 0.35, 0.12],
    [0.0, 0.85, 1.0],
];
pub const HAIR_COLORS: [[f32; 3]; 6] = [
    [0.04, 0.03, 0.03],
    [0.20, 0.12, 0.07],
    [0.45, 0.30, 0.15],
    [0.80, 0.68, 0.45],
    [0.62, 0.62, 0.64],
    [0.85, 0.15, 0.45],
];

/// What the operative wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Outfit {
    /// Briefs, plus a bra on feminine bodies.
    #[default]
    Underwear,
    /// Full-body operative suit.
    OperativeSuit,
}

impl Outfit {
    pub const ALL: [Self; 2] = [Self::Underwear, Self::OperativeSuit];

    pub fn label(self) -> &'static str {
        match self {
            Self::Underwear => "Underwear",
            Self::OperativeSuit => "Operative suit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HairStyle {
    Shaved,
    #[default]
    Buzz,
}

impl HairStyle {
    pub const ALL: [Self; 2] = [Self::Shaved, Self::Buzz];

    pub fn label(self) -> &'static str {
        match self {
            Self::Shaved => "Shaved",
            Self::Buzz => "Buzz cut",
        }
    }
}

/// Player-facing identity shown on their card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerCard {
    pub callsign: String,
    pub title: String,
    pub emblem: u8,
}

impl Default for PlayerCard {
    fn default() -> Self {
        Self {
            callsign: "Operative".into(),
            title: "Zero Protocol".into(),
            emblem: 0,
        }
    }
}

#[derive(Component, Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterAppearance {
    /// Slider id to value in [-1, 1]; missing sliders are neutral.
    pub sliders: BTreeMap<String, f32>,
    pub skin_tone: [f32; 3],
    pub eye_color: [f32; 3],
    pub hair: HairStyle,
    pub hair_color: [f32; 3],
    pub outfit: Outfit,
    /// Hair mesh worn (a proxy name), or empty for the painted style.
    pub hair_mesh: String,
    /// Eyebrow mesh, or empty for painted brows.
    pub brow_mesh: String,
    pub lashes: bool,
    /// Clothing worn over the base layer (proxy names).
    pub clothing: Vec<String>,
    pub suit: [f32; 3],
    pub accent: [f32; 3],
    pub card: PlayerCard,
}

impl Default for CharacterAppearance {
    fn default() -> Self {
        Self::preset(Preset::Vanguard)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Vanguard,
    Wraith,
    Hana,
    Jun,
    Bruiser,
    Runner,
}

impl Preset {
    pub const ALL: [Self; 6] = [
        Self::Vanguard,
        Self::Wraith,
        Self::Hana,
        Self::Jun,
        Self::Bruiser,
        Self::Runner,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Vanguard => "Vanguard",
            Self::Wraith => "Wraith",
            Self::Hana => "Hana",
            Self::Jun => "Jun",
            Self::Bruiser => "Bruiser",
            Self::Runner => "Runner",
        }
    }
}

impl CharacterAppearance {
    pub fn preset(preset: Preset) -> Self {
        let sliders: &[(&str, f32)] = match preset {
            Preset::Vanguard => &[
                ("gender", -0.9),
                ("muscle", 0.55),
                ("weight", -0.1),
                ("height", 0.25),
                ("proportions", 0.6),
                ("torso_vshape", 0.4),
                ("jaw_width", 0.3),
                ("jaw_bones", 0.4),
            ],
            Preset::Wraith => &[
                ("gender", 0.95),
                ("muscle", 0.3),
                ("weight", -0.25),
                ("height", 0.1),
                ("proportions", 0.7),
                ("cupsize", 0.1),
                ("cheekbones", 0.4),
                ("eye_size", 0.2),
            ],
            Preset::Hana => &[
                ("gender", 1.0),
                ("heritage_asian", 0.9),
                ("age", -0.35),
                ("muscle", 0.1),
                ("weight", -0.3),
                ("height", -0.15),
                ("proportions", 0.8),
                ("cupsize", 0.2),
                ("jaw_width", -0.35),
                ("chin_prominence", -0.15),
                ("nose_width", -0.25),
                ("eye_size", 0.25),
                ("cheek_volume", 0.15),
            ],
            Preset::Jun => &[
                ("gender", -0.85),
                ("heritage_asian", 0.85),
                ("age", -0.3),
                ("muscle", 0.45),
                ("weight", -0.2),
                ("height", 0.1),
                ("proportions", 0.75),
                ("torso_vshape", 0.3),
                ("jaw_width", -0.1),
                ("nose_width", -0.15),
            ],
            Preset::Bruiser => &[
                ("gender", -1.0),
                ("muscle", 1.0),
                ("weight", 0.45),
                ("height", 0.5),
                ("shoulders", 0.6),
                ("neck_width", 0.6),
                ("jaw_width", 0.7),
            ],
            Preset::Runner => &[
                ("gender", 0.85),
                ("muscle", 0.35),
                ("weight", -0.5),
                ("height", -0.1),
                ("upperleg_length", 0.3),
                ("lowerleg_length", 0.3),
            ],
        };
        let hair = match preset {
            Preset::Vanguard | Preset::Bruiser => "short02",
            Preset::Wraith => "bob02",
            Preset::Hana => "long01",
            Preset::Jun => "short04",
            Preset::Runner => "ponytail01",
        };
        let (skin, suit, accent) = match preset {
            Preset::Vanguard => (2, 0, 0),
            Preset::Wraith => (1, 2, 1),
            Preset::Hana => (1, 0, 1),
            Preset::Jun => (1, 0, 0),
            Preset::Bruiser => (5, 1, 2),
            Preset::Runner => (3, 4, 3),
        };
        Self {
            sliders: sliders.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(),
            skin_tone: SKIN_TONES[skin],
            eye_color: EYE_COLORS[0],
            hair: HairStyle::Buzz,
            hair_color: HAIR_COLORS[0],
            outfit: Outfit::Underwear,
            hair_mesh: hair.to_owned(),
            brow_mesh: "eyebrow001".to_owned(),
            lashes: true,
            clothing: match preset {
                Preset::Runner => vec!["female_sportsuit01".to_owned(), "shoes02".to_owned()],
                Preset::Bruiser => vec!["male_worksuit01".to_owned(), "shoes01".to_owned()],
                _ => Vec::new(),
            },
            suit: SUIT_COLORS[suit],
            accent: ACCENT_COLORS[accent],
            card: PlayerCard::default(),
        }
    }

    pub fn slider(&self, id: &str) -> f32 {
        self.sliders
            .get(id)
            .copied()
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
            .clamp(-1.0, 1.0)
    }

    pub fn set_slider(&mut self, id: &str, value: f32) {
        let value = if value.is_finite() {
            value.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        if value == 0.0 {
            self.sliders.remove(id);
        } else {
            self.sliders.insert(id.to_owned(), value);
        }
    }

    /// Randomizes sliders gently (most values stay near neutral) with a
    /// deterministic generator, keeping colours and the player card.
    pub fn randomize(&mut self, ids: &[&str], seed: u64) {
        let mut state = seed | 1;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f32 / (1u64 << 53) as f32
        };
        for id in ids {
            let r = next() * 2.0 - 1.0;
            let value = if *id == "gender" {
                r
            } else {
                r * r * r.signum() * 0.8
            };
            self.set_slider(id, value);
        }
        self.skin_tone = SKIN_TONES[(next() * SKIN_TONES.len() as f32) as usize % SKIN_TONES.len()];
    }

    /// Keeps names short and printable for the card and the network.
    pub fn sanitized(mut self) -> Self {
        let clean = |s: &str, max: usize| -> String {
            s.chars()
                .filter(|c| !c.is_control())
                .take(max)
                .collect::<String>()
                .trim()
                .to_owned()
        };
        self.card.callsign = clean(&self.card.callsign, 16);
        if self.card.callsign.is_empty() {
            self.card.callsign = PlayerCard::default().callsign;
        }
        self.card.title = clean(&self.card.title, 24);
        let safe_name =
            |s: &str| s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !safe_name(&self.hair_mesh) {
            self.hair_mesh.clear();
        }
        if !safe_name(&self.brow_mesh) {
            self.brow_mesh.clear();
        }
        self.clothing.retain(|c| safe_name(c));
        self.clothing.truncate(4);
        self.sliders.retain(|_, v| v.is_finite());
        for v in self.sliders.values_mut() {
            *v = v.clamp(-1.0, 1.0);
        }
        for c in [
            &mut self.skin_tone,
            &mut self.eye_color,
            &mut self.hair_color,
            &mut self.suit,
            &mut self.accent,
        ] {
            for x in c.iter_mut() {
                *x = if x.is_finite() {
                    x.clamp(0.0, 1.0)
                } else {
                    0.5
                };
            }
        }
        self
    }
}

/// Where the local player's appearance is saved.
#[derive(Resource, Debug, Clone)]
pub struct AppearancePath(pub PathBuf);

impl Default for AppearancePath {
    fn default() -> Self {
        Self(PathBuf::from(APPEARANCE_FILE))
    }
}

pub fn load_appearance(path: &Path) -> Option<CharacterAppearance> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<CharacterAppearance>(&text)
        .ok()
        .map(CharacterAppearance::sanitized)
}

pub fn save_appearance(path: &Path, appearance: &CharacterAppearance) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(appearance).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliders_clamp_and_drop_neutral() {
        let mut a = CharacterAppearance::default();
        a.set_slider("muscle", 3.0);
        assert_eq!(a.slider("muscle"), 1.0);
        a.set_slider("muscle", 0.0);
        assert!(!a.sliders.contains_key("muscle"));
        a.set_slider("weight", f32::NAN);
        assert_eq!(a.slider("weight"), 0.0);
        assert_eq!(a.slider("unknown"), 0.0);
    }

    #[test]
    fn presets_differ_and_randomize_is_deterministic() {
        let looks: Vec<_> = Preset::ALL
            .iter()
            .map(|p| CharacterAppearance::preset(*p))
            .collect();
        for (i, a) in looks.iter().enumerate() {
            for b in &looks[i + 1..] {
                assert_ne!(a, b);
            }
        }
        let ids = ["gender", "muscle", "nose_width"];
        let (mut a, mut b) = (
            CharacterAppearance::default(),
            CharacterAppearance::default(),
        );
        a.randomize(&ids, 42);
        b.randomize(&ids, 42);
        assert_eq!(a, b);
        assert!(ids.iter().all(|id| a.slider(id).abs() <= 1.0));
    }

    #[test]
    fn sanitizes_hostile_input_and_round_trips() {
        let mut a = CharacterAppearance::default();
        a.card.callsign = "\u{7}".repeat(3) + &"X".repeat(40);
        a.sliders.insert("muscle".into(), f32::INFINITY);
        a.skin_tone = [f32::NAN, 2.0, -1.0];
        let clean = a.sanitized();
        assert_eq!(clean.card.callsign.len(), 16);
        assert!(!clean.sliders.contains_key("muscle"));
        assert_eq!(clean.skin_tone, [0.5, 1.0, 0.0]);
        let path = std::env::temp_dir().join("neon_veil_operative_test.json");
        save_appearance(&path, &clean).expect("save");
        assert_eq!(load_appearance(&path), Some(clean));
        let _ = std::fs::remove_file(path);
    }
}
