//! The warning shown on Export DXF when the result still has open paths.

use super::App;
use super::canvas::{self, draw_path};
use super::icons::{self, Icon};
use super::theme::*;
use super::widgets::{self, Kind};
use dxf_cleaner::Shape;
use dxf_cleaner::clean::Path as CPath;
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, RichText, Sense, Shadow, Stroke, StrokeKind, Vec2, vec2};

pub struct ExportWarning {
    /// Put the keyboard focus on Cancel on the first frame, so Enter does not export.
    focus_cancel: bool,
}

impl ExportWarning {
    pub fn new() -> Self {
        ExportWarning { focus_cancel: true }
    }
}

enum Choice {
    Cancel,
    ShowInDrawing,
    ExportAnyway,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(res) = &app.result else {
        app.export_warning = None;
        return;
    };
    let heading = res.stats.open_path_heading().unwrap_or_default();
    let consequence = res.stats.open_path_consequence().unwrap_or_default();
    let open: Vec<CPath> = res
        .shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Path(p) if !p.is_closed() => Some(p.clone()),
            _ => None,
        })
        .collect();
    let focus_cancel = app.export_warning.as_mut().map(|w| std::mem::take(&mut w.focus_cancel)).unwrap_or(false);

    let frame = egui::Frame::new()
        .fill(DIALOG)
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x3E, 0x47)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(22))
        .shadow(Shadow { offset: [0, 24], blur: 64, spread: 0, color: Color32::from_black_alpha(128) });
    let mut choice = None;
    let modal = egui::Modal::new(egui::Id::unique("open-path-warning")).frame(frame).backdrop_color(Color32::from_black_alpha(168)).show(ctx, |ui| {
        ui.set_width(416.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            let (tile, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
            ui.painter().rect_filled(tile, CornerRadius::same(10), OPEN_BG);
            icons::paint(ui.painter(), Icon::Warning, egui::Rect::from_center_size(tile.center(), Vec2::splat(22.0)), C_OPEN, 2.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(RichText::new(&heading).size(16.0).strong().color(TEXT));
                ui.label(RichText::new(&consequence).color(TEXT_2));
            });
        });
        ui.add_space(14.0);
        if let Some(first) = open.first() {
            preview(ui, first, open.len());
        }
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            if widgets::button(ui, None, "Show in drawing", Kind::Link, 40.0).on_hover_text("Close this and zoom to the open paths").clicked() {
                choice = Some(Choice::ShowInDrawing);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, None, "Export anyway", Kind::Warning, 40.0).clicked() {
                    choice = Some(Choice::ExportAnyway);
                }
                let cancel = widgets::button(ui, None, "Cancel", Kind::Secondary, 40.0);
                if focus_cancel {
                    cancel.request_focus();
                }
                if cancel.clicked() {
                    choice = Some(Choice::Cancel);
                }
            });
        });
    });
    if choice.is_none() && modal.should_close() {
        choice = Some(Choice::Cancel);
    }
    match choice {
        None => {}
        Some(Choice::Cancel) => app.export_warning = None,
        Some(Choice::ShowInDrawing) => {
            app.export_warning = None;
            app.fit_to = app.open_paths_bbox();
            app.view = None;
        }
        Some(Choice::ExportAnyway) => {
            app.export_warning = None;
            app.save_dialog();
        }
    }
}

/// A small drawing of the first open path, with its layer.
fn preview(ui: &mut egui::Ui, path: &CPath, count: usize) {
    egui::Frame::new()
        .fill(BG)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let (rect, _) = ui.allocate_exact_size(vec2(82.0, 60.0), Sense::hover());
                let painter = ui.painter_at(rect);
                let view = canvas::fit(path.bbox(), rect, 9.0);
                draw_path(&painter, rect, &view, path, true);
                painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, BORDER), StrokeKind::Inside);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let more = match count {
                        1 => String::new(),
                        2 => " and 1 more".into(),
                        n => format!(" and {} more", n - 1),
                    };
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("Layer {}{more}", path.layer())).color(TEXT));
                    ui.label(RichText::new("Open ends are circled in yellow in the After view.").size(12.0).color(MUTED));
                });
            });
        });
}
