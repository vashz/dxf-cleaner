//! What the views show before a file is open: one large place to drop a DXF.

use super::App;
use super::icons::{self, Icon};
use super::theme::*;
use super::widgets::{self, Kind};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, UiBuilder, Vec2, pos2, vec2};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, Sense::hover());
    let over = ui.input(|i| !i.raw.hovered_files.is_empty());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, CornerRadius::same(8), if over { Color32::from_rgb(0x14, 0x20, 0x2E) } else { CANVAS });
    let grid = Stroke::new(1.0, Color32::from_rgb(0x1A, 0x1D, 0x21));
    let mut x = rect.left() + 32.0;
    while x < rect.right() {
        painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], grid);
        x += 32.0;
    }
    let mut y = rect.top() + 32.0;
    while y < rect.bottom() {
        painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], grid);
        y += 32.0;
    }
    let edge = rect.shrink(1.0);
    let corners: Vec<Pos2> = vec![edge.left_top(), edge.right_top(), edge.right_bottom(), edge.left_bottom(), edge.left_top()];
    painter.extend(egui::Shape::dashed_line(&corners, Stroke::new(2.0, if over { ACCENT } else { CONTROL_BORDER }), 8.0, 6.0));

    // tile, heading, line, "or", button: about 260 points tall, centred
    let c = rect.center();
    let top = c.y - 130.0;
    let tile = Rect::from_center_size(pos2(c.x, top + 44.0), Vec2::splat(88.0));
    painter.rect(tile, CornerRadius::same(20), Color32::from_rgb(0x1E, 0x22, 0x28), Stroke::new(1.0, Color32::from_rgb(0x2E, 0x33, 0x3B)), StrokeKind::Inside);
    icons::paint(&painter, Icon::FileDown, Rect::from_center_size(tile.center(), Vec2::splat(44.0)), if over { ACCENT } else { MUTED }, 1.5);

    let heading = if over { "Release to open" } else { "Drop a DXF file here" };
    painter.text(pos2(c.x, top + 106.0), Align2::CENTER_TOP, heading, FontId::proportional(20.0), TEXT);
    painter.text(
        pos2(c.x, top + 138.0),
        Align2::CENTER_TOP,
        "Exports from Illustrator, AutoCAD and other drawing programs",
        FontId::proportional(14.0),
        MUTED,
    );
    if over {
        return;
    }
    let or_y = top + 182.0;
    let or = painter.text(pos2(c.x, or_y), Align2::CENTER_CENTER, "or", FontId::proportional(13.5), MUTED);
    let rule = Stroke::new(1.0, Color32::from_rgb(0x2E, 0x33, 0x3B));
    painter.line_segment([pos2(or.left() - 52.0, or_y), pos2(or.left() - 12.0, or_y)], rule);
    painter.line_segment([pos2(or.right() + 12.0, or_y), pos2(or.right() + 52.0, or_y)], rule);

    let label_w = ui.painter().layout_no_wrap("Open DXF".into(), FontId::proportional(13.5), TEXT).size().x;
    let button = Rect::from_center_size(pos2(c.x, top + 230.0), vec2(16.0 * 2.0 + 24.0 + label_w, BUTTON_H));
    let clicked = ui.scope_builder(UiBuilder::new().max_rect(button), |ui| widgets::button(ui, Some(Icon::Folder), "Open DXF", Kind::Primary, BUTTON_H).clicked()).inner;
    if clicked {
        app.open_dialog();
    }
}
