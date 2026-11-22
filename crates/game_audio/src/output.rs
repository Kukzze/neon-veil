//! Speaker output through kira: plays [`ResolvedSfxEvent`]s from the
//! synthesized bank (panned and attenuated from the listener), runs the
//! rain and music beds, and plays voice lines as they start.
//!
//! Without an audio device the plugin logs once and stays silent, so the
//! game still runs on machines with no output.

use crate::audio_bus::{
    AudioChannelBus, AudioMixerSettings, ResolvedSfxEvent, VoiceLineStartedEvent,
};
use crate::dynamic_music::MusicOutputLevels;
use crate::synth::{self, Rendered, SAMPLE_RATE};
use bevy::prelude::*;
use kira::manager::backend::DefaultBackend;
use kira::manager::{AudioManager, AudioManagerSettings};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::sound::Region;
use kira::tween::Tween;
use kira::{Frame, Volume};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Variations rendered for frequent cues so repeats do not sound robotic.
const VARIATIONS: u32 = 4;
/// Distance beyond which positional sounds fall silent, metres.
const HEARING_RANGE: f32 = 45.0;
const RAIN_LEVEL: f64 = 0.32;
const MUSIC_LEVEL: f64 = 0.22;

/// Marks the entity whose transform is the listener (the game camera).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct AudioListener;

/// Where voice-line files live, resolved against the asset root.
#[derive(Resource, Debug, Clone)]
pub struct VoiceAssetRoot(pub PathBuf);

fn to_kira(rendered: &Rendered) -> StaticSoundData {
    let frames: Arc<[Frame]> = rendered
        .left
        .iter()
        .zip(&rendered.right)
        .map(|(l, r)| Frame {
            left: *l,
            right: *r,
        })
        .collect();
    let mut settings = StaticSoundSettings::new();
    if rendered.looping {
        settings = settings.loop_region(Region::default());
    }
    StaticSoundData {
        sample_rate: SAMPLE_RATE,
        frames,
        settings,
        slice: None,
    }
}

/// Which synthesized cue family a catalog id plays.
pub fn bank_key(sound_path: &str) -> Option<&'static str> {
    let key = match sound_path {
        p if p.starts_with("sfx_footstep") => "footstep",
        "sfx_jump_takeoff" | "sfx_wallkick_impulse" => "jump",
        "sfx_land_soft" | "sfx_landing_roll" | "sfx_mantle_grab" | "sfx_vault_hand_plant" => {
            "land_soft"
        }
        "sfx_land_hard" | "sfx_posture_shatter" => "land_hard",
        "sfx_slide_loop" => "slide",
        "sfx_katana_swing_light" => "swing_light",
        "sfx_katana_swing_heavy" => "swing_heavy",
        "sfx_parry_spark" => "parry",
        p if p.contains("impact") || p.contains("hit") => "impact",
        p if p.starts_with("ui_") || p.starts_with("sfx_ui") => "ui",
        _ => return None,
    };
    Some(key)
}

/// Stereo pan (0 left, 0.5 centre, 1 right) and distance gain.
pub fn spatialize(listener: &GlobalTransform, emitter: Vec3) -> (f64, f32) {
    let offset = emitter - listener.translation();
    let distance = offset.length();
    if distance < 1e-3 {
        return (0.5, 1.0);
    }
    let side = listener.right().dot(offset / distance);
    let pan = 0.5 + 0.4 * side;
    let gain = (1.0 - distance / HEARING_RANGE).clamp(0.0, 1.0).powi(2);
    (f64::from(pan), gain)
}

struct Bank {
    sounds: HashMap<&'static str, Vec<StaticSoundData>>,
    rain: StaticSoundData,
    drone: StaticSoundData,
}

impl Bank {
    fn render() -> Self {
        let mut sounds: HashMap<&'static str, Vec<StaticSoundData>> = HashMap::new();
        let mut add = |key, rendered: Rendered| {
            sounds.entry(key).or_default().push(to_kira(&rendered));
        };
        for v in 0..VARIATIONS {
            add("footstep", synth::footstep(0x100 + v));
            add("swing_light", synth::katana_swing(false, 0x200 + v));
            add("impact", synth::impact(0x300 + v));
        }
        add("swing_heavy", synth::katana_swing(true, 0x400));
        add("jump", synth::jump_takeoff());
        add("land_soft", synth::land(false));
        add("land_hard", synth::land(true));
        // Played once per slide, so the bank copy does not loop.
        add(
            "slide",
            Rendered {
                looping: false,
                ..synth::slide_loop()
            },
        );
        add("parry", synth::parry_spark());
        add("ui", synth::ui_click());
        Self {
            sounds,
            rain: to_kira(&synth::rain_loop()),
            drone: to_kira(&synth::night_drone()),
        }
    }
}

/// Live output state; a non-send resource because the device stream is
/// owned by the main thread on some platforms.
pub struct AudioOutput {
    manager: AudioManager<DefaultBackend>,
    bank: Bank,
    rotation: u32,
    rain: Option<StaticSoundHandle>,
    drone: Option<StaticSoundHandle>,
    voice: Option<StaticSoundHandle>,
    voice_cache: HashMap<String, Option<StaticSoundData>>,
}

impl AudioOutput {
    fn variant(&mut self, key: &str) -> Option<StaticSoundData> {
        let list = self.bank.sounds.get(key)?;
        self.rotation = self.rotation.wrapping_add(1);
        list.get(self.rotation as usize % list.len()).cloned()
    }
}

fn amplitude(value: f32) -> Volume {
    Volume::Amplitude(f64::from(value.clamp(0.0, 2.0)))
}

fn start_audio_output(world: &mut World) {
    let manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
        Ok(manager) => manager,
        Err(error) => {
            warn!("audio output unavailable, continuing silently: {error}");
            return;
        }
    };
    let mut output = AudioOutput {
        manager,
        bank: Bank::render(),
        rotation: 0,
        rain: None,
        drone: None,
        voice: None,
        voice_cache: HashMap::new(),
    };
    let rain = output.bank.rain.clone().volume(Volume::Amplitude(0.0));
    output.rain = output.manager.play(rain).ok();
    let drone = output.bank.drone.clone().volume(Volume::Amplitude(0.0));
    output.drone = output.manager.play(drone).ok();
    world.insert_non_send_resource(output);
}

/// Plays resolved effects, panned and attenuated from the listener.
pub fn play_resolved_sfx(
    output: Option<NonSendMut<AudioOutput>>,
    mut events: EventReader<ResolvedSfxEvent>,
    listeners: Query<&GlobalTransform, With<AudioListener>>,
) {
    let Some(mut output) = output else {
        events.clear();
        return;
    };
    let listener = listeners.iter().next();
    for event in events.read() {
        let Some(key) = bank_key(event.sound_path) else {
            continue;
        };
        let (pan, gain) = match (event.world_position, listener) {
            (Some(at), Some(listener)) => spatialize(listener, at),
            _ => (0.5, 1.0),
        };
        let volume = event.volume * gain;
        if volume <= 0.001 {
            continue;
        }
        let Some(sound) = output.variant(key) else {
            continue;
        };
        let sound = sound
            .volume(amplitude(volume))
            .playback_rate(f64::from(event.pitch))
            .panning(pan);
        if let Err(error) = output.manager.play(sound) {
            warn!("could not play {}: {error}", event.sound_path);
        }
    }
}

/// Plays each voice line as it starts, replacing any line still playing.
pub fn play_voice_lines(
    output: Option<NonSendMut<AudioOutput>>,
    root: Option<Res<VoiceAssetRoot>>,
    mut events: EventReader<VoiceLineStartedEvent>,
) {
    let Some(mut output) = output else {
        events.clear();
        return;
    };
    for event in events.read() {
        let path = event.cue.audio_path.clone();
        let data = output
            .voice_cache
            .entry(path.clone())
            .or_insert_with(|| {
                let file = root.as_ref().map_or(PathBuf::from(&path), |r| {
                    r.0.join(path.trim_start_matches("assets/"))
                });
                StaticSoundData::from_file(&file)
                    .map_err(|e| warn!("voice line {} unavailable: {e}", file.display()))
                    .ok()
            })
            .clone();
        let Some(data) = data else {
            continue;
        };
        if let Some(previous) = output.voice.as_mut() {
            previous.stop(Tween {
                duration: Duration::from_millis(80),
                ..default()
            });
        }
        output.voice = output
            .manager
            .play(data.volume(amplitude(event.volume)))
            .ok();
    }
}

/// Follows the mixer: rain and music beds track their bus volumes and the
/// adaptive music level.
pub fn update_beds(
    output: Option<NonSendMut<AudioOutput>>,
    settings: Res<AudioMixerSettings>,
    music: Option<Res<MusicOutputLevels>>,
) {
    let Some(mut output) = output else {
        return;
    };
    if !settings.is_changed() && !music.as_ref().is_some_and(|m| m.is_changed()) {
        return;
    }
    let tween = Tween {
        duration: Duration::from_millis(400),
        ..default()
    };
    let rain = settings.compute_effective_volume(AudioChannelBus::Ambience, 1.0);
    if let Some(handle) = output.rain.as_mut() {
        handle.set_volume(Volume::Amplitude(f64::from(rain) * RAIN_LEVEL), tween);
    }
    let music_bus = settings.compute_effective_volume(AudioChannelBus::DynamicMusic, 1.0);
    let intensity = music.map_or(1.0, |m| 0.6 + 0.4 * m.parkour.max(m.combat).max(m.ambient));
    if let Some(handle) = output.drone.as_mut() {
        handle.set_volume(
            Volume::Amplitude(f64::from(music_bus * intensity) * MUSIC_LEVEL),
            tween,
        );
    }
}

/// Speaker output for the windowed client.
pub struct AudioOutputPlugin;

impl Plugin for AudioOutputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_audio_output).add_systems(
            Update,
            (play_resolved_sfx, play_voice_lines, update_beds)
                .after(crate::audio_bus::route_sfx_events),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_map_to_bank_sounds() {
        assert_eq!(bank_key("sfx_footstep_metal"), Some("footstep"));
        assert_eq!(bank_key("sfx_katana_swing_heavy"), Some("swing_heavy"));
        assert_eq!(bank_key("sfx_land_hard"), Some("land_hard"));
        assert_eq!(bank_key("sfx_parry_spark"), Some("parry"));
        assert_eq!(bank_key("sfx_zipline_whine"), None);
    }

    #[test]
    fn spatialization_pans_and_fades() {
        let listener = GlobalTransform::from(Transform::IDENTITY);
        let (pan, gain) = spatialize(&listener, Vec3::new(5.0, 0.0, 0.0));
        assert!(pan > 0.8 && gain > 0.5, "right of the listener");
        let (pan, _) = spatialize(&listener, Vec3::new(-5.0, 0.0, 0.0));
        assert!(pan < 0.2);
        let (_, far) = spatialize(&listener, Vec3::new(0.0, 0.0, -HEARING_RANGE * 2.0));
        assert_eq!(far, 0.0);
    }

    #[test]
    fn bank_converts_to_frames() {
        let data = to_kira(&synth::slide_loop());
        assert_eq!(data.sample_rate, SAMPLE_RATE);
        assert!(data.frames.len() > 1000);
        assert!(data.settings.loop_region.is_some());
    }
}
