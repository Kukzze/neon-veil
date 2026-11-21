//! Four-stem adaptive music and velocity-driven parkour wind.
//!
//! The four stems play in sync and only their levels move. The game state
//! picks a target mix and each stem eases toward it exponentially with a
//! frame-rate independent factor, so levels never step (no pops) and never
//! overshoot.

use crate::audio_bus::{AudioChannelBus, AudioMixerSettings};
use bevy::prelude::*;
use game_ai::{AiPerception, AlertState};
use game_core::identifiers::PlayerCharacterMarker;
use game_parkour::{FlowState, KinematicVelocity, ParkourState};

/// Flow above which the parkour stem takes over.
pub const PARKOUR_FLOW_THRESHOLD: f32 = 40.0;
/// Wind is silent below this horizontal speed, m/s.
pub const WIND_MIN_SPEED: f32 = 8.0;
/// Wind reaches full volume at this horizontal speed, m/s.
pub const WIND_MAX_SPEED: f32 = 25.0;

/// Stem levels as `(ambient, stealth, parkour, combat)`.
pub type StemLevels = (f32, f32, f32, f32);

#[derive(Resource, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Resource, Debug, PartialEq)]
pub struct DynamicMusicMixer {
    pub ambient_volume: f32,
    pub stealth_volume: f32,
    pub parkour_volume: f32,
    pub combat_volume: f32,
    /// Exponential approach rate, 1/s.
    pub crossfade_speed: f32,
}

impl Default for DynamicMusicMixer {
    fn default() -> Self {
        Self {
            ambient_volume: 1.0,
            stealth_volume: 0.0,
            parkour_volume: 0.0,
            combat_volume: 0.0,
            crossfade_speed: 1.8,
        }
    }
}

impl DynamicMusicMixer {
    /// Target mix for the current situation; combat outranks movement,
    /// movement outranks stealth.
    pub fn determine_targets(
        in_combat: bool,
        flow_meter: f32,
        is_wall_interacting: bool,
        is_crouching: bool,
    ) -> StemLevels {
        if in_combat {
            (0.2, 0.0, 0.4, 1.0)
        } else if flow_meter > PARKOUR_FLOW_THRESHOLD || is_wall_interacting {
            (0.3, 0.0, 1.0, 0.0)
        } else if is_crouching {
            (0.3, 1.0, 0.0, 0.0)
        } else {
            (1.0, 0.0, 0.0, 0.0)
        }
    }

    pub fn levels(&self) -> StemLevels {
        (
            self.ambient_volume,
            self.stealth_volume,
            self.parkour_volume,
            self.combat_volume,
        )
    }

    pub fn update_crossfade(&mut self, targets: StemLevels, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let t = 1.0 - (-self.crossfade_speed.max(0.0) * dt).exp();
        let ease = |current: &mut f32, target: f32| {
            *current += (target.clamp(0.0, 1.0) - *current) * t;
        };
        ease(&mut self.ambient_volume, targets.0);
        ease(&mut self.stealth_volume, targets.1);
        ease(&mut self.parkour_volume, targets.2);
        ease(&mut self.combat_volume, targets.3);
    }
}

/// Wind loop volume for a horizontal speed: silent below 8 m/s, linear to
/// full at 25 m/s.
pub fn calculate_parkour_wind_volume(horizontal_speed: f32) -> f32 {
    if !horizontal_speed.is_finite() {
        return 0.0;
    }
    ((horizontal_speed - WIND_MIN_SPEED) / (WIND_MAX_SPEED - WIND_MIN_SPEED)).clamp(0.0, 1.0)
}

/// Final output levels of the music stems and wind loop after the mixer
/// (bus fader, master, voice ducking), for the output backend.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Reflect)]
#[reflect(Resource, Debug, PartialEq)]
pub struct MusicOutputLevels {
    pub ambient: f32,
    pub stealth: f32,
    pub parkour: f32,
    pub combat: f32,
    pub wind: f32,
}

/// Picks the stem mix from the local player's movement and enemy
/// awareness, crossfades, and publishes the mixed output levels.
pub fn update_adaptive_music_stems(
    time: Res<Time>,
    settings: Res<AudioMixerSettings>,
    mut mixer: ResMut<DynamicMusicMixer>,
    mut output: ResMut<MusicOutputLevels>,
    players: Query<
        (&ParkourState, &FlowState, Option<&KinematicVelocity>),
        With<PlayerCharacterMarker>,
    >,
    enemies: Query<&AiPerception>,
) {
    let in_combat = enemies.iter().any(|p| p.alert_state == AlertState::Combat);
    let player = players.iter().next();
    let targets = match player {
        Some((state, flow, _)) => DynamicMusicMixer::determine_targets(
            in_combat,
            flow.current_flow,
            state.is_wall_interacting(),
            *state == ParkourState::Crouching,
        ),
        None => DynamicMusicMixer::determine_targets(in_combat, 0.0, false, false),
    };
    mixer.update_crossfade(targets, time.delta_secs());

    let wind = player.and_then(|(_, _, v)| v).map_or(0.0, |v| {
        calculate_parkour_wind_volume(Vec2::new(v.0.x, v.0.z).length())
    });
    let music =
        |level: f32| settings.compute_effective_volume(AudioChannelBus::DynamicMusic, level);
    let levels = MusicOutputLevels {
        ambient: music(mixer.ambient_volume),
        stealth: music(mixer.stealth_volume),
        parkour: music(mixer.parkour_volume),
        combat: music(mixer.combat_volume),
        wind: settings.compute_effective_volume(AudioChannelBus::ParkourMovement, wind),
    };
    if *output != levels {
        *output = levels;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_per_situation() {
        assert_eq!(
            DynamicMusicMixer::determine_targets(false, 0.0, false, false),
            (1.0, 0.0, 0.0, 0.0)
        );
        assert_eq!(
            DynamicMusicMixer::determine_targets(true, 0.0, false, false),
            (0.2, 0.0, 0.4, 1.0)
        );
        assert_eq!(
            DynamicMusicMixer::determine_targets(false, 41.0, false, false),
            (0.3, 0.0, 1.0, 0.0)
        );
        assert_eq!(
            DynamicMusicMixer::determine_targets(false, 40.0, false, false),
            (1.0, 0.0, 0.0, 0.0),
            "strictly above 40"
        );
        assert_eq!(
            DynamicMusicMixer::determine_targets(false, 0.0, true, false),
            (0.3, 0.0, 1.0, 0.0)
        );
        assert_eq!(
            DynamicMusicMixer::determine_targets(false, 0.0, false, true),
            (0.3, 1.0, 0.0, 0.0)
        );
        // Priority: combat > movement > stealth.
        assert_eq!(
            DynamicMusicMixer::determine_targets(true, 90.0, true, true).3,
            1.0
        );
        assert_eq!(
            DynamicMusicMixer::determine_targets(false, 90.0, false, true).2,
            1.0
        );
    }

    #[test]
    fn default_is_exploration() {
        let mixer = DynamicMusicMixer::default();
        assert_eq!(mixer.levels(), (1.0, 0.0, 0.0, 0.0));
        assert_eq!(mixer.crossfade_speed, 1.8);
    }

    #[test]
    fn crossfade_moves_smoothly_toward_the_target() {
        let mut mixer = DynamicMusicMixer::default();
        let combat = DynamicMusicMixer::determine_targets(true, 0.0, false, false);
        mixer.update_crossfade(combat, 0.5);
        let (ambient, _, parkour, intense) = mixer.levels();
        let step = 1.0 - (-0.9f32).exp();
        assert!((intense - step).abs() < 1e-5, "{intense}");
        assert!(
            intense > 0.0 && intense < 1.0,
            "part of the way, not a jump"
        );
        assert!(ambient < 1.0 && ambient > 0.2);
        assert!((parkour - 0.4 * step).abs() < 1e-5);
        for _ in 0..20 {
            mixer.update_crossfade(combat, 0.5);
        }
        assert!((mixer.combat_volume - 1.0).abs() < 1e-6);
        assert!((mixer.ambient_volume - 0.2).abs() < 1e-6);
    }

    #[test]
    fn crossfade_is_frame_rate_independent_and_never_overshoots() {
        let target = (0.0, 0.0, 0.0, 1.0);
        let mut coarse = DynamicMusicMixer::default();
        coarse.update_crossfade(target, 1.0);
        let mut fine = DynamicMusicMixer::default();
        for _ in 0..144 {
            fine.update_crossfade(target, 1.0 / 144.0);
        }
        assert!((coarse.combat_volume - fine.combat_volume).abs() < 1e-4);
        let mut huge = DynamicMusicMixer::default();
        huge.update_crossfade(target, 1_000.0);
        assert_eq!(huge.levels(), target);
        huge.update_crossfade((5.0, -1.0, 0.0, 1.0), 1_000.0);
        assert_eq!(huge.ambient_volume, 1.0, "targets are clamped");
        let before = huge;
        huge.update_crossfade(target, f32::NAN);
        assert_eq!(huge, before);
    }

    #[test]
    fn wind_scales_between_8_and_25_m_s() {
        assert_eq!(calculate_parkour_wind_volume(5.0), 0.0);
        assert_eq!(calculate_parkour_wind_volume(8.0), 0.0);
        assert!((calculate_parkour_wind_volume(16.5) - 0.5).abs() < 1e-6);
        assert_eq!(calculate_parkour_wind_volume(25.0), 1.0);
        assert_eq!(calculate_parkour_wind_volume(30.0), 1.0);
        assert_eq!(calculate_parkour_wind_volume(f32::NAN), 0.0);
        assert_eq!(calculate_parkour_wind_volume(-20.0), 0.0);
    }
}
