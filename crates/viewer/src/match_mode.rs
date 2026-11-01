//! Match mode: watch a full simulated match, inspect any player's role, principles and decision breakdown, and
//! change team instructions and pitch conditions live.

use std::collections::VecDeque;

use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, StrokeKind};
use match_engine::ai::analysis::pitch_control;
use match_engine::ai::on_ball::{Action, Situation, evaluate};
use match_engine::ball::BallMode;
use match_engine::config::SimConfig;
use match_engine::match_state::{PLAYERS_PER_TEAM, PlayPhase};
use match_engine::pitch::{HALF_LENGTH, HALF_WIDTH, PitchConditions, Side};
use match_engine::player::{GROUPS, Intent, PlayerId};
use match_engine::team::{BlockHeight, Channel, TeamData};
use match_engine::{Simulation, Vec2, Vec3};

use crate::Catalog;
use crate::view::{self, PitchView};

const SPEEDS: [f32; 7] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];
/// Seconds of ball trail kept.
const TRAIL_S: f32 = 2.0;

pub struct Overlays {
    targets: bool,
    labels: bool,
    block: bool,
    options: bool,
    control: bool,
    trail: bool,
    prediction: bool,
}

pub struct MatchMode {
    sim: Simulation,
    seed: u64,
    home: TeamData,
    away: TeamData,
    playing: bool,
    speed: f32,
    acc: f32,
    selected: Option<PlayerId>,
    trail: VecDeque<Vec3>,
    ov: Overlays,
}

impl MatchMode {
    pub fn new(seed: u64) -> Self {
        let home = TeamData::builtin("northbridge").expect("bundled squad");
        let away = TeamData::builtin("harbour").expect("bundled squad");
        let sim = Simulation::new(&home, &away, SimConfig::default(), PitchConditions::default(), seed);
        Self {
            sim,
            seed,
            home,
            away,
            playing: false,
            speed: 1.0,
            acc: 0.0,
            selected: None,
            trail: VecDeque::new(),
            ov: Overlays { targets: false, labels: true, block: true, options: true, control: false, trail: true, prediction: true },
        }
    }

    /// Play the match up to `seconds` without drawing.
    pub fn fast_forward(&mut self, seconds: f32) {
        while self.sim.state.time < seconds && !self.sim.finished() {
            self.tick();
        }
    }

    pub fn select(&mut self, id: Option<PlayerId>) {
        self.selected = id.filter(|&i| i < 2 * PLAYERS_PER_TEAM);
    }

    fn restart(&mut self, seed: u64) {
        let cond = self.sim.state.conditions;
        let instr = [self.sim.state.teams[0].instructions.clone(), self.sim.state.teams[1].instructions.clone()];
        self.seed = seed;
        self.sim = Simulation::new(&self.home, &self.away, SimConfig::default(), cond, seed);
        for (t, i) in self.sim.state.teams.iter_mut().zip(instr) {
            t.instructions = i;
        }
        self.sim.refresh_tactics();
        self.trail.clear();
        self.acc = 0.0;
    }

    fn tick(&mut self) {
        self.sim.step();
        let b = self.sim.state.ball.pos;
        if self.trail.back().is_some_and(|l| (l.truncate() - b.truncate()).length() > 6.0) {
            self.trail.clear();
        }
        self.trail.push_back(b);
        let keep = (TRAIL_S / self.sim.cfg.timing.dt) as usize;
        while self.trail.len() > keep {
            self.trail.pop_front();
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, catalog: &Catalog) {
        // Advance the simulation in real time × speed.
        let dt = self.sim.cfg.timing.dt;
        if self.playing && !self.sim.finished() {
            self.acc += ui.input(|i| i.stable_dt).min(0.1) * self.speed;
            let mut n = 0;
            while self.acc >= dt && n < 2000 {
                self.tick();
                self.acc -= dt;
                n += 1;
            }
            ui.ctx().request_repaint();
        }
        if ui.input(|i| i.key_pressed(egui::Key::Space)) {
            self.playing = !self.playing;
        }

        egui::Panel::top("match_top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::left("match_left").default_size(270.0).resizable(true).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.left_panel(ui));
        });
        egui::Panel::right("match_right").default_size(380.0).resizable(true).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.right_panel(ui, catalog));
        });
        egui::CentralPanel::default().show(ui, |ui| self.pitch(ui));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(if self.playing { "⏸ Pause" } else { "▶ Play" }).clicked() {
                self.playing = !self.playing;
            }
            if ui.button("Step").on_hover_text("one 20 ms tick").clicked() {
                self.playing = false;
                self.tick();
            }
            if ui.button("+5 s").clicked() {
                for _ in 0..(5.0 / self.sim.cfg.timing.dt) as usize {
                    self.tick();
                }
            }
            egui::ComboBox::from_id_salt("speed").selected_text(format!("{}×", self.speed)).width(64.0).show_ui(ui, |ui| {
                for s in SPEEDS {
                    ui.selectable_value(&mut self.speed, s, format!("{s}×"));
                }
            });
            ui.separator();
            ui.label(format!("seed {}", self.seed));
            if ui.button("New match").clicked() {
                self.restart(self.seed + 1);
            }
            if ui.button("Replay").clicked() {
                self.restart(self.seed);
            }
            ui.separator();
            let st = &self.sim.state;
            let (t, extra) = (st.clock(), st.stoppage());
            // Law 7.3: the added time announced and played.
            let added = match st.added[st.half as usize - 1] {
                Some(a) if extra > 0.0 => format!(" +{}:{:02} (of +{})", (extra / 60.0) as u32, (extra % 60.0) as u32, (a / 60.0).round() as u32),
                Some(a) => format!("  +{} added", (a / 60.0).round() as u32),
                None => String::new(),
            };
            ui.label(
                RichText::new(format!(
                    "{} {} – {} {}   {:02}:{:02}{}  ({} half)",
                    st.teams[0].name,
                    st.score[0],
                    st.score[1],
                    st.teams[1].name,
                    (t / 60.0) as u32,
                    (t % 60.0) as u32,
                    added,
                    if st.half == 1 { "1st" } else { "2nd" }
                ))
                .strong()
                .size(15.0),
            );
            if self.sim.finished() {
                ui.label(RichText::new("FULL TIME").color(Color32::YELLOW));
            }
        });
    }

    fn left_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Overlays");
        let ov = &mut self.ov;
        ui.checkbox(&mut ov.labels, "role / decision labels");
        ui.checkbox(&mut ov.targets, "movement targets and anchors");
        ui.checkbox(&mut ov.block, "defensive block (DEF-05/06)");
        ui.checkbox(&mut ov.options, "carrier's options (P)");
        ui.checkbox(&mut ov.control, "pitch control");
        ui.checkbox(&mut ov.trail, "ball trail");
        ui.checkbox(&mut ov.prediction, "predicted ball path");

        ui.separator();
        ui.heading("Pitch");
        let mut c = self.sim.state.conditions;
        let mut changed = false;
        changed |= ui.add(egui::Slider::new(&mut c.wetness, 0.0..=1.0).text("wetness")).changed();
        changed |= ui.add(egui::Slider::new(&mut c.firmness, 0.0..=1.0).text("firmness")).changed();
        changed |= ui.add(egui::Slider::new(&mut c.grass_length, 0.0..=1.0).text("grass length")).changed();
        changed |= ui.add(egui::Slider::new(&mut c.unevenness, 0.0..=1.0).text("unevenness")).changed();
        if changed {
            self.sim.set_conditions(c);
        }

        for side in [Side::Home, Side::Away] {
            ui.separator();
            let name = self.sim.state.team(side).name.clone();
            ui.heading(format!("{name} — instructions"));
            ui.label(RichText::new("unchecked = layer-1 optimum from the catalog").weak().small());
            let i = &mut self.sim.state.teams[side.index()].instructions;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("block");
                for (v, l) in [(None, "auto"), (Some(BlockHeight::High), "high"), (Some(BlockHeight::Mid), "mid"), (Some(BlockHeight::Low), "low")] {
                    changed |= ui.selectable_value(&mut i.block_height, v, l).changed();
                }
            });
            ui.horizontal(|ui| {
                ui.label("show");
                for (v, l) in [(None, "auto"), (Some(Channel::Wide), "outside"), (Some(Channel::Inside), "inside")] {
                    changed |= ui.selectable_value(&mut i.channel, v, l).changed();
                }
            });
            changed |= opt_slider(ui, "pressing", &mut i.pressing_intensity, 0.0..=1.0, 0.5);
            changed |= opt_slider(ui, "width", &mut i.width, 0.0..=1.0, 0.5);
            changed |= opt_slider(ui, "directness", &mut i.directness, -1.0..=1.0, 0.0);
            changed |= opt_slider(ui, "tempo", &mut i.tempo, -1.0..=1.0, 0.0);
            if changed {
                self.sim.refresh_tactics();
            }
        }
    }

    fn right_panel(&mut self, ui: &mut egui::Ui, catalog: &Catalog) {
        let st = &self.sim.state;
        ui.heading("Teams");
        egui::Grid::new("teams").striped(true).show(ui, |ui| {
            ui.label("");
            ui.label("phase");
            ui.label("shots");
            ui.label("xG");
            ui.label("passes");
            ui.label("poss.");
            ui.end_row();
            let total = (st.teams.iter().enumerate().map(|(k, _)| self.sim.stats.teams[k].possession_s).sum::<f32>()).max(1.0);
            for side in [Side::Home, Side::Away] {
                let t = &self.sim.stats.teams[side.index()];
                ui.label(RichText::new(&st.team(side).short).color(team_color(&self.sim, side)).strong());
                ui.label(format!("{:?}", st.team_view(side).phase).replace("In(", "").replace("Out(", "").replace(')', ""));
                ui.label(format!("{} ({})", t.shots, t.on_target));
                ui.label(format!("{:.2}", t.xg));
                ui.label(format!("{} ({:.0}%)", t.passes, 100.0 * t.pass_completion()));
                ui.label(format!("{:.0}%", 100.0 * t.possession_s / total));
                ui.end_row();
            }
        });

        ui.separator();
        match self.selected {
            None => {
                ui.heading("Player");
                ui.label("Click a player on the pitch to inspect him.");
            }
            Some(id) => self.player_panel(ui, id, catalog),
        }

        ui.separator();
        ui.heading("Events");
        let st = &self.sim.state;
        let name = |id: PlayerId| format!("{} {}", st.players[id].number, st.players[id].name);
        for e in self.sim.events.iter().rev().take(14) {
            ui.label(RichText::new(e.describe(name)).monospace().small());
        }
    }

    fn player_panel(&mut self, ui: &mut egui::Ui, id: PlayerId, catalog: &Catalog) {
        let st = &self.sim.state;
        let p = &st.players[id];
        let team = st.team(p.side);
        let b = &p.beliefs;
        ui.heading(RichText::new(format!("#{} {}", p.number, p.name)).color(team_color(&self.sim, p.side)));
        let role = p.role_in(b.in_possession);
        ui.label(format!("{} ({}) · {} · {}", role.def().name, role.def().id, team.formation.slots[p.slot].label, team.name));
        let duty = if b.in_possession { format!("{:?}", b.att_duties[p.slot]) } else { format!("{:?}", b.def_duties[p.slot]) };
        ui.label(format!("duty: {duty}   ·   doing: {}", p.debug.label));
        ui.label(format!("intent: {}", intent_text(&p.intent)));
        ui.horizontal(|ui| {
            ui.label("confidence");
            ui.add(egui::ProgressBar::new(p.confidence).desired_width(70.0).show_percentage());
            ui.label("role familiarity");
            ui.add(egui::ProgressBar::new(p.familiarity(b.in_possession)).desired_width(70.0).show_percentage());
        });
        ui.label(format!("speed {:.1} m/s   distance {:.0} m   energy {:.0}%", p.vel.length(), p.distance_covered, 100.0 * p.energy));

        ui.add_space(4.0);
        ui.label(RichText::new("Principles behind the current target / decision").strong());
        if p.debug.rules.is_empty() {
            ui.label(RichText::new("—").weak());
        }
        for r in &p.debug.rules {
            ui.label(RichText::new(format!("{r} · {}", catalog.title(r))).small()).on_hover_text(catalog.summary(r));
        }

        ui.add_space(4.0);
        ui.label(RichText::new("Last on-ball decision").strong());
        if p.debug.options.is_empty() {
            ui.label(RichText::new("no decision yet").weak());
        } else {
            ui.label(RichText::new("utility ×100 (≈ % of a goal): optimum (layer 1) + instruction bias (layer 2) + personality (layer 3); P = softmax").weak().small());
            let mut opts = p.debug.options.clone();
            opts.sort_by(|a, b| b.4.total_cmp(&a.4));
            egui::Grid::new("options").striped(true).show(ui, |ui| {
                for h in ["option", "U opt", "instr", "pers", "P"] {
                    ui.label(RichText::new(h).small().strong());
                }
                ui.end_row();
                for (label, u, instr, pers, prob) in opts.iter().take(12) {
                    ui.label(RichText::new(label).small());
                    ui.label(RichText::new(format!("{:+.2}", 100.0 * u)).small().monospace());
                    ui.label(RichText::new(format!("{:+.2}", 100.0 * instr)).small().monospace());
                    ui.label(RichText::new(format!("{:+.2}", 100.0 * pers)).small().monospace());
                    ui.add(egui::ProgressBar::new(*prob).desired_width(60.0).text(format!("{:.0}%", 100.0 * prob)));
                    ui.end_row();
                }
            });
        }

        let a = p.attrs.clone();
        ui.collapsing("Attributes", |ui| {
            for (g, names) in GROUPS {
                ui.label(RichText::new(g).strong().small());
                egui::Grid::new(g).show(ui, |ui| {
                    for (k, n) in names.iter().enumerate() {
                        let v = a.rating(n).unwrap_or(0.0);
                        ui.label(RichText::new(n.replace('_', " ")).small());
                        ui.label(RichText::new(format!("{v:.0}")).small().monospace().color(view::ramp(v / 100.0)));
                        if k % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            }
        });
        ui.collapsing("Player instructions (layer 2)", |ui| {
            let pi = &mut self.sim.state.players[id].instructions;
            opt_slider(ui, "forward runs", &mut pi.forward_runs, -1.0..=1.0, 0.0);
            opt_slider(ui, "come short", &mut pi.come_short, -1.0..=1.0, 0.0);
            opt_slider(ui, "overlap", &mut pi.overlap, -1.0..=1.0, 0.0);
            opt_slider(ui, "invert", &mut pi.invert, -1.0..=1.0, 0.0);
            opt_slider(ui, "dribble more", &mut pi.dribble_freq, -1.0..=1.0, 0.0);
        });
    }

    fn pitch(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        let (pitch_rect, strip_rect) = view::split(resp.rect);
        let v = PitchView::new(pitch_rect);
        let st = &self.sim.state;
        view::draw_pitch(&painter, &v, st.conditions.wetness);

        // Pitch control for the team in possession.
        if self.ov.control {
            let side = st.possession;
            let col = team_color(&self.sim, side);
            let cell = 3.0;
            let mut x = -HALF_LENGTH;
            while x < HALF_LENGTH {
                let mut y = -HALF_WIDTH;
                while y < HALF_WIDTH {
                    let c = pitch_control(st, side, Vec2::new(x + cell / 2.0, y + cell / 2.0));
                    let fill = if c > 0.5 { col } else { team_color(&self.sim, side.opponent()) };
                    let a = ((c - 0.5).abs() * 2.0 * 110.0) as u8;
                    painter.rect_filled(v.rect(Vec2::new(x, y + cell), Vec2::new(x + cell, y)), 0.0, fill.gamma_multiply_u8(a));
                    y += cell;
                }
                x += cell;
            }
        }

        // Defensive block of the team out of possession (DEF-05 / DEF-06 / BLK-02).
        if self.ov.block {
            let side = st.possession.opponent();
            let t = st.team(side);
            let blk = &st.team_view(side).def_block;
            let a = t.frame.to_world(Vec2::new(blk.line_x, blk.centre_y + blk.half_width));
            let b = t.frame.to_world(Vec2::new(blk.front_x, blk.centre_y - blk.half_width));
            let col = team_color(&self.sim, side);
            painter.rect_filled(v.rect(a, b), 4.0, col.gamma_multiply(0.12));
            painter.rect_stroke(v.rect(a, b), 4.0, Stroke::new(1.0, col.gamma_multiply(0.7)), StrokeKind::Middle);
            let line = t.frame.to_world(Vec2::new(blk.line_x, 0.0)).x;
            painter.line_segment([v.to_screen(Vec2::new(line, HALF_WIDTH)), v.to_screen(Vec2::new(line, -HALF_WIDTH))], Stroke::new(1.0, col.gamma_multiply(0.35)));
        }

        // Carrier's options coloured by choice probability.
        if self.ov.options
            && let Some(c) = st.control
            && st.phase == PlayPhase::Live
        {
            let opts = evaluate(st, &self.sim.cfg, &self.sim.plan, Situation::open_play(c, st.ball.pos));
            let from = v.to_screen(st.ball.ground_pos());
            let mut ranked: Vec<_> = opts.iter().collect();
            ranked.sort_by(|a, b| b.prob.total_cmp(&a.prob));
            for (k, o) in ranked.iter().enumerate().rev() {
                let col = view::ramp(o.p).gamma_multiply(0.35 + 0.65 * o.prob.sqrt());
                let w = 1.0 + 4.0 * o.prob;
                let to = match o.action {
                    Action::Kick(plan) => v.to_screen(plan.target),
                    Action::Dribble { dir, .. } => v.to_screen(st.ball.ground_pos() + dir * 5.0),
                    Action::Hold => continue,
                };
                painter.line_segment([from, to], Stroke::new(w, col));
                if k < 3 && o.prob > 0.05 {
                    painter.text(to + egui::vec2(4.0, -4.0), Align2::LEFT_BOTTOM, format!("{:.0}%", 100.0 * o.prob), FontId::proportional(11.0), Color32::WHITE);
                }
            }
        }

        // Players.
        let r = v.len(0.9).max(7.0);
        for p in st.players.iter().filter(|p| !p.sent_off) {
            let pos = v.to_screen(p.pos);
            let col = if p.is_gk() { gk_color(&self.sim, p.side) } else { team_color(&self.sim, p.side) };
            if self.ov.targets {
                painter.line_segment([pos, v.to_screen(p.target)], Stroke::new(1.0, col.gamma_multiply(0.6)));
                let a = v.to_screen(p.debug.anchor);
                painter.line_segment([a - egui::vec2(3.0, 3.0), a + egui::vec2(3.0, 3.0)], Stroke::new(1.0, col));
                painter.line_segment([a - egui::vec2(3.0, -3.0), a + egui::vec2(3.0, -3.0)], Stroke::new(1.0, col));
            }
            painter.circle_filled(pos, r, col);
            painter.circle_stroke(pos, r, Stroke::new(1.0, Color32::from_black_alpha(160)));
            painter.line_segment([pos + view::egui_vec(p.facing) * r, pos + view::egui_vec(p.facing) * (r + 5.0)], Stroke::new(2.0, Color32::WHITE));
            painter.text(pos, Align2::CENTER_CENTER, p.number.to_string(), FontId::proportional(r * 1.05), text_on(col));
            if st.control == Some(p.id) {
                painter.circle_stroke(pos, r + 3.0, Stroke::new(1.5, Color32::WHITE));
            }
            if p.cautioned {
                let c = pos + egui::vec2(r * 0.8, -r * 0.8);
                painter.rect_filled(egui::Rect::from_min_size(c, egui::vec2(4.0, 6.0)), 0.0, Color32::YELLOW);
            }
            if self.selected == Some(p.id) {
                painter.circle_stroke(pos, r + 6.0, Stroke::new(2.0, Color32::YELLOW));
            }
            if self.ov.labels {
                painter.text(pos + egui::vec2(0.0, r + 2.0), Align2::CENTER_TOP, &p.debug.label, FontId::proportional(10.0), Color32::from_white_alpha(200));
            }
        }

        // Ball.
        let path: Vec<Vec3> = if self.ov.prediction && st.control.is_none() && st.ball.mode != BallMode::Held && st.ball.speed() > 1.0 {
            self.sim.traj.samples.iter().map(|s| s.pos).collect()
        } else {
            Vec::new()
        };
        let trail: Vec<Vec3> = if self.ov.trail { self.trail.iter().copied().collect() } else { Vec::new() };
        view::draw_prediction(&painter, &v, &path, view::PREDICTION);
        view::draw_trail(&painter, &v, &trail);
        view::draw_ball(&painter, &v, st.ball.pos);
        view::draw_elevation(&painter, strip_rect, &v, &trail, &path, st.ball.pos);

        // Overlay text.
        if let PlayPhase::Restart { kind, .. } = st.phase {
            painter.text(pitch_rect.left_top() + egui::vec2(8.0, 6.0), Align2::LEFT_TOP, format!("{kind:?}"), FontId::proportional(14.0), Color32::WHITE);
        } else if let PlayPhase::GoalScored { .. } = st.phase {
            painter.text(pitch_rect.center_top() + egui::vec2(0.0, 6.0), Align2::CENTER_TOP, "GOAL", FontId::proportional(28.0), Color32::YELLOW);
        }

        // Select a player by clicking.
        if resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let w = v.to_world(pos);
            self.selected = (0..2 * PLAYERS_PER_TEAM)
                .map(|i| (i, (st.players[i].pos - w).length()))
                .filter(|(_, d)| *d < 3.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i);
        }
    }
}

/// Checkbox "override" plus slider; unchecked = `None` (layer-1 optimum). Returns whether anything changed.
fn opt_slider(ui: &mut egui::Ui, label: &str, v: &mut Option<f32>, range: std::ops::RangeInclusive<f32>, default: f32) -> bool {
    ui.horizontal(|ui| {
        let mut on = v.is_some();
        let mut changed = ui.checkbox(&mut on, label).changed();
        if on && v.is_none() {
            *v = Some(default);
        } else if !on {
            *v = None;
        }
        match v {
            Some(x) => changed |= ui.add(egui::Slider::new(x, range)).changed(),
            None => {
                ui.label(RichText::new("auto").weak());
            }
        }
        changed
    })
    .inner
}

fn team_color(sim: &Simulation, side: Side) -> Color32 {
    let [r, g, b] = sim.state.team(side).color;
    Color32::from_rgb(r, g, b)
}

fn gk_color(sim: &Simulation, side: Side) -> Color32 {
    let c = team_color(sim, side);
    // Keepers wear a contrasting kit: shift towards yellow / cyan.
    if side == Side::Home { Color32::from_rgb(250, 200, 40) } else { Color32::from_rgb(60, 220, 210) }.lerp_to_gamma(c, 0.2)
}

fn text_on(c: Color32) -> Color32 {
    let l = 0.3 * c.r() as f32 + 0.59 * c.g() as f32 + 0.11 * c.b() as f32;
    if l > 150.0 { Color32::BLACK } else { Color32::WHITE }
}

fn intent_text(i: &Intent) -> String {
    match i {
        Intent::Position => "position".into(),
        Intent::Chase => "chase the ball".into(),
        Intent::Receive { t, .. } => format!("receive (at {t:.1} s)"),
        Intent::Carry { speed, .. } => format!("carry the ball ({speed:.1} m/s)"),
        Intent::Hold => "hold / shield".into(),
        Intent::SetUp { .. } => "set up the kick".into(),
        Intent::GkHold { .. } => "keeper holding the ball".into(),
    }
}
