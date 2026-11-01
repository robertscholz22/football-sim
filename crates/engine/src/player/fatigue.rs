//! Fatigue (M6, docs/attributes.md section 4): energy falls with running and recovers while the player walks or
//! jogs; the long-term reserve, its ceiling, falls with all the work of the match. Stamina slows both. "Fatigue
//! occurred towards the end of matches as well as temporarily during the game" [MOHR03]: the reserve gives the first,
//! the short-term energy the second.

use super::{Player, norm};
use crate::config::PlayerConfig;

/// Advance `p`'s energy and reserve over `dt` at his current speed.
pub fn step(p: &mut Player, cfg: &PlayerConfig, dt: f32) {
    let s = 1.0 + cfg.fatigue_stamina_range * (0.5 - norm(p.attrs.stamina));
    let v = p.speed();
    let work = (v / cfg.fatigue_hi_speed).powi(2);
    p.reserve = (p.reserve - cfg.fatigue_reserve_drain * s * work * v * dt).max(0.0);
    if v > cfg.fatigue_hi_speed {
        p.energy -= cfg.fatigue_hi_drain * s * (work - 1.0) * dt;
    } else {
        p.energy += (p.reserve - p.energy) * dt / cfg.fatigue_recovery_s;
    }
    p.energy = p.energy.clamp(0.0, p.reserve);
}

/// Top speed and acceleration factor at `energy`.
pub fn speed_factor(cfg: &PlayerConfig, energy: f32) -> f32 {
    1.0 - cfg.fatigue_speed_loss * (1.0 - energy)
}
