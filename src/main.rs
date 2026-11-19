#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use dxf_converter::clean::{entity_outline, Path as CPath};
use dxf_converter::geom::{BBox, P};
use dxf_converter::*;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::path::PathBuf;

const BG: Color32 = Color32::from_rgb(22, 24, 28);
const GRID: Color32 = Color32::from_rgb(34, 37, 43);
const C_BEFORE: Color32 = Color32::from_rgb(200, 200, 205);
const C_NODE_BEFORE: Color32 = Color32::from_rgb(255, 80, 80);
const C_LINE: Color32 = Color32::from_rgb(90, 170, 255);
const C_ARC: Color32 = Color32::from_rgb(255, 170, 60);
const C_CIRCLE: Color32 = Color32::from_rgb(110, 220, 120);
const C_NODE_AFTER: Color32 = Color32::from_rgb(255, 255, 255);
const C_OPEN: Color32 = Color32::from_rgb(255, 90, 200);

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 820.0])
        .with_min_inner_size([900.0, 560.0])
        .with_title("DXF Converter")
        .with_drag_and_drop(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let start_file = std::env::args().nth(1).map(PathBuf::from);
    eframe::run_native(
        "DXF Converter",
        options,
        Box::new(move |_cc| {
            let mut app = App::default();
            if let Some(f) = start_file {
                app.load(f);
            }
            Ok(Box::new(app))
        }),
    )
}

struct Outline {
    pts: Vec<P>,
    nodes: Vec<P>,
}

#[derive(Clone, Copy)]
struct View {
    center: P,
    /// pixels per drawing unit
    scale: f64,
}

struct App {
    path: Option<PathBuf>,
    dwg: Option<Drawing>,
    before: Vec<Outline>,
    before_nodes: usize,
    bbox: BBox,
    result: Option<CleanResult>,
    result_stale: bool,
    opt: Options,
    one_layer: bool,
    mode: OutputMode,
    show_nodes: bool,
    view: Option<View>,
    status: String,
    error: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        App {
            path: None,
            dwg: None,
            before: Vec::new(),
            before_nodes: 0,
            bbox: BBox::empty(),
            result: None,
            result_stale: false,
            opt: Options::default(),
            one_layer: false,
            mode: OutputMode::Polylines,
            show_nodes: true,
            view: None,
            status: "Open a DXF file (or drag & drop it onto the window).".into(),
            error: None,
        }
    }
}

impl App {
    fn load(&mut self, path: PathBuf) {
        match read_file(&path) {
            Ok(dwg) => {
                self.before = dwg
                    .entities
                    .iter()
                    .map(|e| {
                        let (pts, nodes) = entity_outline(e);
                        Outline { pts, nodes }
                    })
                    .collect();
                self.before_nodes = self.before.iter().map(|o| o.nodes.len()).sum();
                let mut bb = BBox::empty();
                for o in &self.before {
                    for q in &o.pts {
                        bb.add(*q);
                    }
                }
                self.bbox = bb;
                self.status = format!(
                    "Loaded {} ({} entities). Click \"Clean Up\".",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    dwg.entities.len()
                );
                self.dwg = Some(dwg);
                self.path = Some(path);
                self.result = None;
                self.view = None;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(f) = rfd::FileDialog::new().add_filter("DXF drawing", &["dxf", "DXF"]).pick_file() {
            self.load(f);
        }
    }

    fn run_clean(&mut self) {
        let Some(dwg) = &self.dwg else { return };
        self.opt.single_layer = if self.one_layer { Some("0".into()) } else { None };
        let t = std::time::Instant::now();
        let res = clean(dwg, &self.opt);
        let s = &res.stats;
        self.status = format!(
            "Cleaned in {} ms: {} closed / {} open paths, {} lines, {} arcs, {} circles.",
            t.elapsed().as_millis(),
            s.closed_paths,
            s.open_paths,
            s.lines,
            s.arcs,
            s.circles
        );
        if self.opt.move_to_origin {
            self.view = None; // geometry moved, refit the view
        }
        self.result = Some(res);
        self.result_stale = false;
    }

    fn export_dialog(&mut self) {
        let Some(res) = &self.result else { return };
        if let Some(warning) = res.stats.open_path_warning() {
            let answer = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("Open paths")
                .set_description(format!("{warning}\n\nExport anyway?"))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if answer != rfd::MessageDialogResult::Yes {
                return;
            }
        }
        let (dir, name) = match &self.path {
            Some(p) => (
                p.parent().map(|d| d.to_path_buf()),
                format!("{}_clean.dxf", p.file_stem().unwrap_or_default().to_string_lossy()),
            ),
            None => (None, "clean.dxf".into()),
        };
        let mut dlg = rfd::FileDialog::new().add_filter("DXF drawing", &["dxf"]).set_file_name(&name);
        if let Some(d) = dir {
            dlg = dlg.set_directory(d);
        }
        if let Some(out) = dlg.save_file() {
            let units = self.dwg.as_ref().and_then(|d| d.insunits);
            let text = write_dxf(&res.shapes, self.mode, units);
            match std::fs::write(&out, text) {
                Ok(()) => self.status = format!("Exported {}", out.display()),
                Err(e) => self.error = Some(format!("Could not write {}: {e}", out.display())),
            }
        }
    }

    fn result_bbox(&self) -> BBox {
        let mut bb = self.bbox;
        if let Some(r) = &self.result {
            for s in &r.shapes {
                bb.union(&s.bbox());
            }
        }
        bb
    }

    fn fit_view(&mut self, rect: Rect) {
        let bb = self.result_bbox();
        if !bb.is_valid() {
            self.view = Some(View { center: P::default(), scale: 1.0 });
            return;
        }
        let w = bb.width().max(1e-6);
        let h = bb.height().max(1e-6);
        let scale = ((rect.width() as f64 - 40.0) / w).min((rect.height() as f64 - 40.0) / h).max(1e-9);
        self.view = Some(View { center: bb.center(), scale });
    }
}

fn to_screen(v: &View, rect: Rect, q: P) -> Pos2 {
    let c = rect.center();
    Pos2::new(c.x + ((q.x - v.center.x) * v.scale) as f32, c.y - ((q.y - v.center.y) * v.scale) as f32)
}

fn to_world(v: &View, rect: Rect, s: Pos2) -> P {
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

fn draw_path(painter: &egui::Painter, rect: Rect, v: &View, path: &CPath, nodes: bool) {
    for seg in &path.segs {
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
        for seg in &path.segs {
            painter.circle_filled(to_screen(v, rect, seg.a), 2.5, C_NODE_AFTER);
        }
        if !path.closed {
            painter.circle_stroke(to_screen(v, rect, path.start()), 5.0, Stroke::new(1.5, C_OPEN));
            painter.circle_stroke(to_screen(v, rect, path.end()), 5.0, Stroke::new(1.5, C_OPEN));
        }
    }
}

impl App {
    fn canvas(&mut self, ui: &mut egui::Ui, rect: Rect, id: &str, after: bool) {
        let resp = ui.interact(rect, ui.scope_id().with(id), Sense::click_and_drag());
        if self.view.is_none() {
            self.fit_view(rect);
        }
        let mut v = self.view.unwrap();
        // pan / zoom (shared between both canvases)
        if resp.dragged() {
            let d = resp.drag_delta();
            v.center.x -= d.x as f64 / v.scale;
            v.center.y += d.y as f64 / v.scale;
        }
        if resp.double_clicked() {
            self.fit_view(rect);
            v = self.view.unwrap();
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
        painter.rect_filled(rect, 4.0, BG);
        draw_grid(&painter, rect, &v);

        if !after {
            for o in &self.before {
                let pts: Vec<Pos2> = o.pts.iter().map(|q| to_screen(&v, rect, *q)).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(1.2, C_BEFORE)));
            }
            if self.show_nodes {
                for o in &self.before {
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
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if self.dwg.is_some() { "Click \"Clean Up\" to see the result" } else { "" },
                egui::FontId::proportional(16.0),
                Color32::GRAY,
            );
        }

        // title + cursor coordinates
        let title = if after { "AFTER (clean)" } else { "BEFORE (original)" };
        painter.text(rect.left_top() + Vec2::new(10.0, 8.0), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(15.0), Color32::from_gray(230));
        if let Some(hp) = resp.hover_pos() {
            let w = to_world(&v, rect, hp);
            painter.text(
                rect.left_bottom() + Vec2::new(10.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                format!("X {:.3}   Y {:.3}", w.x, w.y),
                egui::FontId::monospace(12.0),
                Color32::from_gray(160),
            );
        }
    }
}

fn units_name(u: Option<i32>) -> &'static str {
    match u {
        Some(1) => "inches",
        Some(2) => "feet",
        Some(4) => "mm",
        Some(5) => "cm",
        Some(6) => "m",
        _ => "drawing units",
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // drag & drop
        let dropped: Vec<PathBuf> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if let Some(f) = dropped.into_iter().next() {
            self.load(f);
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(egui::RichText::new("📂  Open DXF…").size(16.0))).clicked() {
                    self.open_dialog();
                }
                let can_clean = self.dwg.is_some();
                let clean_label = if self.result_stale { "✨  Clean Up (settings changed)" } else { "✨  Clean Up" };
                if ui
                    .add_enabled(can_clean, egui::Button::new(egui::RichText::new(clean_label).size(16.0).strong()))
                    .clicked()
                {
                    self.run_clean();
                }
                let can_export = self.result.is_some();
                if ui
                    .add_enabled(can_export, egui::Button::new(egui::RichText::new("💾  Export DXF…").size(16.0)))
                    .clicked()
                {
                    self.export_dialog();
                }
                ui.separator();
                if ui.button("Fit view").clicked() {
                    self.view = None;
                }
                ui.checkbox(&mut self.show_nodes, "Show nodes");
                ui.separator();
                if let Some(p) = &self.path {
                    ui.label(egui::RichText::new(p.file_name().unwrap_or_default().to_string_lossy()).strong());
                }
            });
            ui.add_space(6.0);
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(e) = &self.error {
                    ui.colored_label(Color32::from_rgb(255, 110, 110), e);
                } else {
                    ui.label(&self.status);
                }
            });
        });

        egui::Panel::left("settings").resizable(false).exact_size(270.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let units = units_name(self.dwg.as_ref().and_then(|d| d.insunits));
                ui.heading("Settings");
                let mut changed = false;
                ui.label(format!("Tolerance ({units})"));
                changed |= ui
                    .add(egui::DragValue::new(&mut self.opt.tolerance).speed(0.001).range(0.001..=5.0).max_decimals(3))
                    .on_hover_text("Maximum distance the clean result may deviate from the original shape.")
                    .changed();
                ui.label(format!("Join gap ({units})"));
                changed |= ui
                    .add(egui::DragValue::new(&mut self.opt.join_tolerance).speed(0.001).range(0.0..=5.0).max_decimals(3))
                    .on_hover_text("Open ends closer than this are connected into one continuous path.")
                    .changed();
                changed |= ui.checkbox(&mut self.opt.join_paths, "Join touching paths").changed();
                changed |= ui.checkbox(&mut self.opt.remove_duplicates, "Remove duplicate geometry").changed();
                changed |= ui.checkbox(&mut self.opt.move_to_origin, "Move drawing to origin (0,0)").changed();
                changed |= ui.checkbox(&mut self.one_layer, "Put everything on layer 0").changed();
                if changed && self.result.is_some() {
                    self.result_stale = true;
                }
                ui.add_space(6.0);
                ui.label("Export as");
                ui.radio_value(&mut self.mode, OutputMode::Polylines, "Continuous polylines (recommended)");
                ui.radio_value(&mut self.mode, OutputMode::LinesAndArcs, "Separate LINE / ARC entities");

                ui.add_space(10.0);
                ui.separator();
                ui.heading("Before");
                if let Some(d) = &self.dwg {
                    egui::Grid::new("before_stats").num_columns(2).show(ui, |ui| {
                        for (k, v) in &d.counts {
                            ui.label(k);
                            ui.label(v.to_string());
                            ui.end_row();
                        }
                        ui.label("Nodes / points");
                        ui.label(self.before_nodes.to_string());
                        ui.end_row();
                    });
                    if !d.skipped.is_empty() {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Ignored (not cut geometry):").weak());
                        for (k, v) in &d.skipped {
                            ui.label(egui::RichText::new(format!("  {k}: {v}")).weak());
                        }
                    }
                } else {
                    ui.label(egui::RichText::new("No file loaded").weak());
                }

                ui.add_space(10.0);
                ui.separator();
                ui.heading("After");
                if let Some(r) = &self.result {
                    let s = &r.stats;
                    egui::Grid::new("after_stats").num_columns(2).show(ui, |ui| {
                        let row = |ui: &mut egui::Ui, k: &str, v: String, c: Option<Color32>| {
                            match c {
                                Some(c) => ui.colored_label(c, k),
                                None => ui.label(k),
                            };
                            ui.label(v);
                            ui.end_row();
                        };
                        row(ui, "Closed paths", s.closed_paths.to_string(), None);
                        row(ui, "Open paths", s.open_paths.to_string(), if s.open_paths > 0 { Some(C_OPEN) } else { None });
                        row(ui, "Lines", s.lines.to_string(), Some(C_LINE));
                        row(ui, "Arcs", s.arcs.to_string(), Some(C_ARC));
                        row(ui, "Circles", s.circles.to_string(), Some(C_CIRCLE));
                        row(ui, "Duplicates removed", s.duplicates_removed.to_string(), None);
                        row(ui, "Max deviation", format!("{:.4} {units}", s.max_deviation), None);
                    });
                    if s.open_paths > 0 {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Open path ends are circled in pink. Increase \"Join gap\" if they should connect.")
                                .small()
                                .color(C_OPEN),
                        );
                    }
                } else {
                    ui.label(egui::RichText::new("Not cleaned yet").weak());
                }

                ui.add_space(12.0);
                ui.separator();
                ui.label(egui::RichText::new("Mouse: drag = pan, wheel = zoom, double-click = fit").small().weak());
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            let gap = 8.0;
            let w = (full.width() - gap) / 2.0;
            let left = Rect::from_min_size(full.min, Vec2::new(w, full.height()));
            let right = Rect::from_min_size(full.min + Vec2::new(w + gap, 0.0), Vec2::new(w, full.height()));
            if self.dwg.is_none() {
                let painter = ui.painter_at(full);
                painter.rect_filled(full, 4.0, BG);
                painter.text(
                    full.center(),
                    egui::Align2::CENTER_CENTER,
                    "Drop a DXF file here, or click \"Open DXF…\"",
                    egui::FontId::proportional(20.0),
                    Color32::GRAY,
                );
                return;
            }
            self.canvas(ui, left, "before", false);
            self.canvas(ui, right, "after", true);
        });
    }
}
