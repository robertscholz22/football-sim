//! Layer 3 off the ball: deviation from the layer-1+2 target, driven by intelligence, scaled by confidence, energy
//! (the current stamina level) and familiarity with the role of the phase. A player with intelligence 100 at full
//! confidence, energy and familiarity is on the optimum exactly; anything less in any
//! of them produces a slow Ornstein–Uhlenbeck positional drift (drift, not jitter), noise on positioning utilities
//! and a reaction lag (longer intervals between re-evaluations).

use glam::Vec2;

use crate::config::AiConfig;
use crate::player::{Player, norm};
use crate::rng::SimRng;

/// How much of a mental attribute a player can use in his state: 1 at full `x` (confidence, energy or
/// familiarity), `floor` at none, linear between.
pub fn state_factor(floor: f32, x: f32) -> f32 {
    floor + (1.0 - floor) * x.clamp(0.0, 1.0)
}

/// Tactical reliability 0..1: intelligence (understanding), scaled by confidence, energy and role familiarity
/// (`AiConfig::reliability_*_floor`). 1 only for intelligence 100 at full confidence, energy and familiarity: that
/// player is always on the layer-1+2 optimum.
pub fn reliability(p: &Player, ai: &AiConfig) -> f32 {
    norm(p.attrs.intelligence)
        * state_factor(ai.reliability_confidence_floor, p.confidence)
        * state_factor(ai.reliability_energy_floor, p.energy)
        * state_factor(ai.reliability_familiarity_floor, p.familiarity(p.beliefs.in_possession))
}

/// Stationary σ (m) of the positional drift.
pub fn drift_sigma(p: &Player, ai: &AiConfig) -> f32 {
    ai.drift_sigma_max * (1.0 - reliability(p, ai))
}

/// σ of the noise added to positioning candidate scores.
pub fn noise_sigma(p: &Player, ai: &AiConfig) -> f32 {
    ai.position_noise_max * (1.0 - reliability(p, ai))
}

/// Seconds until the next off-ball re-evaluation (reaction lag): the reliable player re-reads the game at
/// `offball_interval_min`, the unreliable one at `offball_interval_max`.
pub fn decision_interval(p: &Player, ai: &AiConfig, rng: &mut SimRng) -> f32 {
    let base = ai.offball_interval_max + (ai.offball_interval_min - ai.offball_interval_max) * reliability(p, ai);
    base * rng.range(0.85, 1.15)
}

/// Concentration (M6): lapses per second, `lapse_rate` per minute × (1 − `lapse_composure` · composure) ×
/// (1 + `lapse_fatigue` · (1 − energy)).
pub fn lapse_rate(p: &Player, ai: &AiConfig) -> f32 {
    ai.lapse_rate / 60.0 * (1.0 - ai.lapse_composure * norm(p.attrs.composure)) * (1.0 + ai.lapse_fatigue * (1.0 - p.energy))
}

/// Advance the OU drift by `dt` (exact discretisation, stationary σ = `sigma`).
pub fn update_drift(drift: Vec2, dt: f32, sigma: f32, theta: f32, rng: &mut SimRng) -> Vec2 {
    let a = (-theta * dt).exp();
    let s = sigma * (1.0 - a * a).sqrt();
    drift * a + Vec2::new(rng.normal(), rng.normal()) * s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_is_stationary_with_requested_sigma() {
        let mut rng = SimRng::new(3);
        let mut d = Vec2::ZERO;
        let mut sum = 0.0;
        let n = 20000;
        for _ in 0..n {
            d = update_drift(d, 0.4, 2.0, 0.5, &mut rng);
            sum += d.x * d.x;
        }
        let sigma = (sum / n as f32).sqrt();
        assert!((sigma - 2.0).abs() < 0.2, "{sigma}");
    }
}
