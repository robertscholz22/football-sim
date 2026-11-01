//! `time_to_reach`: the core primitive for interceptions, loose-ball races, pressing and pitch control.
//! It models the same kinematics as `player::locomotion` (reaction, turning, braking, acceleration that falls
//! with speed) in closed form.

use glam::Vec2;

use crate::ball::Trajectory;
use crate::pitch::{HALF_LENGTH, HALF_WIDTH};
use crate::config::PlayerConfig;
use crate::player::{Kinematics, Player};

/// Horizon and sampling step (s) of `time_to_meet`.
const MEET_HORIZON: f32 = 2.0;
const MEET_STEP: f32 = 0.1;

/// Time (s) for a player starting at rest to run `d` metres, with initial speed `v0` along the path.
/// Speed follows dv/dt = a·(1 − v/vmax).
pub fn run_time(d: f32, v0: f32, vmax: f32, accel: f32) -> f32 {
    if d <= 0.0 {
        return 0.0;
    }
    let v0 = v0.clamp(0.0, vmax);
    let tau = vmax / accel;
    // Start from the asymptote (an upper bound) and Newton-iterate; x(t) is convex so this converges from above.
    let mut t = (d + (vmax - v0) * tau) / vmax;
    for _ in 0..5 {
        let e = (-t / tau).exp();
        let x = vmax * t - (vmax - v0) * tau * (1.0 - e);
        let v = vmax - (vmax - v0) * e;
        let step = (x - d) / v.max(0.1);
        t -= step;
        if step.abs() < 1e-3 {
            break;
        }
    }
    t.max(0.0)
}

/// Time (s) for player `p` to get within `radius` of `point` (world frame).
/// With `react` the player first needs his reaction time (he keeps his velocity meanwhile).
pub fn time_to_reach(p: &Player, point: Vec2, radius: f32, react: bool) -> f32 {
    time_to_reach_after(p, point, radius, if react { p.kin.reaction } else { 0.0 })
}

/// As `time_to_reach`, with the player first needing `reaction` (s), in which he keeps his velocity.
pub fn time_to_reach_after(p: &Player, point: Vec2, radius: f32, reaction: f32) -> f32 {
    time_to_reach_from(&p.kin, p.pos, p.vel, point, radius, reaction)
}

/// As `time_to_reach_after`, for a player with kinematics `k` standing at `pos` and moving at `vel` (a predicted
/// state: where an opponent will be once he has responded to a movement, docs/player-brain.md section 8.2).
pub fn time_to_reach_from(k: &Kinematics, pos: Vec2, vel: Vec2, point: Vec2, radius: f32, reaction: f32) -> f32 {
    let mut t = reaction;
    let pos = pos + vel * reaction;
    let delta = point - pos;
    let dist = delta.length() - radius;
    if dist <= 0.0 {
        return t;
    }
    let dir = delta / delta.length();
    let v_along = vel.dot(dir);
    let v_lat = (vel - v_along * dir).length();
    // Turning: the sideways velocity is braked while the player already accelerates towards the point
    // (as in `locomotion::step`); the sideways drift lengthens the path.
    let turn_acc = k.decel.max(k.lateral_accel).max(0.1);
    let t_turn = v_lat / turn_acc;
    let s_lat = v_lat * v_lat / (2.0 * turn_acc);
    let mut d = (dist * dist + s_lat * s_lat).sqrt();
    let mut v0 = v_along;
    if v_along < 0.0 {
        // Stop first, then run back.
        t += -v_along / k.decel;
        d += v_along * v_along / (2.0 * k.decel);
        v0 = 0.0;
    }
    t + run_time(d, v0, k.top_speed, k.accel).max(t_turn)
}

/// Time (s) for `p` to meet a point that starts at `point` and moves at `vel` (e.g. the contain point goal-side of a
/// running carrier): the earliest sampled `t` with `time_to_reach(point + vel·t) ≤ t`. A player who cannot meet it
/// within the horizon (a chaser from behind) gets the time to where it is then, so he ranks behind anyone who can.
pub fn time_to_meet(p: &Player, point: Vec2, vel: Vec2, react: bool) -> f32 {
    if vel.length_squared() < 0.25 {
        return time_to_reach(p, point, 0.0, react);
    }
    let at = |t: f32| {
        let q = point + vel * t;
        Vec2::new(q.x.clamp(-HALF_LENGTH, HALF_LENGTH), q.y.clamp(-HALF_WIDTH, HALF_WIDTH))
    };
    let mut t = 0.0;
    while t <= MEET_HORIZON + 1e-3 {
        if time_to_reach(p, at(t), 0.0, react) <= t {
            return t;
        }
        t += MEET_STEP;
    }
    time_to_reach(p, at(MEET_HORIZON), 0.0, react).max(MEET_HORIZON)
}

/// ATT-24: where a free receiver takes a pass: the point of the ball's path closest to the pass `target`, or else the
/// latest point before it, that `p` reaches `margin` seconds before the ball with the ball no higher than
/// `max_height`. A ball that is still above that height there (overhit) he takes where it comes down after it. `None`
/// if there is no such point (he has to meet it earlier: `intercept`).
pub fn reception_point(p: &Player, traj: &Trajectory, target: Vec2, max_height: f32, margin: f32) -> Option<(f32, Vec2)> {
    let n = traj.samples.iter().take_while(|s| traj.exit_time.is_none_or(|e| s.t <= e)).count();
    let path = &traj.samples[..n];
    let closest = (0..n).min_by(|&a, &b| {
        let da = (path[a].pos.truncate() - target).length_squared();
        let db = (path[b].pos.truncate() - target).length_squared();
        da.total_cmp(&db)
    })?;
    let ok = |s: &&crate::ball::trajectory::TrajSample| {
        s.pos.z - crate::ball::BALL_RADIUS <= max_height && time_to_reach(p, s.pos.truncate(), 0.3, false) + margin <= s.t
    };
    path[..=closest].iter().rev().find(ok).or_else(|| path[closest + 1..].iter().find(ok)).map(|s| (s.t, s.pos.truncate()))
}

/// IND-08: how long `p` still needs to react to a pass released `since` (s) ago: his reaction time less what he reads
/// before the ball is struck (`PlayerConfig::pass_cue_s`), not below zero.
pub fn pass_reaction(p: &Player, since: f32, pc: &PlayerConfig) -> f32 {
    (p.kin.reaction - pc.pass_cue_s - since).max(0.0)
}

/// Earliest point on a predicted ball path that `p` can reach before the ball, with the ball no higher than
/// `max_height` (bottom of ball), starting after `reaction` (s). Returns (time, ground point). If the ball comes to
/// rest, the player may arrive after it stops.
pub fn intercept(p: &Player, traj: &Trajectory, max_height: f32, radius: f32, reaction: f32) -> Option<(f32, Vec2)> {
    for s in &traj.samples {
        if traj.exit_time.is_some_and(|e| s.t > e) {
            return None;
        }
        if s.pos.z - crate::ball::BALL_RADIUS > max_height {
            continue;
        }
        let g = s.pos.truncate();
        if time_to_reach_after(p, g, radius, reaction) <= s.t {
            return Some((s.t, g));
        }
    }
    let end = traj.end()?;
    if traj.exit_time.is_some() || end.vel.length() > 0.3 {
        return None;
    }
    let g = end.pos.truncate();
    Some((time_to_reach_after(p, g, radius, reaction).max(end.t), g))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PlayerConfig, SimConfig};
    use crate::pitch::{PitchConditions, Side};
    use crate::player::{Attributes, Kinematics, locomotion};
    use crate::scenario::make_player;

    #[test]
    fn run_time_matches_integration() {
        let (vmax, a) = (8.5, 5.0);
        for &d in &[2.0f32, 10.0, 30.0] {
            let mut x = 0.0;
            let mut v = 0.0;
            let mut t = 0.0;
            let dt = 0.001;
            while x < d {
                v += a * (1.0 - v / vmax) * dt;
                x += v * dt;
                t += dt;
            }
            assert!((run_time(d, 0.0, vmax, a) - t).abs() < 0.02, "d={d}");
        }
    }

    /// `time_to_reach` agrees with the simulated locomotion within tolerance.
    #[test]
    fn agrees_with_locomotion() {
        let cfg = SimConfig::default();
        for (attr, start_vel, target) in [
            (47.0, Vec2::ZERO, Vec2::new(20.0, 0.0)),
            (90.0, Vec2::ZERO, Vec2::new(5.0, 3.0)),
            (21.0, Vec2::new(5.0, 0.0), Vec2::new(0.0, 15.0)),
            (58.0, Vec2::new(-4.0, 0.0), Vec2::new(25.0, -5.0)),
        ] {
            let mut p = make_player(0, Side::Home, Attributes::uniform(attr), &cfg);
            p.kin = Kinematics::new(&p.attrs, &PlayerConfig::default(), &PitchConditions::default(), 1.0);
            p.vel = start_vel;
            p.target = target;
            p.urgency = 1.0;
            p.brake = false;
            let predicted = time_to_reach(&p, target, 0.5, false);
            let mut t = 0.0;
            while (p.pos - target).length() > 0.5 && t < 20.0 {
                locomotion::step(&mut p, 0.01, t);
                t += 0.01;
            }
            let tol = 0.12 * t + 0.1;
            assert!((predicted - t).abs() < tol, "attr {attr} vel {start_vel}: predicted {predicted}, simulated {t}");
        }
    }

    /// IND-01 pursuit: a defender holding a contain point that moves with a running carrier keeps the gap; without
    /// the target's velocity, arrival braking towards a point that only jumps on at each decision lets the carrier go.
    #[test]
    fn pursuit_keeps_up_with_a_moving_target() {
        let cfg = SimConfig::default();
        let carrier_v = Vec2::new(-6.5, 0.0);
        let gap = |pursuit: bool| {
            let mut p = make_player(0, Side::Home, Attributes::uniform(58.0), &cfg);
            p.kin = Kinematics::new(&p.attrs, &PlayerConfig::default(), &PitchConditions::default(), 1.0);
            p.vel = carrier_v;
            p.urgency = 1.0;
            p.brake = true;
            let mut contain = Vec2::new(-1.8, 0.0);
            let mut t = 0.0;
            while t < 4.0 {
                // A new decision every 0.4 s re-sets the target on the contain point.
                if (t / 0.4f32).fract() < 0.01 / 0.4 {
                    p.target = contain;
                    p.target_vel = if pursuit { carrier_v } else { Vec2::ZERO };
                }
                locomotion::step(&mut p, 0.01, t);
                contain += carrier_v * 0.01;
                t += 0.01;
            }
            (p.pos - contain).length()
        };
        let (with, without) = (gap(true), gap(false));
        assert!(with < 0.5, "pursuit gap {with}");
        assert!(without > 2.0 * with, "gap without pursuit {without}, with {with}");
    }

    /// A point running at 7 m/s is met sooner by a player in its path than by one the same distance behind it, who
    /// cannot catch it within the horizon; a still point gives the static time.
    #[test]
    fn time_to_meet_favours_the_player_in_the_path() {
        let cfg = SimConfig::default();
        let player = |pos: Vec2| {
            let mut p = make_player(0, Side::Home, Attributes::uniform(58.0), &cfg);
            p.pos = pos;
            p
        };
        let (point, vel) = (Vec2::ZERO, Vec2::new(7.0, 0.0));
        let (ahead, behind) = (player(Vec2::new(8.0, 0.0)), player(Vec2::new(-8.0, 0.0)));
        let (ta, tb) = (time_to_meet(&ahead, point, vel, true), time_to_meet(&behind, point, vel, true));
        assert!(ta < 1.2, "ahead meets after {ta}");
        assert!(tb >= MEET_HORIZON, "behind meets after {tb}");
        let still = time_to_meet(&ahead, point, Vec2::ZERO, true);
        assert!((still - time_to_reach(&ahead, point, 0.0, true)).abs() < 1e-6);
    }
}
