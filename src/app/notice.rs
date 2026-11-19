//! One short message over the top of the views: an error that stays until closed, or a
//! confirmation (such as "Exported …") that fades out by itself.

use super::App;
use super::icons::{self, Icon};
use super::theme::*;
use eframe::egui::{self, CornerRadius, CursorIcon, FontId, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use std::time::{Duration, Instant};

const INFO_SECONDS: f32 = 4.0;
const FADE_SECONDS: f32 = 0.5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Error,
    Info,
}

pub struct Notice {
    kind: Kind,
    text: String,
    shown: Instant,
}

impl Notice {
    pub fn error(text: impl Into<String>) -> Self {
        Notice { kind: Kind::Error, text: text.into(), shown: Instant::now() }
    }
    pub fn info(text: impl Into<String>) -> Self {
        Notice { kind: Kind::Info, text: text.into(), shown: Instant::now() }
    }
}

/// Draws the notice, if any, at the top centre of `area`.
pub fn show(app: &mut App, ui: &mut egui::Ui, area: Rect) {
    let Some(n) = &app.notice else { return };
    let opacity = match n.kind {
        Kind::Error => 1.0,
        Kind::Info => {
            let t = n.shown.elapsed().as_secs_f32();
            if t >= INFO_SECONDS {
                app.notice = None;
                return;
            }
            ui.ctx().request_repaint_after(Duration::from_millis(50));
            ((INFO_SECONDS - t) / FADE_SECONDS).min(1.0)
        }
    };
    let (fill, border, icon_color, icon) = match n.kind {
        Kind::Error => (ERROR_BG, ERROR_BORDER, ERROR_ICON, Icon::Warning),
        Kind::Info => (INFO_BG, INFO_BORDER, TEXT, Icon::CheckCircle),
    };
    let fg = TEXT;
    let width = (area.width() - 32.0).min(560.0);
    let text_w = width - 16.0 - 16.0 - 12.0 - 32.0 - 12.0;
    let galley = ui.painter().layout(n.text.clone(), FontId::proportional(13.5), fg.gamma_multiply(opacity), text_w);
    let height = galley.size().y.max(20.0) + 20.0;
    let rect = Rect::from_min_size(pos2(area.center().x - width / 2.0, area.top() + 12.0), vec2(width, height));

    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(8), fill.gamma_multiply(opacity), Stroke::new(1.0, border.gamma_multiply(opacity)), StrokeKind::Inside);
    icons::paint(painter, icon, Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), Vec2::splat(16.0)), icon_color.gamma_multiply(opacity), 2.0);
    painter.galley(pos2(rect.left() + 44.0, rect.center().y - galley.size().y / 2.0), galley, fg);

    let close = Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), Vec2::splat(32.0));
    let resp = ui.interact(close, ui.scope_id().with("notice-close"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    if resp.hovered() {
        painter.rect_filled(close, CornerRadius::same(6), border.gamma_multiply(0.6 * opacity));
    }
    icons::paint(painter, Icon::Close, Rect::from_center_size(close.center(), Vec2::splat(14.0)), fg.gamma_multiply(opacity), 2.0);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Close message"));
    if resp.clicked() {
        app.notice = None;
    }
}
