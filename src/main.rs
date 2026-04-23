#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod engine;

use config::{load_config, save_config, AppConfig, BindMode};
use eframe::egui;
use egui::{Align, Color32, Frame, Layout, Margin, RichText, Rounding, Stroke, Vec2};
use engine::{ClickEngine, SharedState};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("IMCLICKER V2")
            .with_inner_size([760.0, 820.0])
            .with_min_inner_size([560.0, 640.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "IMCLICKER V2",
        native_options,
        Box::new(|cc| Box::new(ImclickerApp::new(cc))),
    )
}

struct ImclickerApp {
    config: AppConfig,
    shared: Arc<SharedState>,
    _engine: ClickEngine,
    waiting_for_bind: bool,
    last_save_attempt: Instant,
    save_error: Option<String>,
}

impl ImclickerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);

        let config = load_config();
        let shared = SharedState::new(&config);
        let engine = ClickEngine::spawn(Arc::clone(&shared));

        Self {
            config,
            shared,
            _engine: engine,
            waiting_for_bind: false,
            last_save_attempt: Instant::now() - Duration::from_secs(10),
            save_error: None,
        }
    }

    fn apply_config_to_shared(&self) {
        self.shared
            .target_cps
            .store(self.config.target_cps.clamp(1, 1000), Ordering::Relaxed);
        self.shared
            .mode
            .store(self.config.mode.as_u8(), Ordering::Relaxed);
        self.shared
            .bind_vk
            .store(self.config.bind_vk, Ordering::Relaxed);
        self.shared
            .manual_active
            .store(self.config.manual_active, Ordering::Relaxed);
    }

    fn persist_config(&mut self) {
        if self.last_save_attempt.elapsed() < Duration::from_millis(150) {
            return;
        }

        self.last_save_attempt = Instant::now();
        self.save_error = save_config(&self.config).err().map(|err| err.to_string());
    }

    fn header_ui(&self, ui: &mut egui::Ui, is_active: bool) {
        ui.horizontal(|ui| {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("◎").size(28.0).color(color_accent()));
                ui.add_space(8.0);
                ui.label(
                    RichText::new("IMCLICKER V2")
                        .size(32.0)
                        .color(color_text())
                        .strong(),
                );
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (label, dot_color) = if is_active {
                    ("ACTIVE", Color32::from_rgb(57, 235, 171))
                } else {
                    ("READY", Color32::from_rgb(0, 231, 255))
                };

                status_pill(ui, label, dot_color);
            });
        });
    }

    fn target_card_ui(&mut self, ui: &mut egui::Ui) {
        card(ui, 18.0, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("TARGET CPS")
                        .size(21.0)
                        .color(color_muted())
                        .strong(),
                );
            });

            ui.add_space(16.0);
            ui.columns(2, |columns| {
                columns[0].with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(self.config.target_cps.to_string())
                            .size(96.0)
                            .color(color_text())
                            .strong(),
                    );
                });

                columns[1].with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(12.0);
                    Frame::none()
                        .fill(Color32::from_rgb(6, 14, 27))
                        .stroke(Stroke::new(1.0, color_outline()))
                        .rounding(Rounding::same(18.0))
                        .inner_margin(Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                if neon_button(ui, "+", Vec2::new(80.0, 54.0), false).clicked() {
                                    self.config.target_cps = (self.config.target_cps + 1).min(1000);
                                    self.apply_config_to_shared();
                                    self.persist_config();
                                }
                                ui.add_space(8.0);
                                if neon_button(ui, "−", Vec2::new(80.0, 54.0), false).clicked() {
                                    self.config.target_cps = self.config.target_cps.saturating_sub(1).max(1);
                                    self.apply_config_to_shared();
                                    self.persist_config();
                                }
                            });
                        });
                });
            });
        });
    }

    fn live_cps_card_ui(&self, ui: &mut egui::Ui) {
        let live = self.shared.live_cps_x10.load(Ordering::Relaxed);
        let display = if live % 10 == 0 {
            format!("{}", live / 10)
        } else {
            format!("{:.1}", live as f32 / 10.0)
        };

        card(ui, 18.0, |ui| {
            ui.label(
                RichText::new("LIVE CPS")
                    .size(21.0)
                    .color(color_muted())
                    .strong(),
            );
            ui.add_space(20.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(
                    RichText::new(display)
                        .size(104.0)
                        .color(Color32::from_rgb(79, 224, 255))
                        .strong(),
                );
            });
        });
    }

    fn controls_ui(&mut self, ui: &mut egui::Ui) {
        ui.columns(3, |columns| {
            compact_card(&mut columns[0], |ui| {
                ui.label(
                    RichText::new("MODE")
                        .size(18.0)
                        .color(color_muted())
                        .strong(),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let toggle_selected = self.config.mode == BindMode::Toggle;
                    let hold_selected = self.config.mode == BindMode::Hold;

                    if neon_button(ui, "Toggle", Vec2::new(110.0, 42.0), toggle_selected).clicked() {
                        self.config.mode = BindMode::Toggle;
                        self.apply_config_to_shared();
                        self.persist_config();
                    }
                    if neon_button(ui, "Hold", Vec2::new(96.0, 42.0), hold_selected).clicked() {
                        self.config.mode = BindMode::Hold;
                        self.apply_config_to_shared();
                        self.persist_config();
                    }
                });
            });

            compact_card(&mut columns[1], |ui| {
                let active = self.shared.active.load(Ordering::Relaxed);
                let button_text = if active { "STOP" } else { "TOGGLE" };
                if neon_button(ui, button_text, Vec2::new(ui.available_width(), 76.0), true).clicked() {
                    self.config.manual_active = !self.config.manual_active;
                    self.apply_config_to_shared();
                    self.persist_config();
                }
            });

            compact_card(&mut columns[2], |ui| {
                ui.label(
                    RichText::new("BIND")
                        .size(18.0)
                        .color(color_muted())
                        .strong(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let bind_name = if self.waiting_for_bind {
                        "PRESS KEY...".to_string()
                    } else {
                        vk_name(self.config.bind_vk)
                    };
                    ui.label(
                        RichText::new(bind_name)
                            .size(34.0)
                            .color(color_text())
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if neon_button(ui, "CHANGE", Vec2::new(110.0, 42.0), false).clicked() {
                            self.waiting_for_bind = true;
                        }
                    });
                });
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("Current hotkey mode: {}", self.config.mode.label()))
                        .size(15.0)
                        .color(color_subtle()),
                );
            });
        });
    }

    fn presets_ui(&mut self, ui: &mut egui::Ui) {
        let presets = [50_u32, 100, 200, 500, 800];

        card(ui, 18.0, |ui| {
            ui.label(
                RichText::new("PRESET CPS")
                    .size(21.0)
                    .color(color_muted())
                    .strong(),
            );
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(14.0, 12.0);
                for preset in presets {
                    let selected = self.config.target_cps == preset;
                    if neon_button(ui, &preset.to_string(), Vec2::new(110.0, 52.0), selected).clicked() {
                        self.config.target_cps = preset;
                        self.apply_config_to_shared();
                        self.persist_config();
                    }
                }
            });
        });
    }

    fn footer_ui(&self, ui: &mut egui::Ui) {
        let active = self.shared.active.load(Ordering::Relaxed);
        let engine_alive = self.shared.worker_alive.load(Ordering::Relaxed);
        let total_clicks = self.shared.total_clicks.load(Ordering::Relaxed);
        let live = self.shared.live_cps_x10.load(Ordering::Relaxed);

        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!(
                    "ENGINE {}  •  TOTAL CLICKS {}  •  LIVE {:.1} CPS",
                    if engine_alive { "ONLINE" } else { "OFFLINE" },
                    total_clicks,
                    live as f32 / 10.0,
                ))
                .size(15.0)
                .color(if active { Color32::from_rgb(57, 235, 171) } else { color_subtle() }),
            );

            ui.add_space(8.0);

            if self.waiting_for_bind {
                ui.label(
                    RichText::new("Нажми любую поддерживаемую клавишу в окне приложения")
                        .size(15.0)
                        .color(color_accent()),
                );
            } else if let Some(error) = &self.save_error {
                ui.label(
                    RichText::new(format!("Config save error: {error}"))
                        .size(14.0)
                        .color(Color32::from_rgb(255, 136, 136)),
                );
            } else {
                ui.label(
                    RichText::new(
                        "Оптимизация сделана через отдельный рабочий поток, гибридное ожидание и минимальную нагрузку на UI.",
                    )
                    .size(14.0)
                    .color(color_subtle()),
                );
            }
        });
    }
}

impl Drop for ImclickerApp {
    fn drop(&mut self) {
        let _ = save_config(&self.config);
    }
}

impl eframe::App for ImclickerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        capture_bind_key(ctx, self);

        let active = self.shared.active.load(Ordering::Relaxed);
        ctx.request_repaint_after(if active {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(40)
        });

        let screen = ctx.screen_rect();
        let width_scale = screen.width() / 760.0;
        let height_scale = screen.height() / 820.0;
        let zoom = width_scale.min(height_scale).clamp(0.74, 1.0);
        ctx.set_zoom_factor(zoom);

        egui::CentralPanel::default()
            .frame(Frame::none().fill(Color32::from_rgb(2, 7, 15)))
            .show(ctx, |ui| {
                draw_background(ui);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(12.0);
                        self.header_ui(ui, active);
                        ui.add_space(18.0);
                        self.target_card_ui(ui);
                        ui.add_space(14.0);
                        self.live_cps_card_ui(ui);
                        ui.add_space(14.0);
                        self.controls_ui(ui);
                        ui.add_space(14.0);
                        self.presets_ui(ui);
                        ui.add_space(18.0);
                        self.footer_ui(ui);
                        ui.add_space(8.0);
                    });
            });
    }
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(12.0, 12.0);
    style.visuals.override_text_color = Some(color_text());
    style.visuals.window_fill = Color32::from_rgb(2, 7, 15);
    style.visuals.panel_fill = Color32::from_rgb(2, 7, 15);
    style.visuals.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(6, 14, 27);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(10, 28, 52);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(18, 50, 92);
    style.visuals.selection.bg_fill = color_accent();
    ctx.set_style(style);
}

fn draw_background(ui: &egui::Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();

    painter.rect_filled(rect, 0.0, Color32::from_rgb(2, 7, 15));

    let red_center = egui::pos2(rect.center().x, rect.bottom() - 160.0);
    painter.circle_filled(
        red_center,
        130.0,
        Color32::from_rgba_unmultiplied(210, 24, 55, 26),
    );
    painter.circle_filled(
        egui::pos2(red_center.x - 70.0, red_center.y + 8.0),
        96.0,
        Color32::from_rgba_unmultiplied(150, 12, 30, 16),
    );
    painter.circle_filled(
        egui::pos2(red_center.x + 70.0, red_center.y + 8.0),
        96.0,
        Color32::from_rgba_unmultiplied(150, 12, 30, 16),
    );

    let step = 34.0;
    let mut x = rect.left();
    while x < rect.right() {
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(54, 98, 150, 18)),
        );
        x += step;
    }
}

fn card(ui: &mut egui::Ui, rounding: f32, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::none()
        .fill(Color32::from_rgba_unmultiplied(4, 12, 24, 232))
        .stroke(Stroke::new(1.0, color_outline()))
        .rounding(Rounding::same(rounding))
        .inner_margin(Margin::same(18.0))
        .show(ui, add_contents);
}

fn compact_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::none()
        .fill(Color32::from_rgba_unmultiplied(4, 12, 24, 232))
        .stroke(Stroke::new(1.0, color_outline()))
        .rounding(Rounding::same(18.0))
        .inner_margin(Margin::same(18.0))
        .show(ui, add_contents);
}

fn neon_button(ui: &mut egui::Ui, label: &str, size: Vec2, selected: bool) -> egui::Response {
    let fill = if selected {
        Color32::from_rgb(8, 45, 92)
    } else {
        Color32::from_rgb(6, 14, 27)
    };

    let stroke = if selected {
        Stroke::new(1.6, Color32::from_rgb(0, 231, 255))
    } else {
        Stroke::new(1.0, color_outline())
    };

    ui.add_sized(
        size,
        egui::Button::new(
            RichText::new(label)
                .size(22.0)
                .strong()
                .color(if selected { Color32::from_rgb(167, 242, 255) } else { color_text() }),
        )
        .fill(fill)
        .stroke(stroke)
        .rounding(Rounding::same(16.0)),
    )
}

fn status_pill(ui: &mut egui::Ui, text: &str, dot_color: Color32) {
    Frame::none()
        .fill(Color32::from_rgba_unmultiplied(4, 15, 32, 240))
        .stroke(Stroke::new(1.2, color_outline()))
        .rounding(Rounding::same(20.0))
        .inner_margin(Margin::symmetric(16.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(dot_color, RichText::new("●").size(18.0));
                ui.label(
                    RichText::new(text)
                        .size(20.0)
                        .color(color_text())
                        .strong(),
                );
            });
        });
}

fn capture_bind_key(ctx: &egui::Context, app: &mut ImclickerApp) {
    if !app.waiting_for_bind {
        return;
    }

    let mut captured = None;

    ctx.input(|input| {
        for event in &input.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } = event
            {
                captured = egui_key_to_vk(*key);
                if captured.is_some() {
                    break;
                }
            }
        }
    });

    if let Some(vk) = captured {
        app.config.bind_vk = vk;
        app.waiting_for_bind = false;
        app.apply_config_to_shared();
        app.persist_config();
    }
}

fn egui_key_to_vk(key: egui::Key) -> Option<u16> {
    use egui::Key;

    Some(match key {
        Key::A => b'A' as u16,
        Key::B => b'B' as u16,
        Key::C => b'C' as u16,
        Key::D => b'D' as u16,
        Key::E => b'E' as u16,
        Key::F => b'F' as u16,
        Key::G => b'G' as u16,
        Key::H => b'H' as u16,
        Key::I => b'I' as u16,
        Key::J => b'J' as u16,
        Key::K => b'K' as u16,
        Key::L => b'L' as u16,
        Key::M => b'M' as u16,
        Key::N => b'N' as u16,
        Key::O => b'O' as u16,
        Key::P => b'P' as u16,
        Key::Q => b'Q' as u16,
        Key::R => b'R' as u16,
        Key::S => b'S' as u16,
        Key::T => b'T' as u16,
        Key::U => b'U' as u16,
        Key::V => b'V' as u16,
        Key::W => b'W' as u16,
        Key::X => b'X' as u16,
        Key::Y => b'Y' as u16,
        Key::Z => b'Z' as u16,
        Key::Num0 => b'0' as u16,
        Key::Num1 => b'1' as u16,
        Key::Num2 => b'2' as u16,
        Key::Num3 => b'3' as u16,
        Key::Num4 => b'4' as u16,
        Key::Num5 => b'5' as u16,
        Key::Num6 => b'6' as u16,
        Key::Num7 => b'7' as u16,
        Key::Num8 => b'8' as u16,
        Key::Num9 => b'9' as u16,
        Key::F1 => 0x70,
        Key::F2 => 0x71,
        Key::F3 => 0x72,
        Key::F4 => 0x73,
        Key::F5 => 0x74,
        Key::F6 => 0x75,
        Key::F7 => 0x76,
        Key::F8 => 0x77,
        Key::F9 => 0x78,
        Key::F10 => 0x79,
        Key::F11 => 0x7A,
        Key::F12 => 0x7B,
        Key::Space => 0x20,
        Key::Tab => 0x09,
        _ => return None,
    })
}

fn vk_name(vk: u16) -> String {
    match vk {
        0x20 => "SPACE".to_string(),
        0x09 => "TAB".to_string(),
        0x70..=0x7B => format!("F{}", vk - 0x6F),
        _ => {
            if ((b'0' as u16)..=(b'9' as u16)).contains(&vk) || ((b'A' as u16)..=(b'Z' as u16)).contains(&vk) {
                (vk as u8 as char).to_string()
            } else {
                format!("VK {vk}")
            }
        },
    }
}

fn color_text() -> Color32 {
    Color32::from_rgb(232, 240, 252)
}

fn color_muted() -> Color32 {
    Color32::from_rgb(144, 166, 194)
}

fn color_subtle() -> Color32 {
    Color32::from_rgb(108, 129, 154)
}

fn color_outline() -> Color32 {
    Color32::from_rgb(28, 72, 126)
}

fn color_accent() -> Color32 {
    Color32::from_rgb(0, 231, 255)
}
