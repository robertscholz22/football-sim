//! Slim 2D viewer for the match engine: a full match with tactical overlays and decision breakdowns, and a ball
//! physics sandbox. Height is shown by the ball's shadow, the ball drawn raised above it, and a side-elevation strip;
//! curve by the trail and the predicted path.

mod match_mode;
mod sandbox;
mod view;

use std::collections::BTreeMap;

use eframe::egui;

/// Principle titles and summaries from `docs/tactics/principles.md`, for the "why" panel.
pub struct Catalog {
    entries: BTreeMap<String, (String, String)>,
}

impl Catalog {
    fn parse(text: &str) -> Self {
        let mut entries = BTreeMap::new();
        let mut current: Option<(String, String, String)> = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("### ") {
                if let Some((id, title, body)) = current.take() {
                    entries.insert(id, (title, body));
                }
                let (id, title) = rest.split_once(" · ").unwrap_or((rest, ""));
                current = Some((id.trim().to_string(), title.trim().to_string(), String::new()));
            } else if let Some((_, _, body)) = current.as_mut()
                && (line.starts_with("- **Source says:**") || line.starts_with("- **Translation (ours):**"))
            {
                body.push_str(line.trim_start_matches("- "));
                body.push_str("\n\n");
            } else if line.starts_with("## ")
                && let Some((id, title, body)) = current.take()
            {
                entries.insert(id, (title, body));
            }
        }
        if let Some((id, title, body)) = current {
            entries.insert(id, (title, body));
        }
        Self { entries }
    }

    pub fn title(&self, id: &str) -> &str {
        self.entries.get(id).map(|e| e.0.as_str()).unwrap_or("")
    }

    pub fn summary(&self, id: &str) -> String {
        self.entries.get(id).map(|e| e.1.replace("**", "")).unwrap_or_default()
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Mode {
    Match,
    Sandbox,
}

struct App {
    mode: Mode,
    game: match_mode::MatchMode,
    sandbox: sandbox::Sandbox,
    catalog: Catalog,
    /// `--shot <file.ppm>`: save a screenshot after a few frames and quit (docs / verification).
    shot: Option<(String, u32)>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("mode").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mode, Mode::Match, "Match");
                ui.selectable_value(&mut self.mode, Mode::Sandbox, "Ball physics sandbox");
                ui.separator();
                ui.label(egui::RichText::new("space = play / pause · click a player to inspect").weak().small());
            });
        });
        match self.mode {
            Mode::Match => self.game.ui(ui, &self.catalog),
            Mode::Sandbox => self.sandbox.ui(ui),
        }
        if let Some((path, frames)) = &mut self.shot {
            *frames += 1;
            ui.ctx().request_repaint();
            if *frames == 15 {
                let (path, ctx) = (path.clone(), ui.ctx().clone());
                ui.ctx().request_screenshot(move |img| {
                    let mut out = format!("P6\n{} {}\n255\n", img.size[0], img.size[1]).into_bytes();
                    out.extend(img.pixels.iter().flat_map(|c| [c.r(), c.g(), c.b()]));
                    if let Err(e) = std::fs::write(&path, out) {
                        eprintln!("screenshot: {e}");
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                });
            }
        }
    }
}

/// Command line: `[seed] [--at <match seconds>] [--select <player id>] [--sandbox] [--shot <file.ppm>]`.
fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() -> eframe::Result {
    let seed = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut game = match_mode::MatchMode::new(seed);
    if let Some(t) = arg("--at").and_then(|s| s.parse().ok()) {
        game.fast_forward(t);
    }
    game.select(arg("--select").and_then(|s| s.parse().ok()));
    let mut sandbox = sandbox::Sandbox::new();
    let sandbox_mode = std::env::args().any(|a| a == "--sandbox");
    if sandbox_mode {
        sandbox.demo();
    }
    let shot = arg("--shot").map(|p| (p, 0));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1500.0, 900.0]).with_title("Match viewer"),
        ..Default::default()
    };
    eframe::run_native(
        "match_viewer",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            Ok(Box::new(App {
                mode: if sandbox_mode { Mode::Sandbox } else { Mode::Match },
                game,
                sandbox,
                catalog: Catalog::parse(include_str!("../../../docs/tactics/principles.md")),
                shot,
            }))
        }),
    )
}
