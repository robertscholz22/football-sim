//! Event-driven confidence (M6, docs/attributes.md section 4). Each event nudges the player's confidence by a small
//! calibrated step, damped by composure ("stability"); conceding a goal nudges the whole team. Between events it
//! returns to the pre-match level with time constant `conf_recovery_s`, faster below it when a leader is near
//! ("lift"). The effects are kept small: goals behave as independent Poisson processes without a self-affirmative
//! effect [HEUER10; HEUER12], a goal just before half-time changes nothing measurable [GAURIOT18], and conceding
//! lowers individual passing for ≈ 3–9 minutes while scoring does not change it [HORRILLO23].

use super::{Player, norm};
use crate::config::PlayerConfig;

/// Nudge `p`'s confidence by `delta`, damped by composure: × (1 − `conf_composure_damp` · composure).
pub fn nudge(p: &mut Player, cfg: &PlayerConfig, delta: f32) {
    let stability = 1.0 - cfg.conf_composure_damp * norm(p.attrs.composure);
    p.confidence = (p.confidence + delta * stability).clamp(0.0, 1.0);
}

/// Return towards the pre-match level over `dt`. Below it, `lead` (the leadership, 0..100, of the strongest team-mate
/// within `conf_lift_radius`) shortens the time constant by 1 + `conf_lift` · leadership.
pub fn step(p: &mut Player, cfg: &PlayerConfig, lead: f32, dt: f32) {
    let lift = if p.confidence < p.base_confidence { 1.0 + cfg.conf_lift * norm(lead) } else { 1.0 };
    p.confidence += (p.base_confidence - p.confidence) * (1.0 - (-dt * lift / cfg.conf_recovery_s).exp());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::{PitchConditions, Side};
    use crate::player::{Attributes, Kinematics};
    use crate::team::{PlayerInstructions, Role};

    fn player(composure: f32) -> Player {
        let cfg = PlayerConfig::default();
        let attrs = Attributes { composure, ..Attributes::default() };
        let kin = Kinematics::new(&attrs, &cfg, &PitchConditions::default(), 1.0);
        Player::new(0, Side::Home, 0, [Role::CB; 2], String::new(), 1, attrs, kin, PlayerInstructions::default(), 0.5, [1.0; 2])
    }

    #[test]
    fn composure_damps_swings_and_a_leader_speeds_recovery() {
        let cfg = PlayerConfig::default();
        let (mut calm, mut nervy) = (player(90.0), player(20.0));
        nudge(&mut calm, &cfg, cfg.conf_conceded);
        nudge(&mut nervy, &cfg, cfg.conf_conceded);
        assert!(nervy.confidence < calm.confidence && calm.confidence < 0.5);
        let (mut led, mut alone) = (nervy.clone(), nervy.clone());
        for _ in 0..300 {
            step(&mut led, &cfg, 90.0, 1.0);
            step(&mut alone, &cfg, 0.0, 1.0);
        }
        assert!(alone.confidence < led.confidence && led.confidence < 0.5);
        let mut up = player(50.0);
        up.confidence = 0.6;
        step(&mut up, &cfg, 90.0, cfg.conf_recovery_s);
        assert!((up.confidence - (0.5 + 0.1 / std::f32::consts::E)).abs() < 1e-3, "{}", up.confidence);
    }
}
