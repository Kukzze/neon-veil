//! Platform services: achievements, rich presence and cloud saves.
//!
//! With the `steam` feature and a running Steam client the native
//! Steamworks service is used; otherwise (DRM-free builds, tests, Steam not
//! running) everything goes to the in-memory [`MockPlatformService`], so
//! game code never branches on the platform.

pub mod achievements;
pub mod mock;
#[cfg(feature = "steam")]
pub mod native;
pub mod rich_presence;

pub use achievements::{find_achievement, SteamAchievement, STEAM_ACHIEVEMENTS};
pub use mock::{MockPlatformService, PlatformService};

use achievements::{
    ACH_ALL_QUESTS_COMPLETE, ACH_LEVEL_99_AGILITY, ACH_LEVEL_99_MELEE, ACH_LEVEL_99_NETRUNNING,
    ACH_PROTOCOL_ZERO,
};
use bevy::prelude::*;
use game_core::identifiers::PlayerCharacterMarker;
use game_rpg::quest_catalog::MASTER_QUEST_CATALOG;
use game_rpg::xp_table::MAX_LEVEL;
use game_rpg::{CharacterSkills, PlayerQuestLog, SkillType};
use std::sync::Arc;

pub const NEON_VEIL_APP_ID: u32 = 3_124_500;
/// The tutorial quest whose completion unlocks `ACH_PROTOCOL_ZERO`.
pub const TUTORIAL_QUEST_ID: u32 = 1;

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct SteamPlatformManager {
    pub is_steam_active: bool,
    pub app_id: u32,
}

impl Default for SteamPlatformManager {
    fn default() -> Self {
        Self {
            is_steam_active: false,
            app_id: NEON_VEIL_APP_ID,
        }
    }
}

/// The active platform service.
#[derive(Resource, Clone)]
pub struct PlatformServiceResource(pub Arc<dyn PlatformService>);

impl PlatformServiceResource {
    pub fn mock() -> Self {
        Self(Arc::new(MockPlatformService::new()))
    }
}

/// Raised once per achievement when this session unlocks it.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AchievementUnlockedEvent {
    pub id: &'static str,
}

/// Achievements a set of skills has earned.
pub fn skill_achievements(skills: &CharacterSkills) -> impl Iterator<Item = &'static str> + '_ {
    [
        (SkillType::Agility, ACH_LEVEL_99_AGILITY),
        (SkillType::CyberMelee, ACH_LEVEL_99_MELEE),
        (SkillType::Netrunning, ACH_LEVEL_99_NETRUNNING),
    ]
    .into_iter()
    .filter(|(skill, _)| skills.get_level(*skill) >= MAX_LEVEL)
    .map(|(_, id)| id)
}

/// Achievements a quest log has earned.
pub fn quest_achievements(log: &PlayerQuestLog) -> impl Iterator<Item = &'static str> + '_ {
    let all_done = MASTER_QUEST_CATALOG.iter().all(|q| log.is_completed(q.id));
    [
        (log.is_completed(TUTORIAL_QUEST_ID), ACH_PROTOCOL_ZERO),
        (all_done, ACH_ALL_QUESTS_COMPLETE),
    ]
    .into_iter()
    .filter(|(earned, _)| *earned)
    .map(|(_, id)| id)
}

fn unlock(
    platform: &PlatformServiceResource,
    id: &'static str,
    events: &mut EventWriter<AchievementUnlockedEvent>,
) {
    if !platform.0.is_achievement_unlocked(id) && platform.0.unlock_achievement(id) {
        events.send(AchievementUnlockedEvent { id });
    }
}

/// Unlocks skill and quest achievements as the player earns them
/// (including when a loaded save already qualifies).
pub fn unlock_progress_achievements(
    platform: Res<PlatformServiceResource>,
    skills: Query<&CharacterSkills, (With<PlayerCharacterMarker>, Changed<CharacterSkills>)>,
    quests: Query<&PlayerQuestLog, (With<PlayerCharacterMarker>, Changed<PlayerQuestLog>)>,
    mut events: EventWriter<AchievementUnlockedEvent>,
) {
    for skills in &skills {
        for id in skill_achievements(skills) {
            unlock(&platform, id, &mut events);
        }
    }
    for log in &quests {
        for id in quest_achievements(log) {
            unlock(&platform, id, &mut events);
        }
    }
}

/// The native Steam service if Steam is running (registering its callback
/// pump), otherwise the offline service.
#[cfg(feature = "steam")]
fn connect_platform(app: &mut App) -> PlatformServiceResource {
    let app_id = app.world().resource::<SteamPlatformManager>().app_id;
    match native::SteamPlatformService::init(app_id) {
        Ok((service, callbacks)) => {
            app.insert_resource(callbacks)
                .add_systems(First, native::run_steam_callbacks);
            app.world_mut()
                .resource_mut::<SteamPlatformManager>()
                .is_steam_active = true;
            PlatformServiceResource(Arc::new(service))
        }
        Err(error) => {
            warn!("Steam unavailable ({error}); using offline platform services");
            PlatformServiceResource::mock()
        }
    }
}

#[cfg(not(feature = "steam"))]
fn connect_platform(_app: &mut App) -> PlatformServiceResource {
    PlatformServiceResource::mock()
}

/// Picks the native Steam service when available, otherwise the offline
/// mock. A service inserted before the plugin is kept.
pub struct SteamworksPlugin;

impl Plugin for SteamworksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SteamPlatformManager>()
            .add_event::<AchievementUnlockedEvent>();
        if !app.world().contains_resource::<PlatformServiceResource>() {
            let service = connect_platform(app);
            app.insert_resource(service);
        }
        app.add_systems(
            Update,
            (
                unlock_progress_achievements,
                rich_presence::update_steam_rich_presence,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_parkour::ParkourState;
    use game_rpg::xp_table::level_to_xp;

    fn app_with(mock: Arc<MockPlatformService>) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(PlatformServiceResource(mock))
            .add_plugins(SteamworksPlugin);
        app
    }

    #[test]
    fn defaults_to_the_offline_service() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, SteamworksPlugin));
        let manager = app.world().resource::<SteamPlatformManager>();
        assert_eq!(
            (manager.is_steam_active, manager.app_id),
            (false, 3_124_500)
        );
        assert_eq!(
            app.world()
                .resource::<PlatformServiceResource>()
                .0
                .backend_name(),
            "offline"
        );
        app.update();
    }

    #[test]
    fn earned_achievements_from_state() {
        let mut skills = CharacterSkills::default();
        assert_eq!(skill_achievements(&skills).count(), 0);
        skills.add_xp(SkillType::Agility, level_to_xp(99));
        skills.add_xp(SkillType::Netrunning, level_to_xp(98));
        assert_eq!(
            skill_achievements(&skills).collect::<Vec<_>>(),
            vec![ACH_LEVEL_99_AGILITY]
        );

        let mut log = PlayerQuestLog::default();
        assert_eq!(quest_achievements(&log).count(), 0);
        log.completed_quests.insert(TUTORIAL_QUEST_ID);
        assert_eq!(
            quest_achievements(&log).collect::<Vec<_>>(),
            vec![ACH_PROTOCOL_ZERO]
        );
        log.completed_quests.extend(1..=30);
        assert_eq!(
            quest_achievements(&log).collect::<Vec<_>>(),
            vec![ACH_PROTOCOL_ZERO, ACH_ALL_QUESTS_COMPLETE]
        );
    }

    #[test]
    fn playing_unlocks_achievements_once() {
        let mock = Arc::new(MockPlatformService::new());
        let mut app = app_with(Arc::clone(&mock));
        let player = app
            .world_mut()
            .spawn((
                PlayerCharacterMarker,
                CharacterSkills::default(),
                PlayerQuestLog::default(),
                Transform::from_xyz(0.0, 10.0, 0.0),
                ParkourState::Sprinting,
            ))
            .id();
        app.update();
        assert!(mock.unlocked_achievements().is_empty());
        assert_eq!(
            mock.rich_presence(),
            "Level 8 // The Undercity Slums (Sector 0) // Sprinting"
        );

        app.world_mut()
            .get_mut::<PlayerQuestLog>(player)
            .unwrap()
            .completed_quests
            .insert(TUTORIAL_QUEST_ID);
        app.world_mut()
            .get_mut::<CharacterSkills>(player)
            .unwrap()
            .add_xp(SkillType::CyberMelee, level_to_xp(99));
        app.update();
        assert!(mock.is_achievement_unlocked(ACH_PROTOCOL_ZERO));
        assert!(mock.is_achievement_unlocked(ACH_LEVEL_99_MELEE));
        let events: Vec<_> = app
            .world_mut()
            .resource_mut::<Events<AchievementUnlockedEvent>>()
            .drain()
            .collect();
        assert_eq!(events.len(), 2);

        // Further changes do not re-announce what is already unlocked.
        app.world_mut()
            .get_mut::<CharacterSkills>(player)
            .unwrap()
            .add_xp(SkillType::CyberMelee, 1);
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Events<AchievementUnlockedEvent>>()
                .drain()
                .count(),
            0
        );
        assert!(mock.rich_presence().starts_with("Level 106 //"));
    }
}
