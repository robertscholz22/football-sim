//! Goalkeeper bench: shots from fixed spots at the far corner against a keeper that positions himself (GK-01),
//! everyone else out of the way. Prints on-target and save rates per spot.
//! `cargo run -p match_engine --release --example keeper -- [trials]`

use glam::Vec2;
use match_engine::ball::KickSpec;
use match_engine::events::EventKind;
use match_engine::pitch::{GOAL_HALF_WIDTH, HALF_LENGTH, Side};
use match_engine::player::{KickKind, KickPlan};
use match_engine::scenario::Scenario;

fn main() {
    let trials: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(200);
    let shooter = 9; // home striker
    println!("spot (dist, angle)   on target  saved  goals   save% of on-target   goal%");
    for (dist, ang) in [(8.0f32, 0.0f32), (12.0, 0.0), (16.0, 0.0), (16.0, 30.0), (22.0, 0.0), (22.0, 25.0), (28.0, 0.0), (35.0, 0.0)] {
        let (mut on, mut saved, mut goals) = (0, 0, 0);
        for seed in 0..trials {
            let mut sc = Scenario::new(1000 + seed);
            let fr = sc.sim.state.frame(Side::Home);
            // Everyone else far away.
            for id in 0..22 {
                if id != shooter && !sc.sim.state.players[id].is_gk() {
                    let y = if id < 11 { -30.0 } else { 30.0 };
                    sc.place(id, fr.to_world(Vec2::new(-20.0 + id as f32, y)));
                }
            }
            let a = ang.to_radians();
            let spot = Vec2::new(HALF_LENGTH - dist * a.cos(), dist * a.sin());
            sc.place(shooter, fr.to_world(spot));
            sc.place(11, fr.to_world(Vec2::new(HALF_LENGTH - 2.0, 0.0)));
            sc.sim.state.players[shooter].facing = fr.to_world(Vec2::X) * 1.0;
            sc.give_ball(shooter).think();
            sc.sim.state.players[shooter].next_decision = 1e9;
            sc.run(1.0);
            sc.give_ball(shooter);
            let now = sc.sim.state.time;
            let far = if fr.to_team(sc.sim.state.players[11].pos).y > 0.0 { -1.0 } else { 1.0 };
            let target = fr.to_world(Vec2::new(HALF_LENGTH, far * (GOAL_HALF_WIDTH - 0.5)));
            let max = sc.sim.state.players[shooter].kin.max_kick_speed;
            let plan = KickPlan {
                spec: KickSpec::Through { target: target.extend(0.35), speed: 0.8 * max, sidespin: 0.0, topspin: 0.0 },
                kind: KickKind::Shot,
                receiver: None,
                target,
                arrival_t: dist / (0.8 * max),
                xg: 0.1,
                speed: 0.8 * max,
                foot: None,
            };
            let n0 = sc.sim.events.len();
            sc.sim.execute_kick(shooter, plan, false, now);
            let on_goal = sc.sim.state.flight.is_some_and(|f| f.on_goal);
            for _ in 0..150 {
                sc.sim.step();
                if sc.sim.state.flight.is_none() || !sc.sim.state.is_live() {
                    break;
                }
            }
            let ev = &sc.sim.events[n0..];
            on += on_goal as u32;
            if ev.iter().any(|e| matches!(e.kind, EventKind::Goal { .. })) {
                goals += 1;
            } else if on_goal && ev.iter().any(|e| matches!(e.kind, EventKind::Save { .. })) {
                saved += 1;
            }
        }
        let n = trials as f32;
        println!(
            "{dist:>5.0} m {ang:>4.0}°         {:>5.0}%  {:>5.0}%  {:>4.0}%         {:>5.0}%            {:>5.0}%",
            100.0 * on as f32 / n,
            100.0 * saved as f32 / n,
            100.0 * goals as f32 / n,
            100.0 * saved as f32 / on.max(1) as f32,
            100.0 * goals as f32 / n
        );
    }
}
