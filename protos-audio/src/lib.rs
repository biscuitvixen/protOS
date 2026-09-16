//! Band energies for a voice-reactive face: 32 log-spaced bands from
//! 80 Hz to 8 kHz, each in [0, 1], plus a broadband level and the
//! spectral centroid. This crate is the contract every producer of
//! `/protos/voice/bands` shares, so two processes analysing different
//! audio agree on what a band means.
//!
//! The analyser is a bank of constant-Q bandpass filters rather than
//! an FFT. The lowest bands are about twelve hertz wide, narrower than
//! any FFT bin short enough to keep a display responsive, and a filter
//! bank puts every band centre exactly on its log-spaced frequency
//! with no bin sharing. Each band is two cascaded RBJ biquads, so the
//! skirts fall at 12 dB per octave and a pure tone lights a handful of
//! neighbours rather than a third of the display. Powers are read out
//! once per hop, mapped to decibels relative to a full-scale sine and
//! squashed onto [0, 1] from a floor upward; an attack/release
//! smoother then holds syllables between hops. There is no gain
//! control: a quiet microphone gives quiet bands, and the floor is the
//! knob to move.

use std::f64::consts::{LN_2, PI};

pub const BAND_COUNT: usize = 32;

/// Analyser tuning. The defaults are the wire contract; a producer
/// changes `sample_rate` to match its device and leaves the rest.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub sample_rate: f32,
    /// Lower edge of band 0 in Hz.
    pub f_lo: f32,
    /// Upper edge of the last band in Hz.
    pub f_hi: f32,
    /// Band power at or below this many dB re a full-scale sine reads
    /// as 0; 0 dB reads as 1.
    pub floor_db: f32,
    /// Smoother time constants in seconds, applied at the hop rate.
    pub attack_s: f32,
    pub release_s: f32,
    /// Samples per readout.
    pub hop: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            f_lo: 80.0,
            f_hi: 8_000.0,
            // A typical microphone's self-noise sits below this, so
            // silence reads as zero rather than a faint glow.
            floor_db: -40.0,
            attack_s: 0.015,
            release_s: 0.120,
            // 10 ms at 48 kHz: 100 readouts a second, comfortably
            // above a 60 Hz display.
            hop: 480,
        }
    }
}

/// Lower edge of band `k`; `band_edge_hz(cfg, BAND_COUNT)` is the top.
pub fn band_edge_hz(cfg: &Config, k: usize) -> f32 {
    cfg.f_lo * (cfg.f_hi / cfg.f_lo).powf(k as f32 / BAND_COUNT as f32)
}

/// Geometric centre of band `k`.
pub fn band_centre_hz(cfg: &Config, k: usize) -> f32 {
    cfg.f_lo * (cfg.f_hi / cfg.f_lo).powf((k as f32 + 0.5) / BAND_COUNT as f32)
}

/// Transposed direct form II biquad; state in f64 so poles a few
/// hundredths of a radian from the unit circle keep their shape.
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    s1: f64,
    s2: f64,
}

impl Biquad {
    /// RBJ constant-0 dB-peak bandpass centred on `f0` with a bandwidth
    /// of `bw_octaves`, corrected for the digital frequency warp.
    fn bandpass(sample_rate: f64, f0: f64, bw_octaves: f64) -> Self {
        let w0 = 2.0 * PI * f0 / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 * (LN_2 / 2.0 * bw_octaves * w0 / sin_w0).sinh();
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }
}

pub struct BandAnalyser {
    cfg: Config,
    /// Two identical stages per band, cascaded.
    stages: [[Biquad; 2]; BAND_COUNT],
    /// Running sum of squared output over the current hop.
    power: [f64; BAND_COUNT],
    broadband: f64,
    in_hop: usize,
    bands: [f32; BAND_COUNT],
    level: f32,
    centroid: f32,
}

impl BandAnalyser {
    pub fn new(cfg: Config) -> Self {
        let bw_octaves = ((cfg.f_hi / cfg.f_lo) as f64).log2() / BAND_COUNT as f64;
        let mut stages = [[Biquad::default(); 2]; BAND_COUNT];
        for (k, pair) in stages.iter_mut().enumerate() {
            let stage = Biquad::bandpass(
                cfg.sample_rate as f64,
                band_centre_hz(&cfg, k) as f64,
                bw_octaves,
            );
            *pair = [stage, stage];
        }
        Self {
            cfg,
            stages,
            power: [0.0; BAND_COUNT],
            broadband: 0.0,
            in_hop: 0,
            bands: [0.0; BAND_COUNT],
            level: 0.0,
            centroid: 0.0,
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Feed mono samples in [-1, 1]. Returns true if at least one hop
    /// completed, so the caller knows the readouts changed.
    pub fn push(&mut self, samples: &[f32]) -> bool {
        let mut completed = false;
        for &x in samples {
            let x = x as f64;
            self.broadband += x * x;
            for (pair, power) in self.stages.iter_mut().zip(self.power.iter_mut()) {
                let mid = pair[0].run(x);
                let y = pair[1].run(mid);
                *power += y * y;
            }
            self.in_hop += 1;
            if self.in_hop >= self.cfg.hop.max(1) {
                self.finish_hop();
                completed = true;
            }
        }
        completed
    }

    fn finish_hop(&mut self) {
        let n = self.in_hop as f64;
        let hop_s = n / self.cfg.sample_rate as f64;
        let attack = 1.0 - (-hop_s / self.cfg.attack_s.max(1e-4) as f64).exp();
        let release = 1.0 - (-hop_s / self.cfg.release_s.max(1e-4) as f64).exp();
        let floor = self.cfg.floor_db as f64;
        for (band, power) in self.bands.iter_mut().zip(self.power.iter_mut()) {
            follow(band, unit_from_power(*power / n, floor), attack, release);
            *power = 0.0;
        }
        follow(
            &mut self.level,
            unit_from_power(self.broadband / n, floor),
            attack,
            release,
        );
        self.broadband = 0.0;
        self.in_hop = 0;
        let total: f32 = self.bands.iter().sum();
        // Below this the centroid is noise; hold the last value.
        if total > 1e-3 {
            let weighted: f32 = self
                .bands
                .iter()
                .enumerate()
                .map(|(k, b)| k as f32 * b)
                .sum();
            self.centroid = weighted / ((BAND_COUNT - 1) as f32 * total);
        }
    }

    pub fn bands(&self) -> [f32; BAND_COUNT] {
        self.bands
    }

    /// Broadband level through the same dB mapping as the bands.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Energy-weighted mean band index over [0, 1]: 0 is all bass, 1
    /// all treble.
    pub fn centroid(&self) -> f32 {
        self.centroid
    }
}

/// One-pole step toward `x` with separate rise and fall coefficients.
fn follow(y: &mut f32, x: f32, attack: f64, release: f64) {
    let a = if x > *y { attack } else { release };
    *y += (a * (x - *y) as f64) as f32;
}

/// Mean power to [0, 1]: 0 dB is a full-scale sine (mean power one
/// half), `floor_db` and below are 0.
fn unit_from_power(mean_power: f64, floor_db: f64) -> f32 {
    let db = 10.0 * (mean_power / 0.5 + 1e-12).log10();
    ((db - floor_db) / -floor_db).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, sample_rate: f32, seconds: f32) -> Vec<f32> {
        let n = (sample_rate * seconds) as usize;
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin())
            .collect()
    }

    fn analysed(freq: f32, cfg: Config) -> BandAnalyser {
        let mut a = BandAnalyser::new(cfg);
        let rate = a.config().sample_rate;
        assert!(
            a.push(&sine(freq, rate, 0.5)),
            "half a second completes hops"
        );
        a
    }

    fn argmax(bands: &[f32; BAND_COUNT]) -> usize {
        bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(k, _)| k)
            .unwrap()
    }

    // 32 * ln(1000 / 80) / ln(100) = 17.55, so 1 kHz sits in band 17.
    const KHZ_BAND: usize = 17;

    #[test]
    fn band_edges_are_strictly_increasing_and_span_the_configured_range() {
        let cfg = Config::default();
        let mut last = 0.0;
        for k in 0..BAND_COUNT {
            let lo = band_edge_hz(&cfg, k);
            let c = band_centre_hz(&cfg, k);
            let hi = band_edge_hz(&cfg, k + 1);
            assert!(lo > last && lo < c && c < hi, "band {k} is out of order");
            last = lo;
        }
        assert!(
            (band_edge_hz(&cfg, 0) - 80.0).abs() < 1e-3,
            "bottom edge is 80 Hz"
        );
        assert!(
            (band_edge_hz(&cfg, BAND_COUNT) - 8000.0).abs() < 1e-2,
            "top edge is 8 kHz"
        );
    }

    #[test]
    fn a_full_scale_one_kilohertz_sine_peaks_in_its_band_near_one() {
        let a = analysed(1000.0, Config::default());
        let bands = a.bands();
        assert_eq!(argmax(&bands), KHZ_BAND, "1 kHz should peak in band 17");
        assert!(
            bands[KHZ_BAND] >= 0.8,
            "band 17 should be near full, got {}",
            bands[KHZ_BAND]
        );
        assert!(bands[0] < 0.05, "band 0 should be dark, got {}", bands[0]);
        assert!(
            bands[31] < 0.05,
            "band 31 should be dark, got {}",
            bands[31]
        );
        assert!(
            a.level() >= 0.8,
            "level should read near full, got {}",
            a.level()
        );
        let c = a.centroid();
        assert!(
            (0.5..=0.62).contains(&c),
            "centroid should sit near band 17, got {c}"
        );
    }

    #[test]
    fn silence_reads_as_zero_everywhere() {
        let mut a = BandAnalyser::new(Config::default());
        a.push(&vec![0.0; 48_000]);
        assert!(a.bands().iter().all(|&b| b == 0.0), "bands should be zero");
        assert_eq!(a.level(), 0.0, "level should be zero");
    }

    #[test]
    fn a_tone_an_octave_above_the_top_band_leaves_every_band_dark() {
        let a = analysed(16_000.0, Config::default());
        let worst = a.bands().iter().cloned().fold(0.0, f32::max);
        assert!(
            worst < 0.1,
            "16 kHz should not light any band, worst {worst}"
        );
    }

    #[test]
    fn the_band_a_tone_lands_in_does_not_depend_on_the_sample_rate() {
        let cfg = Config {
            sample_rate: 24_000.0,
            ..Config::default()
        };
        let a = analysed(1000.0, cfg);
        assert_eq!(
            argmax(&a.bands()),
            KHZ_BAND,
            "1 kHz at 24 kHz should still peak in band 17"
        );
    }

    #[test]
    fn the_release_is_slower_than_the_attack() {
        let mut a = BandAnalyser::new(Config::default());
        a.push(&sine(1000.0, 48_000.0, 0.5));
        let full = a.bands()[KHZ_BAND];
        a.push(&vec![0.0; 480 * 3]);
        let after_30ms = a.bands()[KHZ_BAND];
        assert!(
            after_30ms > 0.5 * full,
            "30 ms of silence should not drop a 120 ms release below half, got {after_30ms}"
        );
        a.push(&vec![0.0; 48_000]);
        assert!(
            a.bands()[KHZ_BAND] < 1e-3,
            "a second of silence should release fully"
        );
    }
}
