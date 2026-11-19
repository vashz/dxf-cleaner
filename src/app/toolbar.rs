//! The top bar: app name, Open / Clean Up / Export, and the file and view controls.

use super::App;
use super::icons::Icon;
use super::theme::*;
use super::widgets::{self, Kind};
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, RichText, Sense, Stroke, vec2};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    egui::Panel::top("toolbar").frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(16, 10))).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some(tex) = &app.icon {
                ui.add(egui::Image::new((tex.id(), vec2(24.0, 24.0))));
            }
            ui.label(RichText::new("DXF Cleaner").strong().size(14.5).color(TEXT));
            ui.add_space(12.0);

            if widgets::button(ui, Some(Icon::Folder), "Open DXF", Kind::Secondary, BUTTON_H).clicked() {
                app.open_dialog();
            }
            let loaded = app.dwg.is_some();
            let clean = ui.add_enabled_ui(loaded, |ui| widgets::button(ui, Some(Icon::Wand), "Clean Up", Kind::Primary, BUTTON_H));
            if clean.inner.clicked() {
                app.run_clean();
            }
            let export = ui.add_enabled_ui(app.result.is_some(), |ui| widgets::button(ui, Some(Icon::Download), "Export DXF", Kind::Secondary, BUTTON_H));
            if export.inner.clicked() {
                app.export();
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(loaded, |ui| nodes_toggle(ui, &mut app.show_nodes));
                let fit = ui.add_enabled_ui(loaded, |ui| widgets::icon_button(ui, Icon::Fit, "Fit view"));
                if fit.inner.clicked() {
                    app.view = None;
                    app.fit_to = None;
                }
                if let Some(p) = &app.path {
                    let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                    file_chip(ui, &name, app.units());
                }
            });
        });
    });
}

fn nodes_toggle(ui: &mut egui::Ui, show: &mut bool) {
    let stroke = if ui.is_enabled() { CONTROL_BORDER } else { BORDER };
    egui::Frame::new()
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| ui.checkbox(show, "Nodes").on_hover_text("Show the points each path is made of"));
}

/// The loaded file's name and units, with a dot that says it is open.
fn file_chip(ui: &mut egui::Ui, name: &str, units: &str) {
    egui::Frame::new().fill(CONTROL).corner_radius(CornerRadius::same(16)).inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
        // left to right, although the toolbar lays out its right side from the right
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.0, Color32::from_rgb(0x6E, 0xDC, 0x78));
            // Shortened by hand: a truncating label would stretch the chip across the toolbar.
            const MAX: usize = 40;
            let shown = if name.chars().count() > MAX { format!("{}…", name.chars().take(MAX - 1).collect::<String>()) } else { name.to_string() };
            ui.label(RichText::new(shown).color(TEXT)).on_hover_text(name);
            ui.label(RichText::new(units).color(MUTED));
        });
    });
}
