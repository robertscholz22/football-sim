//! Trajectory prediction and inverse kick solving.
//!
//! Both use exactly the same integration step as the live simulation, but without surface noise:
//! players plan with true physics on an ideal pitch, and bumps are what they cannot foresee.

use glam::{Vec2, Vec3};

use super::physics::{BallEvent, BallParams, step_ball};
use super::{BALL_RADIUS, Ball, BallMode};
use crate::pitch::{HALF_LENGTH, HALF_WIDTH};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrajSample {
    pub t: f32,
    pub pos: Vec3,
    pub vel: Vec3,
}

/// A predicted ball path, sampled at a fixed interval.
#[derive(Clone, Debug, Default)]
pub struct Trajectory {
    pub sample_dt: f32,
    pub samples: Vec<TrajSample>,
    /// First bounce after the start (for airborne balls).
    pub first_bounce: Option<TrajSample>,
    /// The ball leaves the pitch or enters a goal at this time.
    pub exit_time: Option<f32>,
    /// The exit is into a goal (the shot is on target if nobody touches it) …
    pub on_goal: bool,
    /// … at this end (sign of the goal line's x).
    pub goal_x_sign: f32,
}

impl Trajectory {
    pub fn duration(&self) -> f32 {
        self.samples.last().map_or(0.0, |s| s.t)
    }

    /// Interpolated state at time `t` (clamped to the predicted range).
    pub fn at(&self, t: f32) -> TrajSample {
        if self.samples.is_empty() {
            return TrajSample { t, pos: Vec3::ZERO, vel: Vec3::ZERO };
        }
        let f = (t / self.sample_dt).max(0.0);
        let i = f.floor() as usize;
        if i + 1 >= self.samples.len() {
            return *self.samples.last().unwrap();
        }
        let a = self.samples[i];
        let b = self.samples[i + 1];
        let s = f - i as f32;
        TrajSample { t, pos: a.pos.lerp(b.pos, s), vel: a.vel.lerp(b.vel, s) }
    }

    pub fn end(&self) -> Option<TrajSample> {
        self.samples.last().copied()
    }
}

/// Run the physics from `start` until `stop` returns true or `max_t` elapses. Returns the elapsed time.
fn simulate(start: &Ball, p: &BallParams, max_t: f32, mut stop: impl FnMut(f32, &Ball, bool) -> bool) -> f32 {
    let mut ball = *start;
    let mut events = Vec::new();
    let mut t = 0.0;
    while t < max_t {
        events.clear();
        step_ball(&mut ball, p.substep_dt, p, None, &mut events);
        t += p.substep_dt;
        let bounced = events.iter().any(|e| matches!(e, BallEvent::Bounce { .. }));
        if stop(t, &ball, bounced) {
            break;
        }
    }
    t
}

/// Predict the ball path for up to `horizon` seconds, sampling every `sample_dt` seconds.
pub fn predict(start: &Ball, p: &BallParams, horizon: f32, sample_dt: f32) -> Trajectory {
    let every = (sample_dt / p.substep_dt).round().max(1.0) as usize;
    let mut traj = Trajectory { sample_dt: every as f32 * p.substep_dt, ..Default::default() };
    traj.samples.push(TrajSample { t: 0.0, pos: start.pos, vel: start.vel });
    if start.mode == BallMode::Held {
        return traj;
    }
    let mut ball = *start;
    let mut events = Vec::new();
    let mut step = 0usize;
    let steps = (horizon / p.substep_dt) as usize;
    while step < steps {
        events.clear();
        step_ball(&mut ball, p.substep_dt, p, None, &mut events);
        step += 1;
        let t = step as f32 * p.substep_dt;
        for e in &events {
            match e {
                BallEvent::Bounce { .. } if traj.first_bounce.is_none() => {
                    traj.first_bounce = Some(TrajSample { t, pos: ball.pos, vel: ball.vel });
                }
                BallEvent::Goal { .. } | BallEvent::Out { .. } if traj.exit_time.is_none() => {
                    traj.exit_time = Some(t);
                    if let &BallEvent::Goal { world_x_sign } = e {
                        traj.on_goal = true;
                        traj.goal_x_sign = world_x_sign;
                    }
                }
                _ => {}
            }
        }
        if step.is_multiple_of(every) {
            traj.samples.push(TrajSample { t, pos: ball.pos, vel: ball.vel });
        }
        let stopped = ball.mode == BallMode::Rolling && ball.vel.length_squared() < 1e-6;
        let far_out = ball.pos.x.abs() > HALF_LENGTH + 3.0 || ball.pos.y.abs() > HALF_WIDTH + 3.0;
        if stopped || far_out {
            if !step.is_multiple_of(every) {
                traj.samples.push(TrajSample { t, pos: ball.pos, vel: ball.vel });
            }
            break;
        }
    }
    traj
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KickSpec {
    /// Rolling pass that reaches `target` with `arrival_speed`.
    Ground { target: Vec2, arrival_speed: f32 },
    /// Airborne ball whose first bounce is at `target`. Angles in radians; spins in rad/s
    /// (`backspin` > 0 holds the ball up, `sidespin` > 0 curls it to the left).
    Lofted { target: Vec2, launch_angle: f32, backspin: f32, sidespin: f32 },
    /// Ball that passes through `target` (e.g. a point in the goal mouth) launched at `speed`.
    Through { target: Vec3, speed: f32, sidespin: f32, topspin: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KickSolution {
    pub vel: Vec3,
    pub spin: Vec3,
    /// Time until the ball reaches the target.
    pub time: f32,
    /// Ball state at the target.
    pub arrival_pos: Vec3,
    pub arrival_vel: Vec3,
    /// Lofted kicks: metres the landing point moves along the kick per m/s of kick speed, from the solver's last
    /// secant step (drag and spin included); 0 for other kicks.
    pub range_per_speed: f32,
}

impl KickSolution {
    pub fn speed(&self) -> f32 {
        self.vel.length()
    }
}

/// Spin vector for a kick in horizontal direction `dir`: `backspin` about the axis perpendicular to travel,
/// `sidespin` about +z.
fn spin_vector(dir: Vec2, backspin: f32, sidespin: f32) -> Vec3 {
    // Topspin for travel +x is +y; backspin is the opposite.
    let topspin_axis = Vec3::new(-dir.y, dir.x, 0.0);
    -backspin * topspin_axis + Vec3::new(0.0, 0.0, sidespin)
}

fn launch(dir: Vec2, speed: f32, elevation: f32) -> Vec3 {
    (dir * elevation.cos()).extend(elevation.sin()) * speed
}

fn rotate(v: Vec2, angle: f32) -> Vec2 {
    Vec2::from_angle(angle).rotate(v)
}

/// Solve for the kick that realises `spec` from a ball at `from`. Returns `None` if it needs more than
/// `max_speed` or cannot be solved.
pub fn solve_kick(from: Vec3, spec: &KickSpec, p: &BallParams, max_speed: f32) -> Option<KickSolution> {
    match *spec {
        KickSpec::Ground { target, arrival_speed } => solve_ground(from, target, arrival_speed, p, max_speed),
        KickSpec::Lofted { target, launch_angle, backspin, sidespin } => {
            solve_lofted(from, target, launch_angle, backspin, sidespin, p, max_speed)
        }
        KickSpec::Through { target, speed, sidespin, topspin } => {
            solve_through(from, target, speed.min(max_speed), sidespin, topspin, p)
        }
    }
}

fn start_ball(from: Vec3, vel: Vec3, spin: Vec3) -> Ball {
    let mut b = Ball { pos: from, vel: Vec3::ZERO, spin: Vec3::ZERO, mode: BallMode::Rolling };
    if from.z < BALL_RADIUS {
        b.pos.z = BALL_RADIUS;
    }
    b.kick(vel, spin);
    b
}

fn solve_ground(from: Vec3, target: Vec2, arrival_speed: f32, p: &BallParams, max_speed: f32) -> Option<KickSolution> {
    let delta = target - from.truncate();
    let d = delta.length();
    if d < 0.3 {
        return None;
    }
    let dir = delta / d;
    let (a0, k) = (p.rolling_decel, p.drag_k);
    // Analytic solution of v dv/ds = −(a0 + k v²).
    let v0_sq = ((a0 + k * arrival_speed * arrival_speed) * (2.0 * k * d).exp() - a0) / k;
    let mut v0 = v0_sq.max(0.0).sqrt();

    // Speed at distance d along `dir` (negative remaining distance if the ball stops short).
    let eval = |v0: f32| -> (f32, f32, Vec3, Vec3) {
        let ball = start_ball(from, (dir * v0).extend(0.0), Vec3::ZERO);
        let mut result = (-d, 0.0, ball.pos, ball.vel);
        let t = simulate(&ball, p, 15.0, |t, b, _| {
            let progress = (b.pos.truncate() - from.truncate()).dot(dir);
            if progress >= d {
                result = (b.vel.length(), t, b.pos, b.vel);
                return true;
            }
            if b.vel.length_squared() < 1e-6 {
                result = (progress - d, t, b.pos, b.vel);
                return true;
            }
            false
        });
        let _ = t;
        result
    };

    let mut best = None;
    let (mut x0, mut f0) = (v0, eval(v0).0 - arrival_speed);
    let mut x1 = v0 * 1.03 + 0.1;
    for _ in 0..6 {
        let r = eval(x1);
        let f1 = r.0 - arrival_speed;
        best = Some((x1, r));
        if f1.abs() < 0.15 {
            break;
        }
        let denom = f1 - f0;
        let next = if denom.abs() > 1e-5 { x1 - f1 * (x1 - x0) / denom } else { x1 * 1.05 };
        x0 = x1;
        f0 = f1;
        x1 = next.clamp(0.5, max_speed * 1.5);
    }
    let (speed, (_, time, pos, vel)) = best?;
    v0 = speed;
    if v0 > max_speed {
        return None;
    }
    Some(KickSolution { vel: (dir * v0).extend(0.0), spin: Vec3::ZERO, time, arrival_pos: pos, arrival_vel: vel, range_per_speed: 0.0 })
}

fn solve_lofted(
    from: Vec3,
    target: Vec2,
    launch_angle: f32,
    backspin: f32,
    sidespin: f32,
    p: &BallParams,
    max_speed: f32,
) -> Option<KickSolution> {
    let delta = target - from.truncate();
    let d = delta.length();
    if d < 2.0 {
        return None;
    }
    let base_dir = delta / d;
    let mut yaw = 0.0f32;
    // Vacuum estimate, inflated for drag.
    let mut v0 = (p.gravity * d / (2.0 * launch_angle).sin().max(0.1)).sqrt() * 1.08;
    let mut result = None;

    for _outer in 0..4 {
        let dir = rotate(base_dir, yaw);
        let landing = |v0: f32| -> Option<TrajSample> {
            let ball = start_ball(from, launch(dir, v0, launch_angle), spin_vector(dir, backspin, sidespin));
            let mut landing = None;
            simulate(&ball, p, 10.0, |t, b, bounced| {
                if bounced {
                    landing = Some(TrajSample { t, pos: b.pos, vel: b.vel });
                    return true;
                }
                false
            });
            landing
        };
        // Secant on speed for the landing distance along base_dir.
        let dist = |s: &TrajSample| (s.pos.truncate() - from.truncate()).dot(base_dir);
        let mut x0 = v0;
        let mut f0 = landing(x0).map_or(-d, |s| dist(&s) - d);
        let mut x1 = v0 * 1.05;
        let mut last = None;
        let mut slope = 0.0;
        for _ in 0..8 {
            let s = landing(x1)?;
            let f1 = dist(&s) - d;
            if (x1 - x0).abs() > 1e-4 {
                slope = (f1 - f0) / (x1 - x0);
            }
            last = Some((x1, s));
            if f1.abs() < 0.2 {
                break;
            }
            let denom = f1 - f0;
            let next = if denom.abs() > 1e-4 { x1 - f1 * (x1 - x0) / denom } else { x1 * 1.05 };
            x0 = x1;
            f0 = f1;
            x1 = next.clamp(1.0, max_speed * 1.5);
        }
        let (speed, s) = last?;
        v0 = speed;
        // Correct the aim for curl.
        let lateral = (s.pos.truncate() - target).dot(base_dir.perp());
        result = Some((speed, dir, s, slope));
        if lateral.abs() < 0.25 {
            break;
        }
        yaw -= (lateral / d).atan();
    }
    let (speed, dir, s, range_per_speed) = result?;
    if speed > max_speed || (s.pos.truncate() - target).length() > 1.0 {
        return None;
    }
    Some(KickSolution {
        vel: launch(dir, speed, launch_angle),
        spin: spin_vector(dir, backspin, sidespin),
        time: s.t,
        arrival_pos: s.pos,
        arrival_vel: s.vel,
        range_per_speed,
    })
}

fn solve_through(from: Vec3, target: Vec3, speed: f32, sidespin: f32, topspin: f32, p: &BallParams) -> Option<KickSolution> {
    let delta = target.truncate() - from.truncate();
    let d = delta.length();
    if d < 1.0 {
        return None;
    }
    let base_dir = delta / d;
    let flight = d / speed;
    let mut elev = ((target.z - from.z + 0.5 * p.gravity * flight * flight) / d).atan();
    let mut yaw = 0.0f32;

    // Ball state when its progress along base_dir reaches d.
    let cross = |elev: f32, yaw: f32| -> Option<TrajSample> {
        let dir = rotate(base_dir, yaw);
        let ball = start_ball(from, launch(dir, speed, elev), spin_vector(dir, -topspin, sidespin));
        let mut out = None;
        simulate(&ball, p, 6.0, |t, b, _| {
            if (b.pos.truncate() - from.truncate()).dot(base_dir) >= d {
                out = Some(TrajSample { t, pos: b.pos, vel: b.vel });
                return true;
            }
            b.vel.length_squared() < 1e-4
        });
        out
    };

    let mut best: Option<(f32, f32, TrajSample)> = None;
    for _ in 0..10 {
        let s = cross(elev, yaw)?;
        let dz = s.pos.z - target.z;
        let lateral = (s.pos.truncate() - target.truncate()).dot(base_dir.perp());
        best = Some((elev, yaw, s));
        if dz.abs() < 0.08 && lateral.abs() < 0.08 {
            break;
        }
        // Small-angle corrections: height error over distance, lateral error over distance.
        elev -= (dz / d).atan() * 0.9;
        yaw -= (lateral / d).atan() * 0.9;
        elev = elev.clamp(-0.3, 1.3);
    }
    let (elev, yaw, s) = best?;
    if (s.pos - target).length() > 0.5 {
        return None;
    }
    let dir = rotate(base_dir, yaw);
    Some(KickSolution {
        vel: launch(dir, speed, elev),
        spin: spin_vector(dir, -topspin, sidespin),
        time: s.t,
        arrival_pos: s.pos,
        arrival_vel: s.vel,
        range_per_speed: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BallConfig;
    use crate::pitch::PitchConditions;

    fn params() -> BallParams {
        BallParams::new(&BallConfig::default(), &PitchConditions::default(), 0.005)
    }

    /// Fly the solved kick with the live step function and return the ball at the solution's arrival time.
    fn fly(from: Vec3, sol: &KickSolution, p: &BallParams) -> Ball {
        let mut ball = start_ball(from, sol.vel, sol.spin);
        let steps = (sol.time / p.substep_dt).round() as usize;
        for _ in 0..steps {
            step_ball(&mut ball, p.substep_dt, p, None, &mut Vec::new());
        }
        ball
    }

    #[test]
    fn ground_passes_hit_target() {
        let p = params();
        let from = Vec3::new(-20.0, 5.0, BALL_RADIUS);
        for d in [5.0, 12.0, 20.0, 30.0, 40.0] {
            for angle in [0.0f32, 0.7, -2.0] {
                let target = from.truncate() + Vec2::from_angle(angle) * d;
                let sol = solve_kick(from, &KickSpec::Ground { target, arrival_speed: 6.0 }, &p, 40.0)
                    .unwrap_or_else(|| panic!("no solution for d={d}"));
                let ball = fly(from, &sol, &p);
                assert!((ball.ground_pos() - target).length() < 0.5, "d={d} miss {}", (ball.ground_pos() - target).length());
                assert!((sol.arrival_vel.length() - 6.0).abs() < 0.5);
            }
        }
    }

    #[test]
    fn lofted_and_curled_passes_land_on_target() {
        let p = params();
        let from = Vec3::new(-30.0, -20.0, BALL_RADIUS);
        for d in [15.0, 30.0, 45.0] {
            for (backspin, sidespin) in [(0.0, 0.0), (20.0, 0.0), (5.0, 30.0), (5.0, -30.0)] {
                let target = from.truncate() + Vec2::new(0.8, 0.6) * d;
                let spec = KickSpec::Lofted { target, launch_angle: 0.5, backspin, sidespin };
                let sol = solve_kick(from, &spec, &p, 40.0).unwrap_or_else(|| panic!("no solution d={d} spin={sidespin}"));
                let landing = sol.arrival_pos.truncate();
                assert!((landing - target).length() < 0.5, "d={d} sidespin={sidespin} miss {}", (landing - target).length());
            }
        }
    }

    #[test]
    fn shots_pass_through_goal_points() {
        let p = params();
        let from = Vec3::new(HALF_LENGTH - 20.0, 8.0, BALL_RADIUS);
        for (y, z) in [(-3.0, 0.3), (3.0, 2.0), (0.0, 1.0)] {
            for (speed, side) in [(28.0, 0.0), (22.0, 35.0), (22.0, -35.0)] {
                let target = Vec3::new(HALF_LENGTH, y, z);
                let spec = KickSpec::Through { target, speed, sidespin: side, topspin: 0.0 };
                let sol = solve_kick(from, &spec, &p, 35.0).unwrap_or_else(|| panic!("no shot to ({y},{z}) spin {side}"));
                assert!((sol.arrival_pos - target).length() < 0.5);
            }
        }
    }

    #[test]
    fn prediction_matches_live_physics() {
        let p = params();
        let mut ball = start_ball(Vec3::new(0.0, 0.0, BALL_RADIUS), Vec3::new(15.0, 3.0, 6.0), Vec3::new(0.0, 0.0, 20.0));
        let traj = predict(&ball, &p, 3.0, 0.1);
        for _ in 0..(2.0 / p.substep_dt).round() as usize {
            step_ball(&mut ball, p.substep_dt, &p, None, &mut Vec::new());
        }
        assert!((traj.at(2.0).pos - ball.pos).length() < 0.05);
        assert!(traj.first_bounce.is_some());
    }
}

