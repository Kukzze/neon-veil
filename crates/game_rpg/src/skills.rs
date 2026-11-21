//! The eight character skills and XP gain.

use crate::xp_table::{xp_to_level, MAX_LEVEL, XP_CAP};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const SKILL_COUNT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum SkillType {
    /// Parkour duration, vault speed, stamina recharge.
    Agility = 0,
    /// Blade/fist damage, combo speed, parry window.
    CyberMelee = 1,
    /// Spread bloom, recoil dampening, reload speed.
    Ballistics = 2,
    /// Cyber-deck RAM, intrusion puzzles, drone subversion.
    Netrunning = 3,
    /// Weapon fabrication, mod fusing efficiency.
    Engineering = 4,
    /// Footstep dampening, sight-cone camo, stealth takedowns.
    Shadow = 5,
    /// Max health and shields, cyberware tolerance, status resistance.
    Endurance = 6,
    /// Syndicate standing, vendor discounts, exchange tax relief.
    StreetCredit = 7,
}

impl SkillType {
    pub const ALL: [SkillType; SKILL_COUNT] = [
        Self::Agility,
        Self::CyberMelee,
        Self::Ballistics,
        Self::Netrunning,
        Self::Engineering,
        Self::Shadow,
        Self::Endurance,
        Self::StreetCredit,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Experience and current (possibly boosted) levels.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct CharacterSkills {
    pub xp: [u32; SKILL_COUNT],
    /// Effective levels including temporary boosts; follow the base level
    /// unless boosted.
    pub boosted_levels: [u8; SKILL_COUNT],
}

impl Default for CharacterSkills {
    fn default() -> Self {
        Self {
            xp: [0; SKILL_COUNT],
            boosted_levels: [1; SKILL_COUNT],
        }
    }
}

/// A skill crossed a level boundary.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelUpEvent {
    pub entity: Entity,
    pub skill: SkillType,
    pub new_level: u8,
}

/// Request to grant XP; resolved by [`apply_xp_grants`], which emits
/// [`LevelUpEvent`]s.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct XpGrantEvent {
    pub entity: Entity,
    pub skill: SkillType,
    pub amount: u32,
}

impl CharacterSkills {
    pub fn get_xp(&self, skill: SkillType) -> u32 {
        self.xp[skill.index()]
    }

    /// Base level from XP.
    pub fn get_level(&self, skill: SkillType) -> u8 {
        xp_to_level(self.get_xp(skill))
    }

    pub fn get_boosted_level(&self, skill: SkillType) -> u8 {
        self.boosted_levels[skill.index()]
    }

    /// Sum of base levels: 8 for a new character, 792 when maxed.
    pub fn get_total_level(&self) -> u16 {
        SkillType::ALL
            .iter()
            .map(|&s| self.get_level(s) as u16)
            .sum()
    }

    pub fn get_total_xp(&self) -> u64 {
        self.xp.iter().map(|&x| x as u64).sum()
    }

    /// Adds XP (capped at [`XP_CAP`]). Returns the new level on a level-up.
    /// A boosted level keeps its boost relative to the new base level.
    pub fn add_xp(&mut self, skill: SkillType, amount: u32) -> Option<u8> {
        let i = skill.index();
        let old_level = self.get_level(skill);
        self.xp[i] = self.xp[i].saturating_add(amount).min(XP_CAP);
        let new_level = self.get_level(skill);
        if new_level <= old_level {
            return None;
        }
        let boost = self.boosted_levels[i] as i16 - old_level as i16;
        self.boosted_levels[i] = (new_level as i16 + boost).clamp(1, u8::MAX as i16) as u8;
        Some(new_level)
    }

    /// Temporarily raises (or with a negative amount, drains) a level,
    /// limited to `base + amount` and never below 1.
    pub fn boost(&mut self, skill: SkillType, amount: i8) {
        let base = self.get_level(skill) as i16;
        self.boosted_levels[skill.index()] =
            (base + amount as i16).clamp(1, MAX_LEVEL as i16 + 20) as u8;
    }

    /// Ends every boost.
    pub fn restore_boosts(&mut self) {
        for skill in SkillType::ALL {
            self.boosted_levels[skill.index()] = self.get_level(skill);
        }
    }
}

/// Applies XP grants and emits level-ups.
pub fn apply_xp_grants(
    mut grants: EventReader<XpGrantEvent>,
    mut skills: Query<&mut CharacterSkills>,
    mut level_ups: EventWriter<LevelUpEvent>,
) {
    for grant in grants.read() {
        let Ok(mut character) = skills.get_mut(grant.entity) else {
            continue;
        };
        if let Some(new_level) = character.add_xp(grant.skill, grant.amount) {
            level_ups.send(LevelUpEvent {
                entity: grant.entity,
                skill: grant.skill,
                new_level,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xp_table::{level_to_xp, MAX_XP};

    #[test]
    fn new_character_is_level_one_everywhere() {
        let s = CharacterSkills::default();
        assert_eq!(s.get_total_level(), 8);
        assert_eq!(s.get_total_xp(), 0);
        for skill in SkillType::ALL {
            assert_eq!(s.get_level(skill), 1);
            assert_eq!(s.get_boosted_level(skill), 1);
        }
    }

    #[test]
    fn maxed_character_has_total_level_792() {
        let s = CharacterSkills {
            xp: [MAX_XP; SKILL_COUNT],
            boosted_levels: [99; SKILL_COUNT],
        };
        assert_eq!(s.get_total_level(), 792);
        assert_eq!(s.get_total_xp(), MAX_XP as u64 * 8);
    }

    #[test]
    fn xp_crossing_a_boundary_reports_the_level_up() {
        let mut s = CharacterSkills::default();
        assert_eq!(s.add_xp(SkillType::Agility, 82), None);
        assert_eq!(s.add_xp(SkillType::Agility, 1), Some(2));
        assert_eq!(
            s.add_xp(SkillType::Agility, 1_000),
            Some(9),
            "multi-level jump"
        );
        assert_eq!(s.get_level(SkillType::Agility), 9);
        assert_eq!(
            s.get_boosted_level(SkillType::Agility),
            9,
            "boost follows base"
        );
        assert_eq!(s.get_level(SkillType::Shadow), 1, "other skills untouched");
    }

    #[test]
    fn xp_is_capped_at_200_million() {
        let mut s = CharacterSkills::default();
        s.add_xp(SkillType::Ballistics, u32::MAX);
        assert_eq!(s.get_xp(SkillType::Ballistics), XP_CAP);
        assert_eq!(s.add_xp(SkillType::Ballistics, 1_000), None);
        assert_eq!(s.get_level(SkillType::Ballistics), 99);
    }

    #[test]
    fn boosts_survive_level_ups_and_restore() {
        let mut s = CharacterSkills::default();
        s.add_xp(SkillType::CyberMelee, level_to_xp(40));
        s.boost(SkillType::CyberMelee, 5);
        assert_eq!(s.get_boosted_level(SkillType::CyberMelee), 45);
        s.add_xp(SkillType::CyberMelee, level_to_xp(41) - level_to_xp(40));
        assert_eq!(s.get_boosted_level(SkillType::CyberMelee), 46, "boost kept");
        s.boost(SkillType::CyberMelee, -60);
        assert_eq!(
            s.get_boosted_level(SkillType::CyberMelee),
            1,
            "drain floors at 1"
        );
        s.restore_boosts();
        assert_eq!(s.get_boosted_level(SkillType::CyberMelee), 41);
    }

    #[test]
    fn grant_system_emits_level_up_events() {
        let mut app = App::new();
        app.add_event::<XpGrantEvent>()
            .add_event::<LevelUpEvent>()
            .add_systems(Update, apply_xp_grants);
        let e = app.world_mut().spawn(CharacterSkills::default()).id();
        for amount in [50, 50, 10] {
            app.world_mut().send_event(XpGrantEvent {
                entity: e,
                skill: SkillType::Endurance,
                amount,
            });
        }
        app.update();
        let events: Vec<_> = app
            .world_mut()
            .resource_mut::<Events<LevelUpEvent>>()
            .drain()
            .collect();
        assert_eq!(
            events,
            vec![LevelUpEvent {
                entity: e,
                skill: SkillType::Endurance,
                new_level: 2
            }]
        );
        let skills = app.world().get::<CharacterSkills>(e).expect("skills");
        assert_eq!(skills.get_xp(SkillType::Endurance), 110);
    }
}
