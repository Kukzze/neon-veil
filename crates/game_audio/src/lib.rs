//! Adaptive music, positional audio, dialogue and the foley event bus.
//!
//! Everything here computes levels and playback requests deterministically
//! and runs headless. An output backend consumes [`ResolvedSfxEvent`],
//! [`VoiceLineStartedEvent`], [`MusicOutputLevels`] and
//! [`SpatialAudioOutput`].

pub mod audio_bus;
pub mod dynamic_music;
pub mod foley_catalog;
pub mod spatial;
pub mod voiceover_player;

pub use audio_bus::{
    AudioChannelBus, AudioMixerSettings, PlaySfxEvent, ResolvedSfxEvent, SubtitleDisplayEvent,
    TriggerVoiceCueEvent, VoiceLineStartedEvent, DUCKED_MUSIC_MULTIPLIER,
};
pub use dynamic_music::{calculate_parkour_wind_volume, DynamicMusicMixer, MusicOutputLevels};
pub use foley_catalog::{find_sound_cue, SoundCueDefinition, MASTER_AUDIO_CATALOG};
pub use spatial::{calculate_spatial_gain, SpatialAudioEmitter, SpatialAudioOutput};
pub use voiceover_player::{DialogueEntry, DialogueManifest, VoiceCueData, VoiceoverQueue};

use bevy::prelude::*;

pub struct AudioPlugin;

/// Alias that avoids clashing with Bevy's own audio plugin name.
pub type GameAudioPlugin = AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioMixerSettings>()
            .init_resource::<DynamicMusicMixer>()
            .init_resource::<MusicOutputLevels>()
            .init_resource::<VoiceoverQueue>()
            .register_type::<AudioChannelBus>()
            .register_type::<AudioMixerSettings>()
            .register_type::<DynamicMusicMixer>()
            .register_type::<MusicOutputLevels>()
            .register_type::<SpatialAudioEmitter>()
            .register_type::<SpatialAudioOutput>()
            .add_event::<PlaySfxEvent>()
            .add_event::<ResolvedSfxEvent>()
            .add_event::<TriggerVoiceCueEvent>()
            .add_event::<SubtitleDisplayEvent>()
            .add_event::<VoiceLineStartedEvent>()
            .add_systems(
                Update,
                (
                    audio_bus::process_audio_ducking_system,
                    audio_bus::route_sfx_events,
                    dynamic_music::update_adaptive_music_stems,
                    spatial::update_spatial_audio_emitters,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use game_ai::{AiPerception, AlertState};
    use game_core::identifiers::PlayerCharacterMarker;
    use game_parkour::{FlowState, KinematicVelocity, ParkourState};
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AudioPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                1.0 / 60.0,
            )));
        app.update();
        app
    }

    fn line(id: &str, seconds: f32) -> TriggerVoiceCueEvent {
        TriggerVoiceCueEvent {
            cue_id: id.into(),
            speaker: "AURA".into(),
            audio_file_path: format!("assets/audio/voiceovers/{id}.ogg"),
            subtitle_text: format!("subtitle {id}"),
            duration_seconds: seconds,
        }
    }

    #[test]
    fn voice_cues_duck_music_play_in_order_then_release() {
        let mut app = app();
        for (id, s) in [("a", 0.5), ("b", 0.5), ("c", 0.5)] {
            app.world_mut().send_event(line(id, s));
        }
        app.update();
        assert!(
            app.world()
                .resource::<AudioMixerSettings>()
                .is_ducking_active
        );

        let mut subtitles = Vec::new();
        let mut cursor = app
            .world()
            .resource::<Events<SubtitleDisplayEvent>>()
            .get_cursor();
        let mut lowest: f32 = 1.0;
        for _ in 0..150 {
            app.update();
            let events = app.world().resource::<Events<SubtitleDisplayEvent>>();
            subtitles.extend(cursor.read(events).map(|e| e.text.clone()));
            lowest = lowest.min(
                app.world()
                    .resource::<AudioMixerSettings>()
                    .ducking_multiplier,
            );
        }
        // Events live for two frames, so the first line's subtitle is still
        // readable: all three, each exactly once, in order.
        assert_eq!(subtitles, vec!["subtitle a", "subtitle b", "subtitle c"]);
        assert!(
            (lowest - DUCKED_MUSIC_MULTIPLIER).abs() < 0.02,
            "ducked to -6 dB ({lowest})"
        );
        let settings = *app.world().resource::<AudioMixerSettings>();
        assert!(!settings.is_ducking_active);
        assert!(
            (settings.ducking_multiplier - 1.0).abs() < 1e-3,
            "released after the queue empties"
        );
        assert!(!app.world().resource::<VoiceoverQueue>().is_playing());
    }

    #[test]
    fn voice_lines_start_events_carry_the_mixed_volume() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<AudioMixerSettings>()
            .voice_volume = 0.5;
        app.world_mut().send_event(line("intro", 1.0));
        app.update();
        let started: Vec<VoiceLineStartedEvent> = app
            .world_mut()
            .resource_mut::<Events<VoiceLineStartedEvent>>()
            .drain()
            .collect();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].cue.cue_id, "intro");
        assert_eq!(started[0].volume, 0.5);
    }

    #[test]
    fn sfx_requests_are_mixed() {
        let mut app = app();
        app.world_mut()
            .send_event(PlaySfxEvent::ui("sfx_level_up_fanfare"));
        app.world_mut().send_event(PlaySfxEvent {
            volume: 0.0,
            ..PlaySfxEvent::at(
                "sfx_footstep_metal",
                AudioChannelBus::ParkourMovement,
                Vec3::ONE,
            )
        });
        app.world_mut().send_event(PlaySfxEvent {
            pitch: f32::NAN,
            ..PlaySfxEvent::at("sfx_parry_spark", AudioChannelBus::CombatFoley, Vec3::X)
        });
        app.update();
        let resolved: Vec<ResolvedSfxEvent> = app
            .world_mut()
            .resource_mut::<Events<ResolvedSfxEvent>>()
            .drain()
            .collect();
        assert_eq!(resolved.len(), 2, "silent requests are dropped");
        assert!((resolved[0].volume - 0.9).abs() < 1e-6);
        assert_eq!(resolved[1].world_position, Some(Vec3::X));
        assert!((resolved[1].volume - 0.85).abs() < 1e-6);
        assert_eq!(resolved[1].pitch, 1.0, "bad pitch falls back to unity");
    }

    #[test]
    fn music_follows_the_player_and_enemy_awareness() {
        let mut app = app();
        let player = app
            .world_mut()
            .spawn((
                PlayerCharacterMarker,
                ParkourState::Sprinting,
                FlowState {
                    current_flow: 80.0,
                    ..default()
                },
                KinematicVelocity(Vec3::new(16.5, 0.0, 0.0)),
            ))
            .id();
        for _ in 0..240 {
            app.update();
        }
        let mixer = *app.world().resource::<DynamicMusicMixer>();
        assert!(mixer.parkour_volume > 0.99 && (mixer.ambient_volume - 0.3).abs() < 0.01);
        let out = *app.world().resource::<MusicOutputLevels>();
        assert!(
            (out.parkour - 0.7 * mixer.parkour_volume).abs() < 1e-5,
            "music fader applied"
        );
        assert!(
            (out.wind - 0.5 * 0.765).abs() < 1e-4,
            "16.5 m/s -> half wind on the parkour bus"
        );

        let enemy = app
            .world_mut()
            .spawn(AiPerception {
                alert_state: AlertState::Combat,
                ..default()
            })
            .id();
        for _ in 0..240 {
            app.update();
        }
        let mixer = *app.world().resource::<DynamicMusicMixer>();
        assert!(mixer.combat_volume > 0.99 && (mixer.ambient_volume - 0.2).abs() < 0.01);

        // Voice ducking reaches the stems too.
        app.world_mut().send_event(line("radio", 5.0));
        for _ in 0..60 {
            app.update();
        }
        let out = *app.world().resource::<MusicOutputLevels>();
        assert!((out.combat - 0.7 * 0.5).abs() < 0.01, "{}", out.combat);

        // Fight over, player sneaking: the stealth stem takes over.
        app.world_mut().despawn(enemy);
        app.world_mut().entity_mut(player).insert((
            ParkourState::Crouching,
            FlowState::default(),
            KinematicVelocity(Vec3::ZERO),
        ));
        for _ in 0..240 {
            app.update();
        }
        let mixer = *app.world().resource::<DynamicMusicMixer>();
        assert!(mixer.stealth_volume > 0.99 && mixer.combat_volume < 0.01);
        assert_eq!(app.world().resource::<MusicOutputLevels>().wind, 0.0);
    }
}
