//! Deterministic melee frame data, attack state machine, parry and posture.
//!
//! Frame data is authored at 60 animation frames per second, while the
//! simulation runs at the 64 Hz fixed tick. The animation frame is derived
//! from whole ticks with integer math (`frame = ticks * 60 / 64`), so timing
//! is exact and identical on every machine, and no frame is ever skipped.

use crate::combo_trees::{select_attack, AttackRequest};
use bevy::prelude::*;
use bevy::utils::HashSet;
use game_core::math::sanitize_f32;
use game_core::time::{NetworkTick, NETWORK_TICK_RATE_HZ};
use game_parkour::ParkourState;
use serde::{Deserialize, Serialize};

pub const ANIMATION_FRAMES_PER_SECOND: u32 = 60;
pub const PERFECT_PARRY_WINDOW_SECONDS: f32 = 0.120;
/// Posture damage dealt back to an attacker whose hit is perfectly parried.
pub const PERFECT_PARRY_POSTURE_DAMAGE: f32 = 40.0;
/// Fraction of damage that gets through a (non-perfect) block.
pub const BLOCK_DAMAGE_FRACTION: f32 = 0.30;
/// Fraction of posture damage a block still takes.
pub const BLOCK_POSTURE_FRACTION: f32 = 0.5;
pub const DEFAULT_POSTURE_MAX: f32 = 100.0;
/// Stun after posture breaks, s. Posture resets to zero afterwards.
pub const POSTURE_BREAK_DURATION_SECONDS: f32 = 2.0;
/// Posture recovered per second once undamaged for the regen delay.
pub const POSTURE_REGEN_PER_SECOND: f32 = 12.0;
pub const POSTURE_REGEN_DELAY_SECONDS: f32 = 1.5;
/// A pressed attack waits this long for a valid window before it is dropped.
pub const ATTACK_INPUT_BUFFER_SECONDS: f32 = 0.2;

/// Animation frame reached after `ticks` fixed simulation ticks.
#[inline]
pub const fn frame_for_tick(ticks: u32) -> u32 {
    ((ticks as u64 * ANIMATION_FRAMES_PER_SECOND as u64) / NETWORK_TICK_RATE_HZ as u64) as u32
}

/// Melee weapon families, each with its own combo tree.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Debug, Default, PartialEq, Hash)]
pub enum MeleeWeaponClass {
    #[default]
    CyberKatana,
    MantisDualBlades,
    ShockBaton,
    KineticHeavyGauntlet,
    MonofilamentWhip,
}

/// Phase of the current swing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
#[reflect(Debug, Default, PartialEq, Hash)]
pub enum AttackPhase {
    #[default]
    Idle,
    /// Wind-up; can be cancelled into a parry or dodge.
    Startup,
    /// Hitbox live.
    Active,
    /// Follow-through; the next combo hit may be chained.
    Recovery,
    /// Late recovery; also cancellable into parry, dodge or jump.
    CancelWindow,
}

/// How an incoming attack was received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum ParryResult {
    /// Parried within the perfect window: no damage, attacker takes
    /// posture damage and is staggered.
    PerfectParry,
    /// Blocked: 30 % damage, half posture damage.
    Blocked,
    /// Unguarded: full damage and posture damage.
    Hit,
}

/// Timing and payload of one attack.
///
/// Hitbox geometry is in the attacker's yaw space measured from its feet,
/// in Bevy convention (+X right, +Y up, **-Z forward**). `hitbox_extents` is
/// the full box size.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Debug, PartialEq)]
pub struct AttackFrameData {
    pub name: &'static str,
    pub startup_frames: u32,
    pub active_frames: u32,
    pub recovery_frames: u32,
    /// Frame from which the swing can be cancelled (clamped into recovery).
    pub cancel_frame: u32,
    pub base_damage: f32,
    pub posture_damage: f32,
    pub knockback_impulse: f32,
    pub hitbox_extents: Vec3,
    pub hitbox_offset: Vec3,
    /// Knockback direction in attacker space (normalized on use).
    pub knockback_direction: Vec3,
    /// When positive, only targets within this horizontal radius of the
    /// hitbox center are hit, and knockback pushes radially outward.
    pub area_radius: f32,
    /// When positive, the attacker is driven downward at this speed when the
    /// attack starts.
    pub attacker_dive_speed: f32,
}

impl AttackFrameData {
    pub const fn total_frames(&self) -> u32 {
        self.startup_frames + self.active_frames + self.recovery_frames
    }

    pub const fn active_end_frame(&self) -> u32 {
        self.startup_frames + self.active_frames
    }

    /// First frame of the cancel window.
    pub fn cancel_start_frame(&self) -> u32 {
        self.cancel_frame
            .clamp(self.active_end_frame(), self.total_frames())
    }

    pub fn phase_at_frame(&self, frame: u32) -> AttackPhase {
        if frame >= self.total_frames() {
            AttackPhase::Idle
        } else if frame < self.startup_frames {
            AttackPhase::Startup
        } else if frame < self.active_end_frame() {
            AttackPhase::Active
        } else if frame >= self.cancel_start_frame() {
            AttackPhase::CancelWindow
        } else {
            AttackPhase::Recovery
        }
    }

    pub fn duration_seconds(&self) -> f32 {
        self.total_frames() as f32 / ANIMATION_FRAMES_PER_SECOND as f32
    }
}

/// Melee intent for one tick. Press flags are edges and are cleared once
/// the melee step reads them. Filled from `PlayerInputState` for players;
/// AI can write it directly.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct MeleeInput {
    pub light_pressed: bool,
    pub heavy_pressed: bool,
    pub parry_held: bool,
    /// Action bits of the last packet read, for edge detection.
    pub previous_actions: u32,
    #[reflect(ignore)]
    pub last_packet_tick: Option<NetworkTick>,
}

/// Per-fighter melee state.
#[derive(Component, Reflect, Debug, Clone, PartialEq)]
#[reflect(Component, Default, Debug)]
#[require(MeleeInput)]
pub struct MeleeCombatState {
    pub weapon_class: MeleeWeaponClass,
    /// Index of the current (or last) attack in the weapon's combo tree.
    pub current_combo_index: u8,
    pub current_attack: Option<AttackFrameData>,
    /// Animation frame of the current attack (60 fps reference).
    pub frame_counter: u32,
    pub attack_phase: AttackPhase,
    pub is_parrying: bool,
    /// Seconds since the parry started.
    pub parry_timer: f32,
    pub perfect_parry_window: f32,
    pub posture_current: f32,
    pub posture_max: f32,
    pub is_posture_broken: bool,
    /// Entities already hit by the current swing.
    pub hit_entities: HashSet<Entity>,
    /// Fixed ticks since the current attack started.
    pub attack_ticks: u32,
    /// Whether the current attack belongs to a running combo chain.
    pub in_combo: bool,
    pub queued_attack: Option<AttackRequest>,
    /// Remaining buffer time for `queued_attack`.
    pub queue_timer: f32,
    pub posture_break_timer: f32,
    pub time_since_posture_damage: f32,
}

impl Default for MeleeCombatState {
    fn default() -> Self {
        Self {
            weapon_class: MeleeWeaponClass::CyberKatana,
            current_combo_index: 0,
            current_attack: None,
            frame_counter: 0,
            attack_phase: AttackPhase::Idle,
            is_parrying: false,
            parry_timer: 0.0,
            perfect_parry_window: PERFECT_PARRY_WINDOW_SECONDS,
            posture_current: 0.0,
            posture_max: DEFAULT_POSTURE_MAX,
            is_posture_broken: false,
            hit_entities: HashSet::default(),
            attack_ticks: 0,
            in_combo: false,
            queued_attack: None,
            queue_timer: 0.0,
            posture_break_timer: 0.0,
            time_since_posture_damage: 0.0,
        }
    }
}

/// What happened during one melee step.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeleeStepOutcome {
    pub started: Option<AttackFrameData>,
    /// The hitbox is live this tick.
    pub hitbox_active: bool,
}

impl MeleeCombatState {
    pub fn with_weapon(weapon_class: MeleeWeaponClass) -> Self {
        Self {
            weapon_class,
            ..Default::default()
        }
    }

    /// Begins `attack` at frame 0, clearing the per-swing hit list.
    pub fn start_attack(&mut self, attack: AttackFrameData) {
        self.current_attack = Some(attack);
        self.frame_counter = 0;
        self.attack_ticks = 0;
        self.attack_phase = attack.phase_at_frame(0);
        self.hit_entities.clear();
        self.is_parrying = false;
        self.parry_timer = 0.0;
    }

    /// Drops the current attack and any combo in progress.
    pub fn cancel_attack(&mut self) {
        self.current_attack = None;
        self.frame_counter = 0;
        self.attack_ticks = 0;
        self.attack_phase = AttackPhase::Idle;
        self.current_combo_index = 0;
        self.in_combo = false;
        self.hit_entities.clear();
    }

    /// The next combo hit may start.
    pub fn can_chain_combo(&self) -> bool {
        matches!(
            self.attack_phase,
            AttackPhase::Recovery | AttackPhase::CancelWindow
        )
    }

    /// The swing may be interrupted by a parry or dodge.
    pub fn can_cancel(&self) -> bool {
        matches!(
            self.attack_phase,
            AttackPhase::Startup | AttackPhase::CancelWindow
        )
    }

    /// Raises the guard. Fails while posture is broken or mid-swing outside
    /// a cancellable phase. Returns whether the parry started.
    pub fn start_parry(&mut self) -> bool {
        if self.is_posture_broken {
            return false;
        }
        if self.current_attack.is_some() {
            if !self.can_cancel() {
                return false;
            }
            self.cancel_attack();
        }
        self.is_parrying = true;
        self.parry_timer = 0.0;
        true
    }

    pub fn end_parry(&mut self) {
        self.is_parrying = false;
        self.parry_timer = 0.0;
    }

    pub fn is_in_perfect_parry(&self) -> bool {
        self.is_parrying && self.parry_timer <= self.perfect_parry_window
    }

    /// Adds posture damage. Breaking posture stuns: the swing and guard drop
    /// and posture resets after [`POSTURE_BREAK_DURATION_SECONDS`]. Returns
    /// whether this call broke posture.
    pub fn add_posture(&mut self, amount: f32) -> bool {
        let amount = sanitize_f32(amount).max(0.0);
        if amount <= 0.0 || self.is_posture_broken {
            return false;
        }
        self.time_since_posture_damage = 0.0;
        self.posture_current = (self.posture_current + amount).min(self.posture_max);
        if self.posture_current >= self.posture_max {
            self.is_posture_broken = true;
            self.posture_break_timer = POSTURE_BREAK_DURATION_SECONDS;
            self.cancel_attack();
            self.end_parry();
            self.queued_attack = None;
            return true;
        }
        false
    }

    /// Resolves a hit against this fighter, assuming the guard faces the
    /// attacker. See [`Self::resolve_incoming_attack_from`].
    pub fn resolve_incoming_attack(
        &mut self,
        incoming_damage: f32,
        incoming_posture: f32,
        attacker_posture: &mut f32,
    ) -> (ParryResult, f32) {
        self.resolve_incoming_attack_from(incoming_damage, incoming_posture, attacker_posture, true)
    }

    /// Resolves a hit. A guard only counts when `guard_covers_attacker`
    /// (the attacker is in front). Perfect parries add
    /// [`PERFECT_PARRY_POSTURE_DAMAGE`] to `attacker_posture`. Returns the
    /// result and the damage that gets through.
    pub fn resolve_incoming_attack_from(
        &mut self,
        incoming_damage: f32,
        incoming_posture: f32,
        attacker_posture: &mut f32,
        guard_covers_attacker: bool,
    ) -> (ParryResult, f32) {
        let damage = sanitize_f32(incoming_damage).max(0.0);
        let posture = sanitize_f32(incoming_posture).max(0.0);
        if guard_covers_attacker && self.is_in_perfect_parry() {
            *attacker_posture += PERFECT_PARRY_POSTURE_DAMAGE;
            (ParryResult::PerfectParry, 0.0)
        } else if guard_covers_attacker && self.is_parrying {
            self.add_posture(posture * BLOCK_POSTURE_FRACTION);
            (ParryResult::Blocked, damage * BLOCK_DAMAGE_FRACTION)
        } else {
            self.add_posture(posture);
            (ParryResult::Hit, damage)
        }
    }

    fn update_timers(&mut self, dt: f32) {
        if self.is_parrying {
            self.parry_timer += dt;
        }
        if self.queued_attack.is_some() {
            self.queue_timer -= dt;
            if self.queue_timer <= 0.0 {
                self.queued_attack = None;
            }
        }
        if self.is_posture_broken {
            self.posture_break_timer -= dt;
            if self.posture_break_timer <= 0.0 {
                self.is_posture_broken = false;
                self.posture_current = 0.0;
                self.posture_break_timer = 0.0;
            }
        } else {
            self.time_since_posture_damage += dt;
            if self.time_since_posture_damage >= POSTURE_REGEN_DELAY_SECONDS && !self.is_parrying {
                self.posture_current =
                    (self.posture_current - POSTURE_REGEN_PER_SECOND * dt).max(0.0);
            }
        }
    }

    /// Advances the current attack by one fixed tick.
    fn advance_attack(&mut self) {
        let Some(attack) = self.current_attack else {
            return;
        };
        self.attack_ticks += 1;
        self.frame_counter = frame_for_tick(self.attack_ticks);
        self.attack_phase = attack.phase_at_frame(self.frame_counter);
        if self.attack_phase == AttackPhase::Idle {
            self.current_attack = None;
            self.current_combo_index = 0;
            self.in_combo = false;
            self.hit_entities.clear();
        }
    }
}

/// Whether a fighter in `state` may start an attack at all.
pub fn attacks_allowed_in(state: Option<ParkourState>) -> bool {
    !matches!(
        state,
        Some(
            ParkourState::LedgeMantle
                | ParkourState::LedgeGrab
                | ParkourState::VaultLow
                | ParkourState::VaultMid
                | ParkourState::VaultHigh
                | ParkourState::HardLanding
                | ParkourState::WallClimb
                | ParkourState::WallKick
                | ParkourState::ZiplineTraverse
                | ParkourState::GrapplePull
                | ParkourState::Ragdoll
        )
    )
}

/// Advances one fighter by one fixed tick: timers, guard, buffered attack
/// requests, combo chaining and frame progression.
pub fn step_melee(
    state: &mut MeleeCombatState,
    input: &mut MeleeInput,
    parkour: Option<ParkourState>,
    dt: f32,
) -> MeleeStepOutcome {
    let dt = sanitize_f32(dt).max(0.0);
    state.update_timers(dt);

    let light = std::mem::take(&mut input.light_pressed);
    let heavy = std::mem::take(&mut input.heavy_pressed);
    if state.is_posture_broken {
        return MeleeStepOutcome::default();
    }

    if input.parry_held {
        if !state.is_parrying {
            state.start_parry();
        }
    } else if state.is_parrying {
        state.end_parry();
    }

    if heavy || light {
        state.queued_attack = Some(if heavy {
            AttackRequest::Heavy
        } else {
            AttackRequest::Light
        });
        state.queue_timer = ATTACK_INPUT_BUFFER_SECONDS;
    }

    let mut started = None;
    let window_open = state.current_attack.is_none() || state.can_chain_combo();
    if let Some(request) = state.queued_attack {
        if window_open && !state.is_parrying && attacks_allowed_in(parkour) {
            if let Some(selection) = select_attack(state, request, parkour) {
                state.start_attack(selection.attack);
                state.current_combo_index = selection.combo_index;
                state.in_combo = selection.continues_combo;
                state.queued_attack = None;
                started = Some(selection.attack);
            }
        }
    }
    if started.is_none() {
        state.advance_attack();
    }

    MeleeStepOutcome {
        started,
        hitbox_active: state.attack_phase == AttackPhase::Active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combo_trees::CYBER_KATANA_COMBO_TREE;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    fn light_1() -> AttackFrameData {
        CYBER_KATANA_COMBO_TREE[0]
    }

    #[test]
    fn tick_to_frame_conversion_is_exact_and_never_skips() {
        assert_eq!(frame_for_tick(0), 0);
        assert_eq!(frame_for_tick(64), 60);
        assert_eq!(frame_for_tick(128), 120);
        let mut previous = 0;
        for tick in 1..=640 {
            let frame = frame_for_tick(tick);
            assert!(frame - previous <= 1, "skipped a frame at tick {tick}");
            previous = frame;
        }
    }

    #[test]
    fn phases_follow_frame_data() {
        let a = light_1(); // startup 7, active 6, recovery 14, cancel 20
        assert_eq!(a.total_frames(), 27);
        assert_eq!(a.phase_at_frame(0), AttackPhase::Startup);
        assert_eq!(a.phase_at_frame(6), AttackPhase::Startup);
        assert_eq!(a.phase_at_frame(7), AttackPhase::Active);
        assert_eq!(a.phase_at_frame(12), AttackPhase::Active);
        assert_eq!(a.phase_at_frame(13), AttackPhase::Recovery);
        assert_eq!(a.phase_at_frame(19), AttackPhase::Recovery);
        assert_eq!(a.phase_at_frame(20), AttackPhase::CancelWindow);
        assert_eq!(a.phase_at_frame(26), AttackPhase::CancelWindow);
        assert_eq!(a.phase_at_frame(27), AttackPhase::Idle);
        assert!((a.duration_seconds() - 0.45).abs() < 1e-6);
    }

    #[test]
    fn cancel_frame_is_clamped_into_recovery() {
        let early = AttackFrameData {
            cancel_frame: 2,
            ..light_1()
        };
        assert_eq!(early.cancel_start_frame(), early.active_end_frame());
        assert_eq!(
            early.phase_at_frame(8),
            AttackPhase::Active,
            "cannot cancel an active hitbox"
        );
    }

    fn run_attack_to_idle(state: &mut MeleeCombatState) -> Vec<AttackPhase> {
        let mut input = MeleeInput::default();
        let mut phases = vec![state.attack_phase];
        for _ in 0..200 {
            step_melee(state, &mut input, None, DT);
            if phases.last() != Some(&state.attack_phase) {
                phases.push(state.attack_phase);
            }
            if state.attack_phase == AttackPhase::Idle {
                break;
            }
        }
        phases
    }

    #[test]
    fn attack_progresses_through_every_phase_in_order() {
        let mut s = MeleeCombatState::default();
        s.start_attack(light_1());
        assert_eq!(
            run_attack_to_idle(&mut s),
            vec![
                AttackPhase::Startup,
                AttackPhase::Active,
                AttackPhase::Recovery,
                AttackPhase::CancelWindow,
                AttackPhase::Idle
            ]
        );
        assert!(s.current_attack.is_none());
    }

    #[test]
    fn swing_lasts_its_authored_duration_at_64_hz() {
        let mut s = MeleeCombatState::default();
        let mut input = MeleeInput {
            light_pressed: true,
            ..Default::default()
        };
        step_melee(&mut s, &mut input, None, DT);
        let mut ticks = 0;
        let mut active_ticks = 0;
        while s.current_attack.is_some() {
            step_melee(&mut s, &mut input, None, DT);
            ticks += 1;
            if s.attack_phase == AttackPhase::Active {
                active_ticks += 1;
            }
        }
        // 27 frames at 60 fps = 0.45 s = 28.8 ticks -> idle on tick 29.
        assert_eq!(ticks, 29);
        // 6 active frames = 0.1 s -> 6 or 7 ticks, never fewer than 6.
        assert!(
            (6..=7).contains(&active_ticks),
            "active ticks {active_ticks}"
        );
    }

    #[test]
    fn chaining_only_in_recovery_or_cancel_window() {
        let mut s = MeleeCombatState::default();
        assert!(!s.can_chain_combo());
        s.start_attack(light_1());
        for frame in 0..light_1().total_frames() {
            s.attack_phase = light_1().phase_at_frame(frame);
            let expected = frame >= 13;
            assert_eq!(s.can_chain_combo(), expected, "frame {frame}");
            let cancellable = !(7..20).contains(&frame);
            assert_eq!(s.can_cancel(), cancellable, "frame {frame}");
        }
    }

    #[test]
    fn hit_list_resets_per_swing() {
        let mut s = MeleeCombatState::default();
        let target = Entity::from_raw(7);
        s.start_attack(light_1());
        assert!(s.hit_entities.insert(target));
        assert!(
            !s.hit_entities.insert(target),
            "second hit in the same swing"
        );
        s.start_attack(light_1());
        assert!(s.hit_entities.insert(target), "a new swing can hit again");
    }

    fn parrying_for(seconds: f32) -> MeleeCombatState {
        let mut s = MeleeCombatState::default();
        assert!(s.start_parry());
        s.parry_timer = seconds;
        s
    }

    #[test]
    fn perfect_parry_within_120ms() {
        let mut defender = parrying_for(0.100);
        let mut attacker_posture = 10.0;
        let (result, damage) = defender.resolve_incoming_attack(48.0, 22.0, &mut attacker_posture);
        assert_eq!(result, ParryResult::PerfectParry);
        assert_eq!(damage, 0.0);
        assert_eq!(attacker_posture, 50.0, "+40 posture to the attacker");
        assert_eq!(defender.posture_current, 0.0);
    }

    #[test]
    fn late_parry_is_a_block() {
        let mut defender = parrying_for(0.130);
        let mut attacker_posture = 0.0;
        let (result, damage) = defender.resolve_incoming_attack(100.0, 40.0, &mut attacker_posture);
        assert_eq!(result, ParryResult::Blocked);
        assert!((damage - 30.0).abs() < 1e-4, "70 % mitigated");
        assert!(
            (defender.posture_current - 20.0).abs() < 1e-4,
            "half posture"
        );
        assert_eq!(attacker_posture, 0.0);
    }

    #[test]
    fn unguarded_or_flanked_takes_full_hit() {
        let mut open = MeleeCombatState::default();
        let mut p = 0.0;
        assert_eq!(
            open.resolve_incoming_attack(48.0, 22.0, &mut p),
            (ParryResult::Hit, 48.0)
        );
        assert_eq!(open.posture_current, 22.0);

        let mut flanked = parrying_for(0.05);
        let (result, damage) = flanked.resolve_incoming_attack_from(48.0, 22.0, &mut p, false);
        assert_eq!(
            (result, damage),
            (ParryResult::Hit, 48.0),
            "guard faces the wrong way"
        );
    }

    #[test]
    fn posture_breaks_at_max_then_recovers() {
        let mut s = MeleeCombatState::default();
        s.start_attack(light_1());
        let mut p = 0.0;
        s.resolve_incoming_attack(10.0, 60.0, &mut p);
        assert!(!s.is_posture_broken);
        s.resolve_incoming_attack(10.0, 60.0, &mut p);
        assert!(s.is_posture_broken);
        assert_eq!(s.posture_current, s.posture_max);
        assert!(s.current_attack.is_none(), "break interrupts the swing");
        assert!(!s.start_parry(), "no guard while broken");

        let mut input = MeleeInput {
            light_pressed: true,
            ..Default::default()
        };
        step_melee(&mut s, &mut input, None, DT);
        assert!(s.current_attack.is_none(), "no attacks while broken");

        for _ in 0..(64 * 2 + 1) {
            step_melee(&mut s, &mut MeleeInput::default(), None, DT);
        }
        assert!(!s.is_posture_broken);
        assert_eq!(s.posture_current, 0.0);
    }

    #[test]
    fn posture_regenerates_after_delay() {
        let mut s = MeleeCombatState::default();
        s.add_posture(50.0);
        for _ in 0..64 {
            step_melee(&mut s, &mut MeleeInput::default(), None, DT);
        }
        assert_eq!(s.posture_current, 50.0, "no regen within 1.5 s");
        for _ in 0..64 {
            step_melee(&mut s, &mut MeleeInput::default(), None, DT);
        }
        assert!(s.posture_current < 50.0 && s.posture_current > 40.0);
    }

    #[test]
    fn parry_cancels_startup_but_not_active() {
        let mut s = MeleeCombatState::default();
        s.start_attack(light_1());
        assert!(s.start_parry(), "startup is cancellable");
        assert!(s.current_attack.is_none() && s.is_parrying);

        let mut s = MeleeCombatState::default();
        s.start_attack(light_1());
        s.attack_phase = AttackPhase::Active;
        assert!(!s.start_parry(), "active swing cannot be cancelled");
    }

    #[test]
    fn hostile_inputs_are_sanitized() {
        let mut s = MeleeCombatState::default();
        let mut p = 0.0;
        let (_, damage) = s.resolve_incoming_attack(f32::NAN, f32::INFINITY, &mut p);
        assert_eq!(damage, 0.0);
        assert_eq!(s.posture_current, 0.0);
        assert!(!s.add_posture(-50.0));
    }
}
