//! The strip under the views: whether the result is ready to cut, and what it is made of.

use super::App;
use super::icons::{self, Icon};
use super::theme::*;
use super::widgets::{self, thousands};
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, Vec2, vec2};

pub fn show(app: &App, ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(18, 8))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.horizontal_centered(|ui| {
                let Some(res) = &app.result else {
                    ui.label(RichText::new("Not cleaned yet. Click Clean Up to see the result.").color(MUTED));
                    return;
                };
                let s = &res.stats;
                if app.result_stale {
                    chip(ui, Icon::Wand, super::STALE_TEXT, Color32::from_rgb(0x1B, 0x2C, 0x40), ACCENT);
                }
                match s.open_path_heading() {
                    Some(h) => chip(ui, Icon::Warning, &h, OPEN_BG, OPEN_TEXT),
                    None => chip(ui, Icon::CheckCircle, "All paths closed", Color32::from_rgb(0x1D, 0x33, 0x24), Color32::from_rgb(0x9B, 0xE7, 0xA6)),
                }
                ui.add_space(12.0);
                count(ui, s.closed_paths, if s.closed_paths == 1 { "closed path" } else { "closed paths" });
                ui.add_space(12.0);
                widgets::line_swatch(ui, C_LINE);
                count(ui, s.lines, if s.lines == 1 { "line" } else { "lines" });
                ui.add_space(6.0);
                widgets::line_swatch(ui, C_ARC);
                count(ui, s.arcs, if s.arcs == 1 { "arc" } else { "arcs" });
                ui.add_space(6.0);
                widgets::ring_swatch(ui, C_CIRCLE);
                count(ui, s.circles, if s.circles == 1 { "circle" } else { "circles" });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    deviation(ui, s.max_deviation, app.result_tolerance);
                });
            });
        });
}

/// A rounded label with an icon, e.g. the open-path warning.
fn chip(ui: &mut egui::Ui, icon: Icon, text: &str, fill: Color32, fg: Color32) {
    egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(6)).inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
            icons::paint(ui.painter(), icon, r, fg, 2.0);
            ui.label(RichText::new(text).strong().color(fg));
        });
    });
}

fn count(ui: &mut egui::Ui, n: usize, what: &str) {
    ui.spacing_mut().item_spacing.x = 5.0;
    ui.label(RichText::new(thousands(n)).strong().color(TEXT));
    ui.label(RichText::new(what).color(TEXT_2));
    ui.spacing_mut().item_spacing.x = 8.0;
}

/// "Max deviation 0.012 / 0.050 mm" over a bar filled to deviation ÷ tolerance. Both are mm.
fn deviation(ui: &mut egui::Ui, dev: f64, tol: f64) {
    ui.allocate_ui_with_layout(vec2(250.0, 36.0), Layout::top_down(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Max deviation").color(TEXT_2));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{dev:.3} / {tol:.3} mm")).monospace().color(TEXT));
            });
        })
        .response
        .on_hover_text("How far the result is from the original, against the Tolerance");
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
        ui.painter().rect_filled(bar, CornerRadius::same(3), BORDER);
        let ratio = if tol > 0.0 { (dev / tol).clamp(0.0, 1.0) as f32 } else { 1.0 };
        let color = if dev <= tol { C_CIRCLE } else { C_OPEN };
        let filled = Rect::from_min_size(bar.min, vec2((bar.width() * ratio).max(3.0), bar.height()));
        ui.painter().rect_filled(filled, CornerRadius::same(3), color);
    });
}
