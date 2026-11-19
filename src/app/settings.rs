//! The settings rail on the left: Clean Up settings, export format and the source file.

use super::App;
use super::theme::*;
use super::widgets::{self, thousands};
use dxf_cleaner::OutputMode;
use eframe::egui::{self, Align, Color32, CornerRadius, DragValue, Layout, Margin, RichText, Stroke, TextStyle};
use std::ops::RangeInclusive;

const TOLERANCE_HELP: &str = "The furthest Clean Up may move any part of the original geometry.";
const JOIN_GAP_HELP: &str = "Path ends closer than this are joined into one continuous path.";

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    egui::Panel::left("settings")
        .resizable(false)
        .exact_size(280.0)
        .frame(egui::Frame::new().fill(RAIL).inner_margin(Margin::same(20)))
        .show(ui, |ui| {
            egui::Panel::bottom("rail_hint")
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(RAIL).inner_margin(Margin { top: 12, ..Margin::ZERO }))
                .show(ui, |ui| {
                    ui.label(RichText::new("Drag to pan · wheel to zoom\nDouble-click to fit").size(12.0).color(MUTED));
                });
            egui::CentralPanel::default().frame(egui::Frame::new().fill(RAIL)).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| sections(app, ui));
            });
        });
}

fn sections(app: &mut App, ui: &mut egui::Ui) {
    let units = app.units();
    widgets::section_heading(ui, "Clean Up settings");
    ui.add_space(2.0);
    let mut changed = false;
    changed |= number_field(ui, "Tolerance", units, &mut app.opt.tolerance, 0.001..=5.0, TOLERANCE_HELP);
    ui.add_space(4.0);
    changed |= number_field(ui, "Join gap", units, &mut app.opt.join_gap, 0.0..=5.0, JOIN_GAP_HELP);
    ui.add_space(6.0);
    changed |= ui.checkbox(&mut app.opt.join_paths, "Join touching paths").changed();
    changed |= ui.checkbox(&mut app.opt.remove_duplicates, "Remove duplicate geometry").changed();
    changed |= ui.checkbox(&mut app.opt.move_to_origin, "Move drawing to origin (0,0)").changed();
    changed |= ui.checkbox(&mut app.one_layer, "Put everything on layer 0").changed();
    if changed && app.result.is_some() {
        app.result_stale = true;
    }

    ui.add_space(18.0);
    widgets::section_heading(ui, "Export as");
    widgets::segmented(ui, &mut app.mode, [(OutputMode::Polylines, "Polylines"), (OutputMode::LinesAndArcs, "LINE / ARC")]);

    ui.add_space(18.0);
    widgets::section_heading(ui, "Source file");
    match &app.dwg {
        Some(d) => {
            for (k, v) in &d.counts {
                row(ui, k, &thousands(*v), TEXT_2, TEXT);
            }
            row(ui, "Nodes", &thousands(app.before_nodes), TEXT_2, TEXT);
            for (k, v) in &d.skipped {
                row(ui, &format!("Ignored {k}"), &thousands(*v), MUTED, MUTED)
                    .on_hover_text("Not cut geometry: left out of the result");
            }
        }
        None => {
            ui.label(RichText::new("No file loaded").color(MUTED));
        }
    }
}

/// A labelled number box with its unit inside it, as in the design.
fn number_field(ui: &mut egui::Ui, label: &str, unit: &str, value: &mut f64, range: RangeInclusive<f64>, help: &str) -> bool {
    ui.label(RichText::new(label).color(TEXT)).on_hover_text(help);
    let mut changed = false;
    egui::Frame::new()
        .fill(BG)
        .stroke(Stroke::new(1.0, CONTROL_BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(6, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let v = ui.visuals_mut();
                for w in [&mut v.widgets.inactive, &mut v.widgets.hovered] {
                    w.bg_fill = Color32::TRANSPARENT;
                    w.weak_bg_fill = Color32::TRANSPARENT;
                    w.bg_stroke = Stroke::NONE;
                }
                ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                let width = ui.available_width() - 44.0;
                let resp = ui.add_sized([width, 26.0], DragValue::new(value).speed(0.001).range(range).min_decimals(3).max_decimals(3));
                changed = resp.on_hover_text(help).changed();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new(unit).color(MUTED));
                });
            });
        });
    changed
}

fn row(ui: &mut egui::Ui, key: &str, value: &str, key_color: Color32, value_color: Color32) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).monospace().size(12.0).color(key_color));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).monospace().size(12.0).color(value_color));
        });
    })
    .response
}
