//! Small stroke icons, drawn with the painter on a 24 × 24 grid (the same shapes as the
//! design's SVG icons), so the app needs no icon font or image files.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Folder,
    Wand,
    Download,
    Fit,
    Warning,
    FileDown,
    CheckCircle,
    Close,
}

/// Paints `icon` into `rect` (square), with a stroke `width` given on the 24-unit grid.
pub fn paint(painter: &Painter, icon: Icon, rect: Rect, color: Color32, width: f32) {
    let k = rect.width() / 24.0;
    let at = |x: f32, y: f32| Pos2::new(rect.left() + x * k, rect.top() + y * k);
    let stroke = Stroke::new(width * k, color);
    let line = |pts: &[(f32, f32)]| {
        painter.add(Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
    };
    let closed = |pts: &[(f32, f32)]| {
        painter.add(Shape::closed_line(pts.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
    };
    match icon {
        Icon::Folder => closed(&[(3.0, 7.0), (5.0, 5.0), (9.0, 5.0), (11.0, 7.0), (19.0, 7.0), (21.0, 9.0), (21.0, 17.0), (19.0, 19.0), (5.0, 19.0), (3.0, 17.0)]),
        Icon::Wand => {
            line(&[(4.0, 20.0), (15.0, 9.0)]);
            line(&[(13.0, 7.0), (17.0, 11.0)]);
            for (cx, cy, h) in [(18.0, 4.5, 2.0), (20.5, 13.5, 1.5), (7.0, 5.0, 1.5)] {
                line(&[(cx, cy - h), (cx, cy + h)]);
                line(&[(cx - h, cy), (cx + h, cy)]);
            }
        }
        Icon::Download => {
            line(&[(12.0, 4.0), (12.0, 15.0)]);
            line(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
            line(&[(5.0, 20.0), (19.0, 20.0)]);
        }
        Icon::Fit => {
            line(&[(4.0, 9.0), (4.0, 4.0), (9.0, 4.0)]);
            line(&[(20.0, 9.0), (20.0, 4.0), (15.0, 4.0)]);
            line(&[(4.0, 15.0), (4.0, 20.0), (9.0, 20.0)]);
            line(&[(20.0, 15.0), (20.0, 20.0), (15.0, 20.0)]);
        }
        Icon::Warning => {
            closed(&[(12.0, 3.5), (21.5, 20.0), (2.5, 20.0)]);
            line(&[(12.0, 9.0), (12.0, 13.5)]);
            painter.circle_filled(at(12.0, 16.8), 1.3 * k, color);
        }
        Icon::FileDown => {
            closed(&[(14.0, 3.0), (7.0, 3.0), (5.0, 5.0), (5.0, 19.0), (7.0, 21.0), (17.0, 21.0), (19.0, 19.0), (19.0, 8.0)]);
            line(&[(14.0, 3.0), (14.0, 8.0), (19.0, 8.0)]);
            line(&[(12.0, 11.0), (12.0, 17.0)]);
            line(&[(9.5, 14.5), (12.0, 17.0), (14.5, 14.5)]);
        }
        Icon::CheckCircle => {
            painter.circle_stroke(at(12.0, 12.0), 9.0 * k, stroke);
            line(&[(8.0, 12.5), (10.5, 15.0), (16.0, 9.5)]);
        }
        Icon::Close => {
            line(&[(6.0, 6.0), (18.0, 18.0)]);
            line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
    }
}
