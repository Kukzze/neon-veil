//! Fixed-rate simulation clock shared by client prediction and the server.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Authoritative simulation and network tick rate.
pub const NETWORK_TICK_RATE_HZ: u32 = 64;

/// Length of one fixed simulation step in seconds.
pub const FIXED_TIMESTEP_SECS: f32 = 1.0 / NETWORK_TICK_RATE_HZ as f32;

/// Alias kept for the server-side naming used across the design documents.
pub const SERVER_TICK_DURATION_SECS: f32 = FIXED_TIMESTEP_SECS;

/// Exact length of one tick in nanoseconds (1 s / 64 = 15.625 ms).
pub const TICK_DURATION_NANOS: u64 = 1_000_000_000 / NETWORK_TICK_RATE_HZ as u64;

/// Length of one fixed simulation step as a [`Duration`].
pub const FIXED_TIMESTEP: Duration = Duration::from_nanos(TICK_DURATION_NANOS);

/// Monotonic simulation tick counter.
///
/// Arithmetic wraps on overflow so that [`NetworkTick::diff`] stays correct
/// across a rollover; at 64 Hz a `u64` would take billions of years to wrap.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct NetworkTick(pub u64);

impl NetworkTick {
    pub const ZERO: Self = Self(0);

    /// The tick immediately after this one.
    #[inline]
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// The tick immediately before this one.
    #[inline]
    #[must_use]
    pub const fn prev(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }

    /// Signed number of ticks from `other` to `self` (`self - other`).
    ///
    /// Uses modular arithmetic, so the result is correct across a rollover as
    /// long as the two ticks are less than `2^63` apart.
    #[inline]
    pub const fn diff(self, other: NetworkTick) -> i64 {
        self.0.wrapping_sub(other.0) as i64
    }

    /// Wall-clock time elapsed from `other` to `self`.
    ///
    /// Saturates to [`Duration::ZERO`] when `other` is later than `self`.
    #[inline]
    pub fn duration_since(self, other: NetworkTick) -> Duration {
        let ticks = self.diff(other);
        if ticks <= 0 {
            return Duration::ZERO;
        }
        ticks_to_duration(ticks.unsigned_abs())
    }
}

/// Converts a tick count to an exact [`Duration`] without overflow.
#[inline]
pub fn ticks_to_duration(ticks: u64) -> Duration {
    let rate = u64::from(NETWORK_TICK_RATE_HZ);
    Duration::from_secs(ticks / rate) + Duration::from_nanos((ticks % rate) * TICK_DURATION_NANOS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_consistent() {
        assert_eq!(NETWORK_TICK_RATE_HZ, 64);
        assert!((FIXED_TIMESTEP_SECS - 0.015_625).abs() < f32::EPSILON);
        assert_eq!(FIXED_TIMESTEP, Duration::from_micros(15_625));
        assert_eq!(
            FIXED_TIMESTEP * NETWORK_TICK_RATE_HZ,
            Duration::from_secs(1)
        );
    }

    #[test]
    fn next_and_prev_step_by_one() {
        let t = NetworkTick(100);
        assert_eq!(t.next(), NetworkTick(101));
        assert_eq!(t.prev(), NetworkTick(99));
        assert_eq!(t.next().prev(), t);
        assert_eq!(NetworkTick::default(), NetworkTick::ZERO);
    }

    #[test]
    fn rollover_wraps_and_diff_survives_it() {
        let max = NetworkTick(u64::MAX);
        assert_eq!(max.next(), NetworkTick::ZERO);
        assert_eq!(NetworkTick::ZERO.prev(), max);

        assert_eq!(NetworkTick::ZERO.diff(max), 1);
        assert_eq!(max.diff(NetworkTick::ZERO), -1);
        assert_eq!(NetworkTick(5).diff(NetworkTick(u64::MAX - 4)), 10);
        assert_eq!(
            NetworkTick(5).duration_since(NetworkTick(u64::MAX - 4)),
            FIXED_TIMESTEP * 10
        );
    }

    #[test]
    fn ordering_follows_tick_value() {
        let mut ticks = vec![
            NetworkTick(7),
            NetworkTick(2),
            NetworkTick(9),
            NetworkTick(2),
        ];
        ticks.sort();
        assert_eq!(
            ticks,
            vec![
                NetworkTick(2),
                NetworkTick(2),
                NetworkTick(7),
                NetworkTick(9)
            ]
        );
        assert!(NetworkTick(1) < NetworkTick(2));
        assert_eq!(NetworkTick(3).max(NetworkTick(8)), NetworkTick(8));
    }

    #[test]
    fn diff_is_signed() {
        assert_eq!(NetworkTick(110).diff(NetworkTick(100)), 10);
        assert_eq!(NetworkTick(100).diff(NetworkTick(110)), -10);
        assert_eq!(NetworkTick(42).diff(NetworkTick(42)), 0);
    }

    #[test]
    fn duration_since_converts_exactly() {
        assert_eq!(
            NetworkTick(64).duration_since(NetworkTick(0)),
            Duration::from_secs(1)
        );
        assert_eq!(
            NetworkTick(96).duration_since(NetworkTick(0)),
            Duration::from_millis(1_500)
        );
        assert_eq!(
            NetworkTick(1).duration_since(NetworkTick(0)),
            FIXED_TIMESTEP
        );
        assert_eq!(
            NetworkTick(0).duration_since(NetworkTick(10)),
            Duration::ZERO
        );
    }

    #[test]
    fn ticks_to_duration_does_not_overflow() {
        let d = ticks_to_duration(u64::MAX);
        assert_eq!(d.as_secs(), u64::MAX / 64);
    }

    #[test]
    fn serde_roundtrip() {
        let t = NetworkTick(123_456_789);
        let bytes = bitcode::serialize(&t).expect("encode");
        assert_eq!(
            bitcode::deserialize::<NetworkTick>(&bytes).expect("decode"),
            t
        );
        let json = serde_json::to_string(&t).expect("json encode");
        assert_eq!(json, "123456789");
    }
}
