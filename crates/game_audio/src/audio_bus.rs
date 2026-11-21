//! Mixer buses, volume settings, music ducking and the SFX / voice event
//! routing.
//!
//! Gameplay sends [`PlaySfxEvent`] and [`TriggerVoiceCueEvent`]. The router
//! turns sound effects into [`ResolvedSfxEvent`]s carrying the final mixed
//! volume, and the voice queue plays lines one at a time while music is
//! ducked to half volume. All of this is plain arithmetic, so it runs
//! identically headless; an output backend only consumes the resolved
//! events and levels.

use crate::voiceover_player::{VoiceCueData, VoiceoverQueue};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Music level while a voice line plays (-6 dB).
pub const DUCKED_MUSIC_MULTIPLIER: f32 = 0.50;
/// Rate at which the duck engages and releases, 1/s.
pub const DUCKING_RATE: f32 = 8.0;
/// Parkour movement sits slightly under the combat foley it shares a fader with.
pub const PARKOUR_BUS_TRIM: f32 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum AudioChannelBus {
    Master,
    Voiceover,
    CombatFoley,
    ParkourMovement,
    DynamicMusic,
    Ambience,
    UserInterface,
}

impl AudioChannelBus {
    pub const ALL: [Self; 7] = [
        Self::Master,
        Self::Voiceover,
        Self::CombatFoley,
        Self::ParkourMovement,
        Self::DynamicMusic,
        Self::Ambience,
        Self::UserInterface,
    ];

    /// Mixing priority; higher wins clarity (voice ducks music).
    pub const fn priority(self) -> u8 {
        match self {
            Self::Master => 255,
            Self::Voiceover => 100,
            Self::CombatFoley => 80,
            Self::UserInterface => 70,
            Self::ParkourMovement => 60,
            Self::DynamicMusic => 40,
            Self::Ambience => 20,
        }
    }
}

#[derive(Event, Debug, Clone, PartialEq)]
pub struct PlaySfxEvent {
    pub sound_path: &'static str,
    pub bus: AudioChannelBus,
    /// `None` for non-positional (2D) sounds.
    pub world_position: Option<Vec3>,
    pub volume: f32,
    pub pitch: f32,
}

impl PlaySfxEvent {
    pub fn ui(sound_path: &'static str) -> Self {
        Self {
            sound_path,
            bus: AudioChannelBus::UserInterface,
            world_position: None,
            volume: 1.0,
            pitch: 1.0,
        }
    }

    pub fn at(sound_path: &'static str, bus: AudioChannelBus, position: Vec3) -> Self {
        Self {
            sound_path,
            bus,
            world_position: Some(position),
            volume: 1.0,
            pitch: 1.0,
        }
    }
}

#[derive(Event, Debug, Clone, PartialEq)]
pub struct TriggerVoiceCueEvent {
    pub cue_id: String,
    pub speaker: String,
    pub audio_file_path: String,
    pub subtitle_text: String,
    pub duration_seconds: f32,
}

impl From<&TriggerVoiceCueEvent> for VoiceCueData {
    fn from(event: &TriggerVoiceCueEvent) -> Self {
        Self {
            cue_id: event.cue_id.clone(),
            speaker: event.speaker.clone(),
            text: event.subtitle_text.clone(),
            audio_path: event.audio_file_path.clone(),
            duration_seconds: event.duration_seconds,
        }
    }
}

#[derive(Event, Debug, Clone, PartialEq)]
pub struct SubtitleDisplayEvent {
    pub speaker: String,
    pub text: String,
    pub duration_seconds: f32,
}

/// A sound effect with its final mixed volume, ready for an output backend.
#[derive(Event, Debug, Clone, PartialEq)]
pub struct ResolvedSfxEvent {
    pub sound_path: &'static str,
    pub bus: AudioChannelBus,
    pub world_position: Option<Vec3>,
    pub volume: f32,
    pub pitch: f32,
}

/// A voice line that just started, with its mixed volume.
#[derive(Event, Debug, Clone, PartialEq)]
pub struct VoiceLineStartedEvent {
    pub cue: VoiceCueData,
    pub volume: f32,
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Resource, Debug, PartialEq)]
pub struct AudioMixerSettings {
    pub master_volume: f32,
    pub voice_volume: f32,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub ambience_volume: f32,
    pub ui_volume: f32,
    pub is_ducking_active: bool,
    /// Current music duck, 1.0 = none.
    pub ducking_multiplier: f32,
}

impl Default for AudioMixerSettings {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            voice_volume: 1.0,
            sfx_volume: 0.85,
            music_volume: 0.70,
            ambience_volume: 0.60,
            ui_volume: 0.90,
            is_ducking_active: false,
            ducking_multiplier: 1.0,
        }
    }
}

impl AudioMixerSettings {
    /// Fader level of `bus` before the master fader (music includes the duck).
    pub fn bus_volume(&self, bus: AudioChannelBus) -> f32 {
        match bus {
            AudioChannelBus::Master => 1.0,
            AudioChannelBus::Voiceover => self.voice_volume,
            AudioChannelBus::CombatFoley => self.sfx_volume,
            AudioChannelBus::ParkourMovement => self.sfx_volume * PARKOUR_BUS_TRIM,
            AudioChannelBus::DynamicMusic => self.music_volume * self.ducking_multiplier,
            AudioChannelBus::Ambience => self.ambience_volume,
            AudioChannelBus::UserInterface => self.ui_volume,
        }
    }

    /// Final linear volume in `[0, 1]` for a sound of `base_volume` on `bus`.
    pub fn compute_effective_volume(&self, bus: AudioChannelBus, base_volume: f32) -> f32 {
        let base = if base_volume.is_finite() {
            base_volume
        } else {
            0.0
        };
        (base * self.bus_volume(bus) * self.master_volume).clamp(0.0, 1.0)
    }

    /// Moves the duck toward -6 dB while active and back to unity when not.
    pub fn tick_ducking(&mut self, dt: f32) {
        let target = if self.is_ducking_active {
            DUCKED_MUSIC_MULTIPLIER
        } else {
            1.0
        };
        self.ducking_multiplier +=
            (target - self.ducking_multiplier) * (DUCKING_RATE * dt).min(1.0);
        if (self.ducking_multiplier - target).abs() < 1e-4 {
            self.ducking_multiplier = target;
        }
    }
}

/// Queues voice cues, advances the dialogue, and ducks music while a line
/// is playing.
pub fn process_audio_ducking_system(
    time: Res<Time>,
    mut settings: ResMut<AudioMixerSettings>,
    mut queue: ResMut<VoiceoverQueue>,
    mut voice_events: EventReader<TriggerVoiceCueEvent>,
    mut subtitles: EventWriter<SubtitleDisplayEvent>,
    mut started: EventWriter<VoiceLineStartedEvent>,
) {
    for cue in voice_events.read() {
        queue.enqueue(VoiceCueData::from(cue));
    }
    if let Some(subtitle) = queue.tick(time.delta_secs()) {
        if let Some(cue) = queue.current_cue.clone() {
            started.send(VoiceLineStartedEvent {
                cue,
                volume: settings.compute_effective_volume(AudioChannelBus::Voiceover, 1.0),
            });
        }
        subtitles.send(subtitle);
    }
    let speaking = queue.is_playing();
    if settings.is_ducking_active != speaking {
        settings.is_ducking_active = speaking;
    }
    settings.tick_ducking(time.delta_secs());
}

/// Mixes every requested sound effect.
pub fn route_sfx_events(
    settings: Res<AudioMixerSettings>,
    mut requests: EventReader<PlaySfxEvent>,
    mut resolved: EventWriter<ResolvedSfxEvent>,
) {
    for sfx in requests.read() {
        let volume = settings.compute_effective_volume(sfx.bus, sfx.volume);
        if volume <= 0.0 {
            continue;
        }
        resolved.send(ResolvedSfxEvent {
            sound_path: sfx.sound_path,
            bus: sfx.bus,
            world_position: sfx.world_position,
            volume,
            pitch: if sfx.pitch.is_finite() && sfx.pitch > 0.0 {
                sfx.pitch
            } else {
                1.0
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_levels_follow_the_faders() {
        let m = AudioMixerSettings::default();
        assert_eq!(
            m.compute_effective_volume(AudioChannelBus::Voiceover, 1.0),
            1.0
        );
        assert!(
            (m.compute_effective_volume(AudioChannelBus::CombatFoley, 1.0) - 0.85).abs() < 1e-6
        );
        assert!(
            (m.compute_effective_volume(AudioChannelBus::ParkourMovement, 1.0) - 0.765).abs()
                < 1e-6
        );
        assert!(
            (m.compute_effective_volume(AudioChannelBus::DynamicMusic, 1.0) - 0.70).abs() < 1e-6
        );
        assert!((m.compute_effective_volume(AudioChannelBus::Ambience, 0.5) - 0.30).abs() < 1e-6);
        assert!(
            (m.compute_effective_volume(AudioChannelBus::UserInterface, 1.0) - 0.90).abs() < 1e-6
        );
        let quiet = AudioMixerSettings {
            master_volume: 0.5,
            ..m
        };
        assert!(
            (quiet.compute_effective_volume(AudioChannelBus::CombatFoley, 1.0) - 0.425).abs()
                < 1e-6
        );
    }

    #[test]
    fn effective_volume_is_clamped_and_sanitized() {
        let m = AudioMixerSettings::default();
        assert_eq!(
            m.compute_effective_volume(AudioChannelBus::Voiceover, 5.0),
            1.0
        );
        assert_eq!(
            m.compute_effective_volume(AudioChannelBus::Voiceover, -1.0),
            0.0
        );
        assert_eq!(
            m.compute_effective_volume(AudioChannelBus::Voiceover, f32::NAN),
            0.0
        );
    }

    #[test]
    fn only_music_is_ducked() {
        let m = AudioMixerSettings {
            ducking_multiplier: 0.5,
            ..default()
        };
        assert!(
            (m.compute_effective_volume(AudioChannelBus::DynamicMusic, 1.0) - 0.35).abs() < 1e-6
        );
        assert!(
            (m.compute_effective_volume(AudioChannelBus::CombatFoley, 1.0) - 0.85).abs() < 1e-6
        );
    }

    #[test]
    fn ducking_engages_and_releases_smoothly() {
        let mut m = AudioMixerSettings {
            is_ducking_active: true,
            ..default()
        };
        let mut levels = vec![m.ducking_multiplier];
        for _ in 0..60 {
            m.tick_ducking(1.0 / 60.0);
            levels.push(m.ducking_multiplier);
        }
        assert!(levels.windows(2).all(|w| w[1] <= w[0]), "monotonic");
        assert!(levels[1] > 0.9, "no instant drop");
        assert_eq!(m.ducking_multiplier, DUCKED_MUSIC_MULTIPLIER);
        m.is_ducking_active = false;
        for _ in 0..60 {
            m.tick_ducking(1.0 / 60.0);
        }
        assert_eq!(m.ducking_multiplier, 1.0);
        // A huge frame cannot overshoot.
        m.is_ducking_active = true;
        m.tick_ducking(10.0);
        assert_eq!(m.ducking_multiplier, DUCKED_MUSIC_MULTIPLIER);
    }

    #[test]
    fn priorities_rank_voice_over_music() {
        assert!(AudioChannelBus::Voiceover.priority() > AudioChannelBus::CombatFoley.priority());
        assert!(
            AudioChannelBus::CombatFoley.priority() > AudioChannelBus::ParkourMovement.priority()
        );
        assert!(
            AudioChannelBus::ParkourMovement.priority() > AudioChannelBus::DynamicMusic.priority()
        );
        assert!(AudioChannelBus::DynamicMusic.priority() > AudioChannelBus::Ambience.priority());
    }
}
