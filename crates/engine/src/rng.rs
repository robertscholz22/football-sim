//! Deterministic random numbers. One `SimRng` per match; the same seed must reproduce the same match.

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_distr::StandardNormal;

#[derive(Clone, Debug)]
pub struct SimRng(ChaCha8Rng);

impl SimRng {
    pub fn new(seed: u64) -> Self {
        Self(ChaCha8Rng::seed_from_u64(seed))
    }

    /// Uniform in [0, 1).
    pub fn uniform(&mut self) -> f32 {
        self.0.random::<f32>()
    }

    /// Uniform in [lo, hi).
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.uniform()
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.uniform() < p
    }

    /// Standard normal sample.
    pub fn normal(&mut self) -> f32 {
        self.0.sample(StandardNormal)
    }

    pub fn gauss(&mut self, mean: f32, sigma: f32) -> f32 {
        mean + sigma * self.normal()
    }

    /// Sample an index from unnormalised non-negative weights. Returns 0 if all weights are zero.
    pub fn weighted_index(&mut self, weights: &[f32]) -> usize {
        let total: f32 = weights.iter().sum();
        if total <= 0.0 {
            return 0;
        }
        let mut r = self.uniform() * total;
        for (i, w) in weights.iter().enumerate() {
            if r < *w {
                return i;
            }
            r -= w;
        }
        weights.len() - 1
    }

    /// Uniform in (0, 1), fixed by `seed` and `key`: the same pair always gives the same number.
    pub fn hashed_unit(seed: u64, key: &[u8]) -> f64 {
        // FNV-1a over the key, then the splitmix64 finaliser.
        let mut h = 0xcbf2_9ce4_8422_2325u64 ^ seed;
        for &b in key {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        h ^= h >> 31;
        ((h >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    /// Derive an independent seed (e.g. for the pitch surface field).
    pub fn next_seed(&mut self) -> u64 {
        self.0.random::<u64>()
    }
}
