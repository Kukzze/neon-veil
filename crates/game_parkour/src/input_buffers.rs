//! Coyote time and input buffering for forgiving jump and slide timing.

use crate::kinematics::ParkourPhysicsConstants;
use crate::state_machine::ParkourState;
use bevy::prelude::*;
use game_core::math::sanitize_f32;

/// Per-character timing windows. All timers count down in seconds and are
/// considered active while greater than zero.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct LocomotionBuffer {
    /// Time left to jump after leaving the ground.
    pub coyote_timer: f32,
    /// Time left for a jump press to be honored when it becomes possible.
    pub jump_buffer_timer: f32,
    /// Time left for a slide press to be honored when it becomes possible.
    pub slide_buffer_timer: f32,
    /// Fixed-clock time (s) the character was last grounded.
    pub last_grounded_time: f32,
}

#[inline]
fn sanitize_duration(seconds: f32) -> f32 {
    sanitize_f32(seconds).max(0.0)
}

#[inline]
fn take(timer: &mut f32) -> bool {
    if *timer > 0.0 {
        *timer = 0.0;
        true
    } else {
        false
    }
}

impl LocomotionBuffer {
    /// Records a jump press, valid for `duration` seconds.
    pub fn buffer_jump(&mut self, duration: f32) {
        self.jump_buffer_timer = sanitize_duration(duration);
    }

    /// Consumes a buffered jump press if one is still valid.
    pub fn consume_jump_buffer(&mut self) -> bool {
        take(&mut self.jump_buffer_timer)
    }

    /// Records a slide press, valid for `duration` seconds.
    pub fn buffer_slide(&mut self, duration: f32) {
        self.slide_buffer_timer = sanitize_duration(duration);
    }

    /// Consumes a buffered slide press if one is still valid.
    pub fn consume_slide_buffer(&mut self) -> bool {
        take(&mut self.slide_buffer_timer)
    }

    /// Whether a jump is still allowed after leaving the ground.
    pub fn can_coyote_jump(&self) -> bool {
        self.coyote_timer > 0.0
    }

    /// Spends the coyote window so it cannot grant a second jump.
    pub fn consume_coyote_jump(&mut self) -> bool {
        take(&mut self.coyote_timer)
    }

    /// Advances all windows by `dt`. While grounded the coyote window is held
    /// at `coyote_max`; once airborne it drains.
    pub fn tick(&mut self, is_grounded: bool, dt: f32, coyote_max: f32) {
        let dt = sanitize_duration(dt);
        if is_grounded {
            self.coyote_timer = sanitize_duration(coyote_max);
        } else {
            self.coyote_timer = (self.coyote_timer - dt).max(0.0);
        }
        self.jump_buffer_timer = (self.jump_buffer_timer - dt).max(0.0);
        self.slide_buffer_timer = (self.slide_buffer_timer - dt).max(0.0);
    }
}

/// Ticks every character's buffers on the fixed clock.
pub fn update_locomotion_buffers(
    time: Res<Time<Fixed>>,
    constants: Res<ParkourPhysicsConstants>,
    mut query: Query<(&mut LocomotionBuffer, &ParkourState)>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    for (mut buffer, state) in &mut query {
        let grounded = state.is_grounded();
        buffer.tick(grounded, dt, constants.coyote_time_duration);
        if grounded {
            buffer.last_grounded_time = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;
    use std::time::Duration;

    const COYOTE: f32 = 0.15;
    const JUMP_BUFFER: f32 = 0.12;

    fn grounded_buffer() -> LocomotionBuffer {
        let mut b = LocomotionBuffer::default();
        b.tick(true, DT, COYOTE);
        b
    }

    fn airborne_for(buffer: &mut LocomotionBuffer, seconds: f32) {
        let ticks = (seconds / DT).round() as u32;
        for _ in 0..ticks {
            buffer.tick(false, DT, COYOTE);
        }
    }

    #[test]
    fn coyote_window_allows_jump_within_150ms() {
        let mut b = grounded_buffer();
        assert!(b.can_coyote_jump());
        // 9 ticks = 140.6 ms after walking off the edge.
        airborne_for(&mut b, 0.140_625);
        assert!(b.can_coyote_jump());
        assert!(b.consume_coyote_jump());
        assert!(!b.can_coyote_jump(), "coyote grants exactly one jump");
        assert!(!b.consume_coyote_jump());
    }

    #[test]
    fn coyote_jump_fails_after_160ms() {
        let mut b = grounded_buffer();
        airborne_for(&mut b, 0.160);
        assert!(!b.can_coyote_jump());
        assert!(!b.consume_coyote_jump());
    }

    #[test]
    fn coyote_expires_on_the_tick_after_150ms() {
        let mut b = grounded_buffer();
        let mut last_valid_tick = 0;
        for tick in 1..=20 {
            b.tick(false, DT, COYOTE);
            if b.can_coyote_jump() {
                last_valid_tick = tick;
            }
        }
        // 9 * 15.625 ms = 140.6 ms is the last tick inside 150 ms.
        assert_eq!(last_valid_tick, 9);
    }

    #[test]
    fn jump_pressed_100ms_before_landing_is_consumed_on_landing() {
        let mut b = LocomotionBuffer::default();
        b.buffer_jump(JUMP_BUFFER);
        airborne_for(&mut b, 0.093_75); // 6 airborne ticks
        b.tick(true, DT, COYOTE); // landing tick, ~109 ms after the press
        assert!(b.consume_jump_buffer());
        assert!(!b.consume_jump_buffer(), "buffer consumed once");
    }

    #[test]
    fn stale_jump_buffer_is_not_consumed() {
        let mut b = LocomotionBuffer::default();
        b.buffer_jump(JUMP_BUFFER);
        airborne_for(&mut b, 0.125);
        b.tick(true, DT, COYOTE);
        assert!(!b.consume_jump_buffer());
    }

    #[test]
    fn slide_buffer_behaves_like_jump_buffer() {
        let mut b = LocomotionBuffer::default();
        b.buffer_slide(0.1);
        b.tick(true, 0.05, COYOTE);
        assert!(b.consume_slide_buffer());
        b.buffer_slide(0.1);
        b.tick(true, 0.2, COYOTE);
        assert!(!b.consume_slide_buffer());
    }

    #[test]
    fn hostile_durations_are_sanitized() {
        let mut b = LocomotionBuffer::default();
        b.buffer_jump(f32::NAN);
        assert_eq!(b.jump_buffer_timer, 0.0);
        b.buffer_jump(-1.0);
        assert_eq!(b.jump_buffer_timer, 0.0);
        b.tick(true, f32::NAN, f32::INFINITY);
        assert_eq!(b.coyote_timer, 0.0);
        b.tick(false, f32::INFINITY, COYOTE);
        assert!(b.coyote_timer.is_finite());
    }

    #[test]
    fn system_ticks_on_fixed_clock() {
        let mut app = App::new();
        let mut fixed = Time::<Fixed>::from_hz(64.0);
        fixed.advance_by(Duration::from_secs_f32(DT));
        app.insert_resource(fixed)
            .init_resource::<ParkourPhysicsConstants>()
            .add_systems(Update, update_locomotion_buffers);

        let grounded = app
            .world_mut()
            .spawn((LocomotionBuffer::default(), ParkourState::Sprinting))
            .id();
        let falling = app
            .world_mut()
            .spawn((
                LocomotionBuffer {
                    coyote_timer: 0.1,
                    jump_buffer_timer: 0.1,
                    ..default()
                },
                ParkourState::JumpDescent,
            ))
            .id();
        app.update();

        let world = app.world();
        let g = world.get::<LocomotionBuffer>(grounded).expect("buffer");
        assert_eq!(g.coyote_timer, 0.15);
        assert!((g.last_grounded_time - DT).abs() < 1e-6);

        let f = world.get::<LocomotionBuffer>(falling).expect("buffer");
        assert!((f.coyote_timer - (0.1 - DT)).abs() < 1e-6);
        assert!((f.jump_buffer_timer - (0.1 - DT)).abs() < 1e-6);
        assert_eq!(f.last_grounded_time, 0.0);
    }
}
