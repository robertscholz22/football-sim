//! How the defence lets shots happen. For every shot: how the attack got there (how and where the possession began,
//! when the ball came within 30 m of the goal line, the assisting pass, the shooter's time on the ball, carry and
//! take-ons) and where the outfield defenders were at the entry, when the shooter got the ball and at the shot
//! (between ball and goal line, in the box, in the shooter–posts triangle, nearest distance, what the nearest was
//! doing). Every 0.2 s while an attacker has the ball 11–30 m from goal it also samples the defending team's block
//! line (as followed, and its uncapped target), the back four's depth and width, the block front, the team's length
//! and what the back four are doing (up to 50 m from goal).
//! `DUMP=file.jsonl` writes one record per shot in the shooting team's goal frame (x = metres from the goal
//! line, y = lateral) in the layout of the StatsBomb comparison; `CONFIG=file.ron` overrides the default config.
//! `cargo run -p match_engine --release --example defending -- [matches]`

use std::collections::BTreeMap;
use std::fmt::Write as _;

use glam::Vec2;
use match_engine::Simulation;
use match_engine::config::SimConfig;
use match_engine::ai::analysis::pressure_at;
use match_engine::ai::brain::{OutPhase, TeamPhase, box04_block, defensive_block, shape_units};
use match_engine::events::EventKind;
use match_engine::match_state::{MatchState, side_of};
use match_engine::pitch::{GOAL_HALF_WIDTH, HALF_LENGTH, PENALTY_AREA_DEPTH, PENALTY_AREA_HALF_WIDTH, PitchConditions, Side};
use match_engine::player::{Body, KickKind};
use match_engine::team::TeamData;

const BANDS: [f32; 7] = [0.0, 6.0, 11.0, 16.5, 22.0, 30.0, 200.0];
const ENTRY_M: f32 = 30.0;

/// The outfield defenders relative to the ball (goal frame of the attacking side).
#[derive(Clone, Copy, Default)]
struct Snap {
    behind: u32,
    in_box: u32,
    tri: u32,
    near: f32,
    phase: &'static str,
}

struct Assist {
    from: Vec2,
    to: Vec2,
    kind: KickKind,
    len: f32,
    dt: f32,
}

struct Shot {
    loc: Vec2,
    header: bool,
    ft: bool,
    /// 0 goal, 1 saved, 2 blocked, 3 off target / other.
    outcome: u8,
    t_poss: f32,
    start: String,
    start_x: f32,
    t_entry: Option<f32>,
    passes: u32,
    passes_after_entry: u32,
    spell: Option<f32>,
    carry: f32,
    beaten: u32,
    assist: Option<Assist>,
    at_entry: Option<Snap>,
    at_control: Option<Snap>,
    at_shot: Snap,
    pressure: f32,
    near_label: String,
    near_recovering: bool,
    near_closing: f32,
    opp: Vec<Vec2>,
    mates: Vec<Vec2>,
    gk: Vec2,
}

/// Back-line samples per ball band (11–16.5, 16.5–22, 22–30 m) and context (settled, transition).
#[derive(Default)]
struct LineAcc {
    n: u32,
    block: f32,
    target: f32,
    four_x: f32,
    four_target_x: f32,
    four_width: f32,
    deepest4_x: f32,
    deepest4_width: f32,
    front: f32,
    length: f32,
    pressured: u32,
    labels: BTreeMap<String, u32>,
}

const LINE_BANDS: [f32; 6] = [11.0, 16.5, 22.0, 30.0, 40.0, 50.0];

fn gf(st: &MatchState, side: Side, w: Vec2) -> Vec2 {
    let q = st.frame(side).to_team(w);
    Vec2::new(HALF_LENGTH - q.x, q.y)
}

fn in_box(q: Vec2) -> bool {
    q.x < PENALTY_AREA_DEPTH && q.y.abs() < PENALTY_AREA_HALF_WIDTH
}

fn in_triangle(q: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let s = |p: Vec2, q: Vec2, r: Vec2| (q - p).perp_dot(r - p);
    let (d1, d2, d3) = (s(a, b, q), s(b, c, q), s(c, a, q));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

fn phase_name(p: TeamPhase) -> &'static str {
    match p {
        TeamPhase::Out(OutPhase::Block) => "block",
        TeamPhase::Out(OutPhase::Recovery) => "recovery",
        TeamPhase::Out(OutPhase::CounterPress) => "counter-press",
        TeamPhase::In(_) => "in possession",
    }
}

/// Defenders of `side`'s opponent relative to the ball at `ball` (goal frame of `side`).
fn snap(st: &MatchState, side: Side, ball: Vec2) -> Snap {
    let posts = (Vec2::new(0.0, GOAL_HALF_WIDTH), Vec2::new(0.0, -GOAL_HALF_WIDTH));
    let mut s = Snap { near: f32::MAX, phase: phase_name(st.team_view(side.opponent()).phase), ..Snap::default() };
    for o in st.squad(side.opponent()).iter().filter(|o| !o.is_gk()) {
        let q = gf(st, side, o.pos);
        s.behind += (q.x < ball.x) as u32;
        s.in_box += in_box(q) as u32;
        s.tri += in_triangle(q, ball, posts.0, posts.1) as u32;
        s.near = s.near.min((q - ball).length());
    }
    s
}

fn play(seed: u64) -> (Vec<Shot>, Vec<LineAcc>) {
    let cfg = std::env::var("CONFIG").map_or_else(|_| SimConfig::default(), |p| SimConfig::from_ron(&std::fs::read_to_string(p).expect("read config")).expect("parse config"));
    let mut sim = Simulation::new(&TeamData::builtin("northbridge").unwrap(), &TeamData::builtin("harbour").unwrap(), cfg, PitchConditions::default(), seed);
    let mut shots: Vec<Shot> = Vec::new();
    let mut seen = 0;
    // Possession: side, start time, start x (goal frame), cause; entry time and snapshot.
    let mut poss: (Side, f32, f32, String) = (Side::Home, 0.0, HALF_LENGTH, "kick-off".into());
    let mut poss_since = f32::MIN;
    let mut entry: Option<(f32, Snap)> = None;
    let (mut passes, mut passes_after_entry) = (0u32, 0u32);
    let mut beaten = [0u32; 22];
    // Last cause of a change of possession: time, side gaining the ball, label.
    let mut cause: (f32, Side, String) = (0.0, Side::Home, "kick-off".into());
    // Per player: last pass to him (time, from, kind, length), spell start, carry, snapshot at control.
    let mut pass_to: Vec<Option<(f32, Vec2, KickKind, f32)>> = vec![None; 22];
    let mut spell_t = [f32::MIN; 22];
    let mut carry = [0f32; 22];
    let mut ctrl: Vec<Option<(Snap, Vec2)>> = vec![None; 22];
    let mut prev_pos: Vec<Vec2> = sim.state.players.iter().map(|p| p.pos).collect();
    // Shot in flight: index, shooter, time.
    let mut flying: Option<(usize, usize, f32)> = None;
    let mut lines: Vec<LineAcc> = (0..2 * (LINE_BANDS.len() - 1)).map(|_| LineAcc::default()).collect();
    let mut tick = 0u64;
    while !sim.finished() {
        sim.step();
        tick += 1;
        let st = &sim.state;
        if tick.is_multiple_of(10)
            && st.is_live()
            && let Some(c) = st.control
        {
            let side = side_of(c);
            let ball = gf(st, side, st.ball.ground_pos());
            let d = ball.length();
            if let Some(b) = LINE_BANDS.windows(2).position(|w| (w[0]..w[1]).contains(&d)).filter(|_| ball.y.abs() <= 12.0) {
                let settled = poss.0 == side && st.time - poss.1 >= 10.0 && poss.2 > 40.0;
                let a = &mut lines[2 * b + (!settled) as usize];
                let def = st.team(side.opponent());
                let br = st.team_view(side.opponent());
                let recovery = br.phase == TeamPhase::Out(OutPhase::Recovery);
                let tc = &sim.cfg.tactics;
                let d = &def.tactics.defending;
                let target = box04_block(defensive_block(d, br.ball_ref, br.pressured, recovery, tc), br.ball_ref, shape_units(&def.formation), d, tc);
                let four: Vec<Vec2> = st.squad(side.opponent()).iter().filter(|o| o.out_role.is_back_line()).map(|o| gf(st, side, o.pos)).collect();
                let mut outfield: Vec<Vec2> = st.squad(side.opponent()).iter().filter(|o| !o.is_gk()).map(|o| gf(st, side, o.pos)).collect();
                outfield.sort_by(|p, q| p.x.total_cmp(&q.x));
                let width = |v: &[Vec2]| v.iter().map(|q| q.y).fold(f32::MIN, f32::max) - v.iter().map(|q| q.y).fold(f32::MAX, f32::min);
                a.n += 1;
                a.block += br.def_block.line_x + HALF_LENGTH;
                a.target += target.line_x + HALF_LENGTH;
                a.four_x += mean(four.iter().map(|q| q.x));
                a.four_target_x += mean(st.squad(side.opponent()).iter().filter(|o| o.out_role.is_back_line()).map(|o| gf(st, side, o.target).x));
                a.four_width += width(&four);
                a.deepest4_x += mean(outfield[..4].iter().map(|q| q.x));
                a.deepest4_width += width(&outfield[..4]);
                a.front += HALF_LENGTH - br.def_block.front_x;
                a.length += outfield[0].x - outfield[outfield.len() - 1].x;
                a.pressured += br.pressured as u32;
                for o in st.squad(side.opponent()).iter().filter(|o| o.out_role.is_back_line()) {
                    let l = o.debug.label.split([' ', '(']).next().unwrap_or("").to_string();
                    *a.labels.entry(l).or_default() += 1;
                }
            }
        }
        if let Some((i, by, t0)) = flying {
            let k = match st.last_touch {
                Some(x) if x != by && side_of(x) != side_of(by) => Some(if st.players[x].is_gk() { 1 } else { 2 }),
                Some(x) if x != by => Some(3),
                _ if st.time - t0 > 3.0 || st.control.is_some_and(|c| c != by) => Some(3),
                _ => None,
            };
            if let Some(k) = k {
                shots[i].outcome = k;
                flying = None;
            }
        }
        while seen < sim.events.len() {
            let e = sim.events[seen].clone();
            seen += 1;
            let ball = st.ball.ground_pos();
            match e.kind {
                EventKind::Interception { by } => cause = (e.t, side_of(by), "interception".into()),
                EventKind::Tackle { by, on, outcome } => {
                    use match_engine::duel::TackleOutcome::*;
                    match outcome {
                        Won | Deflected => cause = (e.t, side_of(by), "tackle".into()),
                        Beaten => beaten[on] += 1,
                    }
                }
                EventKind::Restart { kind, side } => cause = (e.t, side, format!("{kind:?}")),
                EventKind::Save { gk, .. } => cause = (e.t, side_of(gk), "keeper".into()),
                EventKind::Miscontrol { by } => cause = (e.t, side_of(by).opponent(), "heavy touch".into()),
                EventKind::Kickoff { side } => cause = (e.t, side, "kick-off".into()),
                EventKind::Pass { by, to, kind, length, .. } => {
                    if side_of(by) == poss.0 {
                        passes += 1;
                        passes_after_entry += entry.is_some() as u32;
                    }
                    if let Some(to) = to {
                        pass_to[to] = Some((e.t, gf(st, side_of(by), ball), kind, length));
                    }
                }
                EventKind::Goal { .. } => {
                    if let Some((i, _, _)) = flying.take() {
                        shots[i].outcome = 0;
                    }
                }
                EventKind::Shot { by, first_time, .. } => {
                    let side = side_of(by);
                    let sh = &st.players[by];
                    let loc = gf(st, side, ball);
                    let header = sh.debug.label.contains("header")
                        || sim.events[seen.saturating_sub(2)..seen].iter().any(|e| matches!(e.kind, EventKind::Header { .. }));
                    let ours = poss.0 == side;
                    let spell = (!first_time && spell_t[by] == sh.control_since).then_some(e.t - sh.control_since);
                    let assist = pass_to[by].filter(|&(t, ..)| ours && t >= poss.1).and_then(|(t, from, kind, len)| {
                        let to = if first_time { Some(loc) } else { ctrl[by].map(|(_, b)| b).filter(|_| sh.control_since >= t && sh.control_since - t < 5.0) };
                        to.filter(|_| e.t - t < 8.0).map(|to| Assist { from, to, kind, len, dt: e.t - t })
                    });
                    let at_shot = snap(st, side, loc);
                    let mut near: Option<(f32, usize)> = None;
                    for o in st.squad(side.opponent()).iter().filter(|o| !o.is_gk()) {
                        let d = (o.pos - sh.pos).length();
                        if near.is_none_or(|(n, _)| d < n) {
                            near = Some((d, o.id));
                        }
                    }
                    let (near_label, near_recovering, near_closing) = near.map_or((String::new(), false, 0.0), |(_, id)| {
                        let o = &st.players[id];
                        let label = o.debug.label.split([' ', '(']).next().unwrap_or("").to_string();
                        (label, matches!(o.body, Body::Recovering { .. }), o.vel.dot((sh.pos - o.pos).normalize_or_zero()))
                    });
                    let gk = st.squad(side.opponent()).iter().find(|o| o.is_gk()).map_or(Vec2::ZERO, |o| gf(st, side, o.pos));
                    shots.push(Shot {
                        loc,
                        header,
                        ft: first_time,
                        outcome: 3,
                        t_poss: if ours { e.t - poss.1 } else { 0.0 },
                        start: if ours { poss.3.clone() } else { "no possession".into() },
                        start_x: if ours { poss.2 } else { loc.x },
                        t_entry: entry.filter(|_| ours).map(|(t, _)| e.t - t),
                        passes: if ours { passes } else { 0 },
                        passes_after_entry: if ours { passes_after_entry } else { 0 },
                        spell,
                        carry: if spell.is_some() { carry[by] } else { 0.0 },
                        beaten: if ours { beaten[by] } else { 0 },
                        assist,
                        at_entry: entry.filter(|_| ours).map(|(_, s)| s),
                        at_control: spell.and(ctrl[by].map(|(s, _)| s)),
                        at_shot,
                        pressure: pressure_at(st, side, ball),
                        near_label,
                        near_recovering,
                        near_closing,
                        opp: st.squad(side.opponent()).iter().filter(|o| !o.is_gk()).map(|o| gf(st, side, o.pos)).collect(),
                        mates: st.squad(side).iter().filter(|o| o.id != by).map(|o| gf(st, side, o.pos)).collect(),
                        gk,
                    });
                    flying = Some((shots.len() - 1, by, e.t));
                }
                _ => {}
            }
        }
        if st.possession_since != poss_since {
            poss_since = st.possession_since;
            let side = st.possession;
            let c = if cause.1 == side && st.time - cause.0 < 10.0 { cause.2.clone() } else { "loose ball".into() };
            poss = (side, st.possession_since, gf(st, side, st.ball.ground_pos()).x, c);
            entry = None;
            (passes, passes_after_entry) = (0, 0);
            beaten = [0; 22];
        }
        if st.is_live() {
            let ball = gf(st, poss.0, st.ball.ground_pos());
            if entry.is_none() && ball.x <= ENTRY_M {
                entry = Some((st.time, snap(st, poss.0, ball)));
            }
        }
        if let Some(c) = st.control {
            let p = &st.players[c];
            if spell_t[c] != p.control_since {
                spell_t[c] = p.control_since;
                carry[c] = 0.0;
                let b = gf(st, p.side, st.ball.ground_pos());
                ctrl[c] = Some((snap(st, p.side, b), b));
            }
            carry[c] += (p.pos - prev_pos[c]).length();
        }
        for (q, p) in prev_pos.iter_mut().zip(&st.players) {
            *q = p.pos;
        }
    }
    (shots, lines)
}

fn v2(v: Vec2) -> String {
    format!("[{:.2}, {:.2}]", v.x, v.y)
}

fn dump(path: &str, shots: &[Shot]) {
    let mut out = String::new();
    for s in shots {
        let assist = s.assist.as_ref().map_or("null".to_string(), |a| {
            format!(r#"{{"from": {}, "to": {}, "kind": "{:?}", "len": {:.1}, "dt": {:.2}}}"#, v2(a.from), v2(a.to), a.kind, a.len, a.dt)
        });
        let snap_json = |s: Option<Snap>| {
            s.map_or("null".to_string(), |s| {
                format!(r#"{{"behind": {}, "in_box": {}, "tri": {}, "near": {:.2}, "phase": "{}"}}"#, s.behind, s.in_box, s.tri, s.near, s.phase)
            })
        };
        let list = |v: &[Vec2]| format!("[{}]", v.iter().map(|q| v2(*q)).collect::<Vec<_>>().join(", "));
        let _ = writeln!(
            out,
            r#"{{"loc": {}, "body": "{}", "outcome": "{}", "ft": {}, "t_poss": {:.2}, "start_type": "{}", "start_x": {:.1}, "t_entry": {}, "passes": {}, "passes_after_entry": {}, "spell": {}, "carry": {:.1}, "dribbles_won": {}, "assist": {}, "at_entry": {}, "at_control": {}, "at_shot": {}, "pressure": {:.2}, "near_label": "{}", "near_recovering": {}, "near_closing": {:.2}, "gk": {}, "opp": {}, "mates": {}}}"#,
            v2(s.loc),
            if s.header { "Head" } else { "Foot" },
            ["Goal", "Saved", "Blocked", "Off T"][s.outcome as usize],
            s.ft,
            s.t_poss,
            s.start,
            s.start_x,
            s.t_entry.map_or("null".into(), |t| format!("{t:.2}")),
            s.passes,
            s.passes_after_entry,
            s.spell.map_or("null".into(), |t| format!("{t:.2}")),
            s.carry,
            s.beaten,
            assist,
            snap_json(s.at_entry),
            snap_json(s.at_control),
            snap_json(Some(s.at_shot)),
            s.pressure,
            s.near_label,
            s.near_recovering,
            s.near_closing,
            v2(s.gk),
            list(&s.opp),
            list(&s.mates),
        );
    }
    std::fs::write(path, out).expect("write dump");
}

fn mean(v: impl Iterator<Item = f32>) -> f32 {
    let (s, n) = v.fold((0.0, 0u32), |(s, n), x| (s + x, n + 1));
    s / n.max(1) as f32
}

fn share(v: &[&Shot], f: impl Fn(&Shot) -> bool) -> f32 {
    100.0 * v.iter().filter(|s| f(s)).count() as f32 / v.len().max(1) as f32
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let per_match: Vec<(Vec<Shot>, Vec<LineAcc>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (1..=matches).map(|seed| scope.spawn(move || play(seed))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut shots: Vec<Shot> = Vec::new();
    let mut lines: Vec<LineAcc> = (0..2 * (LINE_BANDS.len() - 1)).map(|_| LineAcc::default()).collect();
    for (s, l) in per_match {
        shots.extend(s);
        for (a, b) in lines.iter_mut().zip(l) {
            a.n += b.n;
            a.block += b.block;
            a.target += b.target;
            a.four_x += b.four_x;
            a.four_target_x += b.four_target_x;
            a.four_width += b.four_width;
            a.deepest4_x += b.deepest4_x;
            a.deepest4_width += b.deepest4_width;
            a.front += b.front;
            a.length += b.length;
            a.pressured += b.pressured;
            for (k, v) in b.labels {
                *a.labels.entry(k).or_default() += v;
            }
        }
    }
    if let Ok(path) = std::env::var("DUMP") {
        dump(&path, &shots);
    }
    let n = matches as f32;
    println!("{matches} matches, {:.1} shots per match; bands by distance to the goal centre", shots.len() as f32 / n);
    let bands: Vec<(String, Vec<&Shot>)> = BANDS
        .windows(2)
        .map(|w| (format!("{}–{} m", w[0], w[1]), shots.iter().filter(|s| (w[0]..w[1]).contains(&s.loc.length())).collect()))
        .collect();
    println!("\nhow the shot came:");
    println!("{:<11} {:>6} {:>6} {:>6} {:>7} {:>8} {:>8} {:>8} {:>8} {:>7} {:>8}", "band", "/match", "first", "header", "assist", "lofted", "spell", "carry", "take-on", "poss<5s", "entry→");
    for (name, v) in &bands {
        println!(
            "{:<11} {:>6.1} {:>5.0}% {:>5.0}% {:>6.0}% {:>7.0}% {:>7.1}s {:>7.1}m {:>7.0}% {:>6.0}% {:>7.1}s",
            name,
            v.len() as f32 / n,
            share(v, |s| s.ft),
            share(v, |s| s.header),
            share(v, |s| s.assist.is_some()),
            share(v, |s| s.assist.as_ref().is_some_and(|a| a.kind == KickKind::LoftedPass)),
            median(v.iter().filter_map(|s| s.spell).collect()),
            median(v.iter().filter(|s| s.spell.is_some()).map(|s| s.carry).collect()),
            share(v, |s| s.beaten > 0),
            share(v, |s| s.t_poss < 5.0),
            median(v.iter().filter_map(|s| s.t_entry).collect()),
        );
    }
    println!("\ndefenders (outfield) between ball and goal line / in the box / in the shooter–posts triangle / nearest:");
    println!("{:<11} {:>22} {:>22} {:>26} {:>9} {:>9}", "band", "at entry (30 m)", "at control", "at shot", "≤ 2 m", "pressure");
    for (name, v) in &bands {
        let f = |g: &dyn Fn(&Shot) -> Option<Snap>| {
            let s: Vec<Snap> = v.iter().filter_map(|x| g(x)).collect();
            format!("{:.1} / {:.1} / {:.2}", mean(s.iter().map(|s| s.behind as f32)), mean(s.iter().map(|s| s.in_box as f32)), mean(s.iter().map(|s| s.tri as f32)))
        };
        println!(
            "{:<11} {:>22} {:>22} {:>19} {:>4.1} m {:>8.0}% {:>9.2}",
            name,
            f(&|s| s.at_entry),
            f(&|s| s.at_control),
            f(&|s| Some(s.at_shot)),
            median(v.iter().map(|s| s.at_shot.near).collect()),
            share(v, |s| s.at_shot.near <= 2.0),
            mean(v.iter().map(|s| s.pressure)),
        );
    }
    println!("\ndefending phase at entry / at the shot, and the nearest defender at the shot:");
    for (name, v) in &bands {
        let count = |g: &dyn Fn(&Shot) -> Option<&'static str>| {
            let mut m: BTreeMap<&str, u32> = BTreeMap::new();
            for s in v {
                if let Some(k) = g(s) {
                    *m.entry(k).or_default() += 1;
                }
            }
            let t = m.values().sum::<u32>().max(1) as f32;
            m.iter().map(|(k, c)| format!("{k} {:.0}%", 100.0 * *c as f32 / t)).collect::<Vec<_>>().join(", ")
        };
        let mut labels: BTreeMap<&str, u32> = BTreeMap::new();
        for s in v {
            *labels.entry(s.near_label.as_str()).or_default() += 1;
        }
        let mut labels: Vec<_> = labels.into_iter().collect();
        labels.sort_by_key(|l| std::cmp::Reverse(l.1));
        let top: Vec<String> = labels.iter().take(5).map(|(k, c)| format!("{k} {:.0}%", 100.0 * *c as f32 / v.len().max(1) as f32)).collect();
        println!("{name}");
        println!("    entry: {}   |   shot: {}", count(&|s| s.at_entry.map(|x| x.phase)), count(&|s| Some(s.at_shot.phase)));
        println!(
            "    nearest: {}; beaten (recovering) {:.0}%, closing at {:.1} m/s",
            top.join(", "),
            share(v, |s| s.near_recovering),
            mean(v.iter().map(|s| s.near_closing))
        );
    }
    println!("\npossession start of shots (all bands): cause / where (x from the goal line)");
    let mut causes: BTreeMap<String, (u32, f32)> = BTreeMap::new();
    for s in &shots {
        let c = causes.entry(s.start.clone()).or_default();
        c.0 += 1;
        c.1 += s.start_x;
    }
    let mut causes: Vec<_> = causes.into_iter().collect();
    causes.sort_by_key(|c| std::cmp::Reverse(c.1.0));
    for (k, (c, x)) in causes {
        println!("  {:<16} {:>4.0}%  mean x {:>5.1} m", k, 100.0 * c as f32 / shots.len().max(1) as f32, x / c.max(1) as f32);
    }
    println!("\nback line while an attacker has the ball (every 0.2 s), metres from the goal line; means");
    println!(
        "{:<22} {:>7} {:>7} {:>8} {:>8} {:>16} {:>18} {:>7} {:>7} {:>9}   back four doing",
        "ball band / context", "samples", "block", "target", "4 aims", "back four x / w", "deepest four x / w", "front", "length", "pressured"
    );
    for (i, a) in lines.iter().enumerate() {
        let k = a.n.max(1) as f32;
        let mut l: Vec<_> = a.labels.iter().collect();
        l.sort_by_key(|x| std::cmp::Reverse(*x.1));
        let tot = a.labels.values().sum::<u32>().max(1) as f32;
        let top: Vec<String> = l.iter().take(5).map(|(n, c)| format!("{n} {:.0}%", 100.0 * **c as f32 / tot)).collect();
        println!(
            "{:<22} {:>7} {:>7.1} {:>8.1} {:>8.1} {:>8.1} / {:>5.1} {:>10.1} / {:>5.1} {:>7.1} {:>7.1} {:>8.0}%   {}",
            format!("{}–{} m {}", LINE_BANDS[i / 2], LINE_BANDS[i / 2 + 1], if i % 2 == 0 { "settled" } else { "transition" }),
            a.n,
            a.block / k,
            a.target / k,
            a.four_target_x / k,
            a.four_x / k,
            a.four_width / k,
            a.deepest4_x / k,
            a.deepest4_width / k,
            a.front / k,
            a.length / k,
            100.0 * a.pressured as f32 / k,
            top.join(", ")
        );
    }
}
