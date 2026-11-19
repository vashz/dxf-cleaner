//! The Workbench's own buttons and small pieces, drawn to match the design.

use super::icons::{self, Icon};
use super::theme::*;
use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, pos2,
    vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Filled accent: the next step (Clean Up).
    Primary,
    /// Outlined.
    Secondary,
    /// Filled warning yellow: going ahead despite open paths.
    Warning,
    /// Text only.
    Link,
}

struct Look {
    fill: Color32,
    stroke: Stroke,
    fg: Color32,
}

fn look(kind: Kind, enabled: bool, hovered: bool) -> Look {
    let none = Stroke::NONE;
    match (kind, enabled) {
        (Kind::Primary, true) => Look { fill: if hovered { Color32::from_rgb(0x7B, 0xBB, 0xFF) } else { ACCENT }, stroke: none, fg: ON_ACCENT },
        (Kind::Primary, false) => Look { fill: Color32::from_rgb(0x26, 0x2A, 0x31), stroke: none, fg: DISABLED },
        (Kind::Secondary, true) => Look {
            fill: if hovered { CONTROL } else { Color32::TRANSPARENT },
            stroke: Stroke::new(1.0, if hovered { Color32::from_rgb(0x4A, 0x51, 0x5B) } else { CONTROL_BORDER }),
            fg: TEXT,
        },
        (Kind::Secondary, false) => Look { fill: Color32::TRANSPARENT, stroke: Stroke::new(1.0, BORDER), fg: DISABLED },
        (Kind::Warning, true) => Look { fill: if hovered { Color32::from_rgb(0xFF, 0xE0, 0x66) } else { C_OPEN }, stroke: none, fg: ON_OPEN },
        (Kind::Warning, false) => Look { fill: Color32::from_rgb(0x26, 0x2A, 0x31), stroke: none, fg: DISABLED },
        (Kind::Link, true) => Look { fill: Color32::TRANSPARENT, stroke: none, fg: if hovered { Color32::from_rgb(0xC2, 0xDF, 0xFF) } else { Color32::from_rgb(0x9C, 0xCB, 0xFF) } },
        (Kind::Link, false) => Look { fill: Color32::TRANSPARENT, stroke: none, fg: DISABLED },
    }
}

/// A button with an optional icon left of its label, `height` points tall.
pub fn button(ui: &mut Ui, icon: Option<Icon>, label: &str, kind: Kind, height: f32) -> Response {
    let enabled = ui.is_enabled();
    let font = FontId::proportional(13.5);
    // PLACEHOLDER: the text takes the colour given when painting, which depends on state.
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let pad = if kind == Kind::Link { 4.0 } else { 16.0 };
    let icon_w = if icon.is_some() { 24.0 } else { 0.0 };
    let size = vec2(pad * 2.0 + icon_w + galley.size().x, height);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = if enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp };
    if ui.is_rect_visible(rect) {
        let l = look(kind, enabled, resp.hovered());
        let painter = ui.painter();
        painter.rect(rect, CornerRadius::same(6), l.fill, l.stroke, StrokeKind::Inside);
        if resp.has_focus() {
            painter.rect_stroke(rect.expand(2.0), CornerRadius::same(8), Stroke::new(2.0, ACCENT), StrokeKind::Outside);
        }
        if let Some(icon) = icon {
            let r = Rect::from_center_size(pos2(rect.left() + pad + 8.0, rect.center().y), Vec2::splat(16.0));
            icons::paint(painter, icon, r, l.fg, 2.0);
        }
        let pos = pos2(rect.left() + pad + icon_w, rect.center().y - galley.size().y / 2.0);
        painter.galley(pos, galley, l.fg);
    }
    resp.widget_info(|| WidgetInfo::labeled(egui::Role::Button, enabled, label));
    resp
}

/// A square outlined button showing only `icon`; `label` names it for screen readers and tooltips.
pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str, size: f32) -> Response {
    let enabled = ui.is_enabled();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let l = look(Kind::Secondary, enabled, resp.hovered());
        let fg = if enabled { TEXT_2 } else { DISABLED };
        ui.painter().rect(rect, CornerRadius::same(6), l.fill, l.stroke, StrokeKind::Inside);
        icons::paint(ui.painter(), icon, Rect::from_center_size(rect.center(), Vec2::splat(18.0)), fg, 2.0);
    }
    resp.widget_info(|| WidgetInfo::labeled(egui::Role::Button, enabled, label));
    resp.on_hover_text(label)
}

/// Small uppercase heading over a group in the settings rail.
pub fn section_heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(11.0).strong().color(MUTED).extra_letter_spacing(0.9));
}

/// Two-option switch; returns true when the choice changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: [(T, &str); 2]) -> bool {
    let mut changed = false;
    let width = ui.available_width();
    egui::Frame::new()
        .fill(BG)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(2)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let cell = vec2((width - 4.0 - 2.0 - 2.0) / 2.0, 34.0);
            ui.horizontal(|ui| {
                for (option, label) in options {
                    let selected = *value == option;
                    let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
                    let fill = if selected {
                        SELECTED
                    } else if resp.hovered() {
                        CONTROL
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
                    let font = FontId::proportional(13.5);
                    let color = if selected { TEXT } else { TEXT_2 };
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, font, color);
                    if resp.has_focus() {
                        ui.painter().rect_stroke(rect, CornerRadius::same(4), Stroke::new(2.0, ACCENT), StrokeKind::Inside);
                    }
                    resp.widget_info(|| WidgetInfo::selected(egui::Role::RadioButton, true, selected, label));
                    if resp.clicked() && !selected {
                        *value = option;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// A short coloured line, as in the legend of segment kinds.
pub fn line_swatch(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    ui.painter().rect_filled(Rect::from_center_size(rect.center(), vec2(14.0, 3.0)), CornerRadius::same(2), color);
}

/// A small ring, as in the legend for circles.
pub fn ring_swatch(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 14.0), Sense::hover());
    ui.painter().circle_stroke(rect.center(), 4.0, Stroke::new(2.0, color));
}

/// "1284" → "1,284".
pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn thousands_groups_digits() {
        assert_eq!(super::thousands(0), "0");
        assert_eq!(super::thousands(999), "999");
        assert_eq!(super::thousands(1284), "1,284");
        assert_eq!(super::thousands(1234567), "1,234,567");
    }
}
