//! Steam achievement definitions. Ids must match the Steamworks partner
//! site configuration for app 3124500.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SteamAchievement {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
}

pub const ACH_PROTOCOL_ZERO: &str = "ACH_PROTOCOL_ZERO";
pub const ACH_WALLRUN_ELITE: &str = "ACH_WALLRUN_ELITE";
pub const ACH_PARRY_MASTER: &str = "ACH_PARRY_MASTER";
pub const ACH_LEVEL_99_AGILITY: &str = "ACH_LEVEL_99_AGILITY";
pub const ACH_LEVEL_99_MELEE: &str = "ACH_LEVEL_99_MELEE";
pub const ACH_LEVEL_99_NETRUNNING: &str = "ACH_LEVEL_99_NETRUNNING";
pub const ACH_GORGON_NIGHTMARE: &str = "ACH_GORGON_NIGHTMARE";
pub const ACH_GE_TYCOON: &str = "ACH_GE_TYCOON";
pub const ACH_ALL_QUESTS_COMPLETE: &str = "ACH_ALL_QUESTS_COMPLETE";

pub static STEAM_ACHIEVEMENTS: &[SteamAchievement] = &[
    SteamAchievement {
        id: ACH_PROTOCOL_ZERO,
        title: "Awakened Cyberware",
        description: "Complete the starting tutorial and escape the clinic into Neo-Veridia.",
    },
    SteamAchievement {
        id: ACH_WALLRUN_ELITE,
        title: "Gravity Defier",
        description: "Sustain a continuous wall-run for more than 50 meters across skybridges.",
    },
    SteamAchievement {
        id: ACH_PARRY_MASTER,
        title: "Temporal Reflexes",
        description: "Perform 100 perfect parries within the 120ms window without taking damage.",
    },
    SteamAchievement {
        id: ACH_LEVEL_99_AGILITY,
        title: "Master of Velocity",
        description: "Achieve Level 99 in Agility / Parkour (13,034,431 XP).",
    },
    SteamAchievement {
        id: ACH_LEVEL_99_MELEE,
        title: "Cyber-Samurai Grandmaster",
        description: "Achieve Level 99 in Cyber-Melee.",
    },
    SteamAchievement {
        id: ACH_LEVEL_99_NETRUNNING,
        title: "Ghost of the Net",
        description: "Achieve Level 99 in Netrunning.",
    },
    SteamAchievement {
        id: ACH_GORGON_NIGHTMARE,
        title: "Apex Predator",
        description: "Defeat Apex Enforcer Gorgon on Nightmare difficulty.",
    },
    SteamAchievement {
        id: ACH_GE_TYCOON,
        title: "Market Mogul",
        description: "Execute trades exceeding 10,000,000 Credits on the Grand Exchange.",
    },
    SteamAchievement {
        id: ACH_ALL_QUESTS_COMPLETE,
        title: "Legend of Neo-Veridia",
        description: "Successfully complete all 30 narrative quests.",
    },
];

pub fn find_achievement(id: &str) -> Option<&'static SteamAchievement> {
    STEAM_ACHIEVEMENTS.iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn nine_unique_achievements() {
        assert_eq!(STEAM_ACHIEVEMENTS.len(), 9);
        let ids: HashSet<&str> = STEAM_ACHIEVEMENTS.iter().map(|a| a.id).collect();
        assert_eq!(ids.len(), 9);
        for a in STEAM_ACHIEVEMENTS {
            assert!(
                a.id.starts_with("ACH_")
                    && a.id
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            );
            assert!(!a.title.is_empty() && !a.description.is_empty());
        }
    }

    #[test]
    fn lookup() {
        assert_eq!(
            find_achievement(ACH_PROTOCOL_ZERO).unwrap().title,
            "Awakened Cyberware"
        );
        assert_eq!(
            find_achievement(ACH_ALL_QUESTS_COMPLETE).unwrap().title,
            "Legend of Neo-Veridia"
        );
        assert_eq!(
            find_achievement(ACH_GE_TYCOON).unwrap().title,
            "Market Mogul"
        );
        assert!(find_achievement("ACH_NOPE").is_none());
    }
}
