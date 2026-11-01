//! Seeded "bumpiness" field of the playing surface.
//!
//! A smooth value-noise height field. Its gradient tilts bounce normals and pushes rolling balls sideways;
//! its amplitude scales with `PitchConditions::unevenness` (0 = perfectly flat).

use glam::Vec2;

#[derive(Clone, Debug)]
pub struct SurfaceField {
    seed: u64,
    /// Size of one noise cell in metres.
    cell: f32,
}

impl SurfaceField {
    pub fn new(seed: u64) -> Self {
        Self { seed, cell: 1.6 }
    }

    fn lattice(&self, ix: i32, iy: i32) -> f32 {
        // SplitMix64-style hash of the lattice coordinates.
        let mut h = self.seed ^ ((ix as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ ((iy as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 31;
        (h >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    }

    /// Normalised height in [−1, 1] at a ground position.
    pub fn height(&self, p: Vec2) -> f32 {
        let q = p / self.cell;
        let (ix, iy) = (q.x.floor() as i32, q.y.floor() as i32);
        let (fx, fy) = (q.x - ix as f32, q.y - iy as f32);
        let s = |t: f32| t * t * (3.0 - 2.0 * t);
        let (sx, sy) = (s(fx), s(fy));
        let a = self.lattice(ix, iy);
        let b = self.lattice(ix + 1, iy);
        let c = self.lattice(ix, iy + 1);
        let d = self.lattice(ix + 1, iy + 1);
        let top = a + (b - a) * sx;
        let bottom = c + (d - c) * sx;
        top + (bottom - top) * sy
    }

    /// Normalised slope (gradient of `height` per cell); magnitude roughly ≤ 1.5.
    pub fn slope(&self, p: Vec2) -> Vec2 {
        let e = 0.1;
        let dx = self.height(p + Vec2::new(e, 0.0)) - self.height(p - Vec2::new(e, 0.0));
        let dy = self.height(p + Vec2::new(0.0, e)) - self.height(p - Vec2::new(0.0, e));
        Vec2::new(dx, dy) * (self.cell / (2.0 * e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_bounded() {
        let a = SurfaceField::new(7);
        let b = SurfaceField::new(7);
        for i in 0..200 {
            let p = Vec2::new(i as f32 * 0.37 - 40.0, i as f32 * 0.21 - 20.0);
            assert_eq!(a.height(p), b.height(p));
            assert!(a.height(p).abs() <= 1.0);
        }
    }
}
