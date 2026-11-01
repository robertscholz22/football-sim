//! The single place where attributes become effective values and physical capabilities.
//!
//! Modifiers: confidence affects technical and mental attributes, energy (fatigue) affects physical ones.

use super::attributes::{Attributes, norm};
use crate::config::PlayerConfig;
use crate::pitch::PitchConditions;

/// Attribute class, deciding which in-game modifiers apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrClass {
    Physical,
    Technical,
    Mental,
}

/// Effective normalised value (0..1, may slightly exceed 1 with high confidence) of an attribute.
pub fn effective(value: f32, class: AttrClass, confidence: f32, energy: f32) -> f32 {
    let n = norm(value);
    let conf = 0.9 + 0.2 * confidence; // 0.9 .. 1.1
    let fat = 0.85 + 0.15 * energy; // 0.85 .. 1.0
    match class {
        AttrClass::Physical => n * fat,
        AttrClass::Technical => n * conf,
        AttrClass::Mental => n * (0.5 * conf + 0.5) * (0.9 + 0.1 * energy),
    }
}

/// Physical capabilities derived from attributes, conditions and fatigue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kinematics {
    pub top_speed: f32,
    pub accel: f32,
    pub decel: f32,
    pub lateral_accel: f32,
    pub reaction: f32,
    pub max_kick_speed: f32,
    pub tackle_reach: f32,
    /// Height (m) up to which the player can reach the ball with the head when jumping.
    pub jump_reach: f32,
    pub gk_dive_reach: f32,
    /// Body turn rate (rad/s) with the ball at the feet (setting up a kick).
    pub ball_turn_rate: f32,
}

impl Kinematics {
    pub fn new(a: &Attributes, cfg: &PlayerConfig, cond: &PitchConditions, energy: f32) -> Self {
        let phys = |v: f32| effective(v, AttrClass::Physical, 0.5, energy);
        let lerp = |lo: f32, hi: f32, t: f32| lo + (hi - lo) * t;
        // Fatigue takes speed and acceleration down directly (`fatigue::speed_factor`), not through the rating.
        let tired = super::fatigue::speed_factor(cfg, energy);
        let traction = 1.0 - cfg.wet_traction_loss * cond.wetness;
        let height_m = a.height_cm / 100.0;
        let fresh = |v: f32| effective(v, AttrClass::Physical, 0.5, 1.0);
        let quick = phys(a.quickness);
        let quick_fresh = fresh(a.quickness);
        Self {
            top_speed: lerp(cfg.top_speed_min, cfg.top_speed_max, fresh(a.speed)) * tired,
            accel: lerp(cfg.accel_min, cfg.accel_max, quick_fresh) * traction * tired,
            decel: lerp(cfg.decel_min, cfg.decel_max, quick_fresh) * traction,
            lateral_accel: lerp(cfg.lateral_accel_min, cfg.lateral_accel_max, quick_fresh) * traction * tired,
            reaction: lerp(cfg.reaction_max, cfg.reaction_min, norm(a.instinct)),
            // Kick power: strength with the better of the two long-range striking skills.
            max_kick_speed: lerp(
                cfg.kick_speed_min,
                cfg.kick_speed_max,
                0.5 * phys(a.strength) + 0.5 * norm(a.long_passing.max(a.long_shots)),
            ),
            tackle_reach: lerp(cfg.tackle_reach_min, cfg.tackle_reach_max, norm(a.tackling)),
            jump_reach: height_m * 1.25 + 0.25 + 0.5 * phys(a.jumping),
            gk_dive_reach: lerp(cfg.gk_dive_reach_min, cfg.gk_dive_reach_max, 0.5 * quick + 0.5 * norm(a.reflexes))
                * (height_m / 1.85),
            ball_turn_rate: lerp(cfg.ball_turn_rate_min, cfg.ball_turn_rate_max, 0.5 * quick + 0.5 * norm(a.dribbling)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn better_attributes_are_faster() {
        let cfg = PlayerConfig::default();
        let cond = PitchConditions::default();
        let slow = Kinematics::new(&Attributes::uniform(20.0), &cfg, &cond, 1.0);
        let fast = Kinematics::new(&Attributes::uniform(90.0), &cfg, &cond, 1.0);
        assert!(fast.top_speed > slow.top_speed);
        assert!(fast.reaction < slow.reaction);
        let tired = Kinematics::new(&Attributes::uniform(90.0), &cfg, &cond, 0.2);
        assert!(tired.top_speed < fast.top_speed);
    }
}
