//! Turns local gameplay into sound requests: footsteps paced by stride,
//! jump thrusters, landings scaled by fall speed, slide friction, katana
//! swings and melee impacts.

use crate::client_network::{ClientPredictionState, LocalPredictedPlayer};
use bevy::prelude::*;
use game_audio::{AudioChannelBus, PlaySfxEvent};
use game_combat::{AttackPhase, MeleeCombatState, MeleeHitEvent};
use game_parkour::KinematicVelocity;

/// Downward speed above which a landing is played as hard.
const HARD_LANDING_SPEED: f32 = 11.0;

/// What the audio layer remembers about the local character between frames.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct FootfallTracker {
    pub distance: f32,
    pub was_grounded: bool,
    pub was_sliding: bool,
    pub last_fall_speed: f32,
    pub last_attack_ticks: Option<u32>,
    pub variation: u32,
}

/// Sounds the local character should make this frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FootfallCues {
    pub footstep: bool,
    pub jump: bool,
    pub land: Option<bool>,
    pub slide: bool,
}

/// Pure step of the footfall logic, for one frame of `dt` seconds.
pub fn step_footfalls(
    tracker: &mut FootfallTracker,
    velocity: Vec3,
    grounded: bool,
    sliding: bool,
    dt: f32,
) -> FootfallCues {
    let mut cues = FootfallCues::default();
    let planar = Vec2::new(velocity.x, velocity.z).length();
    if grounded && !tracker.was_grounded {
        cues.land = Some(tracker.last_fall_speed > HARD_LANDING_SPEED);
        tracker.distance = 0.0;
    }
    if !grounded && tracker.was_grounded && velocity.y > 0.0 {
        cues.jump = true;
    }
    if sliding && !tracker.was_sliding {
        cues.slide = true;
    }
    if grounded && !sliding && planar > 0.5 {
        tracker.distance += planar * dt;
        // One footfall per step of the animated gait (two per cycle).
        let stride = crate::character::pose::stride_length(planar) * 0.5;
        if tracker.distance >= stride {
            tracker.distance -= stride;
            cues.footstep = true;
        }
    }
    if !grounded {
        tracker.last_fall_speed = (-velocity.y).max(0.0);
    }
    tracker.was_grounded = grounded;
    tracker.was_sliding = sliding;
    cues
}

fn varied_pitch(variation: u32) -> f32 {
    const PITCHES: [f32; 5] = [0.94, 1.03, 0.98, 1.07, 1.0];
    PITCHES[variation as usize % PITCHES.len()]
}

pub fn emit_movement_sounds(
    time: Res<Time>,
    mut commands: Commands,
    mut players: Query<
        (
            Entity,
            &Transform,
            &KinematicVelocity,
            &ClientPredictionState,
            Option<&mut FootfallTracker>,
        ),
        With<LocalPredictedPlayer>,
    >,
    mut sfx: EventWriter<PlaySfxEvent>,
) {
    for (entity, transform, velocity, prediction, tracker) in &mut players {
        let Some(mut tracker) = tracker else {
            commands.entity(entity).insert(FootfallTracker {
                was_grounded: true,
                ..default()
            });
            continue;
        };
        let movement = prediction.movement;
        let cues = step_footfalls(
            &mut tracker,
            velocity.0,
            movement.grounded,
            movement.sliding,
            time.delta_secs(),
        );
        let at = transform.translation;
        let mut play = |path, volume: f32, tracker: &mut FootfallTracker| {
            tracker.variation = tracker.variation.wrapping_add(1);
            sfx.send(PlaySfxEvent {
                volume,
                pitch: varied_pitch(tracker.variation),
                ..PlaySfxEvent::at(path, AudioChannelBus::ParkourMovement, at)
            });
        };
        if cues.footstep {
            play("sfx_footstep_concrete", 0.55, &mut tracker);
        }
        if cues.jump {
            play("sfx_jump_takeoff", 0.6, &mut tracker);
        }
        match cues.land {
            Some(true) => play("sfx_land_hard", 0.9, &mut tracker),
            Some(false) => play("sfx_land_soft", 0.6, &mut tracker),
            None => {}
        }
        if cues.slide {
            play("sfx_slide_loop", 0.65, &mut tracker);
        }
    }
}

/// Plays a swing as each attack starts and an impact for every melee hit.
pub fn emit_combat_sounds(
    mut players: Query<
        (&Transform, &MeleeCombatState, Option<&mut FootfallTracker>),
        With<LocalPredictedPlayer>,
    >,
    mut hits: EventReader<MeleeHitEvent>,
    transforms: Query<&GlobalTransform>,
    mut sfx: EventWriter<PlaySfxEvent>,
) {
    for (transform, melee, tracker) in &mut players {
        let Some(mut tracker) = tracker else {
            continue;
        };
        let attacking = melee.attack_phase != AttackPhase::Idle;
        let started = attacking
            && tracker
                .last_attack_ticks
                .is_none_or(|last| melee.attack_ticks < last);
        if started {
            let heavy = melee.current_combo_index % 3 == 2;
            let path = if heavy {
                "sfx_katana_swing_heavy"
            } else {
                "sfx_katana_swing_light"
            };
            tracker.variation = tracker.variation.wrapping_add(1);
            sfx.send(PlaySfxEvent {
                volume: 0.75,
                pitch: varied_pitch(tracker.variation),
                ..PlaySfxEvent::at(path, AudioChannelBus::CombatFoley, transform.translation)
            });
        }
        tracker.last_attack_ticks = attacking.then_some(melee.attack_ticks);
    }
    for hit in hits.read() {
        let Ok(at) = transforms.get(hit.target).map(GlobalTransform::translation) else {
            continue;
        };
        sfx.send(PlaySfxEvent {
            volume: 0.85,
            ..PlaySfxEvent::at("sfx_melee_impact", AudioChannelBus::CombatFoley, at)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_server::{SPRINT_SPEED, WALK_SPEED};

    #[test]
    fn strides_pace_footsteps_and_sprinting_lengthens_them() {
        let mut tracker = FootfallTracker {
            was_grounded: true,
            ..default()
        };
        let mut walk_steps = 0;
        for _ in 0..600 {
            let cues = step_footfalls(
                &mut tracker,
                Vec3::new(0.0, 0.0, -WALK_SPEED),
                true,
                false,
                1.0 / 60.0,
            );
            walk_steps += usize::from(cues.footstep);
        }
        // 10 s at 5.5 m/s over 1.84 m steps.
        assert!((29..=30).contains(&walk_steps), "{walk_steps}");
        let mut sprint_steps = 0;
        for _ in 0..600 {
            let cues = step_footfalls(
                &mut tracker,
                Vec3::new(0.0, 0.0, -SPRINT_SPEED),
                true,
                false,
                1.0 / 60.0,
            );
            sprint_steps += usize::from(cues.footstep);
        }
        // 10 s at 9 m/s over 2.47 m steps: faster cadence, longer steps.
        assert!((36..=37).contains(&sprint_steps), "{sprint_steps}");
    }

    #[test]
    fn jump_then_landing_hardness_follows_fall_speed() {
        let mut tracker = FootfallTracker {
            was_grounded: true,
            ..default()
        };
        let cues = step_footfalls(&mut tracker, Vec3::Y * 8.0, false, false, 0.016);
        assert!(cues.jump);
        step_footfalls(&mut tracker, Vec3::NEG_Y * 6.0, false, false, 0.016);
        let soft = step_footfalls(&mut tracker, Vec3::ZERO, true, false, 0.016);
        assert_eq!(soft.land, Some(false));
        step_footfalls(&mut tracker, Vec3::NEG_Y * 20.0, false, false, 0.016);
        let hard = step_footfalls(&mut tracker, Vec3::ZERO, true, false, 0.016);
        assert_eq!(hard.land, Some(true));
    }

    #[test]
    fn slides_play_once_and_silence_footsteps() {
        let mut tracker = FootfallTracker {
            was_grounded: true,
            ..default()
        };
        let v = Vec3::new(0.0, 0.0, -16.0);
        assert!(step_footfalls(&mut tracker, v, true, true, 0.016).slide);
        for _ in 0..200 {
            let cues = step_footfalls(&mut tracker, v, true, true, 0.016);
            assert!(!cues.slide && !cues.footstep);
        }
    }
}
