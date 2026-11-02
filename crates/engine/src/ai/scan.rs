//! ATT-05 scanning (M6): a player looks away from the ball at a rate set by intelligence, less often with an opponent
//! close (0.44 scans/s on average before receiving, fewest with an opponent within 1 m, rising up to ≈ 4 m [JORDET20]).
//! A scan takes a snapshot of the opponents; between scans, an opponent outside his field of view is where the last
//! snapshot put him, carried on at the velocity it saw for at most `scan_extrapolate_s`. The receiver's judgement of
//! pressure (ATT-06) and the direction of his controlling touch use what he perceives; a recent scan with nobody
//! close lets him settle faster after controlling the ball, a stale picture slows him [JORDET13].

use glam::Vec2;

use crate::config::AiConfig;
use crate::match_state::MatchState;
use crate::player::{Player, norm};

/// The scans a player remembers (for the statistics) and his snapshot of the opponents.
#[derive(Clone, Debug, PartialEq)]
pub struct Scan {
    pub last: f32,
    pub next: f32,
    /// The times of his last scans, newest at `times[count % 8]`.
    pub times: [f32; 8],
    pub count: usize,
    /// Position and velocity of each opponent (by squad index) at the last scan.
    pub seen: [(Vec2, Vec2); 11],
}

impl Default for Scan {
    fn default() -> Self {
        Self { last: f32::MIN, next: 0.0, times: [f32::MIN; 8], count: 0, seen: [(Vec2::ZERO, Vec2::ZERO); 11] }
    }
}

impl Scan {
    /// Scans in the `window` (s) before `now`.
    pub fn within(&self, now: f32, window: f32) -> usize {
        self.times.iter().filter(|&&t| now - t <= window).count()
    }
}

/// Scans per second: `scan_rate_min`..`scan_rate_max` by intelligence, × `scan_pressed_share` with an opponent
/// within 1 m, rising linearly to the full rate at 4 m [JORDET20].
pub fn rate(p: &Player, nearest: f32, ai: &AiConfig) -> f32 {
    let base = ai.scan_rate_min + (ai.scan_rate_max - ai.scan_rate_min) * norm(p.attrs.intelligence);
    let room = ((nearest - 1.0) / 3.0).clamp(0.0, 1.0);
    base * (ai.scan_pressed_share + (1.0 - ai.scan_pressed_share) * room)
}

/// Every player whose next scan is due takes his snapshot and schedules the next.
pub fn update(state: &mut MatchState, ai: &AiConfig, now: f32) {
    for i in 0..state.players.len() {
        if now < state.players[i].scan.next {
            continue;
        }
        let nearest = refresh(state, i, now);
        let p = &mut state.players[i];
        let r = rate(p, nearest, ai).max(0.05);
        let s = &mut p.scan;
        s.count += 1;
        s.times[s.count % 8] = now;
        s.next = now + 1.0 / r;
    }
}

/// Player `i`'s picture of the opponents as it is now (a scan, or a team-mate's "man on" call); returns the distance
/// to the nearest opponent.
pub fn refresh(state: &mut MatchState, i: usize, now: f32) -> f32 {
    let side = state.players[i].side;
    let pos = state.players[i].pos;
    let mut seen = [(Vec2::ZERO, Vec2::ZERO); 11];
    let mut nearest = f32::MAX;
    for (k, o) in state.squad(side.opponent()).iter().enumerate().take(11) {
        seen[k] = (o.pos, o.vel);
        nearest = nearest.min((o.pos - pos).length());
    }
    let s = &mut state.players[i].scan;
    s.seen = seen;
    s.last = now;
    nearest
}

/// Is `point` in `p`'s field of view (half-angle `scan_view_half_angle` around his facing), or touching him?
pub fn in_view(p: &Player, point: Vec2, ai: &AiConfig) -> bool {
    let to = point - p.pos;
    to.length() < 1.5 || p.facing.angle_to(to).abs() <= ai.scan_view_half_angle.to_radians()
}

/// Where `viewer` perceives the opponent with squad index `k` (position, velocity): in view, where he is; otherwise
/// where the last scan saw him, carried on for at most `scan_extrapolate_s`.
pub fn perceived(viewer: &Player, k: usize, o: &Player, now: f32, ai: &AiConfig) -> (Vec2, Vec2) {
    if in_view(viewer, o.pos, ai) || viewer.scan.last == f32::MIN || k >= 11 {
        return (o.pos, o.vel);
    }
    let (pos, vel) = viewer.scan.seen[k];
    (pos + vel * (now - viewer.scan.last).min(ai.scan_extrapolate_s), vel)
}

/// The opponents of `viewer` as he perceives them (position, velocity, the opponent).
pub fn opponents<'a>(state: &'a MatchState, viewer: &'a Player, ai: &'a AiConfig) -> impl Iterator<Item = (Vec2, Vec2, &'a Player)> + 'a {
    state.squad(viewer.side.opponent()).iter().enumerate().map(move |(k, o)| {
        let (pos, vel) = perceived(viewer, k, o, state.time, ai);
        (pos, vel, o)
    })
}

/// Factor on looking up after controlling the ball (`PlayerConfig::settle_composure_s`): 1 − `scan_settle_gain` with a
/// scan within `scan_fresh_s` and no pressure, 1 + `scan_settle_gain` with none within `scan_stale_s`, 1 otherwise.
pub fn settle_factor(p: &Player, pressed: bool, now: f32, ai: &AiConfig) -> f32 {
    let age = now - p.scan.last;
    if age <= ai.scan_fresh_s && !pressed {
        1.0 - ai.scan_settle_gain
    } else if age > ai.scan_stale_s {
        1.0 + ai.scan_settle_gain
    } else {
        1.0
    }
}
