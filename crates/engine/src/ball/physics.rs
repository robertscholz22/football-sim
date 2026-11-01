//! Ball integration: flight (gravity, drag, Magnus), bounce (restitution + spin-coupled friction),
//! rolling (rolling resistance), goal-frame collisions and line-crossing detection.

use glam::{Vec2, Vec3};

use super::{BALL_MASS, BALL_RADIUS, Ball, BallMode, SurfaceField};
use crate::config::BallConfig;
use crate::pitch::{GOAL_DEPTH, GOAL_HALF_WIDTH, GOAL_HEIGHT, HALF_LENGTH, HALF_WIDTH, POST_RADIUS, PitchConditions};
use crate::rng::SimRng;

/// Post / crossbar axis positions (centre of the woodwork).
const POST_Y: f32 = GOAL_HALF_WIDTH + POST_RADIUS;
const BAR_Z: f32 = GOAL_HEIGHT + POST_RADIUS;

/// Physics coefficients derived from the config and the current pitch conditions.
#[derive(Clone, Copy, Debug)]
pub struct BallParams {
    /// Integration sub-step used by both the live simulation and trajectory prediction.
    pub substep_dt: f32,
    pub gravity: f32,
    pub drag_k: f32,
    pub magnus_k: f32,
    pub spin_decay_air: f32,
    pub spin_decay_ground: f32,
    pub restitution: f32,
    pub bounce_friction: f32,
    pub rolling_decel: f32,
    pub roll_threshold: f32,
    pub post_restitution: f32,
    pub slope_accel: f32,
    pub bounce_tilt: f32,
    pub bobble_rate: f32,
    pub stop_speed: f32,
}

impl BallParams {
    pub fn new(cfg: &BallConfig, cond: &PitchConditions, substep_dt: f32) -> Self {
        let area = std::f32::consts::PI * BALL_RADIUS * BALL_RADIUS;
        let restitution = (cfg.restitution_soft + (cfg.restitution_hard - cfg.restitution_soft) * cond.firmness)
            * (1.0 - cfg.restitution_wet_loss * cond.wetness);
        let bounce_friction = cfg.bounce_friction_dry + (cfg.bounce_friction_wet - cfg.bounce_friction_dry) * cond.wetness;
        let c_rr = cfg.rolling_resistance_short
            + (cfg.rolling_resistance_long - cfg.rolling_resistance_short) * cond.grass_length
            - cfg.rolling_resistance_wet_reduction * cond.wetness;
        Self {
            substep_dt,
            gravity: cfg.gravity,
            drag_k: 0.5 * cfg.air_density * cfg.drag_coefficient * area / BALL_MASS,
            magnus_k: cfg.magnus_coeff,
            spin_decay_air: cfg.spin_decay_air,
            spin_decay_ground: cfg.spin_decay_ground,
            restitution,
            bounce_friction,
            rolling_decel: c_rr.max(0.005) * cfg.gravity,
            roll_threshold: cfg.roll_threshold,
            post_restitution: cfg.post_restitution,
            slope_accel: cfg.gravity * cfg.surface_slope_max * cond.unevenness,
            bounce_tilt: cfg.bounce_tilt_max * cond.unevenness,
            bobble_rate: cfg.bobble_rate * cond.unevenness,
            stop_speed: cfg.stop_speed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BallEvent {
    Bounce { pos: Vec3 },
    Woodwork { pos: Vec3 },
    /// The whole ball crossed a goal line between the posts and under the bar.
    /// `world_x_sign` is the sign of the goal line's x (+1 = goal at +x).
    Goal { world_x_sign: f32 },
    /// The whole ball crossed a touchline or goal line (not a goal). `exit` is the crossing point.
    Out { exit: Vec2, over_goal_line: bool },
}

/// Surface noise input for the live simulation. Trajectory prediction passes `None` (players plan on an ideal pitch).
pub struct SurfaceInput<'a> {
    pub field: &'a SurfaceField,
    pub rng: &'a mut SimRng,
}

/// Advance the ball by `dt` seconds (one sub-step). Appends any events.
pub fn step_ball(ball: &mut Ball, dt: f32, p: &BallParams, mut surface: Option<SurfaceInput>, events: &mut Vec<BallEvent>) {
    if ball.mode == BallMode::Held {
        return;
    }
    let prev = ball.pos;
    match ball.mode {
        BallMode::Air => step_air(ball, dt, p, surface.as_mut(), events),
        BallMode::Rolling => step_rolling(ball, dt, p, surface.as_mut()),
        BallMode::Held => unreachable!(),
    }
    collide_woodwork(ball, prev, p, events);
    check_lines(ball, prev, events);
    contain_in_net(ball, prev);
}

fn step_air(ball: &mut Ball, dt: f32, p: &BallParams, surface: Option<&mut SurfaceInput>, events: &mut Vec<BallEvent>) {
    let v = ball.vel;
    let accel = Vec3::new(0.0, 0.0, -p.gravity) - p.drag_k * v.length() * v + p.magnus_k * ball.spin.cross(v);
    ball.vel += accel * dt;
    ball.pos += ball.vel * dt;
    ball.spin *= (-p.spin_decay_air * dt).exp();

    if ball.pos.z <= BALL_RADIUS && ball.vel.z < 0.0 {
        bounce(ball, p, surface);
        events.push(BallEvent::Bounce { pos: ball.pos });
    }
}

fn bounce(ball: &mut Ball, p: &BallParams, surface: Option<&mut SurfaceInput>) {
    ball.pos.z = BALL_RADIUS;
    let mut n = Vec3::Z;
    if let Some(s) = surface
        && p.bounce_tilt > 0.0
    {
        let slope = s.field.slope(ball.pos.truncate()) * p.bounce_tilt;
        n = Vec3::new(-slope.x, -slope.y, 1.0).normalize();
    }
    let vn = ball.vel.dot(n);
    if vn >= 0.0 {
        return;
    }
    let vt = ball.vel - vn * n;
    let mut vn_new = -p.restitution * vn;
    let rolling = vn_new < p.roll_threshold;
    if rolling {
        vn_new = 0.0;
    }
    let normal_impulse = vn_new - vn; // per unit mass, > 0

    // Friction impulse at the contact point, coupled to spin (thin shell: I = 2/3 m r²).
    let r = -BALL_RADIUS * n;
    let u = vt + ball.spin.cross(r);
    let u_len = u.length();
    let mut dvt = Vec3::ZERO;
    if u_len > 1e-6 {
        let needed = u_len / 2.5; // tangential velocity change that reaches pure rolling
        let max = p.bounce_friction * normal_impulse;
        dvt = -u / u_len * needed.min(max);
    }
    ball.spin += (1.5 / BALL_RADIUS) * (-n).cross(dvt);
    ball.vel = vt + dvt + vn_new * n;

    if rolling {
        ball.vel.z = 0.0;
        ball.mode = BallMode::Rolling;
    }
}

fn step_rolling(ball: &mut Ball, dt: f32, p: &BallParams, surface: Option<&mut SurfaceInput>) {
    ball.pos.z = BALL_RADIUS;
    let v = ball.vel.truncate();
    let speed = v.length();
    if speed < p.stop_speed {
        ball.vel = Vec3::ZERO;
        ball.spin = Vec3::ZERO;
        return;
    }
    let dir = v / speed;
    let mut accel = -p.rolling_decel * dir - p.drag_k * speed * v;
    let mut bobble = None;
    if let Some(s) = surface {
        if p.slope_accel > 0.0 {
            accel -= s.field.slope(ball.pos.truncate()) * p.slope_accel;
        }
        if p.bobble_rate > 0.0 && s.rng.chance(p.bobble_rate * speed * dt) {
            bobble = Some(s.rng.range(0.3, 1.0) * speed.min(15.0) * 0.12);
        }
    }
    let mut new_v = v + accel * dt;
    // Resistance must not reverse the ball.
    if new_v.dot(v) <= 0.0 {
        new_v = Vec2::ZERO;
    }
    ball.vel = new_v.extend(0.0);
    ball.pos += ball.vel * dt;
    // Rolling without slipping; sidespin dies quickly on the grass.
    let spin_z = ball.spin.z * (-p.spin_decay_ground * dt).exp();
    ball.spin = Vec3::new(-new_v.y / BALL_RADIUS, new_v.x / BALL_RADIUS, spin_z);

    if let Some(vz) = bobble {
        ball.vel.z = vz;
        ball.mode = BallMode::Air;
    }
}

/// Time of first contact (0..1 along the segment) of a moving circle centre with a circle of radius `r` at `c`.
fn swept_hit(p0: Vec2, p1: Vec2, c: Vec2, r: f32) -> Option<f32> {
    let d = p1 - p0;
    let f = p0 - c;
    let a = d.length_squared();
    let c2 = f.length_squared() - r * r;
    if c2 <= 0.0 {
        // Starting in contact: a hit only when moving further in. A ball leaving the woodwork is let go (else a ball
        // resting on the crossbar is put back on it every step, whatever kicks or heads it).
        return (f.dot(d) < 0.0).then_some(0.0);
    }
    if a < 1e-12 {
        return None;
    }
    let b = f.dot(d);
    let disc = b * b - a * c2;
    if disc < 0.0 || b > 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    (0.0..=1.0).contains(&t).then_some(t)
}

fn collide_woodwork(ball: &mut Ball, prev: Vec3, p: &BallParams, events: &mut Vec<BallEvent>) {
    let reach = BALL_RADIUS + POST_RADIUS;
    // Only near a goal line.
    if (ball.pos.x.abs() - HALF_LENGTH).abs() > 1.0 && (prev.x.abs() - HALF_LENGTH).abs() > 1.0 {
        return;
    }
    let gx = HALF_LENGTH * ball.pos.x.signum();
    // Posts: vertical cylinders, collision in the xy plane.
    for py in [-POST_Y, POST_Y] {
        if ball.pos.z > BAR_Z + reach {
            continue;
        }
        let c = Vec2::new(gx, py);
        if let Some(t) = swept_hit(prev.truncate(), ball.pos.truncate(), c, reach) {
            let hit = prev.lerp(ball.pos, t);
            let normal = (hit.truncate() - c).normalize_or_zero().extend(0.0);
            reflect(ball, hit, normal, p.post_restitution, reach - (hit.truncate() - c).length());
            events.push(BallEvent::Woodwork { pos: hit });
            return;
        }
    }
    // Crossbar: horizontal cylinder along y, collision in the xz plane.
    if ball.pos.y.abs() <= POST_Y {
        let c = Vec2::new(gx, BAR_Z);
        let xz = |v: Vec3| Vec2::new(v.x, v.z);
        if let Some(t) = swept_hit(xz(prev), xz(ball.pos), c, reach) {
            let hit = prev.lerp(ball.pos, t);
            let n2 = (xz(hit) - c).normalize_or_zero();
            reflect(ball, hit, Vec3::new(n2.x, 0.0, n2.y), p.post_restitution, reach - (xz(hit) - c).length());
            ball.mode = BallMode::Air;
            events.push(BallEvent::Woodwork { pos: hit });
        }
    }
}

fn reflect(ball: &mut Ball, hit: Vec3, normal: Vec3, restitution: f32, penetration: f32) {
    let vn = ball.vel.dot(normal);
    if vn < 0.0 {
        let vt = ball.vel - vn * normal;
        ball.vel = vt * 0.9 - restitution * vn * normal;
    }
    ball.pos = hit + normal * penetration.max(0.0);
    ball.spin *= 0.5;
}

/// A goal or the ball out of play when the ball moved from `prev` to its position across a goal line or touchline.
pub fn check_lines(ball: &Ball, prev: Vec3, events: &mut Vec<BallEvent>) {
    let line_x = HALF_LENGTH + BALL_RADIUS;
    let line_y = HALF_WIDTH + BALL_RADIUS;
    // Goal lines.
    for sign in [-1.0f32, 1.0] {
        let (a, b) = (prev.x * sign, ball.pos.x * sign);
        if a <= line_x && b > line_x {
            let t = (line_x - a) / (b - a);
            let at = prev.lerp(ball.pos, t);
            if at.y.abs() < GOAL_HALF_WIDTH && at.z < GOAL_HEIGHT {
                events.push(BallEvent::Goal { world_x_sign: sign });
            } else {
                events.push(BallEvent::Out { exit: at.truncate(), over_goal_line: true });
            }
            return;
        }
    }
    // Touchlines (only while the ball is still in the field of play lengthwise).
    for sign in [-1.0f32, 1.0] {
        let (a, b) = (prev.y * sign, ball.pos.y * sign);
        if a <= line_y && b > line_y && ball.pos.x.abs() <= line_x {
            let t = (line_y - a) / (b - a);
            events.push(BallEvent::Out { exit: prev.lerp(ball.pos, t).truncate(), over_goal_line: false });
            return;
        }
    }
}

/// Keep a ball that went into the goal inside the net.
fn contain_in_net(ball: &mut Ball, prev: Vec3) {
    let behind = ball.pos.x.abs() - HALF_LENGTH;
    let entered = prev.y.abs() < GOAL_HALF_WIDTH && prev.z < GOAL_HEIGHT;
    if behind <= BALL_RADIUS || !entered {
        return;
    }
    let sign = ball.pos.x.signum();
    if behind > GOAL_DEPTH - BALL_RADIUS {
        ball.pos.x = sign * (HALF_LENGTH + GOAL_DEPTH - BALL_RADIUS);
        ball.vel.x *= -0.1;
        ball.vel.y *= 0.3;
    }
    if ball.pos.y.abs() > GOAL_HALF_WIDTH - BALL_RADIUS {
        ball.pos.y = ball.pos.y.signum() * (GOAL_HALF_WIDTH - BALL_RADIUS);
        ball.vel.y *= -0.1;
    }
    if ball.pos.z > GOAL_HEIGHT - BALL_RADIUS {
        ball.pos.z = GOAL_HEIGHT - BALL_RADIUS;
        ball.vel.z = ball.vel.z.min(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BallConfig;

    fn params(cond: PitchConditions) -> BallParams {
        BallParams::new(&BallConfig::default(), &cond, 0.005)
    }

    fn run(ball: &mut Ball, p: &BallParams, seconds: f32, events: &mut Vec<BallEvent>) {
        let dt = 0.005;
        for _ in 0..(seconds / dt) as usize {
            step_ball(ball, dt, p, None, events);
        }
    }

    #[test]
    fn bounce_apex_decays_with_restitution() {
        let p = params(PitchConditions::dry_flat());
        let mut ball = Ball { pos: Vec3::new(0.0, 0.0, 5.0), vel: Vec3::ZERO, spin: Vec3::ZERO, mode: BallMode::Air };
        let dt = 0.002;
        let mut apexes = Vec::new();
        let mut rising = false;
        for _ in 0..(6.0 / dt) as usize {
            step_ball(&mut ball, dt, &p, None, &mut Vec::new());
            if ball.vel.z > 0.0 {
                rising = true;
            } else if rising {
                rising = false;
                apexes.push(ball.height());
            }
        }
        assert!(apexes.len() >= 2, "expected several bounces, got {apexes:?}");
        // Apex ratio ≈ e² (drag makes it slightly smaller).
        let e2 = p.restitution * p.restitution;
        let ratio = apexes[1] / apexes[0];
        assert!((ratio - e2).abs() < 0.08, "ratio {ratio} vs e² {e2}");
    }

    #[test]
    fn wet_ball_rolls_further_than_dry() {
        let roll = |cond| {
            let p = params(cond);
            let mut ball = Ball::at_rest(Vec2::new(-40.0, 0.0));
            ball.kick(Vec3::new(14.0, 0.0, 0.0), Vec3::ZERO);
            run(&mut ball, &p, 20.0, &mut Vec::new());
            ball.pos.x + 40.0
        };
        let dry = roll(PitchConditions::dry_flat());
        let wet = roll(PitchConditions::wet_flat());
        assert!(dry > 20.0 && dry < 90.0, "dry roll {dry}");
        assert!(wet > dry * 1.1, "wet {wet} dry {dry}");
    }

    #[test]
    fn positive_sidespin_curves_left() {
        let p = params(PitchConditions::dry_flat());
        let mut ball = Ball::at_rest(Vec2::new(-20.0, 0.0));
        ball.kick(Vec3::new(25.0, 0.0, 4.0), Vec3::new(0.0, 0.0, 50.0));
        run(&mut ball, &p, 0.9, &mut Vec::new());
        assert!(ball.pos.y > 1.0, "expected a curve to +y, got {}", ball.pos.y);
        let mut ball = Ball::at_rest(Vec2::new(-20.0, 0.0));
        ball.kick(Vec3::new(25.0, 0.0, 4.0), Vec3::new(0.0, 0.0, -50.0));
        run(&mut ball, &p, 0.9, &mut Vec::new());
        assert!(ball.pos.y < -1.0);
    }

    #[test]
    fn topspin_dips_backspin_floats() {
        let p = params(PitchConditions::dry_flat());
        let flight = |spin_y: f32| {
            let mut ball = Ball::at_rest(Vec2::ZERO);
            ball.kick(Vec3::new(20.0, 0.0, 8.0), Vec3::new(0.0, spin_y, 0.0));
            let mut ev = Vec::new();
            let dt = 0.005;
            for _ in 0..1000 {
                step_ball(&mut ball, dt, &p, None, &mut ev);
                if ev.iter().any(|e| matches!(e, BallEvent::Bounce { .. })) {
                    break;
                }
            }
            ball.pos.x
        };
        // Spin about +y with velocity +x: ω × v points down (topspin).
        let top = flight(40.0);
        let back = flight(-40.0);
        assert!(top < back, "topspin {top} should land before backspin {back}");
    }

    #[test]
    fn ball_does_not_pass_through_post() {
        let p = params(PitchConditions::dry_flat());
        let mut ball = Ball::at_rest(Vec2::new(HALF_LENGTH - 5.0, POST_Y));
        ball.pos.z = 1.0;
        ball.kick(Vec3::new(35.0, 0.0, 0.0), Vec3::ZERO);
        ball.mode = BallMode::Air;
        let mut events = Vec::new();
        run(&mut ball, &p, 0.3, &mut events);
        assert!(events.iter().any(|e| matches!(e, BallEvent::Woodwork { .. })), "{events:?}");
        assert!(ball.vel.x < 0.0, "ball should bounce back, vel {}", ball.vel);
    }

    #[test]
    fn a_ball_resting_on_the_crossbar_can_be_headed_off_it() {
        let p = params(PitchConditions::dry_flat());
        // On top of the bar, exactly touching it, as a lob that died there left it.
        let mut ball = Ball::at_rest(Vec2::new(HALF_LENGTH - 0.0002, -2.8));
        ball.pos.z = BAR_Z + BALL_RADIUS + POST_RADIUS;
        ball.mode = BallMode::Air;
        let start = ball.pos;
        ball.kick(Vec3::new(-8.0, 3.0, 5.0), Vec3::ZERO);
        ball.mode = BallMode::Air;
        let mut events = Vec::new();
        run(&mut ball, &p, 0.5, &mut events);
        assert!((ball.pos - start).length() > 2.0, "ball stuck on the bar at {}", ball.pos);
    }

    #[test]
    fn goal_and_out_detection() {
        let p = params(PitchConditions::dry_flat());
        let mut ball = Ball::at_rest(Vec2::new(HALF_LENGTH - 3.0, 1.0));
        ball.kick(Vec3::new(10.0, 0.0, 0.0), Vec3::ZERO);
        let mut events = Vec::new();
        run(&mut ball, &p, 1.0, &mut events);
        assert!(events.contains(&BallEvent::Goal { world_x_sign: 1.0 }));
        assert!(ball.pos.x <= HALF_LENGTH + GOAL_DEPTH, "ball must stay in the net");

        let mut ball = Ball::at_rest(Vec2::new(0.0, HALF_WIDTH - 2.0));
        ball.kick(Vec3::new(0.0, 8.0, 0.0), Vec3::ZERO);
        let mut events = Vec::new();
        run(&mut ball, &p, 1.0, &mut events);
        assert!(events.iter().any(|e| matches!(e, BallEvent::Out { over_goal_line: false, .. })));
    }

    #[test]
    fn flat_pitch_bounces_straight() {
        let p = params(PitchConditions::dry_flat());
        let field = SurfaceField::new(3);
        let mut rng = SimRng::new(1);
        let mut ball = Ball::at_rest(Vec2::new(-30.0, 0.0));
        ball.kick(Vec3::new(18.0, 0.0, 10.0), Vec3::ZERO);
        let mut events = Vec::new();
        for _ in 0..800 {
            step_ball(&mut ball, 0.005, &p, Some(SurfaceInput { field: &field, rng: &mut rng }), &mut events);
        }
        assert!(ball.pos.y.abs() < 1e-4, "y drift {}", ball.pos.y);
    }

    #[test]
    fn bumpy_pitch_deflects() {
        let cond = PitchConditions { unevenness: 1.0, ..PitchConditions::dry_flat() };
        let p = params(cond);
        let field = SurfaceField::new(3);
        let mut total = 0.0;
        for seed in 0..10 {
            let mut rng = SimRng::new(seed);
            let mut ball = Ball::at_rest(Vec2::new(-30.0, seed as f32 * 3.0 - 15.0));
            let y0 = ball.pos.y;
            ball.kick(Vec3::new(18.0, 0.0, 10.0), Vec3::ZERO);
            for _ in 0..800 {
                step_ball(&mut ball, 0.005, &p, Some(SurfaceInput { field: &field, rng: &mut rng }), &mut Vec::new());
            }
            total += (ball.pos.y - y0).abs();
        }
        assert!(total / 10.0 > 0.05, "mean deflection {}", total / 10.0);
    }
}
