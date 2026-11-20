//! The top bar: app name, Open / Clean Up / Export, and the file and view controls.

use super::App;
use super::icons::Icon;
use super::theme::*;
use super::widgets::{self, Kind};
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, RichText, Sense, Stroke, vec2};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    egui::Panel::top("toolbar").frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(12, 6))).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            // The window title already names the app; the icon alone marks it here.
            if let Some(tex) = app.logo.texture(ui.ctx(), TOOLBAR_H) {
                ui.add(egui::Image::new((tex.id(), vec2(TOOLBAR_H, TOOLBAR_H)))).on_hover_text("DXF Cleaner");
            }
            ui.add_space(6.0);

            if widgets::button(ui, Some(Icon::Folder), "Open DXF", Kind::Secondary, TOOLBAR_H).clicked() {
                app.open_dialog();
            }
            let loaded = app.job.is_some();
            let clean = ui.add_enabled_ui(loaded, |ui| widgets::button(ui, Some(Icon::Wand), "Clean Up", Kind::Primary, TOOLBAR_H));
            if clean.inner.clicked() {
                app.run_clean();
            }
            let blocked = app.exportable().err();
            let export = ui.add_enabled_ui(blocked.is_none(), |ui| widgets::button(ui, Some(Icon::Download), "Export DXF", Kind::Secondary, TOOLBAR_H));
            if let Some(why) = blocked {
                export.inner.clone().on_disabled_hover_text(why.hover_text());
            }
            if export.inner.clicked() {
                app.export();
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(loaded, |ui| nodes_toggle(ui, &mut app.show_nodes));
                let fit = ui.add_enabled_ui(loaded, |ui| widgets::icon_button(ui, Icon::Fit, "Fit view", TOOLBAR_H));
                if fit.inner.clicked() {
                    app.view = None;
                    app.fit_to = None;
                }
                if let Some(job) = &app.job {
                    file_chip(ui, &job.file_name(), app.units());
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
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| ui.checkbox(show, "Nodes").on_hover_text("Show the points each path is made of"));
}

/// The loaded file's name and units, with a dot that says it is open.
fn file_chip(ui: &mut egui::Ui, name: &str, units: &str) {
    egui::Frame::new().fill(CONTROL).corner_radius(CornerRadius::same(16)).inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
        // The toolbar's right side runs right to left, and so does this row (a nested
        // left-to-right layout would take the whole free width), so the pieces are added
        // last first: units, name, dot.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(RichText::new(units).color(MUTED));
            // Shortened by hand: a truncating label would also take the whole free width.
            const MAX: usize = 40;
            let shown = if name.chars().count() > MAX { format!("{}…", name.chars().take(MAX - 1).collect::<String>()) } else { name.to_string() };
            ui.label(RichText::new(shown).color(TEXT)).on_hover_text(name);
            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.0, Color32::from_rgb(0x6E, 0xDC, 0x78));
        });
    });
}
