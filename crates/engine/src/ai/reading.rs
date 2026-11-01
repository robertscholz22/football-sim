//! Reading team-mates (docs/player-brain.md section 7). Everything here is a deterministic function of stats and
//! situation: until player i has read team-mate j's current movement or claim, he believes j does what the concept
//! prescribes for him; he reads a deviation after a delay that shrinks with his understanding u_ij. He estimates a
//! team-mate's abilities as far as he knows him (partners).

use glam::Vec2;

use super::deviation::state_factor;
use super::world::Times;
use crate::config::AiConfig;
use crate::match_state::MatchState;
use crate::player::{Player, PlayerId, norm};
use crate::team::{Movement, resolve_player};

/// His instruction load 0..1: a role he is new to, or instructions his team has not practised, take attention until
/// they become routine. The larger of (1 − role familiarity) and (1 − concept practice) × how much he is asked to do
/// beyond the role (the player instructions, summed, up to 1).
pub fn load(state: &MatchState, p: &Player) -> f32 {
    let r = resolve_player(&p.instructions);
    let asked = (r.forward_runs.abs() + r.come_short.abs() + r.overlap.abs() + r.invert.abs() + r.dribble_freq.abs()).min(1.0);
    (1.0 - p.familiarity(p.beliefs.in_possession)).max((1.0 - state.team(p.side).practice.concept) * asked)
}

/// Calls (M6, section 7): the calls per second he can make, `call_rate` per minute × leadership × (0.5 + confidence)
/// × (1 − load); unspent calls build up to `call_max`.
pub fn call_rate(state: &MatchState, ai: &AiConfig, p: &Player) -> f32 {
    ai.call_rate / 60.0 * norm(p.attrs.leadership) * (0.5 + p.confidence) * (1.0 - load(state, p))
}

/// How familiar player `i` is with team-mate `j` and the team's play (1 for himself): f(practice) × f(partners) ×
/// f(1 − load), each f with its floor (`AiConfig::reading_*_floor`), so no single factor zeroes it.
pub fn familiarity(state: &MatchState, ai: &AiConfig, i: PlayerId, j: PlayerId) -> f32 {
    if i == j {
        return 1.0;
    }
    let (p, m) = (&state.players[i], &state.players[j]);
    let team = state.team(p.side);
    let practice = 0.5 * (team.practice.formation + team.practice.concept);
    state_factor(ai.reading_practice_floor, practice)
        * state_factor(ai.reading_partner_floor, team.partners[p.slot][m.slot])
        * state_factor(ai.reading_load_floor, 1.0 - load(state, p))
}

/// u_ij: how well player `i` understands team-mate `j` (1 for himself): his intelligence × `familiarity`.
pub fn understanding(state: &MatchState, ai: &AiConfig, i: PlayerId, j: PlayerId) -> f32 {
    if i == j {
        return 1.0;
    }
    norm(state.players[i].attrs.intelligence) * familiarity(state, ai, i, j)
}

/// d_ij: seconds until `i` reads a movement or claim of `j` that differs from the concept's.
pub fn delay(state: &MatchState, ai: &AiConfig, i: PlayerId, j: PlayerId) -> f32 {
    ai.reading_delay_max_s * (1.0 - understanding(state, ai, i, j))
}

/// Seconds after a pass becomes legible until `receiver` starts to take it (`AiConfig::reading_receive_delay_max_s`).
/// His reading of play in general is in his reaction already; what delays him is how little
/// he knows this passer and the team's play (`familiarity`).
pub fn receive_delay(state: &MatchState, ai: &AiConfig, receiver: PlayerId, passer: PlayerId) -> f32 {
    ai.reading_receive_delay_max_s * (1.0 - familiarity(state, ai, receiver, passer))
}

/// What `i` believes team-mate `j` needs (knowing a team-mate): a partner he knows well exactly, one he does not as a
/// player of average pace would need (the mean top speed of the outfield players on the pitch), blended by how well
/// they know each other. Times scale with 1 / top speed over the distances that decide claims.
fn estimate(state: &MatchState, i: PlayerId, j: PlayerId, t: f32, mean_speed: f32) -> f32 {
    let (p, m) = (&state.players[i], &state.players[j]);
    let k = state.team(p.side).partners[p.slot][m.slot];
    t * (k + (1.0 - k) * m.kin.top_speed / mean_speed)
}

fn mean_speed(state: &MatchState) -> f32 {
    let outfield = state.players.iter().filter(|p| !p.is_gk());
    let (sum, n) = outfield.fold((0.0, 0.0), |(s, n), p| (s + p.kin.top_speed, n + 1.0));
    sum / n
}

/// `times` (by slot, of `i`'s team) as `i` believes them (`estimate`); his own exact.
pub fn estimated(state: &MatchState, i: PlayerId, mut times: Times) -> Times {
    let mean = mean_speed(state);
    let base = state.players[i].side.index() * times.len();
    for (slot, t) in times.iter_mut().enumerate() {
        if base + slot != i && t.is_finite() && *t < f32::MAX {
            *t = estimate(state, i, base + slot, *t, mean);
        }
    }
    times
}

/// The movement and offer of team-mate `m` as `i` sees them: what he does once `i` has read it (he has held it for
/// `delay`), and until then what the concept prescribes for him (`movement::Plan::concept`).
pub fn seen(state: &MatchState, ai: &AiConfig, i: PlayerId, m: &Player) -> (Movement, Option<Vec2>) {
    match m.plan.concept {
        Some((k, pt)) if state.time - m.plan.since < delay(state, ai, i, m.id) => (k, Some(pt)),
        _ => (m.plan.kind, m.plan.offer),
    }
}
