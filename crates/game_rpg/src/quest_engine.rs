//! Quest lifecycle: gating, step progression, completion rewards, failure.
//!
//! A quest can start once its prerequisite quests are complete and every
//! skill requirement is met. Each `advance_step` moves through the quest's
//! steps; finishing the last one completes the quest and pays its XP into
//! the player's skills. A failed quest can be retried.

use crate::quest_catalog::find_quest;
use crate::skills::{CharacterSkills, LevelUpEvent, SkillType};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Debug, PartialEq, Hash)]
pub enum QuestDifficulty {
    Novice,
    Intermediate,
    Experienced,
    Master,
    Grandmaster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum QuestStatus {
    NotStarted,
    InProgress,
    Completed,
    Failed,
}

/// Static description of a quest.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuestDefinition {
    pub id: u32,
    pub title: &'static str,
    pub difficulty: QuestDifficulty,
    pub skill_requirements: &'static [(SkillType, u8)],
    pub quest_prerequisites: &'static [u32],
    pub quest_giver: &'static str,
    pub district: &'static str,
    pub synopsis: &'static str,
    pub steps: &'static [&'static str],
    pub failure_conditions: &'static [&'static str],
    pub xp_rewards: &'static [(SkillType, u32)],
    pub credit_reward: u32,
    pub item_rewards: &'static [&'static str],
}

/// Progress through an active quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestProgress {
    pub quest_id: u32,
    /// Steps completed so far (the index of the current step).
    pub current_step: usize,
}

/// Why a quest cannot start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum QuestStartError {
    #[error("quest {0} does not exist")]
    UnknownQuest(u32),
    #[error("quest already completed")]
    AlreadyCompleted,
    #[error("quest already in progress")]
    AlreadyInProgress,
    #[error("requires quest {0} to be completed first")]
    MissingPrerequisite(u32),
    #[error("requires {skill:?} level {required} (currently {current})")]
    SkillTooLow {
        skill: SkillType,
        required: u8,
        current: u8,
    },
}

/// Rewards granted when a quest completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestCompletion {
    pub quest_id: u32,
    pub credit_reward: u32,
    pub item_rewards: &'static [&'static str],
    /// Skills that levelled up from the XP reward, with their new level.
    pub level_ups: Vec<(SkillType, u8)>,
}

/// A player's quest history.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerQuestLog {
    pub active_quests: HashMap<u32, QuestProgress>,
    pub completed_quests: HashSet<u32>,
    pub failed_quests: HashSet<u32>,
}

impl PlayerQuestLog {
    pub fn status(&self, quest_id: u32) -> QuestStatus {
        if self.completed_quests.contains(&quest_id) {
            QuestStatus::Completed
        } else if self.active_quests.contains_key(&quest_id) {
            QuestStatus::InProgress
        } else if self.failed_quests.contains(&quest_id) {
            QuestStatus::Failed
        } else {
            QuestStatus::NotStarted
        }
    }

    /// Checks, in order: not completed, not in progress, prerequisites,
    /// then skill requirements (against base levels).
    pub fn can_start_quest(
        &self,
        quest: &QuestDefinition,
        skills: &CharacterSkills,
    ) -> Result<(), QuestStartError> {
        if self.completed_quests.contains(&quest.id) {
            return Err(QuestStartError::AlreadyCompleted);
        }
        if self.active_quests.contains_key(&quest.id) {
            return Err(QuestStartError::AlreadyInProgress);
        }
        if let Some(&missing) = quest
            .quest_prerequisites
            .iter()
            .find(|id| !self.completed_quests.contains(id))
        {
            return Err(QuestStartError::MissingPrerequisite(missing));
        }
        for &(skill, required) in quest.skill_requirements {
            let current = skills.get_level(skill);
            if current < required {
                return Err(QuestStartError::SkillTooLow {
                    skill,
                    required,
                    current,
                });
            }
        }
        Ok(())
    }

    /// Starts `quest` at step 0, clearing any earlier failure.
    pub fn start_quest(
        &mut self,
        quest: &QuestDefinition,
        skills: &CharacterSkills,
    ) -> Result<(), QuestStartError> {
        self.can_start_quest(quest, skills)?;
        self.failed_quests.remove(&quest.id);
        self.active_quests.insert(
            quest.id,
            QuestProgress {
                quest_id: quest.id,
                current_step: 0,
            },
        );
        Ok(())
    }

    /// Completes the current step. After the final step the quest moves to
    /// completed and its XP is added to `skills`. Returns the completion, or
    /// `None` if the quest is still in progress or not active.
    pub fn advance_step(
        &mut self,
        quest_id: u32,
        skills: &mut CharacterSkills,
    ) -> Option<QuestCompletion> {
        let quest = find_quest(quest_id)?;
        let progress = self.active_quests.get_mut(&quest_id)?;
        progress.current_step += 1;
        if progress.current_step < quest.steps.len() {
            return None;
        }
        self.active_quests.remove(&quest_id);
        self.completed_quests.insert(quest_id);
        let level_ups = quest
            .xp_rewards
            .iter()
            .filter_map(|&(skill, xp)| skills.add_xp(skill, xp).map(|level| (skill, level)))
            .collect();
        Some(QuestCompletion {
            quest_id,
            credit_reward: quest.credit_reward,
            item_rewards: quest.item_rewards,
            level_ups,
        })
    }

    /// Fails an active quest. Returns whether it was active.
    pub fn fail_quest(&mut self, quest_id: u32) -> bool {
        if self.active_quests.remove(&quest_id).is_some() {
            self.failed_quests.insert(quest_id);
            true
        } else {
            false
        }
    }

    pub fn is_completed(&self, quest_id: u32) -> bool {
        self.completed_quests.contains(&quest_id)
    }

    pub fn total_completed(&self) -> usize {
        self.completed_quests.len()
    }

    pub fn current_step(&self, quest_id: u32) -> Option<usize> {
        self.active_quests.get(&quest_id).map(|p| p.current_step)
    }
}

/// Request to start, advance or fail a quest for a player.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestCommand {
    Start { player: Entity, quest_id: u32 },
    AdvanceStep { player: Entity, quest_id: u32 },
    Fail { player: Entity, quest_id: u32 },
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestStartedEvent {
    pub player: Entity,
    pub quest_id: u32,
}

/// `step_index` is the step just completed.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestStepAdvancedEvent {
    pub player: Entity,
    pub quest_id: u32,
    pub step_index: usize,
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestCompletedEvent {
    pub player: Entity,
    pub quest_id: u32,
    pub credit_reward: u32,
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestFailedEvent {
    pub player: Entity,
    pub quest_id: u32,
}

/// A start request that was refused.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestStartRejectedEvent {
    pub player: Entity,
    pub quest_id: u32,
    pub reason: QuestStartError,
}

/// Applies quest commands and reports the outcome as events.
#[allow(clippy::too_many_arguments)]
pub fn process_quest_commands(
    mut commands: EventReader<QuestCommand>,
    mut players: Query<(&mut PlayerQuestLog, &mut CharacterSkills)>,
    mut started: EventWriter<QuestStartedEvent>,
    mut rejected: EventWriter<QuestStartRejectedEvent>,
    mut advanced: EventWriter<QuestStepAdvancedEvent>,
    mut completed: EventWriter<QuestCompletedEvent>,
    mut failed: EventWriter<QuestFailedEvent>,
    mut level_ups: EventWriter<LevelUpEvent>,
) {
    for &command in commands.read() {
        match command {
            QuestCommand::Start { player, quest_id } => {
                let Ok((mut log, skills)) = players.get_mut(player) else {
                    continue;
                };
                let result = find_quest(quest_id)
                    .ok_or(QuestStartError::UnknownQuest(quest_id))
                    .and_then(|quest| log.start_quest(quest, &skills));
                match result {
                    Ok(()) => {
                        started.send(QuestStartedEvent { player, quest_id });
                    }
                    Err(reason) => {
                        rejected.send(QuestStartRejectedEvent {
                            player,
                            quest_id,
                            reason,
                        });
                    }
                }
            }
            QuestCommand::AdvanceStep { player, quest_id } => {
                let Ok((mut log, mut skills)) = players.get_mut(player) else {
                    continue;
                };
                let Some(step_index) = log.current_step(quest_id) else {
                    continue;
                };
                let completion = log.advance_step(quest_id, &mut skills);
                advanced.send(QuestStepAdvancedEvent {
                    player,
                    quest_id,
                    step_index,
                });
                if let Some(completion) = completion {
                    for (skill, new_level) in completion.level_ups {
                        level_ups.send(LevelUpEvent {
                            entity: player,
                            skill,
                            new_level,
                        });
                    }
                    completed.send(QuestCompletedEvent {
                        player,
                        quest_id,
                        credit_reward: completion.credit_reward,
                    });
                }
            }
            QuestCommand::Fail { player, quest_id } => {
                let Ok((mut log, _)) = players.get_mut(player) else {
                    continue;
                };
                if log.fail_quest(quest_id) {
                    failed.send(QuestFailedEvent { player, quest_id });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xp_table::level_to_xp;

    fn quest(id: u32) -> &'static QuestDefinition {
        find_quest(id).expect("quest in catalog")
    }

    fn skills_at(level: u8) -> CharacterSkills {
        let mut s = CharacterSkills::default();
        for skill in SkillType::ALL {
            s.add_xp(skill, level_to_xp(level));
        }
        s
    }

    fn with_skill(skill: SkillType, level: u8) -> CharacterSkills {
        let mut s = CharacterSkills::default();
        s.add_xp(skill, level_to_xp(level));
        s
    }

    fn completed(ids: impl IntoIterator<Item = u32>) -> PlayerQuestLog {
        PlayerQuestLog {
            completed_quests: ids.into_iter().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn skill_gate_blocks_agility_4_from_q02() {
        let log = completed([1]);
        assert_eq!(
            log.can_start_quest(quest(2), &with_skill(SkillType::Agility, 4)),
            Err(QuestStartError::SkillTooLow {
                skill: SkillType::Agility,
                required: 5,
                current: 4
            })
        );
        assert_eq!(
            log.can_start_quest(quest(2), &with_skill(SkillType::Agility, 5)),
            Ok(())
        );
    }

    #[test]
    fn prerequisite_gate_blocks_q02_without_q01() {
        let log = PlayerQuestLog::default();
        assert_eq!(
            log.can_start_quest(quest(2), &with_skill(SkillType::Agility, 5)),
            Err(QuestStartError::MissingPrerequisite(1))
        );
    }

    #[test]
    fn q01_runs_seven_steps_and_pays_its_xp() {
        let mut log = PlayerQuestLog::default();
        let mut skills = CharacterSkills::default();
        log.start_quest(quest(1), &skills).expect("no requirements");
        assert_eq!(log.status(1), QuestStatus::InProgress);
        assert_eq!(quest(1).steps.len(), 7);
        for step in 0..6 {
            assert_eq!(log.current_step(1), Some(step));
            assert!(
                log.advance_step(1, &mut skills).is_none(),
                "step {step} is not the last"
            );
        }
        let completion = log
            .advance_step(1, &mut skills)
            .expect("seventh step completes");
        assert_eq!(completion.credit_reward, 1_200);
        assert_eq!(completion.item_rewards, &["Stalker Trenchcoat"]);
        assert_eq!(log.status(1), QuestStatus::Completed);
        assert!(log.is_completed(1));
        assert_eq!(log.total_completed(), 1);
        assert_eq!(skills.get_xp(SkillType::Agility), 450);
        assert_eq!(skills.get_xp(SkillType::Ballistics), 300);
        assert_eq!(
            completion.level_ups,
            vec![(SkillType::Agility, 5), (SkillType::Ballistics, 4)]
        );
        assert!(
            log.advance_step(1, &mut skills).is_none(),
            "no double completion"
        );
        assert_eq!(skills.get_xp(SkillType::Agility), 450);
    }

    #[test]
    fn q01_rewards_unlock_q02_immediately() {
        let mut log = PlayerQuestLog::default();
        let mut skills = CharacterSkills::default();
        log.start_quest(quest(1), &skills).expect("start");
        while log.advance_step(1, &mut skills).is_none() {}
        assert_eq!(
            log.can_start_quest(quest(2), &skills),
            Ok(()),
            "450 Agility XP is level 5"
        );
    }

    #[test]
    fn cannot_start_twice_or_repeat_a_completed_quest() {
        let mut log = PlayerQuestLog::default();
        let mut skills = CharacterSkills::default();
        log.start_quest(quest(1), &skills).expect("start");
        assert_eq!(
            log.start_quest(quest(1), &skills),
            Err(QuestStartError::AlreadyInProgress)
        );
        while log.advance_step(1, &mut skills).is_none() {}
        assert_eq!(
            log.start_quest(quest(1), &skills),
            Err(QuestStartError::AlreadyCompleted)
        );
    }

    #[test]
    fn failed_quests_can_be_retried_from_the_start() {
        let mut log = PlayerQuestLog::default();
        let mut skills = CharacterSkills::default();
        log.start_quest(quest(1), &skills).expect("start");
        log.advance_step(1, &mut skills);
        log.advance_step(1, &mut skills);
        assert!(log.fail_quest(1));
        assert_eq!(log.status(1), QuestStatus::Failed);
        assert!(!log.fail_quest(1), "not active any more");
        assert!(log.advance_step(1, &mut skills).is_none());
        log.start_quest(quest(1), &skills).expect("retry");
        assert_eq!(log.current_step(1), Some(0));
        assert_eq!(log.status(1), QuestStatus::InProgress);
        assert!(!log.failed_quests.contains(&1));
    }

    #[test]
    fn grandmaster_gate_needs_all_88s_and_quests_23_to_28() {
        let q29 = quest(29);
        let all_prereqs = completed(23..=28);

        assert!(matches!(
            all_prereqs.can_start_quest(q29, &skills_at(87)),
            Err(QuestStartError::SkillTooLow {
                required: 88,
                current: 87,
                ..
            })
        ));
        let mut nearly = skills_at(88);
        nearly.xp[SkillType::StreetCredit.index()] = level_to_xp(87);
        assert_eq!(
            all_prereqs.can_start_quest(q29, &nearly),
            Err(QuestStartError::SkillTooLow {
                skill: SkillType::StreetCredit,
                required: 88,
                current: 87
            })
        );
        assert_eq!(
            completed([23, 24, 25, 26, 27]).can_start_quest(q29, &skills_at(88)),
            Err(QuestStartError::MissingPrerequisite(28))
        );
        assert_eq!(all_prereqs.can_start_quest(q29, &skills_at(88)), Ok(()));
    }

    #[test]
    fn maxed_player_can_finish_the_whole_campaign_in_order() {
        let mut log = PlayerQuestLog::default();
        let mut skills = skills_at(99);
        let mut credits = 0;
        for id in 1..=30 {
            log.start_quest(quest(id), &skills)
                .unwrap_or_else(|e| panic!("Q{id:02}: {e}"));
            loop {
                if let Some(done) = log.advance_step(id, &mut skills) {
                    credits += done.credit_reward;
                    break;
                }
            }
        }
        assert_eq!(log.total_completed(), 30);
        assert!(log.active_quests.is_empty());
        assert!(
            credits >= 1_000_000,
            "the campaign pays at least the Q29 purse"
        );
    }

    #[test]
    fn command_system_emits_lifecycle_events() {
        let mut app = App::new();
        app.add_event::<QuestCommand>()
            .add_event::<QuestStartedEvent>()
            .add_event::<QuestStartRejectedEvent>()
            .add_event::<QuestStepAdvancedEvent>()
            .add_event::<QuestCompletedEvent>()
            .add_event::<QuestFailedEvent>()
            .add_event::<LevelUpEvent>()
            .add_systems(Update, process_quest_commands);
        let player = app
            .world_mut()
            .spawn((PlayerQuestLog::default(), CharacterSkills::default()))
            .id();

        app.world_mut().send_event(QuestCommand::Start {
            player,
            quest_id: 2,
        });
        app.world_mut().send_event(QuestCommand::Start {
            player,
            quest_id: 1,
        });
        for _ in 0..7 {
            app.world_mut().send_event(QuestCommand::AdvanceStep {
                player,
                quest_id: 1,
            });
        }
        app.update();

        let world = app.world_mut();
        let rejected: Vec<_> = world
            .resource_mut::<Events<QuestStartRejectedEvent>>()
            .drain()
            .collect();
        assert_eq!(rejected[0].reason, QuestStartError::MissingPrerequisite(1));
        assert_eq!(
            world
                .resource_mut::<Events<QuestStartedEvent>>()
                .drain()
                .count(),
            1
        );
        let steps: Vec<_> = world
            .resource_mut::<Events<QuestStepAdvancedEvent>>()
            .drain()
            .map(|e| e.step_index)
            .collect();
        assert_eq!(steps, (0..7).collect::<Vec<_>>());
        let done: Vec<_> = world
            .resource_mut::<Events<QuestCompletedEvent>>()
            .drain()
            .collect();
        assert_eq!(
            done,
            vec![QuestCompletedEvent {
                player,
                quest_id: 1,
                credit_reward: 1_200
            }]
        );
        assert_eq!(
            world.resource_mut::<Events<LevelUpEvent>>().drain().count(),
            2
        );
    }
}
