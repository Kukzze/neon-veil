//! Behavior tree leaves: conditions and actions for enemies.
//!
//! Leaves read an [`AiContext`] snapshot and write an [`AiIntent`]; they
//! never touch the world directly. Intents are applied afterwards through
//! the same components a player drives (`ParkourInput`, `MeleeInput`,
//! `RangedInput`, `ViewAngles`), so enemies move and fight with the exact
//! player primitives.

use crate::behavior_tree::BehaviorStatus;
use crate::perception::{AiBlackboard, AlertState};
use bevy::prelude::*;
use game_combat::{AttackFrameData, StatusEffectType};
use game_parkour::ParkourState;

/// Arrival tolerance for move-to actions, m.
pub const ARRIVAL_RADIUS: f32 = 1.2;
/// Ranged attacks only fire once the view is within this of the target.
pub const AIM_TOLERANCE_DEGREES: f32 = 8.0;
/// Chases sprint when farther than this from the goal, m.
pub const SPRINT_DISTANCE: f32 = 6.0;
pub const COVER_SEARCH_RADIUS: f32 = 6.0;
pub const COVER_SAMPLES: usize = 12;
/// Flanking runs to a point this far to the side of the target, m.
pub const FLANK_OFFSET: f32 = 6.0;
/// Within this distance of the target the flank is over and the chase
/// takes over, m.
pub const FLANK_FINISH_DISTANCE: f32 = 5.0;
pub const WALL_RUN_ENTRY_SPEED: f32 = 8.5;
pub const GUARD_DISTANCE: f32 = 4.0;
pub const GRENADE_RANGE: f32 = 18.0;

/// Conditions and actions available to enemy trees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AiAction {
    // Conditions
    InCombat,
    IsInvestigating,
    HealthBelow(f32),
    TargetWithin(f32),
    TargetAttacking,
    // Movement
    Patrol,
    Investigate,
    Chase {
        stop_distance: f32,
    },
    /// Backs off or closes in to stay within `[min, max]` of the target;
    /// fails once in the band so the next branch can act.
    Reposition {
        min: f32,
        max: f32,
    },
    TakeCover,
    WallRunFlank,
    ChargeTarget,
    // Combat
    MeleeAttack {
        reach: f32,
    },
    RangedAttack {
        max_range: f32,
    },
    Guard,
    ThrowGrenade {
        status: StatusEffectType,
    },
    Hack {
        status: StatusEffectType,
        range: f32,
    },
    SpecialAttack {
        attack: AttackFrameData,
        reach: f32,
    },
    MissileSalvo {
        count: u32,
    },
}

/// What the agent knows about its target this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetInfo {
    pub entity: Entity,
    pub position: Vec3,
    /// The target is winding up or swinging a melee attack.
    pub attacking: bool,
    pub visible: bool,
}

/// Wall and ground readings relevant to movement decisions.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MovementSense {
    pub grounded: bool,
    pub wall_left: bool,
    pub wall_right: bool,
    /// Something blocks the way ahead that cannot be vaulted.
    pub blocked_ahead: bool,
    pub planar_speed: f32,
    pub parkour_state: Option<ParkourState>,
}

/// Read-only snapshot for leaf evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiContext {
    pub position: Vec3,
    pub forward: Vec3,
    pub health_fraction: f32,
    pub alert_state: AlertState,
    pub target: Option<TargetInfo>,
    pub movement: MovementSense,
}

/// A thrown grenade or hack aimed at a target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetedEffect {
    pub target: Entity,
    pub target_position: Vec3,
    pub status: StatusEffectType,
}

/// What the agent wants to do this tick.
#[derive(Component, Reflect, Debug, Default, Clone, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct AiIntent {
    /// World-space XZ movement direction, length <= 1.
    pub move_direction: Vec2,
    pub sprint: bool,
    pub jump: bool,
    pub crouch: bool,
    pub look_at: Option<Vec3>,
    pub trigger_held: bool,
    pub melee_light: bool,
    pub melee_heavy: bool,
    pub guard: bool,
    #[reflect(ignore)]
    pub special_attack: Option<AttackFrameData>,
    #[reflect(ignore)]
    pub grenade: Option<TargetedEffect>,
    #[reflect(ignore)]
    pub hack: Option<TargetedEffect>,
    pub missiles: u32,
    #[reflect(ignore)]
    pub missile_target: Option<Vec3>,
    /// Kamikaze run (drones).
    pub charge: bool,
}

/// Line-of-sight queries for cover search.
pub trait AiWorld {
    /// Whether nothing blocks the straight line from `from` to `to`.
    fn clear_line(&self, from: Vec3, to: Vec3) -> bool;
}

fn planar(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Steers toward `goal`. Returns whether the agent has arrived.
fn move_toward(ctx: &AiContext, intent: &mut AiIntent, goal: Vec3, arrive: f32) -> bool {
    let offset = planar(goal - ctx.position);
    let distance = offset.length();
    if distance <= arrive {
        intent.move_direction = Vec2::ZERO;
        return true;
    }
    let dir = offset / distance;
    intent.move_direction = Vec2::new(dir.x, dir.z);
    intent.sprint = distance > SPRINT_DISTANCE;
    if ctx.movement.blocked_ahead && ctx.movement.grounded {
        intent.jump = true;
    }
    false
}

fn target_distance(ctx: &AiContext) -> Option<f32> {
    ctx.target
        .map(|t| planar(t.position - ctx.position).length())
}

fn aim_error_degrees(ctx: &AiContext, at: Vec3) -> f32 {
    let to = planar(at - ctx.position).normalize_or_zero();
    let forward = planar(ctx.forward).normalize_or_zero();
    if to == Vec3::ZERO || forward == Vec3::ZERO {
        return 0.0;
    }
    forward.angle_between(to).to_degrees()
}

fn status(ok: bool) -> BehaviorStatus {
    if ok {
        BehaviorStatus::Success
    } else {
        BehaviorStatus::Failure
    }
}

/// Nearest spot within [`COVER_SEARCH_RADIUS`] that `threat_eye` cannot see
/// and that the agent can reach in a straight line.
pub fn find_cover(world: &impl AiWorld, position: Vec3, threat_eye: Vec3) -> Option<Vec3> {
    (0..COVER_SAMPLES)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / COVER_SAMPLES as f32;
            position + Vec3::new(angle.cos(), 0.0, angle.sin()) * COVER_SEARCH_RADIUS
        })
        .filter(|spot| {
            let head = *spot + Vec3::Y * 0.6;
            !world.clear_line(threat_eye, head) && world.clear_line(position, *spot)
        })
        .min_by(|a, b| {
            a.distance_squared(position)
                .total_cmp(&b.distance_squared(position))
        })
}

/// Evaluates one leaf.
pub fn run_action(
    action: &AiAction,
    ctx: &AiContext,
    blackboard: &mut AiBlackboard,
    intent: &mut AiIntent,
    world: &impl AiWorld,
) -> BehaviorStatus {
    use BehaviorStatus::*;
    match *action {
        AiAction::InCombat => status(ctx.alert_state == AlertState::Combat && ctx.target.is_some()),
        AiAction::IsInvestigating => status(blackboard.investigate_position.is_some()),
        AiAction::HealthBelow(fraction) => status(ctx.health_fraction < fraction),
        AiAction::TargetWithin(range) => status(target_distance(ctx).is_some_and(|d| d <= range)),
        AiAction::TargetAttacking => status(ctx.target.is_some_and(|t| {
            t.attacking && target_distance(ctx).is_some_and(|d| d <= GUARD_DISTANCE)
        })),

        AiAction::Patrol => {
            let count = blackboard.patrol_waypoints.len();
            if count == 0 {
                return Failure;
            }
            let index = blackboard.current_waypoint_index % count;
            let waypoint = blackboard.patrol_waypoints[index];
            if move_toward(ctx, intent, waypoint, ARRIVAL_RADIUS) {
                blackboard.current_waypoint_index = (index + 1) % count;
                Success
            } else {
                intent.sprint = false;
                Running
            }
        }
        AiAction::Investigate => {
            let Some(spot) = blackboard.investigate_position else {
                return Failure;
            };
            intent.look_at = Some(spot);
            if move_toward(ctx, intent, spot, ARRIVAL_RADIUS) {
                blackboard.investigate_position = None;
                Success
            } else {
                Running
            }
        }
        AiAction::Chase { stop_distance } => {
            let Some(goal) = ctx
                .target
                .map(|t| t.position)
                .or(blackboard.last_known_target_pos)
            else {
                return Failure;
            };
            intent.look_at = Some(goal);
            if move_toward(ctx, intent, goal, stop_distance) {
                Success
            } else {
                Running
            }
        }
        AiAction::Reposition { min, max } => {
            let (Some(target), Some(distance)) = (ctx.target, target_distance(ctx)) else {
                return Failure;
            };
            intent.look_at = Some(target.position);
            let away = planar(ctx.position - target.position).normalize_or_zero();
            if distance < min {
                intent.move_direction = Vec2::new(away.x, away.z);
                Running
            } else if distance > max {
                intent.move_direction = Vec2::new(-away.x, -away.z);
                Running
            } else {
                Failure
            }
        }
        AiAction::TakeCover => {
            let Some(target) = ctx.target else {
                return Failure;
            };
            let threat_eye = target.position + Vec3::Y * crate::perception::EYE_OFFSET;
            let still_valid = blackboard
                .cover_position
                .is_some_and(|spot| !world.clear_line(threat_eye, spot + Vec3::Y * 0.6));
            if !still_valid {
                blackboard.cover_position = find_cover(world, ctx.position, threat_eye);
            }
            let Some(spot) = blackboard.cover_position else {
                blackboard.is_in_cover = false;
                return Failure;
            };
            intent.look_at = Some(target.position);
            if move_toward(ctx, intent, spot, 0.6) {
                blackboard.is_in_cover = true;
                intent.crouch = true;
                Success
            } else {
                blackboard.is_in_cover = false;
                Running
            }
        }
        AiAction::WallRunFlank => {
            let (Some(target), Some(distance)) = (ctx.target, target_distance(ctx)) else {
                return Failure;
            };
            if distance <= FLANK_FINISH_DISTANCE {
                return Failure;
            }
            let to_target = planar(target.position - ctx.position).normalize_or_zero();
            // Swing out to whichever side has a wall to run along (right by
            // default), aiming past the target's flank.
            let right = to_target.cross(Vec3::Y);
            let side = if ctx.movement.wall_left && !ctx.movement.wall_right {
                -right
            } else {
                right
            };
            let flank_point = target.position + side * FLANK_OFFSET * 0.5;
            move_toward(ctx, intent, flank_point, ARRIVAL_RADIUS);
            intent.sprint = true;
            intent.look_at = Some(target.position);
            let fast = ctx.movement.planar_speed >= WALL_RUN_ENTRY_SPEED;
            let wall = ctx.movement.wall_left || ctx.movement.wall_right;
            if ctx.movement.grounded && fast && (wall || ctx.movement.blocked_ahead) {
                // Jumping beside a wall at speed starts a wall run.
                intent.jump = true;
            }
            Running
        }
        AiAction::ChargeTarget => {
            let Some(target) = ctx.target else {
                return Failure;
            };
            move_toward(ctx, intent, target.position, 0.0);
            intent.sprint = true;
            intent.charge = true;
            intent.look_at = Some(target.position);
            Running
        }

        AiAction::MeleeAttack { reach } => {
            let Some(target) = ctx
                .target
                .filter(|_| target_distance(ctx).is_some_and(|d| d <= reach))
            else {
                return Failure;
            };
            intent.look_at = Some(target.position);
            intent.melee_light = true;
            Success
        }
        AiAction::RangedAttack { max_range } => {
            let Some(target) = ctx.target.filter(|t| t.visible) else {
                return Failure;
            };
            if target_distance(ctx).is_some_and(|d| d > max_range) {
                return Failure;
            }
            intent.look_at = Some(target.position);
            intent.trigger_held = aim_error_degrees(ctx, target.position) <= AIM_TOLERANCE_DEGREES;
            Success
        }
        AiAction::Guard => {
            if !ctx.target.is_some_and(|t| t.attacking) {
                return Failure;
            }
            intent.guard = true;
            intent.look_at = ctx.target.map(|t| t.position);
            Success
        }
        AiAction::ThrowGrenade { status: effect } => {
            let Some(target) = ctx
                .target
                .filter(|t| t.visible && target_distance(ctx).is_some_and(|d| d <= GRENADE_RANGE))
            else {
                return Failure;
            };
            intent.grenade = Some(TargetedEffect {
                target: target.entity,
                target_position: target.position,
                status: effect,
            });
            Success
        }
        AiAction::Hack {
            status: effect,
            range,
        } => {
            let Some(target) = ctx
                .target
                .filter(|t| t.visible && target_distance(ctx).is_some_and(|d| d <= range))
            else {
                return Failure;
            };
            intent.hack = Some(TargetedEffect {
                target: target.entity,
                target_position: target.position,
                status: effect,
            });
            Success
        }
        AiAction::SpecialAttack { attack, reach } => {
            let Some(target) = ctx
                .target
                .filter(|_| target_distance(ctx).is_some_and(|d| d <= reach))
            else {
                return Failure;
            };
            intent.look_at = Some(target.position);
            intent.special_attack = Some(attack);
            Success
        }
        AiAction::MissileSalvo { count } => {
            let Some(target) = ctx.target.filter(|t| t.visible) else {
                return Failure;
            };
            intent.missiles = count;
            intent.missile_target = Some(target.position);
            Success
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OpenWorld;
    impl AiWorld for OpenWorld {
        fn clear_line(&self, _: Vec3, _: Vec3) -> bool {
            true
        }
    }

    /// One wall: the plane `x = wall_x`, blocking any segment crossing it.
    struct WallAtX(f32);
    impl AiWorld for WallAtX {
        fn clear_line(&self, from: Vec3, to: Vec3) -> bool {
            (from.x - self.0).signum() == (to.x - self.0).signum()
        }
    }

    fn ctx() -> AiContext {
        AiContext {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            health_fraction: 1.0,
            alert_state: AlertState::Combat,
            target: Some(TargetInfo {
                entity: Entity::from_raw(5),
                position: Vec3::new(0.0, 0.0, -10.0),
                attacking: false,
                visible: true,
            }),
            movement: MovementSense {
                grounded: true,
                ..Default::default()
            },
        }
    }

    fn act(action: AiAction, ctx: &AiContext, bb: &mut AiBlackboard) -> (BehaviorStatus, AiIntent) {
        let mut intent = AiIntent::default();
        let status = run_action(&action, ctx, bb, &mut intent, &OpenWorld);
        (status, intent)
    }

    #[test]
    fn conditions() {
        let mut bb = AiBlackboard::default();
        let c = ctx();
        assert_eq!(
            act(AiAction::InCombat, &c, &mut bb).0,
            BehaviorStatus::Success
        );
        assert_eq!(
            act(AiAction::TargetWithin(12.0), &c, &mut bb).0,
            BehaviorStatus::Success
        );
        assert_eq!(
            act(AiAction::TargetWithin(8.0), &c, &mut bb).0,
            BehaviorStatus::Failure
        );
        assert_eq!(
            act(AiAction::HealthBelow(0.5), &c, &mut bb).0,
            BehaviorStatus::Failure
        );
        let hurt = AiContext {
            health_fraction: 0.3,
            ..c
        };
        assert_eq!(
            act(AiAction::HealthBelow(0.5), &hurt, &mut bb).0,
            BehaviorStatus::Success
        );
        let calm = AiContext {
            alert_state: AlertState::Suspicious,
            ..c
        };
        assert_eq!(
            act(AiAction::InCombat, &calm, &mut bb).0,
            BehaviorStatus::Failure
        );
    }

    #[test]
    fn patrol_walks_waypoints_in_order() {
        let mut bb = AiBlackboard {
            patrol_waypoints: vec![Vec3::new(5.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 5.0)],
            ..Default::default()
        };
        let (status, intent) = act(AiAction::Patrol, &ctx(), &mut bb);
        assert_eq!(status, BehaviorStatus::Running);
        assert!(intent.move_direction.abs_diff_eq(Vec2::X, 1e-5) && !intent.sprint);

        let arrived = AiContext {
            position: Vec3::new(4.5, 0.0, 0.0),
            ..ctx()
        };
        assert_eq!(
            act(AiAction::Patrol, &arrived, &mut bb).0,
            BehaviorStatus::Success
        );
        assert_eq!(bb.current_waypoint_index, 1);
        assert_eq!(
            act(AiAction::Patrol, &ctx(), &mut AiBlackboard::default()).0,
            BehaviorStatus::Failure
        );
    }

    #[test]
    fn chase_falls_back_to_last_known_position() {
        let mut bb = AiBlackboard {
            last_known_target_pos: Some(Vec3::new(-8.0, 0.0, 0.0)),
            ..Default::default()
        };
        let blind = AiContext {
            target: None,
            ..ctx()
        };
        let (status, intent) = act(AiAction::Chase { stop_distance: 1.0 }, &blind, &mut bb);
        assert_eq!(status, BehaviorStatus::Running);
        assert!(intent.move_direction.abs_diff_eq(Vec2::NEG_X, 1e-5));
        assert!(intent.sprint, "far away: sprint");
    }

    #[test]
    fn ranged_attack_waits_for_aim_and_sight() {
        let mut bb = AiBlackboard::default();
        let (status, intent) = act(AiAction::RangedAttack { max_range: 30.0 }, &ctx(), &mut bb);
        assert_eq!(status, BehaviorStatus::Success);
        assert!(intent.trigger_held, "already facing the target");

        let turned = AiContext {
            forward: Vec3::X,
            ..ctx()
        };
        let (_, intent) = act(AiAction::RangedAttack { max_range: 30.0 }, &turned, &mut bb);
        assert!(
            !intent.trigger_held && intent.look_at.is_some(),
            "turn before firing"
        );

        let mut hidden = ctx();
        if let Some(t) = hidden.target.as_mut() {
            t.visible = false;
        }
        assert_eq!(
            act(AiAction::RangedAttack { max_range: 30.0 }, &hidden, &mut bb).0,
            BehaviorStatus::Failure
        );
        assert_eq!(
            act(AiAction::RangedAttack { max_range: 5.0 }, &ctx(), &mut bb).0,
            BehaviorStatus::Failure
        );
    }

    #[test]
    fn melee_and_guard() {
        let mut bb = AiBlackboard::default();
        assert_eq!(
            act(AiAction::MeleeAttack { reach: 2.0 }, &ctx(), &mut bb).0,
            BehaviorStatus::Failure
        );
        let mut close = ctx();
        if let Some(t) = close.target.as_mut() {
            t.position = Vec3::new(0.0, 0.0, -1.5);
            t.attacking = true;
        }
        let (status, intent) = act(AiAction::MeleeAttack { reach: 2.0 }, &close, &mut bb);
        assert_eq!(status, BehaviorStatus::Success);
        assert!(intent.melee_light);
        assert_eq!(
            act(AiAction::TargetAttacking, &close, &mut bb).0,
            BehaviorStatus::Success
        );
        assert!(act(AiAction::Guard, &close, &mut bb).1.guard);
        assert_eq!(
            act(AiAction::Guard, &ctx(), &mut bb).0,
            BehaviorStatus::Failure
        );
    }

    #[test]
    fn reposition_keeps_a_distance_band() {
        let mut bb = AiBlackboard::default();
        let action = AiAction::Reposition {
            min: 15.0,
            max: 25.0,
        };
        let (status, intent) = act(action, &ctx(), &mut bb);
        assert_eq!(status, BehaviorStatus::Running, "too close at 10 m");
        assert!(intent.move_direction.y > 0.0, "backs away (+Z)");
        let mut far = ctx();
        if let Some(t) = far.target.as_mut() {
            t.position = Vec3::new(0.0, 0.0, -20.0);
        }
        assert_eq!(
            act(action, &far, &mut bb).0,
            BehaviorStatus::Failure,
            "in band: let others act"
        );
    }

    #[test]
    fn cover_is_a_spot_the_threat_cannot_see() {
        // Threat at x = -10, wall at x = 3: only spots past the wall hide.
        let world = WallAtX(3.0);
        let threat_eye = Vec3::new(-10.0, 1.7, 0.0);
        let spot = find_cover(&world, Vec3::ZERO, threat_eye);
        assert!(
            spot.is_none(),
            "every hidden spot is behind the wall the agent cannot cross"
        );

        let agent = Vec3::new(4.0, 0.0, 0.0);
        let spot = find_cover(&world, agent, threat_eye).expect("cover");
        assert!(spot.x > 3.0, "on the far side of the wall: {spot:?}");
        assert!(
            find_cover(&OpenWorld, Vec3::ZERO, threat_eye).is_none(),
            "open ground has no cover"
        );
    }

    #[test]
    fn wall_run_flank_jumps_beside_walls_at_speed() {
        let mut bb = AiBlackboard::default();
        let mut running = ctx();
        running.movement = MovementSense {
            grounded: true,
            wall_right: true,
            planar_speed: 12.8,
            ..Default::default()
        };
        let (status, intent) = act(AiAction::WallRunFlank, &running, &mut bb);
        assert_eq!(status, BehaviorStatus::Running);
        assert!(intent.jump && intent.sprint, "jump onto the wall");

        let slow = AiContext {
            movement: MovementSense {
                planar_speed: 4.0,
                ..running.movement
            },
            ..running
        };
        assert!(
            !act(AiAction::WallRunFlank, &slow, &mut bb).1.jump,
            "too slow to wall-run"
        );

        let mut close = running;
        if let Some(t) = close.target.as_mut() {
            t.position = Vec3::new(0.0, 0.0, -3.0);
        }
        assert_eq!(
            act(AiAction::WallRunFlank, &close, &mut bb).0,
            BehaviorStatus::Failure,
            "hand over to the chase"
        );
    }

    #[test]
    fn grenades_and_hacks_need_sight_and_range() {
        let mut bb = AiBlackboard::default();
        let (status, intent) = act(
            AiAction::ThrowGrenade {
                status: StatusEffectType::ShockEmp,
            },
            &ctx(),
            &mut bb,
        );
        assert_eq!(status, BehaviorStatus::Success);
        assert_eq!(
            intent.grenade.map(|g| g.status),
            Some(StatusEffectType::ShockEmp)
        );
        let hack = AiAction::Hack {
            status: StatusEffectType::CryoGlitch,
            range: 5.0,
        };
        assert_eq!(
            act(hack, &ctx(), &mut bb).0,
            BehaviorStatus::Failure,
            "out of hack range"
        );
    }
}
