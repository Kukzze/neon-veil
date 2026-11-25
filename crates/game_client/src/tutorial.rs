//! First-run tutorial on Kuroshio Avenue: AURA walks the player through
//! moving, sprinting, jumping, sliding, the katana combo and guarding,
//! then hands over to Night One. Each step completes on the real action.

use crate::client_network::{ClientPredictionState, LocalPredictedPlayer};
use bevy::prelude::*;
use game_audio::{DialogueManifest, TriggerVoiceCueEvent};
use game_combat::{AttackPhase, MeleeCombatState};
use game_parkour::KinematicVelocity;
use game_server::SPRINT_SPEED;
use game_ui::{Banner, BannerTone, MissionHud};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PROGRESS_FILE: &str = "config/progress.json";
const TUTORIAL_TITLE: &str = "NEURAL REBOOT // TUTORIAL";
/// Seconds a completed step lingers before the next one.
const STEP_PAUSE: f32 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Move,
    Sprint,
    Jump,
    Slide,
    Combo,
    Guard,
    Done,
}

impl Step {
    pub const ORDER: [Self; 7] = [
        Self::Move,
        Self::Sprint,
        Self::Jump,
        Self::Slide,
        Self::Combo,
        Self::Guard,
        Self::Done,
    ];

    pub fn prompt(self) -> &'static str {
        match self {
            Self::Move => "Move with W A S D",
            Self::Sprint => "Hold Shift to sprint",
            Self::Jump => "Press Space to jump",
            Self::Slide => "Sprint, then press C to slide",
            Self::Combo => "Left click three times: full katana combo",
            Self::Guard => "Hold right mouse to guard",
            Self::Done => "Head down Kuroshio Avenue",
        }
    }

    pub fn cue(self) -> &'static str {
        match self {
            Self::Move => "TUT_M1_MOVE",
            Self::Sprint => "TUT_M1_SPRINT",
            Self::Jump => "TUT_M1_JUMP",
            Self::Slide => "TUT_M1_SLIDE",
            Self::Combo => "TUT_M1_COMBO",
            Self::Guard => "TUT_M1_GUARD",
            Self::Done => "TUT_M1_DONE",
        }
    }

    fn next(self) -> Self {
        let i = Self::ORDER.iter().position(|s| *s == self).unwrap_or(0);
        Self::ORDER[(i + 1).min(Self::ORDER.len() - 1)]
    }
}

/// What the player did this frame, as the tutorial sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TutorialSignals {
    pub moved: f32,
    pub speed: f32,
    pub airborne: bool,
    pub sliding: bool,
    pub combo_hit: Option<u8>,
    pub guarding: bool,
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TutorialState {
    pub step: Step,
    pub distance: f32,
    pub sprint_time: f32,
    pub guard_time: f32,
    pub best_combo: u8,
    pub pause: f32,
    pub announced: bool,
}

impl Default for TutorialState {
    fn default() -> Self {
        Self {
            step: Step::Move,
            distance: 0.0,
            sprint_time: 0.0,
            guard_time: 0.0,
            best_combo: 0,
            pause: 0.0,
            announced: false,
        }
    }
}

impl TutorialState {
    /// Advances on this frame's signals; returns the step just completed.
    pub fn advance(&mut self, s: &TutorialSignals, dt: f32) -> Option<Step> {
        if self.step == Step::Done {
            return None;
        }
        if self.pause > 0.0 {
            self.pause -= dt;
            return None;
        }
        let done = match self.step {
            Step::Move => {
                self.distance += s.moved;
                self.distance > 6.0
            }
            Step::Sprint => {
                self.sprint_time = if s.speed > SPRINT_SPEED * 0.9 {
                    self.sprint_time + dt
                } else {
                    0.0
                };
                self.sprint_time > 0.8
            }
            Step::Jump => s.airborne,
            Step::Slide => s.sliding,
            Step::Combo => {
                if let Some(hit) = s.combo_hit {
                    self.best_combo = self.best_combo.max(hit);
                }
                self.best_combo >= 2
            }
            Step::Guard => {
                self.guard_time = if s.guarding {
                    self.guard_time + dt
                } else {
                    0.0
                };
                self.guard_time > 0.6
            }
            Step::Done => false,
        };
        if !done {
            return None;
        }
        let finished = self.step;
        self.step = self.step.next();
        self.pause = STEP_PAUSE;
        self.announced = false;
        Some(finished)
    }
}

/// Saved progress across launches.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub tutorial_done: bool,
    /// Experience and owned passive tree nodes.
    pub passive: game_rpg::PassiveProgress,
    /// Owned weapons and the equipped one.
    pub armory: crate::armory::ArmoryData,
}

#[derive(Resource, Debug, Clone)]
pub struct ProgressPath(pub PathBuf);

impl Default for ProgressPath {
    fn default() -> Self {
        Self(PathBuf::from(PROGRESS_FILE))
    }
}

pub fn load_progress(path: &Path) -> Progress {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_progress(path: &Path, progress: &Progress) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write a sibling file, then swap it in: a reader never sees a
    // half-written save, and a crash leaves the old one intact.
    let text = serde_json::to_string_pretty(progress).map_err(std::io::Error::other)?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)
}

/// Tutorial dialogue (subtitled; voiced when the audio files exist).
#[derive(Resource)]
pub struct TutorialScript(pub DialogueManifest);

impl Default for TutorialScript {
    fn default() -> Self {
        Self(DialogueManifest::onboarding())
    }
}

/// Marks the tutorial as running; Night One waits for it.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TutorialActive;

#[allow(clippy::too_many_arguments)]
pub fn run_tutorial(
    time: Res<Time>,
    mut commands: Commands,
    mut state: ResMut<TutorialState>,
    script: Res<TutorialScript>,
    path: Res<ProgressPath>,
    mut hud: ResMut<MissionHud>,
    mut voice: EventWriter<TriggerVoiceCueEvent>,
    appearance_path: Res<crate::character::AppearancePath>,
    mut next: ResMut<NextState<crate::creator::GameFlow>>,
    players: Query<
        (
            &KinematicVelocity,
            &ClientPredictionState,
            Option<&MeleeCombatState>,
        ),
        With<LocalPredictedPlayer>,
    >,
) {
    let Ok((velocity, prediction, melee)) = players.get_single() else {
        return;
    };
    let dt = time.delta_secs();
    if !state.announced && state.pause <= 0.0 {
        if let Some(line) = script.0.find(state.step.cue()) {
            let cue = line.to_voice_cue();
            voice.send(TriggerVoiceCueEvent {
                cue_id: cue.cue_id,
                speaker: cue.speaker,
                audio_file_path: cue.audio_path,
                subtitle_text: cue.text,
                duration_seconds: cue.duration_seconds,
            });
        }
        state.announced = true;
    }
    let planar = Vec2::new(velocity.0.x, velocity.0.z).length();
    let signals = TutorialSignals {
        moved: planar * dt,
        speed: planar,
        airborne: !prediction.movement.grounded,
        sliding: prediction.movement.sliding,
        combo_hit: melee
            .and_then(|m| (m.attack_phase != AttackPhase::Idle).then_some(m.current_combo_index)),
        guarding: melee.is_some_and(|m| m.is_parrying),
    };
    if state.advance(&signals, dt).is_some() {
        hud.banner = Some(Banner {
            title: "SYNCED".into(),
            subtitle: format!(
                "{} / {}",
                Step::ORDER
                    .iter()
                    .position(|s| *s == state.step)
                    .unwrap_or(0),
                Step::ORDER.len() - 1
            ),
            tone: BannerTone::Success,
        });
    } else if state.pause <= 0.0 && hud.banner.as_ref().is_some_and(|b| b.title == "SYNCED") {
        hud.banner = None;
    }
    hud.title = TUTORIAL_TITLE.into();
    let prompt = state.step.prompt();
    if hud.objective != prompt {
        hud.objective = prompt.into();
    }
    if state.step == Step::Done && state.pause <= 0.0 {
        // Keep everything else saved (passive tree progress) as it is.
        let mut progress = load_progress(&path.0);
        progress.tutorial_done = true;
        if let Err(error) = save_progress(&path.0, &progress) {
            warn!("could not save progress: {error}");
        }
        hud.banner = None;
        commands.remove_resource::<TutorialActive>();
        // First run: build the operative before Night One.
        if !appearance_path.0.exists() {
            next.set(crate::creator::GameFlow::Creator);
        }
    }
}

pub struct TutorialPlugin {
    /// Run the tutorial this launch.
    pub enabled: bool,
}

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TutorialState>()
            .init_resource::<TutorialScript>()
            .init_resource::<ProgressPath>();
        if self.enabled {
            app.insert_resource(TutorialActive);
        }
        app.add_systems(
            Update,
            run_tutorial
                .before(crate::lattice::save_lattice)
                .run_if(resource_exists::<TutorialActive>)
                .run_if(in_state(crate::creator::GameFlow::Playing)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(s: &mut TutorialState, signals: TutorialSignals, frames: usize) -> Vec<Step> {
        (0..frames)
            .filter_map(|_| s.advance(&signals, 1.0 / 60.0))
            .collect()
    }

    #[test]
    fn steps_complete_on_their_actions_in_order() {
        let mut s = TutorialState::default();
        let walk = TutorialSignals {
            moved: 5.5 / 60.0,
            speed: 5.5,
            ..default()
        };
        assert_eq!(frame(&mut s, walk, 120), vec![Step::Move]);
        assert_eq!(s.step, Step::Sprint);
        frame(&mut s, TutorialSignals::default(), 90);
        let sprint = TutorialSignals {
            speed: SPRINT_SPEED,
            ..default()
        };
        assert_eq!(frame(&mut s, sprint, 60), vec![Step::Sprint]);
        frame(&mut s, TutorialSignals::default(), 90);
        assert_eq!(
            frame(
                &mut s,
                TutorialSignals {
                    airborne: true,
                    ..default()
                },
                1
            ),
            vec![Step::Jump]
        );
        frame(&mut s, TutorialSignals::default(), 90);
        frame(
            &mut s,
            TutorialSignals {
                sliding: true,
                ..default()
            },
            1,
        );
        frame(&mut s, TutorialSignals::default(), 90);
        for hit in 0..3 {
            frame(
                &mut s,
                TutorialSignals {
                    combo_hit: Some(hit),
                    ..default()
                },
                1,
            );
        }
        assert_eq!(s.step, Step::Guard);
        frame(&mut s, TutorialSignals::default(), 90);
        frame(
            &mut s,
            TutorialSignals {
                guarding: true,
                ..default()
            },
            60,
        );
        assert_eq!(s.step, Step::Done);
    }

    #[test]
    fn every_step_has_a_scripted_line() {
        let script = TutorialScript::default();
        for step in Step::ORDER {
            assert!(script.0.find(step.cue()).is_some(), "{step:?}");
        }
    }

    #[test]
    fn progress_round_trips() {
        let path = std::env::temp_dir().join("neon_veil_progress_test.json");
        save_progress(
            &path,
            &Progress {
                tutorial_done: true,
                ..default()
            },
        )
        .expect("save");
        assert!(load_progress(&path).tutorial_done);
        let _ = std::fs::remove_file(path);
        assert!(!load_progress(Path::new("missing/progress.json")).tutorial_done);
    }
}
