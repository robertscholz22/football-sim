//! Layer 3 for on-ball decisions: systematic personality biases, kept separate from the optimum so that they can
//! be tuned and inspected. Egotism favours self actions (dribble, shoot, hold) and penalises passes, strongest near
//! goal and when confident; its low end is the team player. Flair favours difficult options (take-ons, through
//! balls, curled shots, ATT-23). Aggression raises risk appetite under pressure.

use crate::config::PersonalityConfig;
use crate::player::{Attributes, norm};

/// Category of an on-ball option, as far as personality is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionKind {
    Pass {
        /// Square or backward pass.
        safe: bool,
        /// Into space ahead of a runner.
        through: bool,
        lofted: bool,
    },
    Shot {
        curled: bool,
    },
    Dribble {
        take_on: bool,
    },
    Hold,
    Clear,
}

impl OptionKind {
    pub fn is_self(self) -> bool {
        matches!(self, OptionKind::Shot { .. } | OptionKind::Dribble { .. } | OptionKind::Hold)
    }
}

/// Situation inputs for the personality bias.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiasCtx {
    /// 0 far from goal .. 1 at the edge of the box or closer.
    pub near_goal: f32,
    pub pressure: f32,
    pub confidence: f32,
    /// Success probability and turnover cost of the option (for risk appetite).
    pub p: f32,
    pub cost: f32,
    /// The pass goes to a teammate who is better placed than the carrier.
    pub better_placed: bool,
}

/// Personality bias (value units) for an option. Egotism and flair are fractions of the local value `scale`, on a
/// shot of the shot's own value (`ctx.p`, its xG); the aggression term discounts the option's own turnover cost.
pub fn bias(a: &Attributes, kind: OptionKind, ctx: &BiasCtx, scale: f32, cfg: &PersonalityConfig) -> f32 {
    let ego = norm(a.egotism) * (0.6 + 0.8 * ctx.near_goal) * (0.7 + 0.6 * ctx.confidence);
    let flair = norm(a.flair) * cfg.flair_bonus;
    let mut b = match kind {
        // Egotists overrate their own chance and so shoot from worse positions. As a share of the local scale, two to
        // three times a long shot's xG at 25 m, the bonus was half a long shot's worth even at egotism 28 and doubled
        // the shot share at 22–30 m; as a share of the shot's xG it grows towards goal, where shots are good.
        OptionKind::Shot { curled } => (cfg.egotism_shot_share * ego + if curled { flair } else { 0.0 }) * ctx.p,
        OptionKind::Dribble { take_on } => (ego * cfg.egotism_self_bonus + if take_on { flair } else { 0.0 }) * scale,
        OptionKind::Hold => 0.5 * ego * cfg.egotism_self_bonus * scale,
        OptionKind::Pass { safe, through, lofted } => {
            let factor = if safe || ctx.better_placed { 2.0 } else { 1.0 };
            (-ego * cfg.egotism_pass_penalty * factor
                + if through { 0.6 * flair } else { 0.0 }
                + if lofted { 0.3 * flair } else { 0.0 })
                * scale
        }
        OptionKind::Clear => 0.0,
    };
    // Aggression: under pressure, part of the turnover cost is ignored.
    b += cfg.aggression_risk_reduction * norm(a.aggression) * ctx.pressure * (1.0 - ctx.p) * ctx.cost;
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> BiasCtx {
        BiasCtx { near_goal: 1.0, pressure: 0.3, confidence: 0.6, p: 0.8, cost: 0.02, better_placed: true }
    }

    #[test]
    fn egotist_prefers_self_actions() {
        let cfg = PersonalityConfig::default();
        let mut ego = Attributes::uniform(50.0);
        ego.egotism = 100.0;
        let mut team = ego.clone();
        team.egotism = 30.0;
        let pass = OptionKind::Pass { safe: false, through: false, lofted: false };
        let shot = OptionKind::Shot { curled: false };
        let gap = |a: &Attributes| bias(a, shot, &ctx(), 0.02, &cfg) - bias(a, pass, &ctx(), 0.02, &cfg);
        assert!(gap(&ego) > gap(&team));
        let mut humble = Attributes::uniform(50.0);
        humble.egotism = 0.0;
        assert!(gap(&ego) > gap(&humble));
    }
}
