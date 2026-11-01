//! Execution error: turns the intended kick into the actual kick. Normal errors on aim, power, elevation and spin,
//! scaled by the kick's own skill, weak foot, first-time play, pressure (damped by composure), fatigue, confidence
//! and, for shots, how hard the ball is struck, plus a small heavy-tailed "shank" for low skill. The physics then
//! decides the outcome.

use glam::{Vec2, Vec3};

use crate::ball::trajectory::KickSolution;
use crate::config::ExecutionConfig;
use crate::player::{AttrClass, Attributes, Foot, KickKind, Player, effective, norm};
use crate::rng::SimRng;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExecInput {
    pub kind: KickKind,
    /// Effective skill for this kick (0..1).
    pub skill: f32,
    pub weak_foot: bool,
    /// Normalised weak-foot ability (0..1).
    pub weak_foot_ability: f32,
    pub first_time: bool,
    pub pressure: f32,
    pub composure: f32,
    pub energy: f32,
    pub confidence: f32,
    /// How far (rad) the body is still turned beyond the set-up angle when the kick is struck.
    pub body_excess: f32,
    /// Kick speed as a fraction of the kicker's maximum (0 = not known: no speed term).
    pub speed_frac: f32,
}

impl ExecInput {
    /// Build the input for player attributes `a`, kicking with `foot` (`natural_foot` gives the usual one). `range`:
    /// the pass length, or for a shot the distance to goal (m); `cross`: a pass that is a cross (`pitch::is_cross`).
    #[allow(clippy::too_many_arguments)]
    pub fn for_player(
        a: &Attributes,
        kind: KickKind,
        range: f32,
        cross: bool,
        foot: Foot,
        first_time: bool,
        pressure: f32,
        confidence: f32,
        energy: f32,
        cfg: &ExecutionConfig,
    ) -> Self {
        let t = |v: f32| effective(v, AttrClass::Technical, confidence, energy);
        // Between `near_m` and `far_m` the skill blends from `near` to `far`.
        let blend = |near: f32, far: f32, near_m: f32, far_m: f32| {
            let w = ((range - near_m) / (far_m - near_m)).clamp(0.0, 1.0);
            (1.0 - w) * t(near) + w * t(far)
        };
        let skill = match kind {
            KickKind::GroundPass | KickKind::LoftedPass if cross => t(a.crossing),
            KickKind::GroundPass => blend(a.short_passing, a.long_passing, cfg.short_pass_full_m, cfg.long_pass_full_m),
            KickKind::LoftedPass | KickKind::Clearance => t(a.long_passing),
            KickKind::Shot => blend(a.finishing, a.long_shots, cfg.finishing_full_m, cfg.long_shots_full_m),
        };
        let weak_foot = foot != a.preferred_foot;
        Self {
            kind,
            skill: skill.clamp(0.0, 1.1),
            weak_foot,
            weak_foot_ability: norm(a.weak_foot),
            first_time,
            pressure,
            composure: norm(a.composure),
            energy,
            confidence,
            body_excess: 0.0,
            speed_frac: 0.0,
        }
    }

    /// Overall error multiplier.
    pub fn scale(&self, cfg: &ExecutionConfig) -> f32 {
        let mut s = cfg.error_scale_worst + (cfg.error_scale_best - cfg.error_scale_worst) * self.skill.min(1.0);
        if self.weak_foot {
            s *= 1.0 + (cfg.weak_foot_max_factor - 1.0) * (1.0 - self.weak_foot_ability);
        }
        if self.first_time {
            s *= cfg.first_time_factor;
        }
        s *= 1.0 + cfg.body_angle_factor * self.body_excess;
        s *= 1.0 + cfg.pressure_factor * self.pressure * (1.0 - self.composure);
        s *= 1.0 + 0.3 * (1.0 - self.energy);
        s *= 1.2 - 0.4 * self.confidence;
        // Speed–accuracy: shot dispersion grows as a power of ball speed (Hunter et al. 2018).
        if self.kind == KickKind::Shot && self.speed_frac > 0.0 {
            s *= (self.speed_frac / cfg.shot_speed_ref).powf(cfg.shot_speed_exp);
        }
        s
    }
}

/// Kick set-up: the largest angle (rad) between body and kick direction at which a kick of `speed` can be struck.
/// Soft passes can be played across the body with the inside of the foot; driven balls and shots need the body square.
pub fn setup_angle(speed: f32, max_speed: f32, cfg: &ExecutionConfig) -> f32 {
    let f = (speed / max_speed.max(1.0)).clamp(0.0, 1.0);
    (cfg.setup_angle_soft_deg + (cfg.setup_angle_hard_deg - cfg.setup_angle_soft_deg) * f).to_radians()
}

/// How far (rad) a foot can kick a ball of `speed` towards its own side (`foot_outside_soft_deg` →
/// `foot_outside_hard_deg`).
pub fn foot_outside(speed: f32, max_speed: f32, cfg: &ExecutionConfig) -> f32 {
    let f = (speed / max_speed.max(1.0)).clamp(0.0, 1.0);
    (cfg.foot_outside_soft_deg + (cfg.foot_outside_hard_deg - cfg.foot_outside_soft_deg) * f).to_radians()
}

/// The foot that strikes a kick of `speed` in world direction `dir` while facing `facing`. A foot swings across the
/// body: the right foot plays to the left, but only `foot_outside` to its own side, and vice versa. Within that of
/// straight ahead either foot can, so the preferred one does.
pub fn natural_foot(preferred: Foot, facing: Vec2, dir: Vec2, speed: f32, max_speed: f32, cfg: &ExecutionConfig) -> Foot {
    let a = facing.angle_to(dir);
    if a.abs() <= foot_outside(speed, max_speed, cfg) {
        preferred
    } else if a > 0.0 {
        Foot::Right
    } else {
        Foot::Left
    }
}

/// Body-to-kick angles (rad, + = kick to the left of the body) at which `foot` can strike a kick of `speed`: up to the
/// set-up angle across the body, but only `foot_outside` to its own side. `None`: either foot.
pub fn foot_cone(foot: Option<Foot>, speed: f32, max_speed: f32, cfg: &ExecutionConfig) -> (f32, f32) {
    let s = setup_angle(speed, max_speed, cfg);
    let o = s.min(foot_outside(speed, max_speed, cfg));
    match foot {
        None => (-s, s),
        Some(Foot::Right) => (-o, s),
        Some(Foot::Left) => (-s, o),
    }
}

/// How far (rad) the body must still turn before `foot` can strike a kick of `speed` in world direction `dir`.
pub fn setup_turn(facing: Vec2, dir: Vec2, foot: Option<Foot>, speed: f32, max_speed: f32, cfg: &ExecutionConfig) -> f32 {
    let (lo, hi) = foot_cone(foot, speed, max_speed, cfg);
    let a = facing.angle_to(dir);
    (lo - a).max(a - hi).max(0.0)
}

/// Whether `p` can strike a kick of `speed` in world direction `dir` with `foot` now: body turned far enough towards
/// it and not running away from it.
pub fn set_up(p: &Player, dir: Vec2, speed: f32, foot: Option<Foot>, cfg: &ExecutionConfig) -> bool {
    setup_turn(p.facing, dir, foot, speed, p.kin.max_kick_speed, cfg) == 0.0 && p.vel.dot(dir) >= -cfg.setup_back_speed
}

/// Time (s) `p` needs to set up a kick of `speed` in world direction `dir` with `foot`: turning his body with the ball
/// at his feet, and braking first if he is running away from the kick direction.
pub fn setup_time(p: &Player, dir: Vec2, speed: f32, foot: Option<Foot>, cfg: &ExecutionConfig) -> f32 {
    let turn = setup_turn(p.facing, dir, foot, speed, p.kin.max_kick_speed, cfg);
    let brake = (-p.vel.dot(dir) - cfg.setup_back_speed).max(0.0);
    (turn / p.kin.ball_turn_rate).max(brake / p.kin.decel)
}

/// One-σ aim error (rad) of a kick of `kind` before scaling.
fn aim_sigma(kind: KickKind, cfg: &ExecutionConfig) -> f32 {
    match kind {
        KickKind::GroundPass => cfg.angle_ground,
        KickKind::LoftedPass | KickKind::Clearance => cfg.angle_lofted,
        KickKind::Shot => cfg.angle_shot,
    }
}

/// One-σ landing error (m) of the lofted kick `sol` over a horizontal `range` (m), along and across its direction:
/// the speed, elevation and aim errors of `perturb` carried through the solver's landing distance per m/s
/// (`KickSolution::range_per_speed`, drag and spin included). The elevation term takes the drag-free ratio of the two
/// sensitivities (∂R/∂θ = v ∂R/∂v · cot 2θ). Shanks are left out.
pub fn landing_spread(inp: &ExecInput, cfg: &ExecutionConfig, sol: &KickSolution, range: f32) -> Vec2 {
    let s = inp.scale(cfg);
    let elev = sol.vel.z.atan2(sol.vel.truncate().length().max(1e-3));
    let per_rel_speed = sol.range_per_speed.max(0.0) * sol.vel.length();
    let cot = ((2.0 * elev).cos() / (2.0 * elev).sin().max(0.05)).abs();
    let along = per_rel_speed * (cfg.speed_rel * s).hypot(cot * cfg.elevation * s);
    Vec2::new(along, range * aim_sigma(inp.kind, cfg) * s)
}

/// Apply execution error to an ideal kick. Returns the actual (velocity, spin) and the yaw error (rad).
pub fn perturb(vel: Vec3, spin: Vec3, inp: &ExecInput, cfg: &ExecutionConfig, rng: &mut SimRng) -> (Vec3, Vec3, f32) {
    let base = aim_sigma(inp.kind, cfg);
    let mut scale = inp.scale(cfg);
    if rng.chance(cfg.shank_probability * (1.0 - inp.skill.min(1.0))) {
        scale *= cfg.shank_factor;
    }
    let yaw = rng.gauss(0.0, base * scale);
    let speed_f = (1.0 + rng.gauss(0.0, cfg.speed_rel * scale)).clamp(0.4, 1.4);
    let h = vel.truncate();
    let hs = h.length();
    let elev = vel.z.atan2(hs.max(1e-3));
    let elev_err = match inp.kind {
        KickKind::GroundPass => 0.0,
        KickKind::Shot => rng.gauss(0.0, cfg.elevation_shot * scale),
        _ => rng.gauss(0.0, cfg.elevation * scale),
    };
    let new_elev = (elev + elev_err).max(0.0);
    let speed = vel.length() * speed_f;
    let dir = Vec2::from_angle(yaw).rotate(if hs > 1e-3 { h / hs } else { Vec2::X });
    let new_vel = (dir * new_elev.cos()).extend(new_elev.sin()) * speed;
    let spin_f = 1.0 + rng.gauss(0.0, cfg.spin_rel * scale);
    // Rotate the spin with the kick direction so curl stays relative to travel.
    let spin_h = Vec2::from_angle(yaw).rotate(spin.truncate());
    let new_spin = spin_h.extend(spin.z) * spin_f;
    (new_vel, new_spin, yaw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mean_abs_yaw(skill_attr: f32, weak: bool, seed: u64) -> f32 {
        let cfg = ExecutionConfig::default();
        let mut a = Attributes::uniform(skill_attr);
        a.weak_foot = 21.0;
        let mut inp = ExecInput::for_player(&a, KickKind::GroundPass, 10.0, false, Foot::Right, false, 0.0, 0.5, 1.0, &cfg);
        inp.weak_foot = weak;
        let mut rng = SimRng::new(seed);
        let n = 5000;
        let mut sum = 0.0;
        for _ in 0..n {
            let (_, _, yaw) = perturb(Vec3::new(15.0, 0.0, 0.0), Vec3::ZERO, &inp, &cfg, &mut rng);
            sum += yaw.abs();
        }
        sum / n as f32
    }

    #[test]
    fn error_falls_with_skill_and_rises_with_weak_foot() {
        let mut last = f32::MAX;
        for skill in [10.0, 30.0, 50.0, 70.0, 95.0] {
            let e = mean_abs_yaw(skill, false, 11);
            assert!(e < last, "skill {skill}: {e} !< {last}");
            last = e;
        }
        assert!(mean_abs_yaw(58.0, true, 11) > mean_abs_yaw(58.0, false, 11));
    }

    #[test]
    fn harder_shots_are_less_accurate() {
        let cfg = ExecutionConfig::default();
        let a = Attributes::uniform(58.0);
        let spread = |speed_frac: f32| {
            let mut inp = ExecInput::for_player(&a, KickKind::Shot, 16.0, false, Foot::Right, false, 0.0, 0.5, 1.0, &cfg);
            inp.speed_frac = speed_frac;
            let mut rng = SimRng::new(5);
            let (mut yaw_sum, mut elev_sum) = (0.0, 0.0);
            for _ in 0..5000 {
                let (v, _, yaw) = perturb(Vec3::new(25.0, 0.0, 2.0), Vec3::ZERO, &inp, &cfg, &mut rng);
                yaw_sum += yaw.abs();
                elev_sum += (v.z.atan2(v.truncate().length()) - 2f32.atan2(25.0)).abs();
            }
            (yaw_sum, elev_sum)
        };
        let (placed, power) = (spread(0.8), spread(0.97));
        assert!(power.0 > 1.3 * placed.0 && power.1 > 1.3 * placed.1, "placed {placed:?}, power {power:?}");
    }

    /// The predicted landing spread of lofted passes matches the spread of kicks perturbed and flown with the physics.
    #[test]
    fn landing_spread_matches_flown_kicks() {
        use crate::ball::physics::BallParams;
        use crate::ball::trajectory::{KickSpec, predict, solve_kick};
        use crate::ball::{BALL_RADIUS, Ball};
        use crate::config::BallConfig;
        use crate::pitch::PitchConditions;

        let cfg = ExecutionConfig { shank_probability: 0.0, ..Default::default() };
        let params = BallParams::new(&BallConfig::default(), &PitchConditions::default(), 0.005);
        let a = Attributes::uniform(58.0);
        let inp = ExecInput::for_player(&a, KickKind::LoftedPass, 35.0, false, a.preferred_foot, false, 0.0, 0.5, 1.0, &cfg);
        let from = Vec3::new(-30.0, 0.0, BALL_RADIUS);
        let landing = |vel: Vec3, spin: Vec3| {
            let mut ball = Ball::at_rest(from.truncate());
            ball.kick(vel, spin);
            let traj = predict(&ball, &params, 6.0, 0.005);
            let s = traj.samples.windows(2).find(|w| w[0].t > 0.3 && w[0].vel.z < 0.0 && w[1].vel.z >= 0.0).unwrap();
            s[1].pos.truncate()
        };
        let mut rng = SimRng::new(3);
        for d in [20.0, 35.0, 50.0] {
            let target = from.truncate() + Vec2::X * d;
            let spec = KickSpec::Lofted { target, launch_angle: 0.5, backspin: 6.0, sidespin: 0.0 };
            let sol = solve_kick(from, &spec, &params, 40.0).unwrap();
            let aim = landing(sol.vel, sol.spin);
            let n = 400;
            let (mut along, mut across) = (0.0, 0.0);
            for _ in 0..n {
                let (vel, spin, _) = perturb(sol.vel, sol.spin, &inp, &cfg, &mut rng);
                let e = landing(vel, spin) - aim;
                along += e.x * e.x;
                across += e.y * e.y;
            }
            let flown = Vec2::new((along / n as f32).sqrt(), (across / n as f32).sqrt());
            let predicted = landing_spread(&inp, &cfg, &sol, d);
            let ratio = predicted / flown;
            assert!((0.8..1.25).contains(&ratio.x) && (0.8..1.25).contains(&ratio.y), "d={d}: predicted {predicted}, flown {flown}");
        }
    }

    #[test]
    fn each_foot_kicks_across_the_body() {
        let cfg = ExecutionConfig::default();
        let left_60 = Vec2::from_angle(60f32.to_radians());
        let right_60 = Vec2::from_angle(-60f32.to_radians());
        // A shot well to the left: the right foot swings across; well to the right: the left foot. A half-speed pass
        // nearly straight on: the preferred one. A soft pass 40° to the right is still played with the right foot.
        assert_eq!(natural_foot(Foot::Left, Vec2::X, left_60, 30.0, 30.0, &cfg), Foot::Right);
        assert_eq!(natural_foot(Foot::Right, Vec2::X, right_60, 30.0, 30.0, &cfg), Foot::Left);
        assert_eq!(natural_foot(Foot::Left, Vec2::X, Vec2::from_angle(0.1), 15.0, 30.0, &cfg), Foot::Left);
        assert_eq!(natural_foot(Foot::Right, Vec2::X, Vec2::from_angle(-0.7), 8.0, 30.0, &cfg), Foot::Right);
        // A full-power shot 35° to the right: the left foot can strike it now, the right foot needs the body turned
        // further, to within `foot_outside_hard_deg`.
        let dir = Vec2::from_angle(-35f32.to_radians());
        assert_eq!(setup_turn(Vec2::X, dir, Some(Foot::Left), 30.0, 30.0, &cfg), 0.0);
        let turn = setup_turn(Vec2::X, dir, Some(Foot::Right), 30.0, 30.0, &cfg).to_degrees();
        assert!((turn - (35.0 - cfg.foot_outside_hard_deg)).abs() < 0.01, "{turn}");
        let a = Attributes::uniform(58.0);
        assert_eq!(natural_foot(a.preferred_foot, Vec2::X, dir, 30.0, 30.0, &cfg), Foot::Left);
        assert!(ExecInput::for_player(&a, KickKind::Shot, 16.0, false, Foot::Left, false, 0.0, 0.5, 1.0, &cfg).weak_foot);
    }

    #[test]
    fn each_kick_uses_its_own_skill() {
        let cfg = ExecutionConfig::default();
        let a = Attributes { short_passing: 90.0, long_passing: 40.0, crossing: 20.0, finishing: 85.0, long_shots: 30.0, ..Attributes::uniform(58.0) };
        let skill = |kind, range, cross| ExecInput::for_player(&a, kind, range, cross, Foot::Right, false, 0.0, 0.5, 1.0, &cfg).skill;
        assert!((skill(KickKind::GroundPass, 10.0, false) - 0.9).abs() < 1e-5);
        assert!((skill(KickKind::GroundPass, 27.5, false) - 0.65).abs() < 1e-5);
        assert!((skill(KickKind::GroundPass, 40.0, false) - 0.4).abs() < 1e-5);
        assert!((skill(KickKind::LoftedPass, 15.0, false) - 0.4).abs() < 1e-5);
        assert!((skill(KickKind::LoftedPass, 30.0, true) - 0.2).abs() < 1e-5);
        assert!((skill(KickKind::GroundPass, 15.0, true) - 0.2).abs() < 1e-5);
        assert!((skill(KickKind::Shot, 8.0, false) - 0.85).abs() < 1e-5);
        assert!((skill(KickKind::Shot, 17.0, false) - 0.575).abs() < 1e-5);
        assert!((skill(KickKind::Shot, 28.0, false) - 0.3).abs() < 1e-5);
    }
}
