//! Flow State combo meter (0-100) and stamina.
//!
//! Chaining acrobatic moves builds Flow; each addition restarts a combo
//! window, and once the window lapses Flow decays. Higher Flow tiers grant a
//! movement speed multiplier, and tier 3 (Overdrive) waives stamina costs.

use crate::kinematics::ParkourPhysicsConstants;
use bevy::prelude::*;
use game_core::math::sanitize_f32;

pub const FLOW_MAX: f32 = 100.0;
pub const FLOW_TIER_1_THRESHOLD: f32 = 25.0;
pub const FLOW_TIER_2_THRESHOLD: f32 = 50.0;
pub const FLOW_OVERDRIVE_THRESHOLD: f32 = 75.0;
pub const FLOW_TIER_1_MULTIPLIER: f32 = 1.05;
pub const FLOW_TIER_2_MULTIPLIER: f32 = 1.12;
/// Default Overdrive multiplier; the live value is
/// `ParkourPhysicsConstants::flow_overdrive_multiplier`.
pub const FLOW_OVERDRIVE_MULTIPLIER: f32 = 1.25;
pub const FLOW_COMBO_WINDOW_SECONDS: f32 = 1.8;
/// Flow lost per second after the combo window lapses.
pub const FLOW_DECAY_PER_SECOND: f32 = 15.0;
pub const FLOW_WALL_KICK_REWARD: f32 = 15.0;
pub const FLOW_LANDING_ROLL_REWARD: f32 = 20.0;

pub const DEFAULT_STAMINA_MAX: f32 = 100.0;
pub const DEFAULT_STAMINA_REGEN_PER_SECOND: f32 = 20.0;

/// Flow tier derived from the current Flow value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FlowTier {
    Base,
    Tier1,
    Tier2,
    Overdrive,
}

impl FlowTier {
    pub fn from_flow(flow: f32) -> Self {
        let flow = sanitize_f32(flow);
        if flow >= FLOW_OVERDRIVE_THRESHOLD {
            Self::Overdrive
        } else if flow >= FLOW_TIER_2_THRESHOLD {
            Self::Tier2
        } else if flow >= FLOW_TIER_1_THRESHOLD {
            Self::Tier1
        } else {
            Self::Base
        }
    }

    /// Speed multiplier for this tier; `overdrive` is the tier 3 value.
    pub fn multiplier(self, overdrive: f32) -> f32 {
        match self {
            Self::Base => 1.0,
            Self::Tier1 => FLOW_TIER_1_MULTIPLIER,
            Self::Tier2 => FLOW_TIER_2_MULTIPLIER,
            Self::Overdrive => sanitize_f32(overdrive).max(1.0),
        }
    }
}

/// Per-character Flow meter and stamina pool.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct FlowState {
    pub current_flow: f32,
    /// Restarted by every Flow gain; decay begins once it finishes.
    pub combo_window: Timer,
    pub current_multiplier: f32,
    pub stamina_current: f32,
    pub stamina_max: f32,
    /// Stamina regained per second.
    pub stamina_regen_rate: f32,
}

impl Default for FlowState {
    fn default() -> Self {
        let mut combo_window = Timer::from_seconds(FLOW_COMBO_WINDOW_SECONDS, TimerMode::Once);
        // Start lapsed: there is no combo to protect yet.
        combo_window.tick(combo_window.duration());
        Self {
            current_flow: 0.0,
            combo_window,
            current_multiplier: 1.0,
            stamina_current: DEFAULT_STAMINA_MAX,
            stamina_max: DEFAULT_STAMINA_MAX,
            stamina_regen_rate: DEFAULT_STAMINA_REGEN_PER_SECOND,
        }
    }
}

impl FlowState {
    pub fn tier(&self) -> FlowTier {
        FlowTier::from_flow(self.current_flow)
    }

    pub fn is_overdrive(&self) -> bool {
        self.current_flow >= FLOW_OVERDRIVE_THRESHOLD
    }

    /// Adds Flow (capped at 100) and restarts the combo window. Negative or
    /// non-finite amounts are ignored.
    pub fn add_flow(&mut self, amount: f32) {
        let amount = sanitize_f32(amount).max(0.0);
        self.current_flow = (self.current_flow + amount).min(FLOW_MAX);
        self.combo_window.reset();
    }

    /// Spends stamina for a move. Always succeeds, without draining, in
    /// Overdrive. Otherwise succeeds only if enough stamina remains.
    pub fn consume_stamina(&mut self, amount: f32) -> bool {
        if self.is_overdrive() {
            return true;
        }
        let amount = sanitize_f32(amount).max(0.0);
        if self.stamina_current >= amount {
            self.stamina_current -= amount;
            true
        } else {
            false
        }
    }

    /// Advances the combo window, Flow decay, stamina regen and multiplier.
    pub fn tick(&mut self, dt: f32, overdrive_multiplier: f32) {
        let dt = sanitize_f32(dt).max(0.0);
        self.combo_window
            .tick(std::time::Duration::from_secs_f32(dt));
        if self.combo_window.finished() {
            self.current_flow = (self.current_flow - FLOW_DECAY_PER_SECOND * dt).max(0.0);
        }
        self.stamina_current =
            (self.stamina_current + self.stamina_regen_rate.max(0.0) * dt).min(self.stamina_max);
        self.current_multiplier = self.tier().multiplier(overdrive_multiplier);
    }
}

/// Ticks every character's Flow and stamina on the fixed clock.
pub fn update_flow_and_stamina(
    time: Res<Time<Fixed>>,
    constants: Res<ParkourPhysicsConstants>,
    mut query: Query<&mut FlowState>,
) {
    let dt = time.delta_secs();
    for mut flow in &mut query {
        flow.tick(dt, constants.flow_overdrive_multiplier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    const EPS: f32 = 1e-3;

    fn run(flow: &mut FlowState, seconds: f32) {
        let ticks = (seconds / DT).round() as u32;
        for _ in 0..ticks {
            flow.tick(DT, FLOW_OVERDRIVE_MULTIPLIER);
        }
    }

    #[test]
    fn tier_thresholds() {
        assert_eq!(FlowTier::from_flow(0.0), FlowTier::Base);
        assert_eq!(FlowTier::from_flow(24.99), FlowTier::Base);
        assert_eq!(FlowTier::from_flow(25.0), FlowTier::Tier1);
        assert_eq!(FlowTier::from_flow(49.99), FlowTier::Tier1);
        assert_eq!(FlowTier::from_flow(50.0), FlowTier::Tier2);
        assert_eq!(FlowTier::from_flow(74.99), FlowTier::Tier2);
        assert_eq!(FlowTier::from_flow(75.0), FlowTier::Overdrive);
        assert_eq!(FlowTier::from_flow(100.0), FlowTier::Overdrive);
        assert_eq!(FlowTier::from_flow(f32::NAN), FlowTier::Base);

        assert_eq!(FlowTier::Base.multiplier(1.25), 1.0);
        assert_eq!(FlowTier::Tier1.multiplier(1.25), 1.05);
        assert_eq!(FlowTier::Tier2.multiplier(1.25), 1.12);
        assert_eq!(FlowTier::Overdrive.multiplier(1.25), 1.25);
        assert_eq!(FlowTier::Overdrive.multiplier(1.5), 1.5);
    }

    #[test]
    fn combo_accumulates_and_caps() {
        let mut f = FlowState::default();
        for _ in 0..3 {
            f.add_flow(FLOW_WALL_KICK_REWARD);
        }
        assert_eq!(f.current_flow, 45.0);
        f.tick(DT, FLOW_OVERDRIVE_MULTIPLIER);
        assert_eq!(f.tier(), FlowTier::Tier1);
        assert_eq!(f.current_multiplier, 1.05);

        for _ in 0..10 {
            f.add_flow(FLOW_LANDING_ROLL_REWARD);
        }
        assert_eq!(f.current_flow, FLOW_MAX);
        f.add_flow(-50.0);
        f.add_flow(f32::NAN);
        assert_eq!(f.current_flow, FLOW_MAX);
    }

    #[test]
    fn flow_holds_during_window_then_decays_at_15_per_second() {
        let mut f = FlowState::default();
        f.add_flow(60.0);
        run(&mut f, 1.75);
        assert_eq!(f.current_flow, 60.0, "no decay inside the 1.8 s window");
        run(&mut f, 0.05 + 1.0);
        // ~1.0 s of decay after the window lapses.
        assert!(
            (f.current_flow - 45.0).abs() < 0.3,
            "flow {}",
            f.current_flow
        );
        run(&mut f, 10.0);
        assert_eq!(f.current_flow, 0.0);
        assert_eq!(f.current_multiplier, 1.0);
    }

    #[test]
    fn new_gain_restarts_the_window() {
        let mut f = FlowState::default();
        f.add_flow(30.0);
        run(&mut f, 1.5);
        f.add_flow(15.0);
        run(&mut f, 1.5);
        assert_eq!(f.current_flow, 45.0);
    }

    #[test]
    fn stamina_consumption_and_regen() {
        let mut f = FlowState::default();
        assert!(f.consume_stamina(60.0));
        assert!((f.stamina_current - 40.0).abs() < EPS);
        assert!(!f.consume_stamina(50.0), "not enough stamina");
        assert!(
            (f.stamina_current - 40.0).abs() < EPS,
            "failed spend is free"
        );

        run(&mut f, 1.0);
        assert!((f.stamina_current - 60.0).abs() < 0.1, "regen 20/s");
        run(&mut f, 10.0);
        assert_eq!(f.stamina_current, f.stamina_max, "regen caps at max");
    }

    #[test]
    fn overdrive_waives_stamina_costs() {
        let mut f = FlowState::default();
        f.add_flow(80.0);
        f.stamina_current = 0.0;
        assert!(f.is_overdrive());
        assert!(f.consume_stamina(50.0));
        assert_eq!(f.stamina_current, 0.0);
        f.tick(DT, 1.25);
        assert_eq!(f.current_multiplier, 1.25);
    }

    #[test]
    fn system_uses_fixed_clock_and_tuned_overdrive() {
        let mut app = App::new();
        let mut fixed = Time::<Fixed>::from_hz(64.0);
        fixed.advance_by(std::time::Duration::from_secs_f32(DT));
        app.insert_resource(fixed)
            .insert_resource(ParkourPhysicsConstants {
                flow_overdrive_multiplier: 1.4,
                ..ParkourPhysicsConstants::default()
            })
            .add_systems(Update, update_flow_and_stamina);
        let mut flow = FlowState::default();
        flow.add_flow(90.0);
        let entity = app.world_mut().spawn(flow).id();
        app.update();
        let flow = app.world().get::<FlowState>(entity).expect("flow");
        assert_eq!(flow.current_multiplier, 1.4);
    }
}
