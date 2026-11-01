//! Match statistics and the calibration metrics from the principle catalog (see the "Calibration targets" table in
//! `docs/tactics/principles.md`).

use std::collections::VecDeque;

use glam::Vec2;

use crate::ai::brain::{InPhase, OutPhase, TeamPhase};
use crate::config::{SimConfig, ValueGrid};
use crate::match_state::{MatchState, side_of};
use crate::pitch::{HALF_LENGTH, LENGTH, Side};

#[derive(Clone, Debug, Default)]
pub struct TeamStats {
    pub goals: u32,
    pub shots: u32,
    pub on_target: u32,
    pub xg: f32,
    pub passes: u32,
    pub passes_completed: u32,
    pub possession_s: f32,
    pub tackles: u32,
    pub tackles_won: u32,
    /// Tackles by why the defender committed (first that applies): BOX-01 shot, IND-03 last ditch, loose touch,
    /// duel odds (ball nearer him), duel odds (controlled ball), aggression.
    pub tackle_reasons: [u32; 6],
    pub interceptions: u32,
    pub saves: u32,
    pub corners: u32,
    pub throw_ins: u32,
    pub goal_kicks: u32,
    /// Laws (M7). Fouls committed, by the third the fouler is in (own, middle, attacking); advantages played after
    /// fouls on the team; cautions and sendings-off (a second caution counts as both); offsides.
    pub fouls: u32,
    pub fouls_by_third: [u32; 3],
    /// TRN-04 tactical fouls among them.
    pub fouls_tactical: u32,
    pub advantages: u32,
    pub yellows: u32,
    pub reds: u32,
    pub offsides: u32,
    /// Restarts awarded: free kicks (direct and indirect), penalty kicks; penalty goals.
    pub free_kicks: u32,
    pub penalties: u32,
    pub penalty_goals: u32,
    /// Direct free kicks shot at goal (and their xG), on target, scored.
    pub fk_shots: u32,
    pub fk_xg: f32,
    pub fk_on_target: u32,
    pub fk_goals: u32,
    /// Corners played short (to a target outside the penalty area); shots and goals within 10 s of a corner.
    pub corners_short: u32,
    pub corner_shots: u32,
    pub corner_goals: u32,
    /// Goal kicks played short (< 32 yd, ≈ 29 m).
    pub goal_kicks_short: u32,
    /// Substitutions, and the match clock (min) of each.
    pub subs: u32,
    pub sub_minutes: Vec<f32>,
}

impl TeamStats {
    pub fn pass_completion(&self) -> f32 {
        if self.passes == 0 { 0.0 } else { self.passes_completed as f32 / self.passes as f32 }
    }
}

impl TeamStats {
    /// Add `o` to the totals (the lab's sums over matches).
    pub fn merge(&mut self, o: &TeamStats) {
        let TeamStats {
            goals,
            shots,
            on_target,
            xg,
            passes,
            passes_completed,
            possession_s,
            tackles,
            tackles_won,
            tackle_reasons,
            interceptions,
            saves,
            corners,
            throw_ins,
            goal_kicks,
            fouls,
            fouls_by_third,
            fouls_tactical,
            advantages,
            yellows,
            reds,
            offsides,
            free_kicks,
            penalties,
            penalty_goals,
            fk_shots,
            fk_xg,
            fk_on_target,
            fk_goals,
            corners_short,
            corner_shots,
            corner_goals,
            goal_kicks_short,
            subs,
            sub_minutes,
        } = o;
        self.goals += goals;
        self.shots += shots;
        self.on_target += on_target;
        self.xg += xg;
        self.passes += passes;
        self.passes_completed += passes_completed;
        self.possession_s += possession_s;
        self.tackles += tackles;
        self.tackles_won += tackles_won;
        for (a, b) in self.tackle_reasons.iter_mut().zip(tackle_reasons) {
            *a += b;
        }
        self.interceptions += interceptions;
        self.saves += saves;
        self.corners += corners;
        self.throw_ins += throw_ins;
        self.goal_kicks += goal_kicks;
        self.fouls += fouls;
        self.fouls_tactical += fouls_tactical;
        for (a, b) in self.fouls_by_third.iter_mut().zip(fouls_by_third) {
            *a += b;
        }
        self.advantages += advantages;
        self.yellows += yellows;
        self.reds += reds;
        self.offsides += offsides;
        self.free_kicks += free_kicks;
        self.penalties += penalties;
        self.penalty_goals += penalty_goals;
        self.fk_shots += fk_shots;
        self.fk_xg += fk_xg;
        self.fk_on_target += fk_on_target;
        self.fk_goals += fk_goals;
        self.corners_short += corners_short;
        self.corner_shots += corner_shots;
        self.corner_goals += corner_goals;
        self.goal_kicks_short += goal_kicks_short;
        self.subs += subs;
        self.sub_minutes.extend(sub_minutes);
    }
}

/// Emergent tactical metrics compared against published FIFA/UEFA figures.
#[derive(Clone, Debug, Default)]
pub struct Calibration {
    /// BLK-01 mid-block (organised, ≥ 4 behind the ball): team length / width (DEF-05).
    pub mid_block_samples: u32,
    pub mid_block_length_sum: f32,
    pub mid_block_width_sum: f32,
    pub mid_block_line_sum: f32,
    pub low_block_samples: u32,
    pub low_block_length_sum: f32,
    pub low_block_width_sum: f32,
    pub low_block_line_sum: f32,
    /// ATT-11 in-possession shape while a player controls the ball, by phase (build-up, progression, final third):
    /// samples, outfield length / width, deepest outfield player's distance from the own goal line.
    pub in_shape: [(u32, f32, f32, f32); 3],
    /// GK-03 goalkeeper depth.
    pub gk_samples: u32,
    pub gk_goal_dist_sum: f32,
    pub gk_line_dist_sum: f32,
    /// PRS-06 pressers per press (time-weighted histogram: 1, 2, 3, 4, 5+).
    pub pressers_hist: [u32; 5],
    /// TRN-02 counter-press episodes.
    pub counter_presses: u32,
    pub counter_press_duration_sum: f32,
    pub counter_press_regains: u32,
    /// Goals: total, one-touch (FT-04), from moves of 1–5 passes.
    pub goals: u32,
    pub goals_one_touch: u32,
    pub goals_short_moves: u32,
    /// TR-01 counter-attacks.
    pub counters: u32,
    pub counter_duration_sum: f32,
    pub counter_distance_sum: f32,
    /// Longest spell of live play (s) with the ball within 1 m of one spot: a guard against stuck states (a carrier
    /// held the ball behind his goal line for 35 minutes, a ball rested on the crossbar for 15).
    pub longest_still_s: f32,
    /// Fatigue (M6), by 15-minute period of the match clock (the last of each half with its added time): distance (m)
    /// of the outfield players who started the match and are still on, the part of it above the high-intensity speed
    /// (`PlayerConfig::fatigue_hi_speed`), and the player-seconds sampled (rates per player-minute compare the
    /// periods; like the tracking studies' full-match players, substitutes are left out); goals.
    pub run_m: [f32; 6],
    pub hi_m: [f32; 6],
    pub run_s: [f32; 6],
    pub goals_by_period: [u32; 6],
    pub shots_by_period: [u32; 6],
    /// Confidence (M6), per side: passes (attempted, completed) within `AFTER_GOAL_S` of conceding; goals within
    /// `AFTER_GOAL_S` of the side's previous goal and the time spent in that window; match time.
    pub passes_after_conceding: [[u32; 2]; 2],
    pub goals_after_scoring: [u32; 2],
    pub after_scoring_s: [f32; 2],
    pub team_s: f32,
    /// ATT-05 scanning (M6): scans in the 10 s before gaining control, summed over receptions; passes (attempted,
    /// completed) by the passer's scans in the 10 s before he received (0–2, 3–4, 5+).
    pub scans_before_receiving: u32,
    pub receptions: u32,
    pub passes_by_scans: [[u32; 2]; 3],
    /// Concentration lapses (M6).
    pub lapses: u32,
    /// Calls (M6): claims, line calls, "mine", "man on".
    pub calls: [u32; 4],
    /// Law 7.3: added time announced (s, first and second half) and played (s, both halves).
    pub added_shown_s: [f32; 2],
    pub stoppage_played_s: f32,
    /// Self-play possession value (M8), per cell of a `ValueGrid` of `VALUE_CELL_M` squares: possessions that had the
    /// ball under control in the cell, and those of them that ended in a goal (the first visit per possession counts;
    /// the possession running at full time is left out).
    pub value_visits: Vec<u32>,
    pub value_goals: Vec<u32>,
}

impl Calibration {
    pub fn merge(&mut self, o: &Calibration) {
        self.mid_block_samples += o.mid_block_samples;
        self.mid_block_length_sum += o.mid_block_length_sum;
        self.mid_block_width_sum += o.mid_block_width_sum;
        self.mid_block_line_sum += o.mid_block_line_sum;
        self.low_block_samples += o.low_block_samples;
        self.low_block_length_sum += o.low_block_length_sum;
        self.low_block_width_sum += o.low_block_width_sum;
        self.low_block_line_sum += o.low_block_line_sum;
        for (a, b) in self.in_shape.iter_mut().zip(&o.in_shape) {
            (a.0, a.1, a.2, a.3) = (a.0 + b.0, a.1 + b.1, a.2 + b.2, a.3 + b.3);
        }
        self.gk_samples += o.gk_samples;
        self.gk_goal_dist_sum += o.gk_goal_dist_sum;
        self.gk_line_dist_sum += o.gk_line_dist_sum;
        for i in 0..5 {
            self.pressers_hist[i] += o.pressers_hist[i];
        }
        self.counter_presses += o.counter_presses;
        self.counter_press_duration_sum += o.counter_press_duration_sum;
        self.counter_press_regains += o.counter_press_regains;
        self.goals += o.goals;
        self.goals_one_touch += o.goals_one_touch;
        self.goals_short_moves += o.goals_short_moves;
        self.counters += o.counters;
        self.counter_duration_sum += o.counter_duration_sum;
        self.counter_distance_sum += o.counter_distance_sum;
        self.longest_still_s = self.longest_still_s.max(o.longest_still_s);
        for i in 0..6 {
            self.run_m[i] += o.run_m[i];
            self.hi_m[i] += o.hi_m[i];
            self.run_s[i] += o.run_s[i];
            self.goals_by_period[i] += o.goals_by_period[i];
            self.shots_by_period[i] += o.shots_by_period[i];
        }
        for s in 0..2 {
            for k in 0..2 {
                self.passes_after_conceding[s][k] += o.passes_after_conceding[s][k];
            }
            self.goals_after_scoring[s] += o.goals_after_scoring[s];
            self.after_scoring_s[s] += o.after_scoring_s[s];
        }
        self.team_s += o.team_s;
        self.scans_before_receiving += o.scans_before_receiving;
        self.receptions += o.receptions;
        self.lapses += o.lapses;
        for k in 0..4 {
            self.calls[k] += o.calls[k];
        }
        for h in 0..2 {
            self.added_shown_s[h] += o.added_shown_s[h];
        }
        self.stoppage_played_s += o.stoppage_played_s;
        for g in 0..3 {
            for k in 0..2 {
                self.passes_by_scans[g][k] += o.passes_by_scans[g][k];
            }
        }
        for (a, b) in [(&mut self.value_visits, &o.value_visits), (&mut self.value_goals, &o.value_goals)] {
            a.resize(a.len().max(b.len()), 0);
            for (x, y) in a.iter_mut().zip(b) {
                *x += y;
            }
        }
    }
}

fn mean(sum: f32, n: u32) -> f32 {
    if n == 0 { f32::NAN } else { sum / n as f32 }
}

impl Calibration {
    pub fn mid_block_length(&self) -> f32 {
        mean(self.mid_block_length_sum, self.mid_block_samples)
    }
    pub fn mid_block_width(&self) -> f32 {
        mean(self.mid_block_width_sum, self.mid_block_samples)
    }
    pub fn low_block_length(&self) -> f32 {
        mean(self.low_block_length_sum, self.low_block_samples)
    }
    /// Deepest outfield player's distance from the own goal line in a mid-block / low block.
    pub fn mid_block_line(&self) -> f32 {
        mean(self.mid_block_line_sum, self.mid_block_samples)
    }
    pub fn low_block_width(&self) -> f32 {
        mean(self.low_block_width_sum, self.low_block_samples)
    }
    pub fn low_block_line(&self) -> f32 {
        mean(self.low_block_line_sum, self.low_block_samples)
    }
    /// In-possession (length, width, line height) for phase 0 = build-up, 1 = progression, 2 = final third.
    pub fn in_shape(&self, phase: usize) -> (f32, f32, f32) {
        let (n, l, w, h) = self.in_shape[phase];
        (mean(l, n), mean(w, n), mean(h, n))
    }
    pub fn gk_goal_dist(&self) -> f32 {
        mean(self.gk_goal_dist_sum, self.gk_samples)
    }
    pub fn gk_line_dist(&self) -> f32 {
        mean(self.gk_line_dist_sum, self.gk_samples)
    }
    pub fn counter_press_duration(&self) -> f32 {
        mean(self.counter_press_duration_sum, self.counter_presses)
    }
    pub fn counter_duration(&self) -> f32 {
        mean(self.counter_duration_sum, self.counters)
    }
    pub fn counter_distance(&self) -> f32 {
        mean(self.counter_distance_sum, self.counters)
    }
    /// Shares of presses with 1, 2–3 and 4+ pressers.
    pub fn presser_shares(&self) -> (f32, f32, f32) {
        let n: u32 = self.pressers_hist.iter().sum();
        if n == 0 {
            return (f32::NAN, f32::NAN, f32::NAN);
        }
        let n = n as f32;
        let h = &self.pressers_hist;
        (h[0] as f32 / n, (h[1] + h[2]) as f32 / n, (h[3] + h[4]) as f32 / n)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub teams: [TeamStats; 2],
    pub cal: Calibration,
    /// Passes by the team in possession since it gained the ball.
    pub passes_in_possession: [u32; 2],
    stationary: [VecDeque<bool>; 2],
    counter_press_start: [Option<f32>; 2],
    counter_start: [Option<(f32, f32)>; 2],
    /// Where the ball has stood since when, and the last sample (a gap in sampling is a stoppage: start again).
    still: Option<(Vec2, f32, f32)>,
    /// Cells (`VALUE_CELL_M`) the ball has been in under each side's control in its current possession.
    value_seen: [Vec<usize>; 2],
}

/// The window (s) after a goal in which the confidence statistics count.
pub const AFTER_GOAL_S: f32 = 600.0;

/// Cell size (m) of the self-play possession value counts (M8).
pub const VALUE_CELL_M: f32 = 3.0;

/// The 15-minute period (0..6) of the match clock; added time counts in the half's last period.
pub fn period(state: &MatchState) -> usize {
    let k = ((state.half_elapsed() / (state.half_length / 3.0)) as usize).min(2);
    3 * (state.half as usize - 1) + k
}

impl Stats {
    /// Sample the outfield players' running (fatigue, M6); called every `dt` seconds, live or not.
    pub fn sample_running(&mut self, state: &MatchState, cfg: &SimConfig, dt: f32) {
        let k = period(state);
        for p in state.players.iter().filter(|p| !p.is_gk() && !p.sent_off && !p.substitute) {
            let v = p.speed();
            self.cal.run_s[k] += dt;
            self.cal.run_m[k] += v * dt;
            if v > cfg.player.fatigue_hi_speed {
                self.cal.hi_m[k] += v * dt;
            }
        }
    }

    /// A goal for `scoring` ends both sides' possessions for the self-play value counts (M8).
    pub fn value_goal(&mut self, scoring: Side) {
        self.end_possession(scoring, true);
        self.end_possession(scoring.opponent(), false);
    }

    fn end_possession(&mut self, side: Side, goal: bool) {
        let (nx, ny) = ValueGrid::dims(VALUE_CELL_M);
        let cal = &mut self.cal;
        cal.value_visits.resize(nx * ny, 0);
        cal.value_goals.resize(nx * ny, 0);
        for c in self.value_seen[side.index()].drain(..) {
            cal.value_visits[c] += 1;
            cal.value_goals[c] += u32::from(goal);
        }
    }

    /// Sample the tactical metrics; called every `dt` seconds while the ball is live.
    pub fn sample(&mut self, state: &MatchState, cfg: &SimConfig, dt: f32) {
        let tc = &cfg.tactics;
        self.teams[state.possession.index()].possession_s += dt;
        // M8 self-play value: a change of possession ends the other side's; the cell of a controlled ball counts once.
        let side = state.possession;
        if !self.value_seen[side.opponent().index()].is_empty() {
            self.end_possession(side.opponent(), false);
        }
        if state.control.is_some_and(|c| side_of(c) == side) {
            let cell = ValueGrid::cell_of(VALUE_CELL_M, state.ball_in(side));
            if !self.value_seen[side.index()].contains(&cell) {
                self.value_seen[side.index()].push(cell);
            }
        }
        let (b, now) = (state.ball.ground_pos(), state.time);
        self.still = match self.still {
            Some((at, t0, last)) if (b - at).length() < 1.0 && now - last < 1.5 * dt => {
                self.cal.longest_still_s = self.cal.longest_still_s.max(now - t0);
                Some((at, t0, now))
            }
            _ => Some((b, now, now)),
        };
        for side in [Side::Home, Side::Away] {
            let si = side.index();
            let fr = state.frame(side);
            let squad = state.squad(side);
            let brain = state.team_view(side);
            let outfield: Vec<_> = squad.iter().filter(|p| !p.is_gk() && !p.sent_off).collect();
            let pos: Vec<_> = outfield.iter().map(|p| fr.to_team(p.pos)).collect();
            let ball = state.ball_in(side);

            // GK-03 goalkeeper depth.
            if let Some(gk) = squad.iter().find(|p| p.is_gk()) {
                let g = fr.to_team(gk.pos);
                let line = pos.iter().map(|q| q.x).fold(f32::MAX, f32::min);
                self.cal.gk_samples += 1;
                self.cal.gk_goal_dist_sum += g.x + HALF_LENGTH;
                self.cal.gk_line_dist_sum += line - g.x;
            }

            // ATT-11 in-possession shape.
            let in_phase = match brain.phase {
                TeamPhase::In(InPhase::BuildUp) => Some(0),
                TeamPhase::In(InPhase::Progression) => Some(1),
                TeamPhase::In(InPhase::FinalThird) => Some(2),
                _ => None,
            };
            if let Some(ph) = in_phase
                && state.control.is_some_and(|c| side_of(c) == side)
            {
                let (lo_x, hi_x) = pos.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.x), b.max(q.x)));
                let (lo_y, hi_y) = pos.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.y), b.max(q.y)));
                let e = &mut self.cal.in_shape[ph];
                (e.0, e.1, e.2, e.3) = (e.0 + 1, e.1 + hi_x - lo_x, e.2 + hi_y - lo_y, e.3 + lo_x + HALF_LENGTH);
            }

            let out_of_poss = state.possession != side;
            // BLK-01: organised = stationary (< 10 km/h) for ≥ 1.6 s of the last 2 s.
            let window = (tc.blk01_organised_window_s / dt).round() as usize;
            let mean_speed = outfield.iter().map(|p| p.speed()).sum::<f32>() / outfield.len().max(1) as f32;
            let st = &mut self.stationary[si];
            st.push_back(out_of_poss && mean_speed < tc.blk01_stationary_speed);
            while st.len() > window {
                st.pop_front();
            }
            let organised = st.iter().filter(|&&s| s).count() as f32 * dt >= tc.blk01_organised_min_s;
            if out_of_poss && organised {
                let behind = pos.iter().filter(|q| q.x < ball.x).count();
                // Thirds of 35 m: the middle third is |x| < 17.5 m.
                let half_third = LENGTH / 6.0;
                let n_mid = pos.iter().filter(|q| q.x.abs() < half_third).count();
                let n_def = pos.iter().filter(|q| q.x <= -half_third).count();
                let (lo_x, hi_x) = pos.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.x), b.max(q.x)));
                let (lo_y, hi_y) = pos.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.y), b.max(q.y)));
                if behind >= tc.blk01_min_behind_ball {
                    let min = tc.blk01_block_min_players;
                    if n_def >= min {
                        self.cal.low_block_samples += 1;
                        self.cal.low_block_length_sum += hi_x - lo_x;
                        self.cal.low_block_width_sum += hi_y - lo_y;
                        self.cal.low_block_line_sum += lo_x + HALF_LENGTH;
                    } else if n_mid >= min || n_mid + n_def >= min {
                        self.cal.mid_block_samples += 1;
                        self.cal.mid_block_length_sum += hi_x - lo_x;
                        self.cal.mid_block_width_sum += hi_y - lo_y;
                        self.cal.mid_block_line_sum += lo_x + HALF_LENGTH;
                    }
                }
            }

            // PRS-06 pressers: defenders within 8 m of the opposing carrier closing in, or within 3 m.
            if out_of_poss
                && brain.press
                && let Some(c) = state.control.filter(|&c| side_of(c) != side)
            {
                let cp = state.players[c].pos;
                let n = outfield
                    .iter()
                    .filter(|p| {
                        let d = cp - p.pos;
                        let dist = d.length();
                        dist < 3.0 || (dist < 8.0 && p.vel.dot(d / dist.max(0.01)) > 1.0)
                    })
                    .count();
                if n > 0 {
                    self.cal.pressers_hist[(n - 1).min(4)] += 1;
                }
            }

            // TRN-02 counter-press episodes.
            let cp_now = brain.phase == TeamPhase::Out(OutPhase::CounterPress);
            match (self.counter_press_start[si], cp_now) {
                (None, true) => self.counter_press_start[si] = Some(state.time),
                (Some(t0), false) => {
                    self.cal.counter_presses += 1;
                    self.cal.counter_press_duration_sum += state.time - t0;
                    if brain.in_possession {
                        self.cal.counter_press_regains += 1;
                    }
                    self.counter_press_start[si] = None;
                }
                _ => {}
            }

            // TR-01 counter-attacks.
            let ca_now = brain.phase == TeamPhase::In(InPhase::CounterAttack);
            match (self.counter_start[si], ca_now) {
                (None, true) => self.counter_start[si] = Some((state.time, ball.x)),
                (Some((t0, x0)), false) => {
                    self.cal.counters += 1;
                    self.cal.counter_duration_sum += state.time - t0;
                    self.cal.counter_distance_sum += (brain.counter_progress_x - x0).max(0.0);
                    self.counter_start[si] = None;
                }
                _ => {}
            }
        }
    }
}
