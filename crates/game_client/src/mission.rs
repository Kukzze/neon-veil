//! Night One: the first mission, played down Kuroshio Avenue.
//!
//! A briefing, three Syndicate encounters and the Gorgon. Each encounter
//! starts when the player reaches its trigger line and ends when its last
//! enemy falls; dying resets the encounter and respawns the player at the
//! checkpoint before it. Defeating the Gorgon completes the mission.

use crate::client_network::{ClientTransportResource, LocalPredictedPlayer};
use bevy::prelude::*;
use game_ai::{
    spawn_boss_gorgon, spawn_enemy, BossDefeatedEvent, BossPhaseTransitionEvent, EnemyArchetype,
    EnemyDefeatedEvent,
};
use game_audio::{DialogueManifest, TriggerVoiceCueEvent, VoiceoverQueue};
use game_core::input::ViewAngles;
use game_core::protocol::ClientMessage;
use game_rpg::{Health, Shields};
use game_ui::{Banner, BannerTone, GameplayHold, MissionHud};

pub const MISSION_TITLE: &str = "NIGHT ONE // KUROSHIO AVENUE";
/// Seconds the death screen holds before the respawn.
pub const FLATLINE_SECONDS: f32 = 4.0;
/// Seconds an encounter title banner stays up.
const BANNER_SECONDS: f32 = 3.0;
/// Briefing length before the first objective appears.
const BRIEFING_SECONDS: f32 = 6.0;
/// Time after spawning before an encounter may count as cleared.
const ENCOUNTER_SETTLE_SECONDS: f32 = 0.5;

/// One Syndicate encounter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Encounter {
    pub name: &'static str,
    /// The encounter starts once the player is past this z (they walk -z).
    pub trigger_z: f32,
    /// Enemies as (archetype, x, z).
    pub spawns: &'static [(EnemyArchetype, f32, f32)],
    pub intro_cue: &'static str,
    pub clear_cue: &'static str,
    pub approach: &'static str,
    pub objective: &'static str,
}

use EnemyArchetype::{CyberNinja, Netrunner, ShieldedHeavy, StreetGrunt, SwarmDrone};

pub const ENCOUNTERS: [Encounter; 3] = [
    Encounter {
        name: "SYNDICATE PATROL",
        trigger_z: -48.0,
        spawns: &[
            (StreetGrunt, -3.5, -72.0),
            (StreetGrunt, 3.0, -76.0),
            (StreetGrunt, 0.0, -82.0),
        ],
        intro_cue: "M1_02_PATROL",
        clear_cue: "M1_03_PATROL_CLEAR",
        approach: "Head down Kuroshio Avenue",
        objective: "Cut down the Syndicate patrol",
    },
    Encounter {
        name: "CLOAKED HUNTERS",
        trigger_z: -116.0,
        spawns: &[
            (CyberNinja, -4.0, -140.0),
            (CyberNinja, 4.0, -144.0),
            (SwarmDrone, -2.0, -150.0),
            (SwarmDrone, 2.5, -152.0),
        ],
        intro_cue: "M1_04_NINJAS",
        clear_cue: "M1_05_NINJAS_CLEAR",
        approach: "Push deeper into the district",
        objective: "Survive the cyber-ninja ambush",
    },
    Encounter {
        name: "HEAVY RESPONSE",
        trigger_z: -180.0,
        spawns: &[
            (ShieldedHeavy, 0.0, -200.0),
            (StreetGrunt, -4.5, -196.0),
            (StreetGrunt, 4.5, -196.0),
            (Netrunner, 0.0, -212.0),
        ],
        intro_cue: "M1_06_HEAVY",
        clear_cue: "M1_07_HEAVY_CLEAR",
        approach: "Follow the Syndicate comms trail",
        objective: "Break the Shielded Heavy and the netrunner",
    },
];

/// The Gorgon's arena.
pub const BOSS_TRIGGER_Z: f32 = -214.0;
pub const BOSS_SPAWN: Vec3 = Vec3::new(0.0, 1.5, -244.0);
/// Enemies stand with their capsule centre this high.
const ENEMY_STANDING_Y: f32 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage {
    Briefing,
    Approach(usize),
    Fight(usize),
    BossApproach,
    BossFight,
    Complete,
}

impl Stage {
    /// Checkpoint index (see `RESPAWN_CHECKPOINTS`) a death here returns to.
    pub fn checkpoint(self) -> u8 {
        match self {
            Self::Briefing | Self::Approach(0) | Self::Fight(0) => 1,
            Self::Approach(i) | Self::Fight(i) => (i + 1).min(3) as u8,
            Self::BossApproach | Self::BossFight | Self::Complete => 4,
        }
    }
}

/// Enemies spawned by the mission, tagged with their encounter (the boss
/// uses `ENCOUNTERS.len()`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissionEnemy {
    pub encounter: usize,
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct MissionDirector {
    pub stage: Stage,
    pub elapsed: f32,
    pub stage_time: f32,
    pub kills: u32,
    pub deaths: u32,
    pub flatline: Option<f32>,
    pub banner_time: f32,
    pub spawned: bool,
}

impl Default for MissionDirector {
    fn default() -> Self {
        Self {
            stage: Stage::Briefing,
            elapsed: 0.0,
            stage_time: 0.0,
            kills: 0,
            deaths: 0,
            flatline: None,
            banner_time: 0.0,
            spawned: false,
        }
    }
}

impl MissionDirector {
    fn enter(&mut self, stage: Stage) {
        info!("mission stage {:?} -> {:?}", self.stage, stage);
        self.stage = stage;
        self.stage_time = 0.0;
        self.spawned = false;
    }

    /// Objective line for the current stage, given living enemies.
    pub fn objective(&self, alive: usize) -> String {
        match self.stage {
            Stage::Briefing => "Get your bearings".into(),
            Stage::Approach(i) => ENCOUNTERS[i].approach.into(),
            Stage::Fight(i) => {
                let total = ENCOUNTERS[i].spawns.len();
                format!(
                    "{}  [{}/{}]",
                    ENCOUNTERS[i].objective,
                    total - alive.min(total),
                    total
                )
            }
            Stage::BossApproach => "Confront the Syndicate enforcer".into(),
            Stage::BossFight => "Destroy the Gorgon".into(),
            Stage::Complete => "Protocol complete".into(),
        }
    }
}

/// Runs a stage transition as a pure function of where the player is and
/// how many encounter enemies remain. Returns the stage to enter, if any.
pub fn next_stage(
    stage: Stage,
    stage_time: f32,
    player_z: f32,
    alive: usize,
    spawned: bool,
) -> Option<Stage> {
    match stage {
        Stage::Briefing if stage_time >= BRIEFING_SECONDS => Some(Stage::Approach(0)),
        Stage::Approach(i) if player_z <= ENCOUNTERS[i].trigger_z => Some(Stage::Fight(i)),
        Stage::Fight(i) if spawned && alive == 0 => Some(if i + 1 < ENCOUNTERS.len() {
            Stage::Approach(i + 1)
        } else {
            Stage::BossApproach
        }),
        Stage::BossApproach if player_z <= BOSS_TRIGGER_Z => Some(Stage::BossFight),
        _ => None,
    }
}

/// Mission dialogue, looked up by cue id.
#[derive(Resource)]
pub struct MissionScript(pub DialogueManifest);

impl Default for MissionScript {
    fn default() -> Self {
        Self(DialogueManifest::mission_one())
    }
}

impl MissionScript {
    pub fn cue(&self, cue_id: &str) -> Option<TriggerVoiceCueEvent> {
        self.0.find(cue_id).map(|line| {
            let cue = line.to_voice_cue();
            TriggerVoiceCueEvent {
                cue_id: cue.cue_id,
                speaker: cue.speaker,
                audio_file_path: cue.audio_path,
                subtitle_text: cue.text,
                duration_seconds: cue.duration_seconds,
            }
        })
    }
}

fn spawn_encounter(commands: &mut Commands, index: usize) {
    for &(archetype, x, z) in ENCOUNTERS[index].spawns {
        let y = if archetype == SwarmDrone {
            3.0
        } else {
            ENEMY_STANDING_Y
        };
        let enemy = spawn_enemy(commands, archetype, Vec3::new(x, y, z));
        // Face back up the avenue, towards the approaching player.
        commands.entity(enemy).insert((
            MissionEnemy { encounter: index },
            ViewAngles::new(std::f32::consts::PI, 0.0),
            Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        ));
    }
}

fn banner(title: &str, subtitle: &str, tone: BannerTone) -> Option<Banner> {
    Some(Banner {
        title: title.into(),
        subtitle: subtitle.into(),
        tone,
    })
}

fn format_time(seconds: f32) -> String {
    let s = seconds.max(0.0) as u32;
    format!("{:02}:{:02}", s / 60, s % 60)
}

#[allow(clippy::too_many_arguments)]
pub fn run_mission(
    time: Res<Time>,
    mut commands: Commands,
    mut director: ResMut<MissionDirector>,
    script: Res<MissionScript>,
    mut hud: ResMut<MissionHud>,
    mut hold: ResMut<GameplayHold>,
    mut voice_queue: ResMut<VoiceoverQueue>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut transport: Option<ResMut<ClientTransportResource>>,
    mut players: Query<(&Transform, &mut Health, Option<&mut Shields>), With<LocalPredictedPlayer>>,
    enemies: Query<(Entity, &MissionEnemy)>,
    mut defeated: EventReader<EnemyDefeatedEvent>,
    mut boss_phases: EventReader<BossPhaseTransitionEvent>,
    mut boss_defeated: EventReader<BossDefeatedEvent>,
    mut voice: EventWriter<TriggerVoiceCueEvent>,
) {
    let Ok((transform, mut health, shields)) = players.get_single_mut() else {
        return;
    };
    let dt = time.delta_secs();
    let mut say = |cue: &str| {
        if let Some(event) = script.cue(cue) {
            voice.send(event);
        }
    };

    if director.stage != Stage::Complete {
        director.elapsed += dt;
    }
    director.stage_time += dt;
    director.banner_time = (director.banner_time - dt).max(0.0);
    director.kills += defeated.read().count() as u32;

    // Death and respawn.
    if let Some(remaining) = director.flatline {
        let remaining = remaining - dt;
        if remaining > 0.0 {
            director.flatline = Some(remaining);
            return;
        }
        director.flatline = None;
        let checkpoint = director.stage.checkpoint();
        info!("respawning at checkpoint {checkpoint}");
        let encounter = match director.stage {
            Stage::Fight(i) => Some(i),
            Stage::BossFight => Some(ENCOUNTERS.len()),
            _ => None,
        };
        for (entity, tag) in &enemies {
            if Some(tag.encounter) == encounter {
                commands.entity(entity).despawn_recursive();
            }
        }
        let back_to = match director.stage {
            Stage::Fight(i) => Stage::Approach(i),
            Stage::BossFight => Stage::BossApproach,
            other => other,
        };
        director.enter(back_to);
        health.current = health.max;
        if let Some(mut shields) = shields {
            shields.current = shields.max;
        }
        if let Some(transport) = transport.as_mut() {
            transport.0.send(ClientMessage::Respawn(checkpoint));
        }
        hold.0 = false;
        hud.banner = None;
        return;
    }
    if health.is_dead() && director.stage != Stage::Complete {
        director.deaths += 1;
        info!("player flatlined in {:?}", director.stage);
        director.flatline = Some(FLATLINE_SECONDS);
        hold.0 = true;
        voice_queue.clear();
        say("M1_11_FLATLINE");
        hud.banner = banner(
            "FLATLINED",
            "Neural link rebooting. Respawning at the last checkpoint.",
            BannerTone::Danger,
        );
        return;
    }

    let encounter_index = match director.stage {
        Stage::Fight(i) => Some(i),
        Stage::BossFight => Some(ENCOUNTERS.len()),
        _ => None,
    };
    let alive = enemies
        .iter()
        .filter(|(_, tag)| Some(tag.encounter) == encounter_index)
        .count();

    // Encounter setup on entering a fight.
    match director.stage {
        Stage::Briefing if director.stage_time <= dt => {
            say("M1_00_INTRO");
            say("M1_01_AURA_ONLINE");
            hud.banner = banner(MISSION_TITLE, "ZERO PROTOCOL", BannerTone::Info);
            director.banner_time = BANNER_SECONDS + 1.5;
        }
        Stage::Fight(i) if !director.spawned => {
            spawn_encounter(&mut commands, i);
            director.spawned = true;
            say(ENCOUNTERS[i].intro_cue);
            hud.banner = banner(ENCOUNTERS[i].name, "Hostiles inbound", BannerTone::Danger);
            director.banner_time = BANNER_SECONDS;
        }
        Stage::BossFight if !director.spawned => {
            let boss = spawn_boss_gorgon(&mut commands, BOSS_SPAWN);
            commands.entity(boss).insert((
                MissionEnemy {
                    encounter: ENCOUNTERS.len(),
                },
                ViewAngles::new(std::f32::consts::PI, 0.0),
                Transform::from_translation(BOSS_SPAWN)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            ));
            director.spawned = true;
            say("M1_08_BOSS_INTRO");
            hud.banner = banner("THE GORGON", "Syndicate enforcer", BannerTone::Danger);
            director.banner_time = BANNER_SECONDS + 1.0;
        }
        _ => {}
    }

    if boss_phases.read().count() > 0 {
        say("M1_09_BOSS_PHASE");
    }
    let boss_down = boss_defeated.read().count() > 0
        || (director.stage == Stage::BossFight
            && director.spawned
            && alive == 0
            && director.stage_time > ENCOUNTER_SETTLE_SECONDS);
    if director.stage == Stage::BossFight && boss_down {
        for (entity, tag) in &enemies {
            if tag.encounter == ENCOUNTERS.len() {
                commands.entity(entity).despawn_recursive();
            }
        }
        director.kills += 1;
        director.enter(Stage::Complete);
        say("M1_10_VICTORY");
    }

    if director.stage == Stage::Complete {
        hold.0 = false;
        hud.banner = banner(
            "PROTOCOL COMPLETE",
            &format!(
                "Time {}   |   Kills {}   |   Deaths {}        Press Enter to replay",
                format_time(director.elapsed),
                director.kills,
                director.deaths
            ),
            BannerTone::Success,
        );
        hud.title = MISSION_TITLE.into();
        hud.objective = director.objective(0);
        if keys.is_some_and(|k| k.just_pressed(KeyCode::Enter)) {
            *director = MissionDirector::default();
            if let Some(transport) = transport.as_mut() {
                transport.0.send(ClientMessage::Respawn(0));
            }
            health.current = health.max;
        }
        return;
    }

    if let Some(next) = next_stage(
        director.stage,
        director.stage_time,
        transform.translation.z,
        alive,
        // Spawned enemies only show up in queries from the next frame.
        director.spawned && director.stage_time > ENCOUNTER_SETTLE_SECONDS,
    ) {
        if let Stage::Fight(i) = director.stage {
            say(ENCOUNTERS[i].clear_cue);
            hud.banner = banner("AREA CLEAR", ENCOUNTERS[i].name, BannerTone::Success);
            director.banner_time = BANNER_SECONDS;
        }
        director.enter(next);
    }

    if director.banner_time <= 0.0 && hud.banner.is_some() {
        hud.banner = None;
    }
    let objective = director.objective(alive);
    if hud.objective != objective {
        hud.objective = objective;
    }
    if hud.title != MISSION_TITLE {
        hud.title = MISSION_TITLE.into();
    }
}

/// Starts the mission at an encounter (1-3, 4 for the Gorgon) instead of
/// the briefing; consumed once the local player exists.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartAtEncounter(pub u8);

pub fn apply_start_encounter(
    mut commands: Commands,
    start: Option<Res<StartAtEncounter>>,
    mut director: ResMut<MissionDirector>,
    mut transport: Option<ResMut<ClientTransportResource>>,
    players: Query<(), With<LocalPredictedPlayer>>,
) {
    let Some(start) = start else {
        return;
    };
    if players.is_empty() {
        return;
    }
    let stage = match start.0 {
        1..=3 => Stage::Fight(usize::from(start.0) - 1),
        _ => Stage::BossFight,
    };
    director.enter(stage);
    if let Some(transport) = transport.as_mut() {
        transport.0.send(ClientMessage::Respawn(stage.checkpoint()));
    }
    commands.remove_resource::<StartAtEncounter>();
}

/// Night One, for the windowed offline client.
pub struct MissionPlugin;

impl Plugin for MissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MissionDirector>()
            .init_resource::<MissionScript>()
            .add_systems(
                Update,
                (apply_start_encounter, run_mission)
                    .chain()
                    .run_if(in_state(crate::creator::GameFlow::Playing))
                    .run_if(not(resource_exists::<crate::tutorial::TutorialActive>)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::protocol::RESPAWN_CHECKPOINTS;

    #[test]
    fn stages_progress_through_the_avenue() {
        let mut stage = Stage::Briefing;
        assert_eq!(next_stage(stage, 1.0, 0.0, 0, false), None);
        stage = next_stage(stage, BRIEFING_SECONDS, 0.0, 0, false).expect("briefing ends");
        assert_eq!(stage, Stage::Approach(0));
        assert_eq!(next_stage(stage, 0.0, -10.0, 0, false), None);
        stage = next_stage(stage, 0.0, -50.0, 0, false).expect("trigger");
        assert_eq!(stage, Stage::Fight(0));
        assert_eq!(
            next_stage(stage, 0.0, -60.0, 0, false),
            None,
            "not spawned yet"
        );
        assert_eq!(
            next_stage(stage, 0.0, -60.0, 2, true),
            None,
            "enemies alive"
        );
        stage = next_stage(stage, 0.0, -60.0, 0, true).expect("cleared");
        assert_eq!(stage, Stage::Approach(1));
        stage = Stage::Fight(ENCOUNTERS.len() - 1);
        assert_eq!(
            next_stage(stage, 0.0, -190.0, 0, true),
            Some(Stage::BossApproach)
        );
        assert_eq!(
            next_stage(Stage::BossApproach, 0.0, BOSS_TRIGGER_Z - 1.0, 0, false),
            Some(Stage::BossFight)
        );
    }

    #[test]
    fn checkpoints_sit_before_each_trigger_and_are_valid() {
        for (i, encounter) in ENCOUNTERS.iter().enumerate() {
            let point = RESPAWN_CHECKPOINTS[usize::from(Stage::Fight(i).checkpoint())];
            assert!(
                point.z > encounter.trigger_z,
                "checkpoint before encounter {i}"
            );
            assert!(point.z - encounter.trigger_z < 12.0, "and close to it");
            for &(_, _, z) in encounter.spawns {
                assert!(
                    z < encounter.trigger_z - 10.0,
                    "enemies wait ahead of the trigger"
                );
            }
        }
        let boss = RESPAWN_CHECKPOINTS[usize::from(Stage::BossFight.checkpoint())];
        assert!(boss.z > BOSS_TRIGGER_Z && BOSS_SPAWN.z < BOSS_TRIGGER_Z - 20.0);
    }

    #[test]
    fn every_mission_cue_is_voiced() {
        let script = MissionScript::default();
        let mut cues = vec![
            "M1_00_INTRO",
            "M1_01_AURA_ONLINE",
            "M1_08_BOSS_INTRO",
            "M1_09_BOSS_PHASE",
            "M1_10_VICTORY",
            "M1_11_FLATLINE",
        ];
        for e in &ENCOUNTERS {
            cues.push(e.intro_cue);
            cues.push(e.clear_cue);
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for cue in cues {
            let event = script.cue(cue).unwrap_or_else(|| panic!("{cue} missing"));
            assert!(root.join(&event.audio_file_path).is_file(), "{cue} audio");
            assert!(event.duration_seconds > 1.0);
        }
    }

    #[test]
    fn objective_counts_kills() {
        let director = MissionDirector {
            stage: Stage::Fight(0),
            ..default()
        };
        assert!(director.objective(3).ends_with("[0/3]"));
        assert!(director.objective(1).ends_with("[2/3]"));
        assert_eq!(format_time(125.4), "02:05");
    }
}
