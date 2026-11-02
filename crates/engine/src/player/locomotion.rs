//! Player movement: "arrive" steering with acceleration that falls with speed, a braking limit,
//! a lateral (turning) acceleration limit and soft separation between players.
//! `ai::reach::time_to_reach` models the same kinematics for planning.

use glam::Vec2;

use super::{Body, Intent, Kinematics, Player};

/// Minimum fraction of the acceleration still available at top speed.
pub const ACCEL_FLOOR: f32 = 0.15;
/// Body turn rate (rad/s) when standing.
const FACING_TURN_RATE: f32 = 9.0;
/// Speeds (m/s) between which the body turns from the look-at point to the run direction.
const FACE_RUN_FROM: f32 = 3.0;
const FACE_RUN_FULL: f32 = 5.5;

/// Desired velocity for reaching `p.target`, which itself moves at `p.target_vel`.
pub fn desired_velocity(p: &Player) -> Vec2 {
    desired_velocity_at(p, p.pos, p.target)
}

/// `desired_velocity` for `p` standing at `pos` with his target at `target`.
fn desired_velocity_at(p: &Player, pos: Vec2, target: Vec2) -> Vec2 {
    let to = target - pos;
    let d = to.length();
    let paced = p.pace_s > 0.0 && p.intent == Intent::Position;
    // A paced movement runs at `pace_speed` while the gap is large and closes the rest in its pace time (M6, watch
    // round): its speed comes from the gap, not from the urgency.
    let max = if paced { p.kin.top_speed.min(p.pace_speed) } else { p.kin.top_speed * p.urgency.clamp(0.05, 1.0) };
    if d < 0.05 {
        return p.target_vel.clamp_length_max(max);
    }
    let mut speed = max;
    if paced {
        // Pacing (M6): he closes the gap in his pace time, not at the speed's limit, but at least at the pace floor.
        speed = speed.min((d / p.pace_s).max(p.pace_min));
    }
    if p.brake {
        // Closing speed from which we can still stop at the target (relative to its motion).
        speed = speed.min((2.0 * p.kin.decel * 0.8 * d).sqrt());
    }
    (p.target_vel + to / d * speed).clamp_length_max(max)
}

/// Integrate one player's motion over `dt`.
pub fn step(p: &mut Player, dt: f32, now: f32) {
    // Timed body states expire back to normal.
    if p.body != Body::Normal && p.can_act(now) {
        p.body = Body::Normal;
    }
    p.target += p.target_vel * dt;
    let mut desired = desired_velocity(p);
    let mut accel_scale = 1.0;
    match p.body {
        Body::Recovering { until } if now < until => {
            desired *= 0.3;
            accel_scale = 0.4;
        }
        Body::Kicking { until } if now < until => {
            desired *= 0.4;
        }
        Body::Tackling { until } if now < until => {
            desired = p.vel * 0.5;
        }
        Body::Diving { until, dir } if now < until => {
            p.vel = dir;
            p.pos += p.vel * dt;
            return;
        }
        _ => {}
    }

    let nv = accelerate(&p.kin, p.vel, desired, dt, accel_scale);
    p.vel = nv;
    p.pos += nv * dt;
    p.distance_covered += nv.length() * dt;

    // Facing: towards the look-at point (usually the ball) when walking or jogging, the run direction when running
    // hard, blended in between so the body does not flip as the speed crosses a threshold.
    let look = (p.look_at - p.pos).normalize_or_zero();
    let run = nv.normalize_or_zero();
    // Setting up a kick: the body turns towards the kick (look-at point) with the ball at the feet.
    let setting_up = matches!(p.intent, Intent::SetUp { .. });
    let w = if setting_up { 0.0 } else { ((nv.length() - FACE_RUN_FROM) / (FACE_RUN_FULL - FACE_RUN_FROM)).clamp(0.0, 1.0) };
    let want = if look == Vec2::ZERO { run } else { (look * (1.0 - w) + run * w).normalize_or(run) };
    if want != Vec2::ZERO {
        let max_turn = if setting_up { p.kin.ball_turn_rate } else { FACING_TURN_RATE } * dt;
        let ang = p.facing.angle_to(want);
        p.facing = Vec2::from_angle(ang.clamp(-max_turn, max_turn)).rotate(p.facing).normalize_or_zero();
        if p.facing == Vec2::ZERO {
            p.facing = want;
        }
    }
}

/// The velocity after `dt` of a player with kinematics `k` moving at `v` who wants `desired`: acceleration that falls
/// with speed, a braking limit and a turning limit.
fn accelerate(k: &Kinematics, v: Vec2, desired: Vec2, dt: f32, accel_scale: f32) -> Vec2 {
    let speed = v.length();
    let fwd = if speed > 0.2 { v / speed } else { desired.normalize_or_zero() };
    let dv = desired - v;
    let dl = dv.dot(fwd);
    let dlat = dv - dl * fwd;
    let max_acc = k.accel * (1.0 - speed / k.top_speed).max(ACCEL_FLOOR) * accel_scale;
    let dl_c = dl.clamp(-k.decel * dt, max_acc * dt);
    // At low speed turning is limited by acceleration rather than grip.
    let lat_limit = if speed < 2.0 { k.lateral_accel.max(k.accel * 1.5) } else { k.lateral_accel };
    let dlat_c = dlat.clamp_length_max(lat_limit * dt);
    let mut nv = v + dl_c * fwd + dlat_c;
    if nv.length() > k.top_speed {
        nv = nv.normalize() * k.top_speed;
    }
    nv
}

/// Integration step (s) of `carry_on`.
const CARRY_ON_DT: f32 = 0.05;

/// Where `p` is and how he moves `dt` s from now if he carries on with his movement (to his target, which moves on at
/// `target_vel`), with the steering of `step` (body states aside): how the AI predicts a player who has not yet
/// responded, e.g. an opponent before he reads a pass (IND-08), so a presser closing in stops at his contain point
/// instead of running on through the carrier.
pub fn carry_on(p: &Player, dt: f32) -> (Vec2, Vec2) {
    let (mut pos, mut vel, mut target) = (p.pos, p.vel, p.target);
    let mut left = dt;
    while left > 1e-4 {
        let h = left.min(CARRY_ON_DT);
        target += p.target_vel * h;
        vel = accelerate(&p.kin, vel, desired_velocity_at(p, pos, target), h, 1.0);
        pos += vel * h;
        left -= h;
    }
    (pos, vel)
}

/// Push overlapping players apart (stronger players are pushed less).
pub fn separate(players: &mut [Player], radius: f32) {
    let min_d = 2.0 * radius;
    for i in 0..players.len() {
        for j in (i + 1)..players.len() {
            let delta = players[j].pos - players[i].pos;
            let d = delta.length();
            if d < min_d && d > 1e-4 {
                let overlap = min_d - d;
                let n = delta / d;
                let si = players[i].attrs.strength;
                let sj = players[j].attrs.strength;
                let wi = sj / (si + sj);
                players[i].pos -= n * overlap * wi;
                players[j].pos += n * overlap * (1.0 - wi);
            }
        }
    }
}
