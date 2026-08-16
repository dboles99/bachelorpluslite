//! BachelorPad+ Phase 1 UI toolkit spike -- egui/eframe shell.
//!
//! Measurement contract, identical in the Slint spike:
//!
//! * `BPSPIKE_READY_MS=<f64>` is printed once, on the first `logic()` call,
//!   i.e. the first frame. It measures `main()` entry to first frame, so it
//!   excludes process creation -- the harness measures that as wall clock.
//! * `--measure-exit` quits as soon as that line is printed, which is what
//!   makes the wall-clock cold-start run terminate on its own.
//! * Without the flag the window stays up, so the harness can sample RSS.

use std::time::Instant;

use eframe::egui;
use egui::Color32;

struct Theme {
    name: &'static str,
    shell: Color32,
    panel: Color32,
    edge: Color32,
    ink: Color32,
    ink_dim: Color32,
    accent: Color32,
}

/// Same four palettes as the Slint spike, to the byte.
const THEMES: [Theme; 4] = [
    Theme {
        name: "Light",
        shell: Color32::from_rgb(0xe8, 0xe6, 0xe1),
        panel: Color32::from_rgb(0xf7, 0xf6, 0xf3),
        edge: Color32::from_rgb(0xc2, 0xbd, 0xb4),
        ink: Color32::from_rgb(0x1b, 0x1b, 0x1b),
        ink_dim: Color32::from_rgb(0x5f, 0x5f, 0x5f),
        accent: Color32::from_rgb(0xb8, 0x54, 0x1f),
    },
    Theme {
        name: "Dark",
        shell: Color32::from_rgb(0x23, 0x26, 0x2b),
        panel: Color32::from_rgb(0x1a, 0x1d, 0x21),
        edge: Color32::from_rgb(0x3a, 0x3f, 0x47),
        ink: Color32::from_rgb(0xe6, 0xe6, 0xe6),
        ink_dim: Color32::from_rgb(0x9a, 0xa0, 0xa8),
        accent: Color32::from_rgb(0x4c, 0x9b, 0xe8),
    },
    Theme {
        name: "Organic",
        shell: Color32::from_rgb(0x2f, 0x26, 0x1c),
        panel: Color32::from_rgb(0x3a, 0x2f, 0x22),
        edge: Color32::from_rgb(0x5a, 0x4a, 0x34),
        ink: Color32::from_rgb(0xf0, 0xe4, 0xd2),
        ink_dim: Color32::from_rgb(0xb9, 0xa1, 0x84),
        accent: Color32::from_rgb(0xd9, 0x88, 0x29),
    },
    Theme {
        name: "Green",
        shell: Color32::from_rgb(0x08, 0x16, 0x0c),
        panel: Color32::from_rgb(0x0d, 0x23, 0x12),
        edge: Color32::from_rgb(0x1c, 0x4a, 0x28),
        ink: Color32::from_rgb(0xb8, 0xf5, 0xc4),
        ink_dim: Color32::from_rgb(0x6f, 0xbf, 0x82),
        accent: Color32::from_rgb(0x3d, 0xdc, 0x6a),
    },
];

const MENUS: [&str; 14] = [
    "File", "Edit", "View", "Insert", "Format", "Data", "Note", "Notebook", "Organize", "Research",
    "Run", "Security", "Tools", "Help",
];

const SAMPLE_TEXT: &str = "BachelorPad+ toolkit spike.\n\n\
Notepad when you want it. More when you need it.\n\n\
This pane is a live editable buffer, not a static mock: the point of the\n\
spike is to compare real text input, real theming and real startup cost.\n";

struct Shell {
    start: Instant,
    measure_exit: bool,
    reported: bool,
    theme: usize,
    applied_theme: Option<usize>,
    text: String,
}

impl Shell {
    fn new(start: Instant, measure_exit: bool) -> Self {
        Self {
            start,
            measure_exit,
            reported: false,
            theme: 2, // Organic
            applied_theme: None,
            text: SAMPLE_TEXT.to_owned(),
        }
    }

    /// Push the palette into egui's visuals. Only on change -- rebuilding
    /// visuals every frame would show up in the frame-time comparison and
    /// make the toolkit look worse than it is.
    fn sync_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme == Some(self.theme) {
            return;
        }
        self.applied_theme = Some(self.theme);
        let t = &THEMES[self.theme];

        let mut v = if self.theme == 0 {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        v.panel_fill = t.panel;
        v.window_fill = t.shell;
        v.extreme_bg_color = t.panel;
        v.override_text_color = Some(t.ink);
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, t.edge);
        v.widgets.inactive.bg_fill = t.shell;
        v.widgets.hovered.bg_fill = t.edge;
        v.widgets.active.bg_fill = t.edge;
        v.selection.bg_fill = t.accent.linear_multiply(0.35);
        ctx.set_visuals(v);
    }
}

impl eframe::App for Shell {
    /// Per-frame work that needs the `Context` rather than a `Ui`.
    ///
    /// eframe 0.36 split the old `update()` into `logic()` + `ui()`; state
    /// changes belong here so that `ui()` draws from settled state.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.reported {
            self.reported = true;
            println!(
                "BPSPIKE_READY_MS={:.3}",
                self.start.elapsed().as_secs_f64() * 1000.0
            );
            if self.measure_exit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        self.sync_theme(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let t = &THEMES[self.theme];

        // --- menu bar -------------------------------------------------
        egui::Panel::top("menu").show(ui, |ui| {
            ui.horizontal(|ui| {
                for m in MENUS {
                    let _ = ui.selectable_label(false, m);
                }
            });
        });

        // --- tab strip ------------------------------------------------
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                let _ = ui.selectable_label(true, "Untitled   ×");
                let _ = ui.selectable_label(false, "+");
            });
        });

        // --- status bar (specs.md section 3) --------------------------
        let mut cycle_theme = false;
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(t.accent, "● Never saved");
                for cell in ["│ TXT", "│ UTF-8", "│ CRLF", "│ Ln 1, Col 1", "│ 100%"] {
                    ui.colored_label(t.ink_dim, cell);
                }
                let label = egui::RichText::new(format!("│ {}", t.name)).color(t.accent);
                cycle_theme = ui.selectable_label(false, label).clicked();
            });
            ui.colored_label(t.ink_dim, "Location not assigned");
        });

        // --- editor pane ----------------------------------------------
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                // Gutter. Static in the spike -- it exists to prove the
                // toolkit can put a themed fixed column beside live text.
                ui.colored_label(
                    t.ink_dim,
                    egui::RichText::new("1\n2\n3\n4\n5\n6\n7\n8").monospace(),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.text)
                        .font(egui::TextStyle::Monospace)
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY),
                );
            });
        });

        if cycle_theme {
            self.theme = (self.theme + 1) % THEMES.len();
        }
    }
}

fn main() -> eframe::Result<()> {
    let start = Instant::now();
    let measure_exit = std::env::args().any(|a| a == "--measure-exit");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BachelorPad+")
            .with_inner_size([1000.0, 680.0]),
        ..Default::default()
    };

    eframe::run_native(
        "BachelorPad+",
        options,
        Box::new(move |_cc| Ok(Box::new(Shell::new(start, measure_exit)))),
    )
}
