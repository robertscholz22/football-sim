//! Follow one carrier's spell on the ball: every `STEP` seconds between `FROM` and `TO` print the carrier (team frame of
//! the attacking side), his decision, and the three nearest defenders with what they are doing.
//! `OPTIONS=1` adds the carrier's options, `EVENTS=1` prints the match events in the window as they happen.
//! Usage: SEED=1 FROM=3734 TO=3754 spell

use glam::Vec2;
use match_engine::Simulation;
use match_engine::match_state::side_of;

fn main() {
    let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d);
    let seed = env("SEED", 1.0) as u64;
    let (from, to, step) = (env("FROM", 0.0), env("TO", 10.0), env("STEP", 0.5));
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut next = from;
    let mut seen = 0;
    let events = std::env::var("EVENTS").is_ok();
    while sim.state.time < to && !sim.finished() {
        sim.step();
        let st = &sim.state;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            if events && e.t >= from {
                let name = |id: usize| st.players[id].name.clone();
                println!("{:7.2} event: {}  (ball {:?}, h {:.2})", e.t, e.describe(name), st.ball.ground_pos(), st.ball.height());
            }
        }
        if st.time < next {
            continue;
        }
        next += step;
        let Some(c) = st.control else {
            println!("{:7.1} loose ball at {:?}, h {:.2}", st.time, st.ball.ground_pos(), st.ball.height());
            if let Some(r) = st.flight.and_then(|f| f.receiver) {
                let q = &st.players[r];
                println!("          receiver {:<14} at {:?} v {:3.1} {:?} | ball {:4.2} m, touch cooldown {:5.2}", q.name, q.pos, q.vel.length(), q.intent,
                    (st.ball.ground_pos() - q.pos).length(), q.touch_cooldown_until - st.time);
                println!("          target {:?} urgency {:.2} brake {} body {:?}", q.target, q.urgency, q.brake, q.body);
                for o in st.players.iter().filter(|o| o.id != r && (o.pos - q.pos).length() < 3.0) {
                    println!("          near: {} at {:.1} m", o.name, (o.pos - q.pos).length());
                }
                // The opponents nearest to the receiver: what they do during the flight.
                let mut opp: Vec<_> = st.squad(q.side.opponent()).iter().collect();
                opp.sort_by(|a, b| (a.pos - q.pos).length().total_cmp(&(b.pos - q.pos).length()));
                for o in opp.iter().take(2) {
                    println!("          opp {:<14} {:4.1} m v {:3.1} target {:4.1} m from receiver, urgency {:.1} brake {} next decision in {:4.2} | {} {:?}",
                        o.name, (o.pos - q.pos).length(), o.vel.length(), (o.target - q.pos).length(), o.urgency, o.brake, o.next_decision - st.time, o.debug.label, o.debug.rules);
                }
            }
            continue;
        };
        let side = side_of(c);
        let fr = st.frame(side);
        let p = &st.players[c];
        let pt = fr.to_team(p.pos);
        println!("{:7.1} {:<14} at ({:5.1},{:5.1}) v {:3.1} intent {:?} | {}", st.time, p.name, pt.x, pt.y, p.vel.length(),
            p.intent, p.debug.label);
        if std::env::var("OPTIONS").is_ok() {
            let bd = (st.ball.ground_pos() - p.pos).length();
            println!("          ball {bd:4.2} m away at {:?} ({:?}, v {:?}), h {:4.2}, facing {:?}, next decision in {:5.2} s, touch cooldown {:5.2} s", st.ball.ground_pos(), st.ball.mode, st.ball.vel.truncate(), st.ball.height(), p.facing, p.next_decision - st.time, p.touch_cooldown_until - st.time);
            let mut opts = p.debug.options.clone();
            opts.sort_by(|a, b| b.4.total_cmp(&a.4));
            for (d, u, i, b, pr) in opts.iter().take(6) {
                println!("          option {pr:4.2}  U {u:6.3} instr {i:6.3} pers {b:6.3}  {d}");
            }
        }
        let mut ds: Vec<_> = st.squad(side.opponent()).iter().filter(|o| !o.is_gk()).collect();
        ds.sort_by(|a, b| (a.pos - p.pos).length().total_cmp(&(b.pos - p.pos).length()));
        for o in ds.iter().take(3) {
            let q = fr.to_team(o.pos);
            let t = fr.to_team(o.target);
            let rel: Vec2 = q - pt;
            println!("          {:<14} {:4.1} m  rel ({:5.1},{:5.1}) target ({:5.1},{:5.1}) v {:3.1} {:?} | {} {:?}", o.name,
                rel.length(), rel.x, rel.y, t.x, t.y, o.vel.length(), o.body, o.debug.label, o.debug.rules);
        }
    }
}
