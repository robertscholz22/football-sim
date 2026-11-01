//! Record a match for offline viewing: one JSON line per frame (world frame: positions, velocities, targets, labels,
//! ball, phases) and one per event, after a header with the squads.
//! `cargo run -p match_engine --release --example film -- [seed] [out.jsonl] [interval_s]`

use std::fmt::Write as _;
use std::io::Write as _;

use match_engine::Simulation;
use match_engine::match_state::PlayPhase;
use match_engine::pitch::Side;

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let out = args.next().unwrap_or_else(|| format!("film_{seed}.jsonl"));
    let every: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0.2);
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut f = std::io::BufWriter::new(std::fs::File::create(&out).expect("create output"));

    let st = &sim.state;
    let mut head = String::from("{\"teams\":[");
    for (i, side) in [Side::Home, Side::Away].into_iter().enumerate() {
        let t = st.team(side);
        write!(head, "{}\"{}\"", if i > 0 { "," } else { "" }, esc(&t.short)).unwrap();
    }
    head += "],\"players\":[";
    for (i, p) in st.players.iter().enumerate() {
        let slot = &st.team(p.side).formation.slots[p.slot].label;
        write!(
            head,
            "{}[{},{},\"{}\",\"{}\"]",
            if i > 0 { "," } else { "" },
            if p.side == Side::Home { 0 } else { 1 },
            p.number,
            esc(&p.name),
            esc(slot)
        )
        .unwrap();
    }
    head += "]}";
    writeln!(f, "{head}").unwrap();

    let mut seen = 0;
    let mut next = 0.0;
    while !sim.finished() {
        let prev = sim.state.clone();
        sim.step();
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            let d = e.describe(|id| format!("{} {}", prev.players[id].number, prev.players[id].name));
            let b = prev.ball.pos;
            writeln!(f, "{{\"e\":{:.2},\"d\":\"{}\",\"b\":[{:.1},{:.1},{:.1}],\"hd\":{}}}", e.t, esc(&d), b.x, b.y, b.z, prev.frame(Side::Home).dir)
                .unwrap();
        }
        let st = &sim.state;
        if st.time + 1e-4 < next {
            continue;
        }
        next = st.time + every;
        let phase = match &st.phase {
            PlayPhase::Live => "live".to_string(),
            PlayPhase::Restart { kind, side, .. } => format!("{kind:?} {side:?}"),
            PlayPhase::GoalScored { .. } => "goal".to_string(),
            PlayPhase::FullTime => "full time".to_string(),
        };
        let b = st.ball.pos;
        let mut line = format!(
            "{{\"t\":{:.2},\"c\":{:.1},\"x\":{:.1},\"h\":{},\"s\":[{},{}],\"ph\":\"{}\",\"hd\":{},\"b\":[{:.2},{:.2},{:.2}],\"ctl\":{},\"pos\":{},\"tp\":[\"{}\",\"{}\"],\"p\":[",
            st.time,
            st.clock(),
            st.stoppage(),
            st.half,
            st.score[0],
            st.score[1],
            esc(&phase),
            st.frame(Side::Home).dir,
            b.x,
            b.y,
            b.z,
            st.control.map(|c| c as i64).unwrap_or(-1),
            if st.possession == Side::Home { 0 } else { 1 },
            esc(&format!("{:?}", st.team_view(Side::Home).phase)),
            esc(&format!("{:?}", st.team_view(Side::Away).phase)),
        );
        for (i, p) in st.players.iter().enumerate() {
            write!(
                line,
                "{}[{:.2},{:.2},{:.2},{:.2},{:.1},{:.1},{},\"{}\"]",
                if i > 0 { "," } else { "" },
                p.pos.x,
                p.pos.y,
                p.vel.x,
                p.vel.y,
                p.target.x,
                p.target.y,
                u8::from(p.sent_off),
                esc(&p.debug.label)
            )
            .unwrap();
        }
        line += "]}";
        writeln!(f, "{line}").unwrap();
    }
    eprintln!("{} {}–{} {}: {out}", sim.state.team(Side::Home).short, sim.state.score[0], sim.state.score[1], sim.state.team(Side::Away).short);
}
