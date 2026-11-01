//! Print both teams' positions (attacking team frame), roles and labels at the first N shots of a match.
//! `cargo run -p match_engine --release --example snapshot -- [seed] [shots]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let max: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut seen = 0;
    let mut n = 0;
    let mut history: Vec<String> = Vec::new();
    while n < max && !sim.finished() {
        let prev = sim.state.clone();
        sim.step();
        while seen < sim.events.len() {
            let e = sim.events[seen].clone();
            seen += 1;
            let st = &prev;
            history.push(e.describe(|id| format!("{} {}", st.players[id].number, st.players[id].name)));
            let EventKind::Shot { by, .. } = e.kind else { continue };
            n += 1;
            let side = side_of(by);
            let fr = st.frame(side);
            println!("==== shot by {} at t={:.1}; last events:", st.players[by].name, e.t);
            for h in history.iter().rev().take(8).rev() {
                println!("   {h}");
            }
            let db = st.team_view(side.opponent());
            println!(
                "defending phase {:?}  def block line_x {:.1} front_x {:.1} (defenders frame)  ball_ref {:?}",
                db.phase, db.def_block.line_x, db.def_block.front_x, db.ball_ref
            );
            let shooter = fr.to_team(st.players[by].pos);
            if let Ok(dir) = std::env::var("SNAP_DIR") {
                // One JSON frame per shot (attacking team frame) for offline rendering.
                let mut js = format!("{{\"t\":{:.1},\"shooter\":{},\"ball\":[{:.2},{:.2}],\"players\":[", e.t, by, fr.to_team(st.ball.pos.truncate()).x, fr.to_team(st.ball.pos.truncate()).y);
                for (i, p) in st.players.iter().enumerate() {
                    let t = fr.to_team(p.pos);
                    let tt = fr.to_team(p.target);
                    js += &format!(
                        "{}{{\"id\":{},\"att\":{},\"num\":{},\"gk\":{},\"x\":{:.2},\"y\":{:.2},\"tx\":{:.2},\"ty\":{:.2},\"label\":\"{}\"}}",
                        if i > 0 { "," } else { "" }, p.id, p.side == side, p.number, p.is_gk(), t.x, t.y, tt.x, tt.y, p.debug.label.replace('"', "'")
                    );
                }
                js += "]}";
                std::fs::write(format!("{dir}/shot_{seed}_{n:02}.json"), js).unwrap();
            }
            println!("  SHOOTER #{} at ({:.1},{:.1}), ball at ({:.1},{:.1})", st.players[by].number, shooter.x, shooter.y, fr.to_team(st.ball.pos.truncate()).x, fr.to_team(st.ball.pos.truncate()).y);
            for p in &st.players {
                let t = fr.to_team(p.pos);
                if p.side == side || (t - shooter).length() > 25.0 {
                    continue;
                }
                let tt = fr.to_team(p.target);
                println!(
                    "  {} #{:<2} {:<4?} ({:6.1},{:6.1}) -> ({:6.1},{:6.1}) v {:4.1}  {:<26} {:?}",
                    if p.side == side { "ATT" } else { "DEF" },
                    p.number,
                    p.role,
                    t.x,
                    t.y,
                    tt.x,
                    tt.y,
                    p.speed(),
                    p.debug.label,
                    p.debug.rules
                );
            }
        }
    }
}
