//! 3D positional audio: inverse-distance rolloff, occlusion filtering and
//! stereo panning, resolved per emitter against the active 3D camera.

use crate::audio_bus::{AudioChannelBus, AudioMixerSettings};
use bevy::prelude::*;

/// Rolloff factor of the inverse-distance model.
pub const DEFAULT_ROLLOFF: f32 = 1.2;
/// Low-pass cutoff with a clear line of sight, Hz.
pub const OPEN_AIR_CUTOFF_HZ: f32 = 20_000.0;
/// Low-pass cutoff behind a solid wall, Hz.
pub const FULLY_OCCLUDED_CUTOFF_HZ: f32 = 800.0;
/// Gain lost to full occlusion.
pub const OCCLUSION_GAIN_LOSS: f32 = 0.6;
/// Pitch drop at full occlusion (a muffled, darker tone).
pub const OCCLUSION_PITCH_DROP: f32 = 0.12;

#[derive(Component, Debug, Clone, PartialEq, Reflect)]
#[reflect(Component, Debug, PartialEq)]
pub struct SpatialAudioEmitter {
    pub sound_path: &'static str,
    pub bus: AudioChannelBus,
    pub max_audible_distance: f32,
    pub reference_distance: f32,
    pub volume: f32,
    pub is_looping: bool,
    /// 0 = clear line of sight, 1 = fully blocked.
    pub occlusion_factor: f32,
}

impl Default for SpatialAudioEmitter {
    fn default() -> Self {
        Self {
            sound_path: "",
            bus: AudioChannelBus::Ambience,
            max_audible_distance: 35.0,
            reference_distance: 2.0,
            volume: 1.0,
            is_looping: false,
            occlusion_factor: 0.0,
        }
    }
}

impl SpatialAudioEmitter {
    pub fn new(sound_path: &'static str, bus: AudioChannelBus) -> Self {
        Self {
            sound_path,
            bus,
            ..default()
        }
    }
}

/// Resolved playback parameters, rewritten every frame.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect)]
#[reflect(Component, Debug, PartialEq)]
pub struct SpatialAudioOutput {
    pub distance: f32,
    /// Distance and occlusion attenuation, before the mixer.
    pub gain: f32,
    /// Final linear volume after the bus and master faders.
    pub effective_volume: f32,
    pub pitch: f32,
    /// -1 = hard left, +1 = hard right.
    pub pan: f32,
    pub lowpass_cutoff_hz: f32,
    pub audible: bool,
}

/// Inverse-distance gain: full inside `reference_distance`, silent past
/// `max_distance`.
pub fn calculate_spatial_gain(
    distance: f32,
    reference_distance: f32,
    max_distance: f32,
    rolloff: f32,
) -> f32 {
    if !distance.is_finite() || distance > max_distance {
        return 0.0;
    }
    if distance <= reference_distance {
        return 1.0;
    }
    (reference_distance / (reference_distance + rolloff * (distance - reference_distance)))
        .clamp(0.0, 1.0)
}

/// Pitch of a sound heard through `occlusion` (0..1) of obstruction.
pub fn calculate_occluded_pitch(base_pitch: f32, occlusion: f32) -> f32 {
    base_pitch * (1.0 - OCCLUSION_PITCH_DROP * occlusion.clamp(0.0, 1.0))
}

/// Low-pass cutoff for `occlusion`, interpolated on a log-frequency scale so
/// the muffling sounds even across the range.
pub fn calculate_occlusion_cutoff_hz(occlusion: f32) -> f32 {
    let t = occlusion.clamp(0.0, 1.0);
    (OPEN_AIR_CUTOFF_HZ.ln() + (FULLY_OCCLUDED_CUTOFF_HZ.ln() - OPEN_AIR_CUTOFF_HZ.ln()) * t).exp()
}

pub fn calculate_occlusion_gain(occlusion: f32) -> f32 {
    1.0 - OCCLUSION_GAIN_LOSS * occlusion.clamp(0.0, 1.0)
}

/// Stereo position of a sound at `emitter` for a listener at `listener`.
pub fn calculate_pan(listener: &GlobalTransform, emitter: Vec3) -> f32 {
    let to_emitter = (emitter - listener.translation()).normalize_or_zero();
    to_emitter.dot(listener.right().as_vec3()).clamp(-1.0, 1.0)
}

/// Resolves every emitter against the active 3D camera with the highest
/// render order.
pub fn update_spatial_audio_emitters(
    mut commands: Commands,
    settings: Res<AudioMixerSettings>,
    listeners: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut emitters: Query<(
        Entity,
        &GlobalTransform,
        &SpatialAudioEmitter,
        Option<&mut SpatialAudioOutput>,
    )>,
) {
    let Some((_, listener)) = listeners
        .iter()
        .filter(|(camera, _)| camera.is_active)
        .max_by_key(|(camera, _)| camera.order)
    else {
        return;
    };
    for (entity, transform, emitter, output) in &mut emitters {
        let position = transform.translation();
        let distance = position.distance(listener.translation());
        let gain = calculate_spatial_gain(
            distance,
            emitter.reference_distance,
            emitter.max_audible_distance,
            DEFAULT_ROLLOFF,
        ) * calculate_occlusion_gain(emitter.occlusion_factor);
        let effective_volume =
            settings.compute_effective_volume(emitter.bus, emitter.volume * gain);
        let resolved = SpatialAudioOutput {
            distance,
            gain,
            effective_volume,
            pitch: calculate_occluded_pitch(1.0, emitter.occlusion_factor),
            pan: calculate_pan(listener, position),
            lowpass_cutoff_hz: calculate_occlusion_cutoff_hz(emitter.occlusion_factor),
            audible: effective_volume > 0.0,
        };
        match output {
            Some(mut current) if *current != resolved => *current = resolved,
            Some(_) => {}
            None => {
                commands.entity(entity).insert(resolved);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_curve() {
        assert_eq!(calculate_spatial_gain(0.0, 2.0, 35.0, DEFAULT_ROLLOFF), 1.0);
        assert_eq!(calculate_spatial_gain(2.0, 2.0, 35.0, DEFAULT_ROLLOFF), 1.0);
        let at_10 = calculate_spatial_gain(10.0, 2.0, 35.0, DEFAULT_ROLLOFF);
        assert!(
            (at_10 - 2.0 / 11.6).abs() < 1e-6 && (at_10 - 0.17).abs() < 0.005,
            "{at_10}"
        );
        assert!(calculate_spatial_gain(35.0, 2.0, 35.0, DEFAULT_ROLLOFF) > 0.0);
        assert_eq!(
            calculate_spatial_gain(35.01, 2.0, 35.0, DEFAULT_ROLLOFF),
            0.0
        );
        assert_eq!(
            calculate_spatial_gain(f32::NAN, 2.0, 35.0, DEFAULT_ROLLOFF),
            0.0
        );
        let gains: Vec<f32> = (0..36)
            .map(|d| calculate_spatial_gain(d as f32, 2.0, 35.0, DEFAULT_ROLLOFF))
            .collect();
        assert!(gains.windows(2).all(|w| w[1] <= w[0]), "monotonic falloff");
    }

    #[test]
    fn occlusion_muffles() {
        assert_eq!(calculate_occluded_pitch(1.0, 0.0), 1.0);
        assert!((calculate_occluded_pitch(1.0, 1.0) - 0.88).abs() < 1e-6);
        assert_eq!(
            calculate_occluded_pitch(2.0, 5.0),
            calculate_occluded_pitch(2.0, 1.0),
            "clamped"
        );
        assert!((calculate_occlusion_cutoff_hz(0.0) - 20_000.0).abs() < 1.0);
        assert!((calculate_occlusion_cutoff_hz(1.0) - 800.0).abs() < 0.1);
        let half = calculate_occlusion_cutoff_hz(0.5);
        assert!((half - 4_000.0).abs() < 1.0, "geometric midpoint: {half}");
        assert!((calculate_occlusion_gain(1.0) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn pan_follows_the_listener_orientation() {
        let listener = GlobalTransform::IDENTITY;
        assert!((calculate_pan(&listener, Vec3::new(5.0, 0.0, 0.0)) - 1.0).abs() < 1e-6);
        assert!((calculate_pan(&listener, Vec3::new(-5.0, 0.0, 0.0)) + 1.0).abs() < 1e-6);
        assert!(
            calculate_pan(&listener, Vec3::new(0.0, 0.0, -5.0)).abs() < 1e-6,
            "dead ahead"
        );
        let turned = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_y(
            std::f32::consts::FRAC_PI_2,
        )));
        assert!(
            (calculate_pan(&turned, Vec3::new(0.0, 0.0, -5.0)) - 1.0).abs() < 1e-5,
            "after turning left, ahead is to the right"
        );
        assert_eq!(calculate_pan(&listener, Vec3::ZERO), 0.0);
    }

    #[test]
    fn system_resolves_emitters_against_the_camera() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .init_resource::<AudioMixerSettings>()
            .add_systems(Update, update_spatial_audio_emitters);
        app.world_mut()
            .spawn((Camera3d::default(), Transform::default()));
        let near = app
            .world_mut()
            .spawn((
                SpatialAudioEmitter::new("sfx_steam_vent_hiss", AudioChannelBus::Ambience),
                Transform::from_xyz(1.0, 0.0, 0.0),
            ))
            .id();
        let mid = app
            .world_mut()
            .spawn((
                SpatialAudioEmitter {
                    occlusion_factor: 1.0,
                    ..SpatialAudioEmitter::new("amb_neon_hum", AudioChannelBus::Ambience)
                },
                Transform::from_xyz(-10.0, 0.0, 0.0),
            ))
            .id();
        let far = app
            .world_mut()
            .spawn((
                SpatialAudioEmitter::default(),
                Transform::from_xyz(0.0, 0.0, -50.0),
            ))
            .id();
        app.update();
        app.update();
        let out = |e: Entity| *app.world().get::<SpatialAudioOutput>(e).unwrap();
        let n = out(near);
        assert_eq!((n.gain, n.audible), (1.0, true));
        assert!((n.effective_volume - 0.6).abs() < 1e-6, "ambience fader");
        assert!((n.pan - 1.0).abs() < 1e-6);
        let m = out(mid);
        assert!((m.gain - (2.0 / 11.6) * 0.4).abs() < 1e-5);
        assert!(m.pan < -0.99 && m.lowpass_cutoff_hz < 801.0 && m.pitch < 1.0);
        let f = out(far);
        assert!(!f.audible && f.effective_volume == 0.0);
        assert!((f.distance - 50.0).abs() < 1e-4);
    }
}
