//! What happens after a change of possession. For every possession: how and where it began (goal frame of the team
//! that won the ball: x = metres from the goal line it attacks), how long it lasted, how close it got, its passes,
//! its first shot, who had the ball next, and the outfield players of both teams at the start and about 3 s / 6 s
//! later. Writes one JSON record per possession to `DUMP` (default `transitions.jsonl`) in the layout of the
//! StatsBomb extraction (`pattern` "Regular Play" for a ball won in open play), plus the losing team's phase and
//! TRN-01 score 0.3 s after the loss. `CONFIG=file.ron` overrides the default config.
//! `cargo run -p match_engine --release --example transitions -- [matches]`

use std::fmt::Write as _;

use glam::Vec2;
use match_engine::Simulation;
use match_engine::ai::brain::{OutPhase, TeamPhase};
use match_engine::config::SimConfig;
use match_engine::events::EventKind;
use match_engine::match_state::{MatchState, side_of};
use match_engine::pitch::{HALF_LENGTH, PitchConditions, Side};
use match_engine::team::TeamData;

fn gf(st: &MatchState, side: Side, w: Vec2) -> Vec2 {
    let q = st.frame(side).to_team(w);
    Vec2::new(HALF_LENGTH - q.x, q.y)
}

fn v2(v: Vec2) -> String {
    format!("[{:.2}, {:.2}]", v.x, v.y)
}

fn list(v: &[Vec2]) -> String {
    format!("[{}]", v.iter().map(|&q| v2(q)).collect::<Vec<_>>().join(", "))
}

/// Ball and the outfield players of both teams in the goal frame of `side` (the team on the ball).
fn frame(st: &MatchState, side: Side, actor: Option<usize>, dt: Option<f32>) -> String {
    let ball = gf(st, side, st.ball.ground_pos());
    let opp: Vec<Vec2> = st.squad(side.opponent()).iter().filter(|o| !o.is_gk()).map(|o| gf(st, side, o.pos)).collect();
    let gk = st.squad(side.opponent()).iter().find(|o| o.is_gk()).map(|o| gf(st, side, o.pos)).unwrap_or_default();
    let mates: Vec<Vec2> = st.squad(side).iter().filter(|o| !o.is_gk() && Some(o.id) != actor).map(|o| gf(st, side, o.pos)).collect();
    let mut s = format!("{{\"ball\": {}, \"opp\": {}, \"gk\": {}, \"mates\": {}, \"vis_min_x\": null", v2(ball), list(&opp), v2(gk), list(&mates));
    if let Some(dt) = dt {
        write!(s, ", \"dt\": {dt:.2}").unwrap();
    }
    s + "}"
}

struct Rec {
    side: Side,
    t0: f32,
    pattern: String,
    cause: String,
    loc: Vec2,
    best_x: f32,
    passes: u32,
    shot: Option<String>,
    shot_t: f32,
    f0: String,
    later: Vec<(u32, String)>,
    lost_phase: Option<(&'static str, u32)>,
    role: String,
    /// First pass / clearance / shot of the possession.
    first: Option<String>,
    /// Distance the ball travelled under control before the first action.
    carry_first: f32,
}

fn phase_name(p: TeamPhase) -> &'static str {
    match p {
        TeamPhase::Out(OutPhase::Block) => "block",
        TeamPhase::Out(OutPhase::Recovery) => "recovery",
        TeamPhase::Out(OutPhase::CounterPress) => "counter-press",
        TeamPhase::In(_) => "in possession",
    }
}

fn play(seed: u64) -> String {
    let cfg = std::env::var("CONFIG").map_or_else(|_| SimConfig::default(), |p| SimConfig::from_ron(&std::fs::read_to_string(p).expect("read config")).expect("parse config"));
    let mut sim = Simulation::new(&TeamData::builtin("northbridge").unwrap(), &TeamData::builtin("harbour").unwrap(), cfg, PitchConditions::default(), seed);
    let mut out = String::new();
    let mut seen = 0;
    let mut cause: (f32, Side, String) = (0.0, Side::Home, "kick-off".into());
    let mut restart: Option<String> = Some("From Kick Off".into());
    let mut poss_since = f32::MIN;
    let mut cur: Option<Rec> = None;
    let close = |r: Rec, next: Option<(bool, String, f32)>, out: &mut String| {
        let next = next.map_or("null".to_string(), |(lost, p, dt)| format!("{{\"team_lost\": {lost}, \"pattern\": \"{p}\", \"dt\": {dt:.2}}}"));
        let later = r.later.iter().map(|(l, f)| format!("\"{l}\": {f}")).collect::<Vec<_>>().join(", ");
        let (lp, score) = r.lost_phase.unwrap_or(("?", 0));
        writeln!(
            out,
            "{{\"m\": {seed}, \"t0\": {:.2}, \"pattern\": \"{}\", \"type\": \"{}\", \"role\": \"{}\", \"first\": {}, \"carry_first\": {:.1}, \"loc\": {}, \"dur\": {:.2}, \"best_x\": {:.2}, \"passes\": {}, \"shot\": {}, \"next\": {next}, \"f0\": {}, \"later\": {{{later}}}, \"lost_phase\": \"{lp}\", \"trn01\": {score}}}",
            r.t0, r.pattern, r.cause, r.role, r.first.unwrap_or("null".into()), r.carry_first, v2(r.loc), r.shot_t, r.best_x, r.passes, r.shot.unwrap_or("null".into()), r.f0
        )
        .unwrap();
    };
    let mut prev_ball = Vec2::ZERO;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        let now = st.time;
        while seen < sim.events.len() {
            let e = sim.events[seen].clone();
            seen += 1;
            match e.kind {
                EventKind::Interception { by } => cause = (e.t, side_of(by), "Interception".into()),
                EventKind::Tackle { by, outcome, .. } => {
                    use match_engine::duel::TackleOutcome::*;
                    if matches!(outcome, Won | Deflected) {
                        cause = (e.t, side_of(by), "Duel".into());
                    }
                }
                EventKind::Save { gk, .. } => cause = (e.t, side_of(gk), "Goal Keeper".into()),
                EventKind::Miscontrol { by } => cause = (e.t, side_of(by).opponent(), "Miscontrol".into()),
                EventKind::Restart { kind, .. } => {
                    use match_engine::match_state::RestartKind::*;
                    restart = Some(match kind {
                        Kickoff => "From Kick Off",
                        ThrowIn => "From Throw In",
                        GoalKick => "From Goal Kick",
                        Corner => "From Corner",
                        FreeKick { .. } => "From Free Kick",
                        // StatsBomb files penalties under "Other".
                        Penalty => "Other",
                    }
                    .into());
                }
                EventKind::Kickoff { .. } => restart = Some("From Kick Off".into()),
                EventKind::Pass { by, to, kind, length, .. } => {
                    if let Some(r) = cur.as_mut().filter(|r| r.side == side_of(by)) {
                        r.passes += 1;
                        if r.first.is_none() {
                            let pressed = match_engine::ai::analysis::pressure_at(st, r.side, st.ball.ground_pos()) > 0.5;
                            r.first = Some(format!(
                                "{{\"act\": \"{}\", \"kind\": \"{kind:?}\", \"len\": {length:.1}, \"dt\": {:.2}, \"targeted\": {}, \"under_pressure\": {pressed}, \"by_role\": \"{:?}\"}}",
                                if kind == match_engine::player::KickKind::Clearance { "Clearance" } else { "Pass" },
                                e.t - r.t0,
                                to.is_some(),
                                st.players[by].role
                            ));
                        }
                    }
                }
                EventKind::Shot { by, xg, .. } => {
                    if let Some(r) = cur.as_mut().filter(|r| r.side == side_of(by) && r.shot.is_none()) {
                        if r.first.is_none() {
                            r.first = Some(format!("{{\"act\": \"Shot\", \"dt\": {:.2}}}", e.t - r.t0));
                        }
                        let loc = gf(st, r.side, st.ball.ground_pos());
                        r.shot = Some(format!("{{\"dt\": {:.2}, \"loc\": {}, \"xg\": {xg:.4}, \"type\": \"Open Play\", \"goal\": false}}", e.t - r.t0, v2(loc)));
                    }
                }
                EventKind::Goal { side, own_goal: false, .. } => {
                    if let Some(s) = cur.as_mut().filter(|r| r.side == side).and_then(|r| r.shot.as_mut()) {
                        *s = s.replace("\"goal\": false", "\"goal\": true");
                    }
                }
                _ => {}
            }
        }
        if st.possession_since != poss_since {
            poss_since = st.possession_since;
            let side = st.possession;
            let open = st.is_live();
            let pattern = if open { "Regular Play".to_string() } else { restart.clone().unwrap_or_else(|| "Other".into()) };
            if let Some(r) = cur.take() {
                let back = side == r.side.opponent();
                let dt = now - r.t0;
                close(r, Some((back, pattern.clone(), dt)), &mut out);
            }
            let c = if open && cause.1 == side && now - cause.0 < 1.0 { cause.2.clone() } else if open { "Ball Recovery".into() } else { "Pass".into() };
            let ball = gf(st, side, st.ball.ground_pos());
            prev_ball = ball;
            cur = Some(Rec {
                side,
                t0: now,
                pattern,
                cause: c,
                loc: ball,
                best_x: ball.x,
                passes: 0,
                shot: None,
                shot_t: 0.0,
                f0: frame(st, side, st.control, None),
                later: Vec::new(),
                lost_phase: None,
                role: st.control.map_or("none".into(), |c| format!("{:?}", st.players[c].role)),
                first: None,
                carry_first: 0.0,
            });
            restart = None;
        }
        if let Some(r) = cur.as_mut() {
            r.shot_t = now - r.t0;
            if st.is_live() && st.control.is_some_and(|c| side_of(c) == r.side) {
                let b = gf(st, r.side, st.ball.ground_pos());
                r.best_x = r.best_x.min(b.x);
                if r.first.is_none() {
                    r.carry_first += (b - prev_ball).length();
                }
            }
            prev_ball = gf(st, r.side, st.ball.ground_pos());
            if r.lost_phase.is_none() && now - r.t0 >= 0.3 {
                let b = st.team_view(r.side.opponent());
                r.lost_phase = Some((phase_name(b.phase), b.trn01_score));
            }
            for lag in [3u32, 6] {
                if !r.later.iter().any(|(l, _)| *l == lag) && now - r.t0 >= lag as f32 && st.is_live() {
                    r.later.push((lag, frame(st, r.side, st.control, Some(now - r.t0))));
                }
            }
        }
    }
    if let Some(r) = cur.take() {
        close(r, None, &mut out);
    }
    out
}

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let per_match: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = (1..=matches).map(|seed| scope.spawn(move || play(seed))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let path = std::env::var("DUMP").unwrap_or_else(|_| "transitions.jsonl".into());
    std::fs::write(&path, per_match.concat()).expect("write dump");
    println!("{matches} matches → {path}");
}
