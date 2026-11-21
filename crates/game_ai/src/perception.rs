//! Vision cones, hearing and alert states.
//!
//! Sight is a horizontal cone plus range check followed by a line-of-sight
//! ray. Seeing a player puts an enemy straight into combat. Gunfire within
//! hearing range alerts it and sends it to investigate; footsteps build
//! suspicion. Losing sight for [`LOSE_TARGET_SECONDS`] drops back to
//! suspicious and investigating the last known position. An enemy entering
//! combat alerts allies within [`ALARM_RADIUS_METERS`].

use crate::archetypes::Enemy;
use avian3d::prelude::*;
use bevy::prelude::*;
use game_combat::ShotFiredEvent;
use game_core::identifiers::PlayerCharacterMarker;
use game_core::input::ViewAngles;
use game_parkour::KinematicVelocity;

pub const LOSE_TARGET_SECONDS: f32 = 2.0;
pub const ALARM_RADIUS_METERS: f32 = 30.0;
/// Suspicion gained per second while hearing footsteps.
pub const FOOTSTEP_SUSPICION_PER_SECOND: f32 = 40.0;
/// Suspicion lost per second without stimuli.
pub const SUSPICION_DECAY_PER_SECOND: f32 = 10.0;
/// Players moving faster than this are audible, m/s.
pub const AUDIBLE_FOOTSTEP_SPEED: f32 = 5.0;
/// Eye height above an agent's capsule center.
pub const EYE_OFFSET: f32 = 0.78;
/// Height above a target's center that sight rays aim at (upper chest).
pub const SIGHT_TARGET_OFFSET: f32 = 0.4;

/// How aware an enemy is.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Reflect)]
#[reflect(Debug, Default, PartialEq, Hash)]
pub enum AlertState {
    #[default]
    Unaware,
    Suspicious,
    Alerted,
    Combat,
}

/// Senses and awareness.
#[derive(Component, Reflect, Debug, Clone, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct AiPerception {
    /// Full horizontal field of view.
    pub vision_angle_degrees: f32,
    /// Sight range while unaware or suspicious.
    pub vision_range_meters: f32,
    /// Sight range once alerted or in combat.
    pub alerted_vision_range_meters: f32,
    /// Footstep hearing range.
    pub hearing_range_meters: f32,
    pub gunfire_hearing_range_meters: f32,
    pub alert_state: AlertState,
    /// 0-100; reaching 100 alerts.
    pub suspicion_meter: f32,
    /// Allies have been told about the current engagement.
    pub alarm_raised: bool,
}

impl Default for AiPerception {
    fn default() -> Self {
        Self {
            vision_angle_degrees: 90.0,
            vision_range_meters: 25.0,
            alerted_vision_range_meters: 40.0,
            hearing_range_meters: 15.0,
            gunfire_hearing_range_meters: 50.0,
            alert_state: AlertState::Unaware,
            suspicion_meter: 0.0,
            alarm_raised: false,
        }
    }
}

/// What an enemy knows about its target and surroundings.
#[derive(Component, Reflect, Debug, Default, Clone, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct AiBlackboard {
    pub target_entity: Option<Entity>,
    pub last_known_target_pos: Option<Vec3>,
    pub time_since_target_seen: f32,
    pub patrol_waypoints: Vec<Vec3>,
    pub current_waypoint_index: usize,
    pub is_in_cover: bool,
    /// Where to go to look for the target (noise source or last sighting).
    pub investigate_position: Option<Vec3>,
    /// Chosen cover spot, kept while it remains valid.
    pub cover_position: Option<Vec3>,
}

/// Something perceived this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stimulus {
    Sighted { target: Entity, position: Vec3 },
    Gunfire { position: Vec3 },
    Footsteps { position: Vec3 },
}

/// An enemy entered combat and alerted nearby allies.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct AlarmRaisedEvent {
    pub source: Entity,
    pub target_position: Vec3,
}

impl AiPerception {
    pub fn effective_vision_range(&self) -> f32 {
        if self.alert_state >= AlertState::Alerted {
            self.alerted_vision_range_meters
        } else {
            self.vision_range_meters
        }
    }

    /// Updates awareness from this tick's stimuli (strongest first:
    /// sightings, then gunfire, then footsteps).
    pub fn observe(&mut self, blackboard: &mut AiBlackboard, stimuli: &[Stimulus], dt: f32) {
        let dt = dt.max(0.0);
        let sighting = stimuli.iter().find_map(|s| match s {
            Stimulus::Sighted { target, position } => Some((*target, *position)),
            _ => None,
        });
        if let Some((target, position)) = sighting {
            self.enter_combat(blackboard, target, position);
            return;
        }

        blackboard.time_since_target_seen += dt;
        if self.alert_state == AlertState::Combat
            && blackboard.time_since_target_seen >= LOSE_TARGET_SECONDS
        {
            self.alert_state = AlertState::Suspicious;
            self.alarm_raised = false;
            blackboard.investigate_position = blackboard.last_known_target_pos;
        }

        let gunfire = stimuli.iter().find_map(|s| match s {
            Stimulus::Gunfire { position } => Some(*position),
            _ => None,
        });
        let footsteps = stimuli.iter().find_map(|s| match s {
            Stimulus::Footsteps { position } => Some(*position),
            _ => None,
        });

        if self.alert_state == AlertState::Combat {
            return;
        }
        if let Some(position) = gunfire {
            self.suspicion_meter = 100.0;
            self.alert_state = AlertState::Alerted;
            blackboard.investigate_position = Some(position);
        } else if let Some(position) = footsteps {
            self.suspicion_meter =
                (self.suspicion_meter + FOOTSTEP_SUSPICION_PER_SECOND * dt).min(100.0);
            blackboard.investigate_position = Some(position);
            self.alert_state = if self.suspicion_meter >= 100.0 {
                AlertState::Alerted
            } else {
                self.alert_state.max(AlertState::Suspicious)
            };
        } else {
            self.suspicion_meter =
                (self.suspicion_meter - SUSPICION_DECAY_PER_SECOND * dt).max(0.0);
            if self.alert_state == AlertState::Alerted && self.suspicion_meter < 50.0 {
                self.alert_state = AlertState::Suspicious;
            }
            if self.alert_state == AlertState::Suspicious
                && self.suspicion_meter <= 0.0
                && blackboard.investigate_position.is_none()
            {
                self.alert_state = AlertState::Unaware;
            }
        }
    }

    /// Engages `target` at `position` (seen, or reported by an ally / hit).
    pub fn enter_combat(&mut self, blackboard: &mut AiBlackboard, target: Entity, position: Vec3) {
        self.alert_state = AlertState::Combat;
        self.suspicion_meter = 100.0;
        blackboard.target_entity = Some(target);
        blackboard.last_known_target_pos = Some(position);
        blackboard.time_since_target_seen = 0.0;
        blackboard.investigate_position = None;
    }
}

/// Whether `target` is inside a horizontal vision cone of full angle
/// `angle_degrees` around `forward`, within `range` of `eye`.
pub fn in_vision_cone(
    eye: Vec3,
    forward: Vec3,
    target: Vec3,
    angle_degrees: f32,
    range: f32,
) -> bool {
    let offset = target - eye;
    if offset.length() > range {
        return false;
    }
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let to_target = Vec3::new(offset.x, 0.0, offset.z).normalize_or_zero();
    if forward == Vec3::ZERO || to_target == Vec3::ZERO {
        return to_target == Vec3::ZERO;
    }
    forward.dot(to_target) >= (angle_degrees.to_radians() * 0.5).cos()
}

/// Planar facing of an agent: its view if it has one, else its transform.
pub fn facing(transform: &Transform, view: Option<&ViewAngles>) -> Vec3 {
    view.map_or(transform.rotation * Vec3::NEG_Z, ViewAngles::forward)
}

/// Whether a solid ray from `eye` to `target` reaches `target_entity`
/// (or nothing) before hitting anything else.
pub fn line_of_sight(
    spatial: &SpatialQuery,
    viewer: Entity,
    eye: Vec3,
    target: Vec3,
    target_entity: Entity,
) -> bool {
    let offset = target - eye;
    let distance = offset.length();
    let Ok(direction) = Dir3::new(offset) else {
        return true;
    };
    let filter = SpatialQueryFilter::default().with_excluded_entities([viewer]);
    match spatial.cast_ray(eye, direction, distance, true, &filter) {
        Some(hit) => hit.entity == target_entity || hit.distance >= distance - 0.05,
        None => true,
    }
}

type PlayerQuery<'a> = (Entity, &'a Transform, Option<&'a KinematicVelocity>);

/// Senses players and updates every enemy's alert state.
pub fn update_ai_perception(
    time: Res<Time<Fixed>>,
    spatial: SpatialQuery,
    mut shots: EventReader<ShotFiredEvent>,
    players: Query<PlayerQuery, With<PlayerCharacterMarker>>,
    mut enemies: Query<
        (
            Entity,
            &Transform,
            Option<&ViewAngles>,
            &mut AiPerception,
            &mut AiBlackboard,
        ),
        Without<PlayerCharacterMarker>,
    >,
) {
    let dt = time.delta_secs();
    let player_shots: Vec<Vec3> = shots
        .read()
        .filter(|shot| players.contains(shot.shooter))
        .map(|shot| shot.origin)
        .collect();

    for (entity, transform, view, mut perception, mut blackboard) in &mut enemies {
        let eye = transform.translation + Vec3::Y * EYE_OFFSET;
        let forward = facing(transform, view);
        let range = perception.effective_vision_range();
        let mut stimuli = Vec::new();

        let mut nearest_seen: Option<(f32, Entity, Vec3)> = None;
        for (player, player_transform, velocity) in &players {
            let position = player_transform.translation;
            let aim = position + Vec3::Y * SIGHT_TARGET_OFFSET;
            let distance = eye.distance(aim);
            if in_vision_cone(eye, forward, aim, perception.vision_angle_degrees, range)
                && line_of_sight(&spatial, entity, eye, aim, player)
                && nearest_seen.map_or(true, |(d, ..)| distance < d)
            {
                nearest_seen = Some((distance, player, position));
            }
            let loud = velocity.is_some_and(|v| v.planar_speed() > AUDIBLE_FOOTSTEP_SPEED);
            if loud && transform.translation.distance(position) <= perception.hearing_range_meters {
                stimuli.push(Stimulus::Footsteps { position });
            }
        }
        if let Some((_, target, position)) = nearest_seen {
            stimuli.push(Stimulus::Sighted { target, position });
        }
        for &origin in &player_shots {
            if transform.translation.distance(origin) <= perception.gunfire_hearing_range_meters {
                stimuli.push(Stimulus::Gunfire { position: origin });
            }
        }
        perception.observe(&mut blackboard, &stimuli, dt);
    }
}

/// Enemies that just entered combat alert allies in range, sharing the
/// target. Allies already in combat keep their own target.
pub fn propagate_alarms(
    mut alarms: EventWriter<AlarmRaisedEvent>,
    mut enemies: Query<(Entity, &Transform, &mut AiPerception, &mut AiBlackboard), With<Enemy>>,
) {
    let mut raised = Vec::new();
    for (entity, transform, mut perception, blackboard) in &mut enemies {
        if perception.alert_state == AlertState::Combat && !perception.alarm_raised {
            perception.alarm_raised = true;
            if let (Some(target), Some(position)) =
                (blackboard.target_entity, blackboard.last_known_target_pos)
            {
                raised.push((entity, transform.translation, target, position));
                alarms.send(AlarmRaisedEvent {
                    source: entity,
                    target_position: position,
                });
            }
        }
    }
    for (source, origin, target, position) in raised {
        for (entity, transform, mut perception, mut blackboard) in &mut enemies {
            if entity == source
                || perception.alert_state == AlertState::Combat
                || transform.translation.distance(origin) > ALARM_RADIUS_METERS
            {
                continue;
            }
            perception.enter_combat(&mut blackboard, target, position);
            // Heard about it, not seen: counts as unseen until sighted.
            perception.alarm_raised = true;
            blackboard.time_since_target_seen = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Entity {
        Entity::from_raw(9)
    }

    #[test]
    fn vision_cone_angle_and_range() {
        let eye = Vec3::ZERO;
        let f = Vec3::NEG_Z;
        assert!(in_vision_cone(
            eye,
            f,
            Vec3::new(0.0, 0.0, -10.0),
            90.0,
            25.0
        ));
        assert!(
            in_vision_cone(eye, f, Vec3::new(9.0, 0.0, -10.0), 90.0, 25.0),
            "42 deg off, inside 45"
        );
        assert!(
            !in_vision_cone(eye, f, Vec3::new(11.0, 0.0, -10.0), 90.0, 25.0),
            "48 deg off"
        );
        assert!(
            !in_vision_cone(eye, f, Vec3::new(0.0, 0.0, 10.0), 90.0, 25.0),
            "behind"
        );
        assert!(
            !in_vision_cone(eye, f, Vec3::new(0.0, 0.0, -30.0), 90.0, 25.0),
            "too far"
        );
        assert!(
            in_vision_cone(eye, f, Vec3::new(0.0, 8.0, -10.0), 90.0, 25.0),
            "cone is horizontal"
        );
    }

    #[test]
    fn sighting_enters_combat_and_records_the_target() {
        let mut p = AiPerception::default();
        let mut bb = AiBlackboard::default();
        let at = Vec3::new(1.0, 0.0, -5.0);
        p.observe(
            &mut bb,
            &[Stimulus::Sighted {
                target: target(),
                position: at,
            }],
            0.1,
        );
        assert_eq!(p.alert_state, AlertState::Combat);
        assert_eq!(bb.target_entity, Some(target()));
        assert_eq!(bb.last_known_target_pos, Some(at));
        assert_eq!(p.effective_vision_range(), 40.0, "alerted range");
    }

    #[test]
    fn losing_sight_drops_to_suspicious_and_investigates() {
        let mut p = AiPerception::default();
        let mut bb = AiBlackboard::default();
        let at = Vec3::new(1.0, 0.0, -5.0);
        p.observe(
            &mut bb,
            &[Stimulus::Sighted {
                target: target(),
                position: at,
            }],
            0.1,
        );
        for _ in 0..19 {
            p.observe(&mut bb, &[], 0.1);
        }
        assert_eq!(p.alert_state, AlertState::Combat, "short loss keeps combat");
        p.observe(&mut bb, &[], 0.11);
        assert_eq!(p.alert_state, AlertState::Suspicious);
        assert_eq!(bb.investigate_position, Some(at));
    }

    #[test]
    fn gunfire_alerts_and_footsteps_build_suspicion() {
        let mut p = AiPerception::default();
        let mut bb = AiBlackboard::default();
        let shot = Vec3::new(20.0, 0.0, 0.0);
        p.observe(&mut bb, &[Stimulus::Gunfire { position: shot }], 0.1);
        assert_eq!(p.alert_state, AlertState::Alerted);
        assert_eq!(bb.investigate_position, Some(shot));

        let mut q = AiPerception::default();
        let mut bb = AiBlackboard::default();
        let steps = Stimulus::Footsteps { position: Vec3::X };
        q.observe(&mut bb, &[steps], 1.0);
        assert_eq!(q.alert_state, AlertState::Suspicious);
        assert!((q.suspicion_meter - 40.0).abs() < 1e-4);
        q.observe(&mut bb, &[steps], 2.0);
        assert_eq!(q.alert_state, AlertState::Alerted, "suspicion reached 100");
    }

    #[test]
    fn suspicion_fades_back_to_unaware_after_investigating() {
        let mut p = AiPerception::default();
        let mut bb = AiBlackboard::default();
        p.observe(&mut bb, &[Stimulus::Footsteps { position: Vec3::X }], 1.0);
        bb.investigate_position = None; // reached the spot
        for _ in 0..50 {
            p.observe(&mut bb, &[], 0.1);
        }
        assert_eq!(p.alert_state, AlertState::Unaware);
        assert_eq!(p.suspicion_meter, 0.0);
    }

    #[test]
    fn noise_does_not_override_combat() {
        let mut p = AiPerception::default();
        let mut bb = AiBlackboard::default();
        p.observe(
            &mut bb,
            &[Stimulus::Sighted {
                target: target(),
                position: Vec3::ZERO,
            }],
            0.1,
        );
        p.observe(&mut bb, &[Stimulus::Gunfire { position: Vec3::X }], 0.1);
        assert_eq!(p.alert_state, AlertState::Combat);
        assert_eq!(bb.investigate_position, None);
    }
}
