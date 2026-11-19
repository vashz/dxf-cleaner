//! The Before and After views: shared pan and zoom, grid, and drawing of the geometry.

use super::App;
use super::theme::*;
use super::widgets::thousands;
use dxf_cleaner::Shape;
use dxf_cleaner::clean::Path as CPath;
use dxf_cleaner::geom::{BBox, P};
use eframe::egui::{self, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

#[derive(Clone, Copy)]
pub struct View {
    pub center: P,
    /// pixels per drawing unit
    pub scale: f64,
}

/// The view that shows all of `bb` inside `rect`, `margin` pixels clear of its edges.
pub fn fit(bb: BBox, rect: Rect, margin: f32) -> View {
    if !bb.is_valid() {
        return View { center: P::default(), scale: 1.0 };
    }
    let w = bb.width().max(1e-6);
    let h = bb.height().max(1e-6);
    let scale = ((rect.width() - 2.0 * margin) as f64 / w).min((rect.height() - 2.0 * margin) as f64 / h).max(1e-9);
    View { center: bb.center(), scale }
}

pub fn to_screen(v: &View, rect: Rect, q: P) -> Pos2 {
    let c = rect.center();
    Pos2::new(c.x + ((q.x - v.center.x) * v.scale) as f32, c.y - ((q.y - v.center.y) * v.scale) as f32)
}

pub fn to_world(v: &View, rect: Rect, s: Pos2) -> P {
    let c = rect.center();
    P { x: v.center.x + (s.x - c.x) as f64 / v.scale, y: v.center.y - (s.y - c.y) as f64 / v.scale }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, v: &View) {
    // grid step: 1-2-5 series, aiming for ~60px spacing
    let target = 60.0 / v.scale;
    let mag = 10f64.powf(target.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= target).unwrap_or(10.0 * mag);
    let tl = to_world(v, rect, rect.left_top());
    let br = to_world(v, rect, rect.right_bottom());
    let mut x = (tl.x / step).floor() * step;
    while x <= br.x {
        let sx = to_screen(v, rect, P { x, y: 0.0 }).x;
        painter.line_segment([Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())], Stroke::new(1.0, GRID));
        x += step;
    }
    let mut y = (br.y / step).floor() * step;
    while y <= tl.y {
        let sy = to_screen(v, rect, P { x: 0.0, y }).y;
        painter.line_segment([Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)], Stroke::new(1.0, GRID));
        y += step;
    }
}

/// Draws a cleaned path: lines and arcs in their colours, plus its nodes and (when open)
/// its ends ringed in the open-path colour.
pub fn draw_path(painter: &egui::Painter, rect: Rect, v: &View, path: &CPath, nodes: bool) {
    for seg in path.segs() {
        let (color, n) = if seg.is_line() {
            (C_LINE, 1)
        } else {
            let px = seg.length() * v.scale;
            (C_ARC, ((px / 4.0) as usize).clamp(4, 256))
        };
        let pts: Vec<Pos2> = seg.samples(n).iter().map(|q| to_screen(v, rect, *q)).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.6, color)));
    }
    if nodes {
        for seg in path.segs() {
            painter.circle_filled(to_screen(v, rect, seg.a), 2.5, C_NODE_AFTER);
        }
    }
    if !path.is_closed() {
        painter.circle_stroke(to_screen(v, rect, path.start()), 5.0, Stroke::new(1.5, C_OPEN));
        painter.circle_stroke(to_screen(v, rect, path.end()), 5.0, Stroke::new(1.5, C_OPEN));
    }
}

impl App {
    fn result_bbox(&self) -> BBox {
        let mut bb = self.before.bbox;
        if let Some(r) = &self.result {
            for s in &r.shapes {
                bb.union(&s.bbox());
            }
        }
        bb
    }

    /// One of the two views. They share `self.view`, so panning or zooming either moves both.
    pub(super) fn canvas(&mut self, ui: &mut egui::Ui, rect: Rect, id: &str, after: bool) {
        let resp = ui.interact(rect, ui.scope_id().with(id), Sense::click_and_drag());
        if self.view.is_none() {
            let bb = self.fit_to.take().unwrap_or_else(|| self.result_bbox());
            self.view = Some(fit(bb, rect, 20.0));
        }
        let mut v = self.view.unwrap();
        if resp.dragged() {
            let d = resp.drag_delta();
            v.center.x -= d.x as f64 / v.scale;
            v.center.y += d.y as f64 / v.scale;
        }
        if resp.double_clicked() {
            v = fit(self.result_bbox(), rect, 20.0);
        }
        if let Some(hp) = resp.hover_pos() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            let zoom = ui.input(|i| i.zoom_delta()) as f64 * (1.0 + scroll as f64 * 0.0015).clamp(0.5, 2.0);
            if (zoom - 1.0).abs() > 1e-6 {
                let before = to_world(&v, rect, hp);
                v.scale *= zoom;
                let after_w = to_world(&v, rect, hp);
                v.center.x += before.x - after_w.x;
                v.center.y += before.y - after_w.y;
            }
        }
        self.view = Some(v);

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(8), CANVAS);
        draw_grid(&painter, rect.shrink(1.0), &v);

        if !after {
            for o in &self.before.outlines {
                let pts: Vec<Pos2> = o.pts.iter().map(|q| to_screen(&v, rect, *q)).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(1.2, C_BEFORE)));
            }
            if self.show_nodes {
                for o in &self.before.outlines {
                    for q in &o.nodes {
                        painter.circle_filled(to_screen(&v, rect, *q), 2.2, C_NODE_BEFORE);
                    }
                }
            }
        } else if let Some(res) = &self.result {
            for s in &res.shapes {
                match s {
                    Shape::Path(p) => draw_path(&painter, rect, &v, p, self.show_nodes),
                    Shape::Circle { c, r, .. } => {
                        let sc = to_screen(&v, rect, *c);
                        painter.circle_stroke(sc, (*r * v.scale) as f32, Stroke::new(1.6, C_CIRCLE));
                        if self.show_nodes {
                            painter.circle_filled(sc, 2.0, C_CIRCLE);
                        }
                    }
                }
            }
        } else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Click Clean Up to see the result", FontId::proportional(15.0), MUTED);
        }
        painter.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, BORDER), StrokeKind::Inside);

        // caption: title and what the view holds
        let (title, detail) = if after {
            let detail = self.result.as_ref().map(|r| {
                let s = &r.stats;
                // A segment is a line or an arc (GLOSSARY.md); full circles are counted apart.
                let n = s.lines + s.arcs;
                let segments = format!("clean · {} {}", thousands(n), if n == 1 { "segment" } else { "segments" });
                match s.circles {
                    0 => segments,
                    1 => format!("{segments} · 1 circle"),
                    c => format!("{segments} · {} circles", thousands(c)),
                }
            });
            ("AFTER", detail.unwrap_or_default())
        } else {
            ("BEFORE", format!("original · {} nodes", thousands(self.before.nodes)))
        };
        let title_pos = rect.left_top() + Vec2::new(14.0, 12.0);
        let title_rect = painter.text(title_pos, egui::Align2::LEFT_TOP, title, FontId::proportional(11.5), TEXT);
        painter.text(
            Pos2::new(title_rect.right() + 10.0, title_rect.bottom()),
            egui::Align2::LEFT_BOTTOM,
            detail,
            FontId::proportional(13.0),
            MUTED,
        );

        if let Some(hp) = resp.hover_pos() {
            let w = to_world(&v, rect, hp);
            painter.text(
                rect.left_bottom() + Vec2::new(14.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                format!("X {:.3}   Y {:.3}", w.x, w.y),
                FontId::monospace(12.0),
                MUTED,
            );
        }
    }
}
