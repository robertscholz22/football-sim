//! Print the on-ball decision breakdown (layer 1 optimum, layer 2 instruction bias, layer 3 personality bias,
//! final probability) for every kick in the first minutes of a match.
//!
//! `cargo run -p match_engine --release --example decisions -- [seed] [seconds]`

use match_engine::Simulation;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let secs: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(120.0);
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut seen = 0;
    while sim.state.time < secs && !sim.finished() {
        sim.step();
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            seen += 1;
            let st = &sim.state;
            println!("{}", e.describe(|id| format!("{} {}", st.players[id].number, st.players[id].name)));
            let by = match e.kind {
                match_engine::events::EventKind::Pass { by, .. } | match_engine::events::EventKind::Shot { by, .. } => by,
                _ => continue,
            };
            let mut opts = st.players[by].debug.options.clone();
            opts.sort_by(|a, b| b.4.total_cmp(&a.4));
            for (label, u, instr, pers, prob) in opts.iter().take(6) {
                println!("      {prob:5.2}  U={u:+.4} instr={instr:+.4} pers={pers:+.4}  {label}");
            }
        }
    }
}
