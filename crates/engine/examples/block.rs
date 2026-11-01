//! Who stretches the mid-block? At mid-block samples (BLK-01 labelling), report the highest and deepest outfield
//! players: their formation role, duty and distance outside the intended block.
//! Usage: block [matches] [minutes]

use std::collections::BTreeMap;

use match_engine::Simulation;
use match_engine::match_state::PlayPhase;
use match_engine::pitch::Side;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    let minutes: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let mut hi: BTreeMap<String, (u32, f32)> = BTreeMap::new();
    let mut lo: BTreeMap<String, (u32, f32)> = BTreeMap::new();
    let (mut n, mut len_sum, mut intended_sum) = (0u32, 0.0f32, 0.0f32);
    for seed in 1..=matches {
        let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
        sim.cfg.timing.half_length_s = minutes * 30.0;
        sim.state.half_length = sim.cfg.timing.half_length_s;
        let mut last = 0u32;
        while !sim.finished() {
            sim.step();
            if sim.stats.cal.mid_block_samples == last || sim.state.phase != PlayPhase::Live {
                last = sim.stats.cal.mid_block_samples;
                continue;
            }
            last = sim.stats.cal.mid_block_samples;
            let side: Side = sim.state.possession.opponent();
            let st = &sim.state;
            let t = st.team(side);
            let fr = t.frame;
            let b = st.team_view(side);
            let out: Vec<_> = st.squad(side).iter().filter(|p| !p.is_gk()).collect();
            let (h, l) = (
                out.iter().max_by(|a, c| fr.to_team(a.pos).x.total_cmp(&fr.to_team(c.pos).x)).unwrap(),
                out.iter().min_by(|a, c| fr.to_team(a.pos).x.total_cmp(&fr.to_team(c.pos).x)).unwrap(),
            );
            let (hx, lx) = (fr.to_team(h.pos).x, fr.to_team(l.pos).x);
            n += 1;
            len_sum += hx - lx;
            intended_sum += b.def_block.front_x - b.def_block.line_x;
            let key = |p: &match_engine::player::Player| format!("{:?}/{:?}", p.role, b.def_duties[p.slot]).replace("Mark(", "Mark").chars().filter(|c| !c.is_ascii_digit()).collect::<String>();
            let e = hi.entry(key(h)).or_default();
            e.0 += 1;
            e.1 += hx - b.def_block.front_x;
            let e = lo.entry(key(l)).or_default();
            e.0 += 1;
            e.1 += b.def_block.line_x - lx;
        }
    }
    println!("samples {n}: measured length {:.1} m, intended block length {:.1} m", len_sum / n as f32, intended_sum / n as f32);
    for (name, m) in [("highest", &hi), ("deepest", &lo)] {
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.1.0));
        println!("{name} player (role / brain role): share, mean metres outside the block");
        for (k, (c, d)) in v.iter().take(8) {
            println!("  {k:<28} {:>4.0}%  {:>5.1} m", 100.0 * *c as f32 / n as f32, d / *c as f32);
        }
    }
}
