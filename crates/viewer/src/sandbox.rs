//! Physics sandbox: press on the pitch to place the ball, drag to aim (length = power), release to kick.
//! Loft and spin come from the sliders; pitch conditions change the bounce and roll. The dashed line is the
//! prediction on an ideal pitch (what the players plan with); the trail is what the bumpy pitch actually did.

use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke};
use match_engine::ball::{Ball, BallEvent, BallMode, BallParams, SurfaceField, predict, step_ball};
use match_engine::ball::physics::SurfaceInput;
use match_engine::config::SimConfig;
use match_engine::pitch::PitchConditions;
use match_engine::rng::SimRng;
use match_engine::{Vec2, Vec3};

use crate::view::{self, PitchView};

const SPEEDS: [f32; 6] = [0.0, 0.1, 0.25, 0.5, 1.0, 2.0];
/// Kick speed per metre of drag (m/s).
const POWER_PER_M: f32 = 1.2;
const MAX_SPEED: f32 = 36.0;

#[derive(Clone, Copy)]
struct Kick {
    from: Vec2,
    to: Vec2,
    vel: Vec3,
    spin: Vec3,
}

pub struct Sandbox {
    cfg: SimConfig,
    cond: PitchConditions,
    live: BallParams,
    plan: BallParams,
    surface: SurfaceField,
    rng: SimRng,
    ball: Ball,
    loft_deg: f32,
    sidespin: f32,
    topspin: f32,
    speed: f32,
    acc: f32,
    aim: Option<(Vec2, Vec2)>,
    last: Option<Kick>,
    trail: Vec<Vec3>,
    ideal: Vec<Vec3>,
    events: Vec<BallEvent>,
    note: String,
    max_h: f32,
    t: f32,
}

impl Sandbox {
    pub fn new() -> Self {
        let cfg = SimConfig::default();
        let cond = PitchConditions::default();
        let (live, plan) = params(&cfg, &cond);
        Self {
            cfg,
            cond,
            live,
            plan,
            surface: SurfaceField::new(7),
            rng: SimRng::new(7),
            ball: Ball::at_rest(Vec2::new(-20.0, 0.0)),
            loft_deg: 12.0,
            sidespin: 0.0,
            topspin: 0.0,
            speed: 1.0,
            acc: 0.0,
            aim: None,
            last: None,
            trail: Vec::new(),
            ideal: Vec::new(),
            events: Vec::new(),
            note: String::new(),
            max_h: 0.0,
            t: 0.0,
        }
    }

    /// An in-swinging cross from the right wing, stopped in mid-flight (for screenshots).
    pub fn demo(&mut self) {
        (self.loft_deg, self.sidespin, self.topspin) = (18.0, 55.0, -5.0);
        let k = self.kick_from(Vec2::new(22.0, -31.0), Vec2::new(38.0, -12.0));
        self.kick(k);
        for _ in 0..55 {
            self.step(self.cfg.timing.dt);
        }
        self.speed = 0.0;
    }

    fn kick_from(&self, from: Vec2, to: Vec2) -> Kick {
        let d = to - from;
        let speed = (d.length() * POWER_PER_M).min(MAX_SPEED);
        let dir = d.normalize_or(Vec2::X);
        let loft = self.loft_deg.to_radians();
        let vel = (dir * speed * loft.cos()).extend(speed * loft.sin());
        // +z spin curves left; topspin about z × dir (Magnus pushes the ball down).
        let spin = Vec3::Z * self.sidespin + Vec3::new(-dir.y, dir.x, 0.0) * self.topspin;
        Kick { from, to, vel, spin }
    }

    fn kick(&mut self, k: Kick) {
        self.ball = Ball::at_rest(k.from);
        self.ball.kick(k.vel, k.spin);
        self.ideal = predict(&self.ball, &self.plan, 8.0, 0.02).samples.iter().map(|s| s.pos).collect();
        self.trail = vec![self.ball.pos];
        self.events.clear();
        self.note.clear();
        self.max_h = 0.0;
        self.t = 0.0;
        self.last = Some(k);
    }

    fn step(&mut self, dt: f32) {
        let n = self.cfg.timing.ball_substeps.max(1);
        let sub = dt / n as f32;
        for _ in 0..n {
            let surface = SurfaceInput { field: &self.surface, rng: &mut self.rng };
            step_ball(&mut self.ball, sub, &self.live, Some(surface), &mut self.events);
        }
        self.t += dt;
        self.max_h = self.max_h.max(self.ball.height());
        self.trail.push(self.ball.pos);
        for e in self.events.drain(..) {
            self.note = match e {
                BallEvent::Goal { .. } => "GOAL".into(),
                BallEvent::Woodwork { .. } => "off the woodwork".into(),
                BallEvent::Out { over_goal_line, .. } => if over_goal_line { "over the goal line" } else { "over the touchline" }.into(),
                BallEvent::Bounce { .. } => continue,
            };
        }
    }

    fn moving(&self) -> bool {
        self.ball.mode != BallMode::Held && (self.ball.mode == BallMode::Air || self.ball.speed() > 0.02)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let dt = self.cfg.timing.dt;
        if self.moving() {
            self.acc += ui.input(|i| i.stable_dt).min(0.1) * self.speed;
            while self.acc >= dt {
                self.step(dt);
                self.acc -= dt;
            }
            ui.ctx().request_repaint();
        } else {
            self.acc = 0.0;
        }
        egui::Panel::left("sandbox_left").default_size(270.0).resizable(true).show(ui, |ui| self.controls(ui));
        egui::CentralPanel::default().show(ui, |ui| self.pitch(ui));
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Kick");
        ui.label(RichText::new("Press on the pitch to place the ball, drag to aim (length = power), release to kick.").small());
        ui.add(egui::Slider::new(&mut self.loft_deg, 0.0..=60.0).text("loft °"));
        ui.add(egui::Slider::new(&mut self.sidespin, -60.0..=60.0).text("sidespin rad/s (+ curls left)"));
        ui.add(egui::Slider::new(&mut self.topspin, -40.0..=40.0).text("topspin rad/s (− = backspin)"));
        ui.horizontal(|ui| {
            for (l, loft, side, top) in [("pass", 0.0, 0.0, 0.0), ("driven", 8.0, 0.0, 10.0), ("lofted", 35.0, 0.0, -15.0), ("curler", 12.0, 45.0, 5.0), ("chip", 50.0, 0.0, -30.0)] {
                if ui.button(l).clicked() {
                    (self.loft_deg, self.sidespin, self.topspin) = (loft, side, top);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(self.last.is_some(), egui::Button::new("Repeat last kick")).on_hover_text("same kick, current pitch").clicked()
                && let Some(k) = self.last
            {
                self.kick(k);
            }
            ui.label("speed");
            egui::ComboBox::from_id_salt("sb_speed").selected_text(format!("{}×", self.speed)).width(60.0).show_ui(ui, |ui| {
                for s in SPEEDS {
                    ui.selectable_value(&mut self.speed, s, format!("{s}×"));
                }
            });
        });

        ui.separator();
        ui.heading("Pitch");
        let mut c = self.cond;
        let mut changed = false;
        changed |= ui.add(egui::Slider::new(&mut c.wetness, 0.0..=1.0).text("wetness")).changed();
        changed |= ui.add(egui::Slider::new(&mut c.firmness, 0.0..=1.0).text("firmness")).changed();
        changed |= ui.add(egui::Slider::new(&mut c.grass_length, 0.0..=1.0).text("grass length")).changed();
        changed |= ui.add(egui::Slider::new(&mut c.unevenness, 0.0..=1.0).text("unevenness")).changed();
        ui.horizontal(|ui| {
            for (l, preset) in [("dry & flat", PitchConditions::dry_flat()), ("wet & flat", PitchConditions::wet_flat()), ("default", PitchConditions::default())] {
                if ui.button(l).clicked() {
                    c = preset;
                    changed = true;
                }
            }
            if ui.button("bumpy").clicked() {
                c.unevenness = 1.0;
                changed = true;
            }
        });
        if changed {
            self.cond = c;
            (self.live, self.plan) = params(&self.cfg, &c);
        }

        ui.separator();
        ui.heading("Ball");
        let b = &self.ball;
        let from = self.last.map(|k| k.from).unwrap_or(b.ground_pos());
        egui::Grid::new("ball_info").show(ui, |ui| {
            for (k, v) in [
                ("time", format!("{:.2} s", self.t)),
                ("speed", format!("{:.1} m/s", b.speed())),
                ("height", format!("{:.2} m", b.height())),
                ("max height", format!("{:.2} m", self.max_h)),
                ("distance", format!("{:.1} m", (b.ground_pos() - from).length())),
                ("spin", format!("side {:+.0}  top {:+.0} rad/s", b.spin.z, b.spin.truncate().length() * b.spin.truncate().perp_dot(b.vel.truncate()).signum())),
                ("mode", format!("{:?}", b.mode)),
            ] {
                ui.label(k);
                ui.label(RichText::new(v).monospace());
                ui.end_row();
            }
        });
        if !self.note.is_empty() {
            ui.label(RichText::new(&self.note).strong().color(Color32::YELLOW));
        }
        ui.add_space(6.0);
        ui.label(RichText::new("dashed = prediction on an ideal pitch (what players plan with); yellow = what actually happened").small().weak());
    }

    fn pitch(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let (pitch_rect, strip_rect) = view::split(resp.rect);
        let v = PitchView::new(pitch_rect);
        view::draw_pitch(&painter, &v, self.cond.wetness);

        // Aiming: place on press, aim while dragging, kick on release.
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let w = v.to_world(p);
            self.ball = Ball::at_rest(w);
            self.trail.clear();
            self.aim = Some((w, w));
        }
        if resp.dragged()
            && let (Some(p), Some((from, _))) = (resp.interact_pointer_pos(), self.aim)
        {
            self.aim = Some((from, v.to_world(p)));
        }
        if resp.drag_stopped()
            && let Some((from, to)) = self.aim.take()
            && (to - from).length() > 0.5
        {
            let k = self.kick_from(from, to);
            self.kick(k);
        }
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.ball = Ball::at_rest(v.to_world(p));
            self.trail.clear();
            self.ideal.clear();
        }

        let mut preview = Vec::new();
        if let Some((from, to)) = self.aim {
            let k = self.kick_from(from, to);
            let mut b = Ball::at_rest(from);
            b.kick(k.vel, k.spin);
            preview = predict(&b, &self.plan, 8.0, 0.02).samples.iter().map(|s| s.pos).collect();
            painter.arrow(v.to_screen(from), view::egui_vec(to - from) * v.len(1.0), Stroke::new(2.0, Color32::WHITE));
            painter.text(
                v.to_screen(to) + egui::vec2(8.0, -8.0),
                Align2::LEFT_BOTTOM,
                format!("{:.1} m/s, loft {:.0}°", k.vel.length(), self.loft_deg),
                FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
        // Straight aim line of the last kick: the curve is the deviation from it.
        if self.aim.is_none()
            && let Some(k) = self.last
        {
            let end = k.from + (k.to - k.from).normalize_or(Vec2::X) * 60.0;
            painter.extend(egui::Shape::dashed_line(&[v.to_screen(k.from), v.to_screen(end)], Stroke::new(1.0, Color32::from_white_alpha(90)), 3.0, 5.0));
        }
        let path = if preview.is_empty() { &self.ideal } else { &preview };
        view::draw_prediction(&painter, &v, path, view::PREDICTION);
        view::draw_trail(&painter, &v, &self.trail);
        view::draw_ball(&painter, &v, self.ball.pos);
        view::draw_elevation(&painter, strip_rect, &v, &self.trail, path, self.ball.pos);
    }
}

fn params(cfg: &SimConfig, c: &PitchConditions) -> (BallParams, BallParams) {
    let sub = cfg.timing.dt / cfg.timing.ball_substeps as f32;
    (BallParams::new(&cfg.ball, c, sub), BallParams::new(&cfg.ball, c, cfg.ai.planning_dt))
}
