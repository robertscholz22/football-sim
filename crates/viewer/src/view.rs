//! Shared pitch drawing: world ↔ screen transform, markings, the ball (shadow + raised ball), its trail and
//! predicted path, and the side-elevation strip that shows height along the pitch.

use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};
use match_engine::pitch::{
    CENTRE_CIRCLE_RADIUS, GOAL_AREA_DEPTH, GOAL_AREA_HALF_WIDTH, GOAL_DEPTH, GOAL_HALF_WIDTH, GOAL_HEIGHT, HALF_LENGTH,
    HALF_WIDTH, LENGTH, PENALTY_AREA_DEPTH, PENALTY_AREA_HALF_WIDTH, PENALTY_SPOT_DISTANCE, WIDTH,
};
use match_engine::{Vec2, Vec3};

/// Grass drawn around the pitch (m).
const MARGIN: f32 = 4.0;
/// Screen lift of an airborne ball above its shadow, in pitch metres per metre of height (oblique view).
const LIFT: f32 = 0.55;
/// Height of the elevation strip (px).
pub const STRIP_HEIGHT: f32 = 120.0;

pub const LINE: Color32 = Color32::from_rgba_premultiplied(220, 220, 220, 220);
pub const TRAIL: Color32 = Color32::from_rgb(255, 225, 90);
pub const PREDICTION: Color32 = Color32::from_rgb(90, 220, 255);

/// Maps pitch metres (origin at the centre spot, x along the length, y to the left) to screen pixels.
#[derive(Clone, Copy)]
pub struct PitchView {
    scale: f32,
    centre: Pos2,
}

impl PitchView {
    pub fn new(rect: Rect) -> Self {
        let scale = (rect.width() / (LENGTH + 2.0 * MARGIN)).min(rect.height() / (WIDTH + 2.0 * MARGIN));
        Self { scale, centre: rect.center() }
    }

    pub fn to_screen(self, p: Vec2) -> Pos2 {
        Pos2::new(self.centre.x + p.x * self.scale, self.centre.y - p.y * self.scale)
    }

    /// A point at height `z`, drawn raised above its ground position.
    pub fn to_screen3(self, p: Vec3) -> Pos2 {
        let g = self.to_screen(p.truncate());
        Pos2::new(g.x, g.y - p.z * LIFT * self.scale)
    }

    pub fn to_world(self, s: Pos2) -> Vec2 {
        Vec2::new((s.x - self.centre.x) / self.scale, (self.centre.y - s.y) / self.scale)
    }

    pub fn len(self, metres: f32) -> f32 {
        metres * self.scale
    }

    pub fn screen_x(self, x: f32) -> f32 {
        self.centre.x + x * self.scale
    }

    pub fn rect(self, a: Vec2, b: Vec2) -> Rect {
        Rect::from_two_pos(self.to_screen(a), self.to_screen(b))
    }
}

/// Grass (darker when wet) and markings.
pub fn draw_pitch(painter: &Painter, v: &PitchView, wetness: f32) {
    let wet = 1.0 - 0.35 * wetness.clamp(0.0, 1.0);
    let shade = |k: f32| Color32::from_rgb((40.0 * k * wet) as u8, (118.0 * k * wet) as u8, (52.0 * k * wet) as u8);
    let m = MARGIN;
    painter.rect_filled(v.rect(Vec2::new(-HALF_LENGTH - m, HALF_WIDTH + m), Vec2::new(HALF_LENGTH + m, -HALF_WIDTH - m)), 0.0, shade(0.9));
    let stripes = 14;
    for i in 0..stripes {
        let x0 = -HALF_LENGTH + i as f32 * LENGTH / stripes as f32;
        let x1 = x0 + LENGTH / stripes as f32;
        let k = if i % 2 == 0 { 1.0 } else { 1.08 };
        painter.rect_filled(v.rect(Vec2::new(x0, HALF_WIDTH), Vec2::new(x1, -HALF_WIDTH)), 0.0, shade(k));
    }
    let s = Stroke::new(1.5, LINE);
    let outline = v.rect(Vec2::new(-HALF_LENGTH, HALF_WIDTH), Vec2::new(HALF_LENGTH, -HALF_WIDTH));
    painter.rect_stroke(outline, 0.0, s, StrokeKind::Middle);
    painter.line_segment([v.to_screen(Vec2::new(0.0, HALF_WIDTH)), v.to_screen(Vec2::new(0.0, -HALF_WIDTH))], s);
    painter.circle_stroke(v.to_screen(Vec2::ZERO), v.len(CENTRE_CIRCLE_RADIUS), s);
    painter.circle_filled(v.to_screen(Vec2::ZERO), 2.0, LINE);
    for sx in [-1.0f32, 1.0] {
        let line = sx * HALF_LENGTH;
        for (depth, hw) in [(PENALTY_AREA_DEPTH, PENALTY_AREA_HALF_WIDTH), (GOAL_AREA_DEPTH, GOAL_AREA_HALF_WIDTH)] {
            painter.rect_stroke(v.rect(Vec2::new(line, hw), Vec2::new(line - sx * depth, -hw)), 0.0, s, StrokeKind::Middle);
        }
        let spot = Vec2::new(line - sx * PENALTY_SPOT_DISTANCE, 0.0);
        painter.circle_filled(v.to_screen(spot), 2.0, LINE);
        // Penalty arc: the part of the circle around the spot outside the area.
        let arc: Vec<Pos2> = (0..=24)
            .map(|k| {
                let a = -0.93 + 1.86 * k as f32 / 24.0;
                spot + Vec2::new(-sx * a.cos(), a.sin()) * CENTRE_CIRCLE_RADIUS
            })
            .filter(|p| sx * p.x <= sx * (line - sx * PENALTY_AREA_DEPTH) + 1e-3)
            .map(|p| v.to_screen(p))
            .collect();
        painter.add(Shape::line(arc, s));
        // Goal: net behind the line, posts.
        let net = v.rect(Vec2::new(line, GOAL_HALF_WIDTH), Vec2::new(line + sx * GOAL_DEPTH, -GOAL_HALF_WIDTH));
        painter.rect_filled(net, 0.0, Color32::from_white_alpha(40));
        painter.rect_stroke(net, 0.0, Stroke::new(1.0, Color32::from_white_alpha(140)), StrokeKind::Middle);
        for py in [-GOAL_HALF_WIDTH, GOAL_HALF_WIDTH] {
            painter.circle_filled(v.to_screen(Vec2::new(line, py)), 2.5, Color32::WHITE);
        }
    }
}

/// Ball: shadow on the ground, the ball raised above it by its height, a stalk joining them when airborne.
pub fn draw_ball(painter: &Painter, v: &PitchView, pos: Vec3) {
    let ground = v.to_screen(pos.truncate());
    let lifted = v.to_screen3(pos);
    let h = (pos.z - 0.11).max(0.0);
    let r = v.len(0.38).max(3.5) * (1.0 + 0.035 * h);
    painter.circle_filled(ground, r * 0.85, Color32::from_black_alpha((150.0 / (1.0 + 0.3 * h)) as u8));
    if h > 0.1 {
        painter.line_segment([ground, lifted], Stroke::new(1.0, Color32::from_black_alpha(90)));
    }
    painter.circle_filled(lifted, r, Color32::WHITE);
    painter.circle_stroke(lifted, r, Stroke::new(1.0, Color32::BLACK));
}

/// Fading trail of recent ball positions: raised path plus its ground track.
pub fn draw_trail(painter: &Painter, v: &PitchView, trail: &[Vec3]) {
    let n = trail.len().max(2) as f32;
    for (i, w) in trail.windows(2).enumerate() {
        let a = (i as f32 + 1.0) / n;
        painter.line_segment([v.to_screen(w[0].truncate()), v.to_screen(w[1].truncate())], Stroke::new(1.5, Color32::from_black_alpha((70.0 * a) as u8)));
        painter.line_segment([v.to_screen3(w[0]), v.to_screen3(w[1])], Stroke::new(2.0, TRAIL.gamma_multiply(a)));
    }
}

/// Predicted path (dashed, raised) with its ground track and bounce points.
pub fn draw_prediction(painter: &Painter, v: &PitchView, path: &[Vec3], color: Color32) {
    if path.len() < 2 {
        return;
    }
    let ground: Vec<Pos2> = path.iter().map(|p| v.to_screen(p.truncate())).collect();
    let raised: Vec<Pos2> = path.iter().map(|p| v.to_screen3(*p)).collect();
    painter.extend(Shape::dashed_line(&ground, Stroke::new(1.0, Color32::from_black_alpha(80)), 4.0, 4.0));
    painter.extend(Shape::dashed_line(&raised, Stroke::new(1.5, color), 6.0, 4.0));
    for w in path.windows(3) {
        if w[1].z < 0.2 && w[0].z > w[1].z + 0.01 && w[2].z > w[1].z + 0.01 {
            painter.circle_stroke(v.to_screen(w[1].truncate()), 3.0, Stroke::new(1.0, color));
        }
    }
}

/// Side elevation: height (m) against the pitch x, aligned with the pitch view above it.
pub fn draw_elevation(painter: &Painter, rect: Rect, v: &PitchView, trail: &[Vec3], path: &[Vec3], ball: Vec3) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(24, 28, 34));
    let peak = trail.iter().chain(path).map(|p| p.z).fold(ball.z, f32::max);
    let top = (peak + 1.0).max(6.0);
    let base = rect.bottom() - 14.0;
    let hscale = (base - rect.top() - 8.0) / top;
    let at = |x: f32, z: f32| Pos2::new(v.screen_x(x), base - z * hscale);
    let faint = Stroke::new(1.0, Color32::from_white_alpha(28));
    for h in [1.0, GOAL_HEIGHT, 5.0, 10.0, 20.0, 30.0] {
        if h < top {
            let y = base - h * hscale;
            painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], faint);
            painter.text(Pos2::new(v.screen_x(HALF_LENGTH + MARGIN) + 4.0, y), Align2::LEFT_CENTER, format!("{h} m"), FontId::monospace(10.0), Color32::from_white_alpha(110));
        }
    }
    painter.line_segment([Pos2::new(v.screen_x(-HALF_LENGTH - MARGIN), base), Pos2::new(v.screen_x(HALF_LENGTH + MARGIN), base)], Stroke::new(1.5, Color32::from_rgb(60, 140, 70)));
    for sx in [-1.0f32, 1.0] {
        let x = sx * HALF_LENGTH;
        let goal = Stroke::new(2.0, Color32::WHITE);
        painter.line_segment([at(x, 0.0), at(x, GOAL_HEIGHT)], goal);
        painter.line_segment([at(x, GOAL_HEIGHT), at(x + sx * GOAL_DEPTH, GOAL_HEIGHT)], goal);
        painter.line_segment([at(-sx * PENALTY_AREA_DEPTH + x, 0.0), at(-sx * PENALTY_AREA_DEPTH + x, 0.4)], Stroke::new(1.0, LINE));
    }
    painter.line_segment([at(0.0, 0.0), at(0.0, 0.4)], Stroke::new(1.0, LINE));
    let n = trail.len().max(2) as f32;
    for (i, w) in trail.windows(2).enumerate() {
        let a = (i as f32 + 1.0) / n;
        painter.line_segment([at(w[0].x, w[0].z), at(w[1].x, w[1].z)], Stroke::new(2.0, TRAIL.gamma_multiply(a)));
    }
    if path.len() > 1 {
        let pts: Vec<Pos2> = path.iter().map(|p| at(p.x, p.z)).collect();
        painter.extend(Shape::dashed_line(&pts, Stroke::new(1.5, PREDICTION), 6.0, 4.0));
    }
    painter.circle_filled(at(ball.x, ball.z), 4.0, Color32::WHITE);
    painter.text(
        Pos2::new(v.screen_x(-HALF_LENGTH - MARGIN), rect.top() + 4.0),
        Align2::LEFT_TOP,
        format!("height {:.2} m   (side view along the pitch)", (ball.z - 0.11).max(0.0)),
        FontId::monospace(11.0),
        Color32::from_white_alpha(170),
    );
}

/// Split the available area into the pitch (top) and the elevation strip (bottom).
pub fn split(rect: Rect) -> (Rect, Rect) {
    let strip = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - STRIP_HEIGHT), rect.max);
    let pitch = Rect::from_min_max(rect.min, Pos2::new(rect.right(), strip.top() - 4.0));
    (pitch, strip)
}

/// Colour on a red → yellow → green scale for 0..1.
pub fn ramp(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (r, g) = if t < 0.5 { (1.0, 2.0 * t) } else { (2.0 - 2.0 * t, 1.0) };
    Color32::from_rgb((230.0 * r) as u8, (210.0 * g) as u8, 60)
}

pub fn egui_vec(v: Vec2) -> egui::Vec2 {
    egui::vec2(v.x, -v.y)
}
