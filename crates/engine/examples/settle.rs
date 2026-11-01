//! Time from gaining control in open play to the first on-ball decision, and how far the ball got from the
//! player in between (the settle touch). Split by whether he received a pass meant for him or won the ball.
//! Also, as in the StatsBomb comparison: from receiving a pass meant for him (or a first-time kick) to his next
//! pass or shot, how often he loses the ball first, and a histogram of that time against StatsBomb's.
//! `cargo run -p match_engine --release --example settle -- [matches]`

use match_engine::Simulation;
use match_engine::events::EventKind;
use match_engine::match_state::side_of;

#[derive(Default, Clone, Copy)]
struct Acc {
    n: u32,
    sum: f64,
    over: [u32; 3],
    lost_before: u32,
    max_dist: f64,
}

fn play(seed: u64) -> ([Acc; 2], Vec<f32>, u32) {
    let mut sim = Simulation::from_builtin("northbridge", "harbour", seed);
    let mut acc = [Acc::default(); 2];
    // Receipt of a pass meant for him: player, time. Times to the next pass / shot; receipts lost first.
    let mut receipt: Option<(usize, f32)> = None;
    let mut to_pass: Vec<f32> = Vec::new();
    let mut lost = 0u32;
    // Per slow / fast settle (> 1 s or not): sum of speed at control, of |angle| run vs touch (deg), of ticks with the
    // ball within control radius while a decision was due, count.
    let mut diag = [[0f64; 4]; 2];
    let mut cur_diag: Option<(f32, f32, u32)> = None;
    // Current spell: player, start, received a pass meant for him, the decision time it waits for, max ball distance.
    let mut spell: Option<(usize, f32, bool, f32, f32)> = None;
    let mut seen = 0;
    let mut last_pass_to: Option<usize> = None;
    while !sim.finished() {
        sim.step();
        let st = &sim.state;
        let now = st.time;
        while seen < sim.events.len() {
            let e = &sim.events[seen];
            let kicked = match e.kind {
                EventKind::Pass { by, first_time, .. } | EventKind::Shot { by, first_time, .. } => Some((by, first_time)),
                _ => None,
            };
            if let Some((by, ft)) = kicked {
                if ft && last_pass_to == Some(by) {
                    to_pass.push(0.0);
                } else if let Some((r, t)) = receipt.filter(|r| r.0 == by) {
                    to_pass.push(e.t - t);
                    let _ = r;
                }
                receipt = None;
            }
            if let EventKind::Pass { to, .. } = e.kind {
                last_pass_to = to;
            }
            seen += 1;
        }
        if let Some((r, _)) = receipt
            && st.control != Some(r)
            && st.is_live()
        {
            lost += 1;
            receipt = None;
        }
        let close = |acc: &mut [Acc; 2], diag: &mut [[f64; 4]; 2], cd: Option<(f32, f32, u32)>, s: (usize, f32, bool, f32, f32), decided: bool| {
            if let Some((sp, ang, ticks)) = cd {
                let k = (now - s.1 > 1.0) as usize;
                if k == 1 && seed == 1 && s.2 && std::env::var("LIST").is_ok() {
                    eprintln!("slow: player {} control {:.2} decided {decided} at {now:.2} speed {sp:.1} angle {ang:.0}", s.0, s.1);
                }
                diag[k][0] += sp as f64;
                diag[k][1] += ang as f64;
                diag[k][2] += ticks as f64;
                diag[k][3] += 1.0;
            }
            let a = &mut acc[s.2 as usize];
            let dt = (now - s.1) as f64;
            a.n += 1;
            a.sum += dt;
            for (i, lim) in [0.5, 1.0, 2.0].iter().enumerate() {
                a.over[i] += (dt > *lim) as u32;
            }
            a.lost_before += (!decided) as u32;
            a.max_dist += s.4 as f64;
        };
        if let Some(s) = spell {
            let p = &st.players[s.0];
            if st.control != Some(s.0) || p.control_since != s.1 {
                close(&mut acc, &mut diag, cur_diag.take(), s, false);
                spell = None;
            } else if p.next_decision > s.3 + 1e-3 {
                close(&mut acc, &mut diag, cur_diag.take(), s, true);
                spell = None;
            } else {
                let d = (st.ball.ground_pos() - p.pos).length();
                if let Some(cd) = cur_diag.as_mut()
                    && now >= s.3
                    && d <= sim.cfg.player.control_radius
                {
                    cd.2 += 1;
                }
                spell = Some((s.0, s.1, s.2, s.3, s.4.max(d)));
            }
        }
        if spell.is_none()
            && st.is_live()
            && let Some(c) = st.control
        {
            let p = &st.players[c];
            // A fresh open-play spell that started this tick.
            if now - p.control_since < 0.021 && !p.is_gk() && st.possession == side_of(c) {
                spell = Some((c, p.control_since, last_pass_to == Some(c), p.next_decision, 0.0));
                let bv = st.ball.ground_vel();
                let ang = if p.vel.length() > 0.5 && bv.length() > 0.3 { p.vel.angle_to(bv).to_degrees().abs() } else { 0.0 };
                cur_diag = Some((p.vel.length(), ang, 0));
                if last_pass_to == Some(c) {
                    receipt = Some((c, p.control_since));
                }
            }
        }
    }
    if seed == 1 {
        for (k, l) in [(0, "settled ≤ 1 s"), (1, "settled > 1 s")] {
            let d = diag[k];
            let n = d[3].max(1.0);
            eprintln!("seed 1, {l}: {:.0} spells, speed at control {:.1} m/s, run vs touch {:.0}°, ticks at feet with a decision due {:.1}", d[3], d[0] / n, d[1] / n, d[2] / n);
        }
    }
    (acc, to_pass, lost)
}

fn main() {
    let matches: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let res: Vec<([Acc; 2], Vec<f32>, u32)> = std::thread::scope(|scope| {
        let hs: Vec<_> = (1..=matches).map(|seed| scope.spawn(move || play(seed))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let all: Vec<[Acc; 2]> = res.iter().map(|r| r.0).collect();
    let mut dts: Vec<f32> = res.iter().flat_map(|r| r.1.iter().copied()).collect();
    let lost: u32 = res.iter().map(|r| r.2).sum();
    dts.sort_by(f32::total_cmp);
    let n = dts.len().max(1) as f32;
    let q = |f: f32| dts[((dts.len() as f32 * f) as usize).min(dts.len() - 1)];
    let share = |lim: f32| 100.0 * dts.iter().filter(|&&d| d > lim).count() as f32 / n;
    println!(
        "receipt → pass/shot: {:.0} per match, lost first {:.0}%; median {:.2} s, mean {:.2} s, p25 {:.2}, p75 {:.2}, p90 {:.2}; > 1 s {:.0}%, > 2 s {:.0}%, > 3 s {:.0}%, first time {:.0}%   [real: lost ≈ 6%, median 1.22, mean 1.59, p25 0.50, p75 2.18, p90 3.63; 59 / 28 / 15 %, first time 18 %]",
        dts.len() as f32 / matches as f32,
        100.0 * lost as f32 / (n + lost as f32),
        q(0.5),
        dts.iter().sum::<f32>() / n,
        q(0.25),
        q(0.75),
        q(0.9),
        share(1.0),
        share(2.0),
        share(3.0),
        100.0 * dts.iter().filter(|&&d| d <= 0.1).count() as f32 / n
    );
    let bins = [0.0, 0.1, 0.3, 0.5, 0.7, 0.9, 1.1, 1.5, 2.0, 3.0, 99.0];
    let hist: Vec<String> = bins
        .windows(2)
        .map(|w| format!("{:.1}-{:.1} s {:.0} %", w[0], w[1], 100.0 * dts.iter().filter(|&&d| d > w[0] && d <= w[1] || (w[0] == 0.0 && d == 0.0)).count() as f32 / n))
        .collect();
    println!("histogram: {}   [real 18, 4, 3, 5, 7, 8, 15, 12, 13, 15 %]", hist.join(", "));
    for (k, label) in [(1, "received a pass"), (0, "won / loose ball")] {
        let mut a = Acc::default();
        for m in &all {
            let b = m[k];
            a.n += b.n;
            a.sum += b.sum;
            a.lost_before += b.lost_before;
            a.max_dist += b.max_dist;
            for i in 0..3 {
                a.over[i] += b.over[i];
            }
        }
        let n = a.n.max(1) as f64;
        println!(
            "{label:<18} {:6.0} per match: to first decision mean {:.2} s, > 0.5 s {:.0}%, > 1 s {:.0}%, > 2 s {:.0}%; lost before deciding {:.0}%; max ball distance mean {:.2} m",
            a.n as f64 / matches as f64,
            a.sum / n,
            100.0 * a.over[0] as f64 / n,
            100.0 * a.over[1] as f64 / n,
            100.0 * a.over[2] as f64 / n,
            100.0 * a.lost_before as f64 / n,
            a.max_dist / n
        );
    }
}
