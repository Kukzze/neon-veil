//! The classic OSRS experience curve, levels 1-99.
//!
//! `difference(i) = floor(i + 300 * 2^(i / 7))` and the XP for level `L` is
//! `floor(sum(difference(1..L)) / 4)`. The table is precomputed so runtime
//! lookups involve no floating point; a test regenerates it from the formula.

pub const MAX_LEVEL: u8 = 99;
pub const MAX_XP: u32 = 13_034_431;
/// Level 92: the well-known "halfway to 99" point (50.0003 % of 13,034,431).
pub const HALFWAY_XP: u32 = 6_517_253;
/// Experience keeps accruing past 99 up to this cap.
pub const XP_CAP: u32 = 200_000_000;

/// Cumulative XP required for each level; index 0 is padding.
pub static XP_TABLE: [u32; 100] = [
    0,          // Level 0 (unused)
    0,          // Level 1
    83,         // Level 2
    174,        // Level 3
    276,        // Level 4
    388,        // Level 5
    512,        // Level 6
    650,        // Level 7
    801,        // Level 8
    969,        // Level 9
    1_154,      // Level 10
    1_358,      // Level 11
    1_584,      // Level 12
    1_833,      // Level 13
    2_107,      // Level 14
    2_411,      // Level 15
    2_746,      // Level 16
    3_115,      // Level 17
    3_523,      // Level 18
    3_973,      // Level 19
    4_470,      // Level 20
    5_018,      // Level 21
    5_624,      // Level 22
    6_291,      // Level 23
    7_028,      // Level 24
    7_842,      // Level 25
    8_740,      // Level 26
    9_730,      // Level 27
    10_824,     // Level 28
    12_031,     // Level 29
    13_363,     // Level 30
    14_833,     // Level 31
    16_456,     // Level 32
    18_247,     // Level 33
    20_224,     // Level 34
    22_406,     // Level 35
    24_815,     // Level 36
    27_473,     // Level 37
    30_408,     // Level 38
    33_648,     // Level 39
    37_224,     // Level 40
    41_171,     // Level 41
    45_529,     // Level 42
    50_339,     // Level 43
    55_649,     // Level 44
    61_512,     // Level 45
    67_983,     // Level 46
    75_127,     // Level 47
    83_014,     // Level 48
    91_721,     // Level 49
    101_333,    // Level 50
    111_945,    // Level 51
    123_660,    // Level 52
    136_594,    // Level 53
    150_872,    // Level 54
    166_636,    // Level 55
    184_040,    // Level 56
    203_254,    // Level 57
    224_466,    // Level 58
    247_886,    // Level 59
    273_742,    // Level 60
    302_288,    // Level 61
    333_804,    // Level 62
    368_599,    // Level 63
    407_015,    // Level 64
    449_428,    // Level 65
    496_254,    // Level 66
    547_953,    // Level 67
    605_032,    // Level 68
    668_051,    // Level 69
    737_627,    // Level 70
    814_445,    // Level 71
    899_257,    // Level 72
    992_895,    // Level 73
    1_096_278,  // Level 74
    1_210_421,  // Level 75
    1_336_443,  // Level 76
    1_475_581,  // Level 77
    1_629_200,  // Level 78
    1_798_808,  // Level 79
    1_986_068,  // Level 80
    2_192_818,  // Level 81
    2_421_087,  // Level 82
    2_673_114,  // Level 83
    2_951_373,  // Level 84
    3_258_594,  // Level 85
    3_597_792,  // Level 86
    3_972_294,  // Level 87
    4_385_776,  // Level 88
    4_842_295,  // Level 89
    5_346_332,  // Level 90
    5_902_831,  // Level 91
    6_517_253,  // Level 92
    7_195_629,  // Level 93
    7_944_614,  // Level 94
    8_771_558,  // Level 95
    9_684_577,  // Level 96
    10_692_629, // Level 97
    11_805_606, // Level 98
    13_034_431, // Level 99
];

/// Level reached with `xp` experience, 1..=99.
pub fn xp_to_level(xp: u32) -> u8 {
    // First level whose threshold exceeds `xp`, minus one.
    let above = XP_TABLE[1..].partition_point(|&threshold| threshold <= xp);
    above.clamp(1, MAX_LEVEL as usize) as u8
}

/// Cumulative XP required for `level` (clamped to 1..=99).
pub fn level_to_xp(level: u8) -> u32 {
    XP_TABLE[level.clamp(1, MAX_LEVEL) as usize]
}

/// XP still needed for the next level; 0 at level 99.
pub fn xp_remaining_to_next_level(xp: u32) -> u32 {
    let level = xp_to_level(xp);
    if level >= MAX_LEVEL {
        0
    } else {
        level_to_xp(level + 1) - xp
    }
}

/// Progress through the current level, 0.0..1.0 (1.0 at level 99).
pub fn progress_to_next_level(xp: u32) -> f32 {
    let level = xp_to_level(xp);
    if level >= MAX_LEVEL {
        return 1.0;
    }
    let floor = level_to_xp(level);
    let span = level_to_xp(level + 1) - floor;
    (xp - floor) as f32 / span as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The formula, evaluated directly.
    fn formula_table() -> Vec<u32> {
        let mut points = 0.0_f64;
        let mut table = vec![0, 0];
        for level in 1..MAX_LEVEL as u32 {
            points += (level as f64 + 300.0 * 2f64.powf(level as f64 / 7.0)).floor();
            table.push((points / 4.0).floor() as u32);
        }
        table
    }

    #[test]
    fn table_matches_the_formula_for_every_level() {
        let expected = formula_table();
        for level in 1..=MAX_LEVEL as usize {
            assert_eq!(XP_TABLE[level], expected[level], "level {level}");
        }
    }

    #[test]
    fn reference_values() {
        assert_eq!(level_to_xp(1), 0);
        assert_eq!(level_to_xp(2), 83);
        assert_eq!(level_to_xp(10), 1_154);
        assert_eq!(level_to_xp(50), 101_333);
        assert_eq!(level_to_xp(64), 407_015);
        assert_eq!(level_to_xp(92), 6_517_253);
        assert_eq!(level_to_xp(92), HALFWAY_XP);
        assert_eq!(level_to_xp(99), 13_034_431);
        assert_eq!(level_to_xp(99), MAX_XP);
        assert_eq!(level_to_xp(0), 0, "clamped");
        assert_eq!(level_to_xp(150), MAX_XP, "clamped");
    }

    #[test]
    fn table_is_strictly_increasing() {
        for level in 2..=MAX_LEVEL as usize {
            assert!(XP_TABLE[level] > XP_TABLE[level - 1], "level {level}");
        }
    }

    #[test]
    fn level_boundaries_are_exact() {
        assert_eq!(xp_to_level(6_517_253), 92);
        assert_eq!(xp_to_level(6_517_252), 91);
        assert_eq!(xp_to_level(0), 1);
        assert_eq!(xp_to_level(82), 1);
        assert_eq!(xp_to_level(83), 2);
        assert_eq!(xp_to_level(13_034_430), 98);
        assert_eq!(xp_to_level(13_034_431), 99);
        assert_eq!(xp_to_level(XP_CAP), 99);
        assert_eq!(xp_to_level(u32::MAX), 99);
        for level in 1..=MAX_LEVEL {
            assert_eq!(xp_to_level(level_to_xp(level)), level);
            if level > 1 {
                assert_eq!(xp_to_level(level_to_xp(level) - 1), level - 1);
            }
        }
    }

    #[test]
    fn remaining_and_progress() {
        assert_eq!(xp_remaining_to_next_level(0), 83);
        assert_eq!(xp_remaining_to_next_level(80), 3);
        assert_eq!(xp_remaining_to_next_level(6_517_252), 1);
        assert_eq!(xp_remaining_to_next_level(MAX_XP), 0);
        assert_eq!(progress_to_next_level(0), 0.0);
        assert!((progress_to_next_level(83 + 45) - 45.0 / 91.0).abs() < 1e-6);
        assert_eq!(progress_to_next_level(MAX_XP), 1.0);
        assert_eq!(progress_to_next_level(XP_CAP), 1.0);
    }

    #[test]
    fn level_92_is_essentially_half_of_99() {
        let fraction = HALFWAY_XP as f64 / MAX_XP as f64;
        assert!((fraction - 0.5).abs() < 1e-5, "{fraction}");
    }
}
