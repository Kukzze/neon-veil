//! Procedural sound design for foley, ambience and the music bed.
//!
//! Every sound is rendered once at startup from small DSP building blocks
//! (noise, oscillators, envelopes and a state-variable filter), so the game
//! ships without sample packs and each cue stays deterministic.

use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 48_000;
const SR: f32 = SAMPLE_RATE as f32;

/// A rendered stereo sound.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// Whether the sound is built to loop seamlessly.
    pub looping: bool,
}

impl Rendered {
    fn mono(samples: Vec<f32>) -> Self {
        Self {
            right: samples.clone(),
            left: samples,
            looping: false,
        }
    }

    pub fn len(&self) -> usize {
        self.left.len()
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    pub fn peak(&self) -> f32 {
        self.left
            .iter()
            .chain(&self.right)
            .fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    /// Scales so the loudest sample sits at `target`.
    fn normalized(mut self, target: f32) -> Self {
        let peak = self.peak();
        if peak > 0.0 {
            let gain = target / peak;
            for s in self.left.iter_mut().chain(self.right.iter_mut()) {
                *s *= gain;
            }
        }
        self
    }
}

/// Deterministic white noise (xorshift).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Chamberlin state-variable filter.
#[derive(Default)]
struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    /// Returns (low, band, high) for one sample at cutoff `hz`, damping `q`.
    fn tick(&mut self, input: f32, hz: f32, q: f32) -> (f32, f32, f32) {
        let f = 2.0 * (std::f32::consts::PI * hz.clamp(20.0, SR * 0.2) / SR).sin();
        let high = input - self.low - q * self.band;
        self.band += f * high;
        self.low += f * self.band;
        (self.low, self.band, high)
    }
}

fn samples(seconds: f32) -> usize {
    (seconds * SR) as usize
}

/// Attack / exponential decay envelope.
fn env(t: f32, attack: f32, decay: f32) -> f32 {
    if t < attack {
        t / attack
    } else {
        (-(t - attack) / decay).exp()
    }
}

/// A sine whose pitch glides from `from` to `to` Hz.
fn thump(n: usize, from: f32, to: f32, decay: f32) -> Vec<f32> {
    let mut phase = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let hz = to + (from - to) * (-t / (decay * 0.5)).exp();
            phase += TAU * hz / SR;
            phase.sin() * env(t, 0.002, decay)
        })
        .collect()
}

fn mix(a: &mut [f32], b: &[f32], gain: f32) {
    for (x, y) in a.iter_mut().zip(b) {
        *x += y * gain;
    }
}

pub fn footstep(seed: u32) -> Rendered {
    let n = samples(0.12);
    let mut out = thump(n, 140.0, 70.0, 0.03);
    let mut noise = Noise(seed | 1);
    let mut svf = Svf::default();
    let grit: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            svf.tick(noise.next(), 2600.0, 0.9).1 * env(t, 0.001, 0.018)
        })
        .collect();
    mix(&mut out, &grit, 1.6);
    Rendered::mono(out).normalized(0.8)
}

pub fn jump_takeoff() -> Rendered {
    let n = samples(0.35);
    let mut noise = Noise(0x5eed_0001);
    let mut svf = Svf::default();
    let mut phase = 0.0;
    let out = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let sweep = 500.0 + 2500.0 * (t / 0.35);
            let whoosh = svf.tick(noise.next(), sweep, 0.6).1 * env(t, 0.04, 0.12);
            phase += TAU * (110.0 - 60.0 * t / 0.35) / SR;
            let thruster = (phase.sin() + 0.5 * (2.0 * phase).sin()) * env(t, 0.005, 0.06);
            whoosh * 1.2 + thruster * 0.6
        })
        .collect();
    Rendered::mono(out).normalized(0.75)
}

pub fn land(hard: bool) -> Rendered {
    let (len, from, to, decay) = if hard {
        (0.4, 120.0, 38.0, 0.12)
    } else {
        (0.22, 150.0, 60.0, 0.06)
    };
    let n = samples(len);
    let mut out = thump(n, from, to, decay);
    let mut noise = Noise(0x1a4d);
    let mut svf = Svf::default();
    let debris: Vec<f32> = (0..n)
        .map(|i| svf.tick(noise.next(), 1400.0, 1.0).0 * env(i as f32 / SR, 0.001, 0.04))
        .collect();
    mix(&mut out, &debris, if hard { 1.4 } else { 0.9 });
    Rendered::mono(out).normalized(if hard { 0.95 } else { 0.8 })
}

/// A seamless loop of surface friction for slides.
pub fn slide_loop() -> Rendered {
    let n = samples(1.0);
    let mut noise = Noise(0x511d);
    let mut svf = Svf::default();
    let raw: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let wobble = 900.0 + 300.0 * (TAU * 3.0 * t).sin();
            svf.tick(noise.next(), wobble, 0.7).1
        })
        .collect();
    let mut out = crossfade_loop(raw, samples(0.08));
    out.looping = true;
    out.normalized(0.6)
}

pub fn katana_swing(heavy: bool, seed: u32) -> Rendered {
    let (len, from, to) = if heavy {
        (0.42, 280.0, 1600.0)
    } else {
        (0.24, 700.0, 4200.0)
    };
    let n = samples(len);
    let mut noise = Noise(seed | 1);
    let mut svf = Svf::default();
    let mut phase = 0.0;
    let out = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let x = t / len;
            let hz = from + (to - from) * (x * std::f32::consts::PI).sin();
            let whoosh = svf.tick(noise.next(), hz, 0.25).1;
            // A faint metallic ring rides on the swoosh.
            phase += TAU * 3100.0 / SR;
            let ring = phase.sin() * env(t, 0.01, 0.08) * 0.08;
            whoosh * (x * std::f32::consts::PI).sin().powi(2) + ring
        })
        .collect();
    Rendered::mono(out).normalized(if heavy { 0.9 } else { 0.8 })
}

pub fn parry_spark() -> Rendered {
    let n = samples(0.9);
    let partials = [(1180.0, 1.0), (2763.0, 0.6), (4317.0, 0.45), (6011.0, 0.25)];
    let mut noise = Noise(0x9a77);
    let out = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let chime: f32 = partials
                .iter()
                .map(|(hz, amp)| (TAU * hz * t).sin() * amp * env(t, 0.001, 0.25 / (hz / 1180.0)))
                .sum();
            chime + noise.next() * env(t, 0.0005, 0.006) * 0.8
        })
        .collect();
    Rendered::mono(out).normalized(0.85)
}

pub fn impact(seed: u32) -> Rendered {
    let n = samples(0.18);
    let mut out = thump(n, 190.0, 80.0, 0.04);
    let mut noise = Noise(seed | 1);
    let mut svf = Svf::default();
    let crack: Vec<f32> = (0..n)
        .map(|i| svf.tick(noise.next(), 3200.0, 0.5).2 * env(i as f32 / SR, 0.0005, 0.02))
        .collect();
    mix(&mut out, &crack, 1.1);
    Rendered::mono(out).normalized(0.9)
}

pub fn ui_click() -> Rendered {
    let n = samples(0.05);
    let out = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            (TAU * 1900.0 * t).sin() * env(t, 0.001, 0.01)
        })
        .collect();
    Rendered::mono(out).normalized(0.5)
}

/// Rain on concrete: decorrelated filtered noise with sparse drips.
pub fn rain_loop() -> Rendered {
    let n = samples(6.0);
    let channel = |seed: u32| {
        let mut noise = Noise(seed);
        let mut drip = Noise(seed ^ 0xd21f);
        let mut svf = Svf::default();
        let mut hiss = Svf::default();
        let raw: Vec<f32> = (0..n)
            .map(|_| {
                let x = noise.next();
                let body = svf.tick(x, 1800.0, 1.2).0;
                let air = hiss.tick(x, 6500.0, 1.0).2 * 0.25;
                let spike = if drip.next() > 0.9993 { 0.9 } else { 0.0 };
                body + air + spike
            })
            .collect();
        crossfade_loop(raw, samples(0.3)).left
    };
    let out = Rendered {
        left: channel(0x7a1e),
        right: channel(0x3b9c),
        looping: true,
    };
    out.normalized(0.5)
}

/// A slow, dark synth bed in D minor that loops every 16 seconds.
pub fn night_drone() -> Rendered {
    let seconds = 16.0;
    let n = samples(seconds);
    // D2, A2, F3 then C3, G2, E3: two chords, eight seconds each.
    let chords = [[73.42, 110.0, 174.61], [65.41, 98.0, 164.81]];
    let channel = |detune: f32| {
        let mut svf = Svf::default();
        let mut phases = [0.0_f32; 6];
        let raw: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / SR;
                let chord = chords[((t / 8.0) as usize) % 2];
                let mut s = 0.0;
                for (k, hz) in chord.iter().enumerate() {
                    for (d, spread) in [(0, 1.0 - detune), (1, 1.0 + detune)] {
                        let p = &mut phases[k * 2 + d];
                        *p = (*p + hz * spread / SR).fract();
                        s += 2.0 * *p - 1.0;
                    }
                }
                let lfo = 0.5 + 0.5 * (TAU * t / seconds * 2.0).sin();
                svf.tick(s, 300.0 + 500.0 * lfo, 1.1).0
            })
            .collect();
        crossfade_loop(raw, samples(0.5)).left
    };
    let out = Rendered {
        left: channel(0.004),
        right: channel(0.006),
        looping: true,
    };
    out.normalized(0.45)
}

/// Blends the tail into the head so the sound loops without a click.
fn crossfade_loop(mut raw: Vec<f32>, fade: usize) -> Rendered {
    let fade = fade.min(raw.len() / 2);
    let len = raw.len() - fade;
    for i in 0..fade {
        let w = i as f32 / fade as f32;
        raw[i] = raw[i] * w + raw[len + i] * (1.0 - w);
    }
    raw.truncate(len);
    Rendered::mono(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_clean(r: &Rendered) {
        assert!(!r.is_empty());
        assert_eq!(r.left.len(), r.right.len());
        assert!(r.left.iter().chain(&r.right).all(|s| s.is_finite()));
        let peak = r.peak();
        assert!(peak > 0.1 && peak <= 1.0, "peak {peak}");
    }

    #[test]
    fn every_sound_renders_finite_and_in_range() {
        for r in [
            footstep(1),
            jump_takeoff(),
            land(false),
            land(true),
            slide_loop(),
            katana_swing(false, 3),
            katana_swing(true, 4),
            parry_spark(),
            impact(5),
            ui_click(),
        ] {
            assert_clean(&r);
        }
    }

    #[test]
    fn loops_are_marked_and_seamless() {
        for r in [slide_loop(), rain_loop(), night_drone()] {
            assert_clean(&r);
            assert!(r.looping);
            let jump = (r.left[0] - r.left[r.len() - 1]).abs();
            assert!(jump < 0.25, "loop seam {jump}");
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        assert_eq!(footstep(7), footstep(7));
        assert_ne!(footstep(7).left, footstep(8).left);
    }
}
