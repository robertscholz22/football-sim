//! Logistic duel models: tackle vs take-on, first touch / control, goalkeeper save.
//! Each is a logistic function of effective-skill differences plus situational terms, so the probability is
//! 0.5 at equal skill and monotonic in the skill gap. The one-sided duels (control, save) separate the slope in the
//! situation's difficulty from the slope in the player's skill around `DuelConfig::skill_pivot`, the reference
//! player the difficulty terms were calibrated with, so the skill slope can be set without moving the baseline.

use crate::config::{DuelConfig, PlayerConfig};
use crate::player::{AttrClass, Attributes, Player, effective, norm};
use crate::rng::SimRng;

pub fn logistic(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn eff(p: &Player, v: f32, class: AttrClass) -> f32 {
    effective(v, class, p.confidence, p.energy)
}

/// Strength in a challenge (aerial or on the ground; 0..1): strength plus `weight_strength_per_kg` rating points per
/// kg of body weight above `weight_ref_kg`. Weight plays no other part.
pub fn challenge_strength(p: &Player, cfg: &DuelConfig) -> f32 {
    let a = &p.attrs;
    eff(p, a.strength + cfg.weight_strength_per_kg * (a.weight_kg - cfg.weight_ref_kg), AttrClass::Physical)
}

/// Defender's tackling skill (0..1). A keeper smothering at the carrier's feet uses his one-on-ones.
pub fn tackle_skill(p: &Player, cfg: &DuelConfig) -> f32 {
    let a = &p.attrs;
    let tackling = if p.is_gk() { a.one_on_ones } else { a.tackling };
    0.5 * eff(p, tackling, AttrClass::Technical)
        + 0.2 * challenge_strength(p, cfg)
        + 0.15 * eff(p, a.instinct, AttrClass::Mental)
        + 0.15 * eff(p, a.aggression, AttrClass::Mental)
}

/// Ball carrier's skill at keeping the ball against a tackle (0..1).
pub fn take_on_skill(p: &Player, cfg: &DuelConfig) -> f32 {
    let a = &p.attrs;
    0.55 * eff(p, a.dribbling, AttrClass::Technical) + 0.2 * eff(p, a.quickness, AttrClass::Physical) + 0.25 * challenge_strength(p, cfg)
}

/// Probability that `defender` wins the ball from `carrier`.
pub fn tackle_win_prob(defender: &Player, carrier: &Player, from_behind: bool, cfg: &DuelConfig) -> f32 {
    let mut x = cfg.tackle_slope * (tackle_skill(defender, cfg) - take_on_skill(carrier, cfg));
    if from_behind {
        x -= cfg.tackle_from_behind_penalty;
    }
    logistic(x)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TackleOutcome {
    /// Defender wins the ball cleanly.
    Won,
    /// Ball poked loose.
    Deflected,
    /// Defender beaten.
    Beaten,
}

pub fn resolve_tackle(p_win: f32, cfg: &DuelConfig, rng: &mut SimRng) -> TackleOutcome {
    if rng.chance(p_win) {
        if rng.chance(cfg.tackle_clean_share) { TackleOutcome::Won } else { TackleOutcome::Deflected }
    } else {
        TackleOutcome::Beaten
    }
}

/// Skill at controlling an incoming ball (0..1).
pub fn control_skill(a: &Attributes, confidence: f32, energy: f32) -> f32 {
    0.85 * effective(a.first_touch, AttrClass::Technical, confidence, energy)
        + 0.15 * effective(a.composure, AttrClass::Mental, confidence, energy)
}

/// Difficulty of controlling a ball arriving with `speed` (m/s, relative to the player) at `height` (m, bottom of
/// ball) under `pressure` (0..1).
pub fn control_difficulty(speed: f32, height: f32, pressure: f32, cfg: &DuelConfig) -> f32 {
    cfg.first_touch_speed_weight * (speed / cfg.first_touch_speed_ref).powi(2) + 0.35 * height + cfg.first_touch_pressure_weight * pressure
}

/// Probability of a clean controlling touch.
pub fn control_prob(a: &Attributes, confidence: f32, energy: f32, speed: f32, height: f32, pressure: f32, cfg: &DuelConfig) -> f32 {
    logistic(
        cfg.first_touch_slope * (cfg.skill_pivot - control_difficulty(speed, height, pressure, cfg) + cfg.first_touch_offset)
            + cfg.first_touch_skill_slope * (control_skill(a, confidence, energy) - cfg.skill_pivot),
    )
}

/// Chance that an outfield opponent with `instinct` gets a touch on a pass running past within reach at `speed`
/// (m/s).
pub fn pass_touch_prob(speed: f32, instinct: f32, pc: &PlayerConfig) -> f32 {
    (pc.pass_touch_base - pc.pass_touch_per_mps * speed).clamp(0.35, 0.95) * (0.75 + 0.25 * norm(instinct))
}

/// Goalkeeper shot-stopping skill (0..1) against a shot struck `shot_dist` m from goal. The closer the shooter, the
/// more it is a direct duel: weight moves from reflexes to one-on-ones between `save_one_on_one_far` and
/// `save_one_on_one_near`.
pub fn save_skill(gk: &Player, shot_dist: f32, cfg: &DuelConfig) -> f32 {
    let a = &gk.attrs;
    let close = ((cfg.save_one_on_one_far - shot_dist) / (cfg.save_one_on_one_far - cfg.save_one_on_one_near)).clamp(0.0, 1.0);
    (0.45 - 0.3 * close) * eff(gk, a.reflexes, AttrClass::Technical)
        + 0.25 * eff(gk, a.handling, AttrClass::Technical)
        + (0.15 + 0.3 * close) * eff(gk, a.one_on_ones, AttrClass::Technical)
        + 0.15 * eff(gk, a.quickness, AttrClass::Physical)
}

/// Probability of saving a shot struck `shot_dist` m from goal. `reach_frac` = 0 for a ball at the keeper's body, 1
/// at the limit of his dive; `reaction_time` = time the keeper had since the shot.
pub fn save_prob(gk: &Player, shot_dist: f32, ball_speed: f32, reach_frac: f32, reaction_time: f32, cfg: &DuelConfig) -> f32 {
    // Little time is hard; extra time beyond half a second (long shots) makes it easier still.
    let time = 1.2 * (0.5 - reaction_time).clamp(0.0, 0.5) - 0.3 * (reaction_time - 0.5).clamp(0.0, 1.0);
    let difficulty = 0.5 * ball_speed / 30.0 + 0.6 * reach_frac.clamp(0.0, 1.0) + time - 0.2;
    logistic(
        cfg.save_slope * (cfg.skill_pivot - difficulty) + cfg.save_skill_slope * (save_skill(gk, shot_dist, cfg) - cfg.skill_pivot) + cfg.save_bias,
    )
}

/// Skill in a contested high ball (0..1), without the reach itself.
pub fn aerial_skill(p: &Player, cfg: &DuelConfig) -> f32 {
    let a = &p.attrs;
    0.4 * eff(p, a.heading, AttrClass::Technical)
        + 0.25 * eff(p, a.jumping, AttrClass::Physical)
        + 0.2 * challenge_strength(p, cfg)
        + 0.15 * eff(p, a.aggression, AttrClass::Mental)
}

/// Probability that `a` wins a contested high ball against `b`: aerial skill and how much higher he reaches.
pub fn aerial_win_prob(a: &Player, b: &Player, cfg: &DuelConfig) -> f32 {
    logistic(cfg.aerial_slope * (aerial_skill(a, cfg) - aerial_skill(b, cfg)) + cfg.aerial_reach_per_m * (a.kin.jump_reach - b.kin.jump_reach))
}

/// Probability of holding (catching) a saved shot rather than parrying it.
pub fn catch_prob(gk: &Player, ball_speed: f32) -> f32 {
    let h = norm(gk.attrs.handling);
    ((0.4 + 0.5 * h) * (1.0 - (ball_speed - 12.0).max(0.0) / 25.0)).clamp(0.05, 0.95)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::pitch::Side;
    use crate::scenario::make_player;

    #[test]
    fn tackle_is_even_at_equal_skill_and_monotonic() {
        let cfg = SimConfig::default();
        let att = make_player(0, Side::Home, Attributes::uniform(58.0), &cfg);
        let even = make_player(11, Side::Away, Attributes::uniform(58.0), &cfg);
        let p = tackle_win_prob(&even, &att, false, &cfg.duel);
        assert!((p - 0.5).abs() < 1e-4, "{p}");
        let mut last = 0.0;
        for v in [5.0, 25.0, 50.0, 70.0, 90.0] {
            let mut a = Attributes::uniform(58.0);
            a.tackling = v;
            let d = make_player(11, Side::Away, a, &cfg);
            let p = tackle_win_prob(&d, &att, false, &cfg.duel);
            assert!(p > last);
            last = p;
        }
        assert!(tackle_win_prob(&even, &att, true, &cfg.duel) < 0.5);
    }

    #[test]
    fn control_gets_harder_with_speed_and_easier_with_skill() {
        let cfg = DuelConfig::default();
        let a = Attributes::uniform(47.0);
        assert!(control_prob(&a, 0.5, 1.0, 20.0, 0.0, 0.0, &cfg) < control_prob(&a, 0.5, 1.0, 8.0, 0.0, 0.0, &cfg));
        let good = Attributes::uniform(90.0);
        assert!(control_prob(&good, 0.5, 1.0, 12.0, 0.0, 0.5, &cfg) > control_prob(&a, 0.5, 1.0, 12.0, 0.0, 0.5, &cfg));
    }

    #[test]
    fn weight_counts_in_challenges_only() {
        let cfg = SimConfig::default();
        let heavy_attrs = Attributes { weight_kg: 90.0, ..Attributes::uniform(58.0) };
        let light_attrs = Attributes { weight_kg: 65.0, ..Attributes::uniform(58.0) };
        let heavy = make_player(1, Side::Home, heavy_attrs.clone(), &cfg);
        let light = make_player(12, Side::Away, light_attrs.clone(), &cfg);
        assert!(aerial_win_prob(&heavy, &light, &cfg.duel) > 0.5);
        assert!(tackle_win_prob(&heavy, &light, false, &cfg.duel) > tackle_win_prob(&light, &heavy, false, &cfg.duel));
        // Locomotion does not see it.
        assert_eq!(heavy.kin, light.kin);
    }

    #[test]
    fn keeper_duels_use_one_on_ones() {
        let cfg = SimConfig::default();
        let carrier = make_player(1, Side::Home, Attributes::uniform(58.0), &cfg);
        let keeper = |one_on_ones: f32| {
            let mut g = make_player(11, Side::Away, Attributes { one_on_ones, ..Attributes::uniform(58.0) }, &cfg);
            g.role = crate::team::Role::GK;
            g
        };
        let (poor, good) = (keeper(20.0), keeper(90.0));
        assert!(tackle_win_prob(&good, &carrier, false, &cfg.duel) > tackle_win_prob(&poor, &carrier, false, &cfg.duel));
        // A close shot is a direct duel; from distance one-on-ones count far less.
        let gain = |d: f32| save_skill(&good, d, &cfg.duel) - save_skill(&poor, d, &cfg.duel);
        assert!(gain(6.0) > 2.5 * gain(25.0), "{} {}", gain(6.0), gain(25.0));
    }
}
