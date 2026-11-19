//! The DXF Cleaner window: the Workbench layout of toolbar, settings rail, Before/After
//! views and result strip, plus the empty state and the open-path export warning.

mod canvas;
mod empty;
mod export_dialog;
mod icons;
mod notice;
mod result_strip;
mod settings;
pub mod theme;
mod toolbar;
mod widgets;

use canvas::View;
use dxf_cleaner::geom::{BBox, P};
use dxf_cleaner::spans::{Outline, outline};
use dxf_cleaner::*;
use eframe::egui::{self, Rect, TextureHandle, UiBuilder, Vec2};
use export_dialog::ExportWarning;
use notice::Notice;
use std::path::PathBuf;

pub const ICON_PNG: &[u8] = include_bytes!("../../assets/icon-256.png");

pub struct App {
    path: Option<PathBuf>,
    dwg: Option<Drawing>,
    before: Vec<Outline>,
    before_nodes: usize,
    bbox: BBox,
    result: Option<CleanResult>,
    result_stale: bool,
    /// The Tolerance `result` was cleaned with; the setting may have changed since.
    result_tolerance: f64,
    opt: Options,
    one_layer: bool,
    mode: OutputMode,
    show_nodes: bool,
    /// Shared by both views; `None` refits them on the next frame.
    view: Option<View>,
    /// What the next refit shows instead of the whole drawing.
    fit_to: Option<BBox>,
    notice: Option<Notice>,
    export_warning: Option<ExportWarning>,
    icon: Option<TextureHandle>,
}

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        let icon = eframe::icon_data::from_png_bytes(ICON_PNG).ok().map(|d| {
            let img = egui::ColorImage::from_rgba_unmultiplied([d.width as usize, d.height as usize], &d.rgba);
            let opts = egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR };
            ctx.load_texture("app-icon", img, opts)
        });
        App {
            path: None,
            dwg: None,
            before: Vec::new(),
            before_nodes: 0,
            bbox: BBox::empty(),
            result: None,
            result_stale: false,
            result_tolerance: 0.0,
            opt: Options::default(),
            one_layer: false,
            mode: OutputMode::Polylines,
            show_nodes: true,
            view: None,
            fit_to: None,
            notice: None,
            export_warning: None,
            icon,
        }
    }

    pub fn load(&mut self, path: PathBuf) {
        match read_file(&path) {
            Ok(dwg) => {
                self.before = dwg.entities.iter().map(|e| outline(&e.geom)).collect();
                self.before_nodes = self.before.iter().map(|o| o.nodes.len()).sum();
                let mut bb = BBox::empty();
                for o in &self.before {
                    for q in &o.pts {
                        bb.add(*q);
                    }
                }
                self.bbox = bb;
                self.dwg = Some(dwg);
                self.path = Some(path);
                self.result = None;
                self.result_stale = false;
                self.view = None;
                self.fit_to = None;
                self.notice = None;
            }
            Err(e) => {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                self.notice = Some(Notice::error(format!("Could not open {name}: {e}")));
            }
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
        self.result = Some(clean(dwg, &self.opt));
        self.result_stale = false;
        self.result_tolerance = self.opt.tolerance;
        if self.opt.move_to_origin {
            self.view = None; // geometry moved, refit the view
        }
    }

    /// Export DXF: first warns about open paths, if there are any.
    fn export(&mut self) {
        let Some(res) = &self.result else { return };
        if res.stats.open_paths > 0 {
            self.export_warning = Some(ExportWarning::new());
        } else {
            self.save_dialog();
        }
    }

    fn save_dialog(&mut self) {
        let Some(res) = &self.result else { return };
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
            self.notice = Some(match std::fs::write(&out, text) {
                Ok(()) => Notice::info(format!("Exported {}", out.file_name().unwrap_or_default().to_string_lossy())),
                Err(e) => Notice::error(format!("Could not write {}: {e}", out.display())),
            });
        }
    }

    /// The open paths of the result, with some room around them.
    fn open_paths_bbox(&self) -> Option<BBox> {
        let res = self.result.as_ref()?;
        let mut bb = BBox::empty();
        for s in &res.shapes {
            if let Shape::Path(p) = s {
                if !p.is_closed() {
                    bb.union(&p.bbox());
                }
            }
        }
        if !bb.is_valid() {
            return None;
        }
        let pad = 0.5 * bb.width().max(bb.height()).max(1.0);
        bb.add(P { x: bb.min.x - pad, y: bb.min.y - pad });
        bb.add(P { x: bb.max.x + pad, y: bb.max.y + pad });
        Some(bb)
    }

    fn units(&self) -> &'static str {
        match self.dwg.as_ref().and_then(|d| d.insunits) {
            Some(1) => "in",
            Some(2) => "ft",
            Some(4) => "mm",
            Some(5) => "cm",
            Some(6) => "m",
            _ => "units",
        }
    }

    /// Both views side by side, with the result strip under them.
    fn workbench(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        let gap = 12.0;
        let strip_h = 64.0;
        let views = Rect::from_min_max(full.min, egui::pos2(full.max.x, full.max.y - strip_h - gap));
        let w = (views.width() - gap) / 2.0;
        let left = Rect::from_min_size(views.min, Vec2::new(w, views.height()));
        let right = Rect::from_min_size(views.min + Vec2::new(w + gap, 0.0), Vec2::new(w, views.height()));
        self.canvas(ui, left, "before", false);
        self.canvas(ui, right, "after", true);
        let strip = Rect::from_min_max(egui::pos2(full.min.x, full.max.y - strip_h), full.max);
        ui.scope_builder(UiBuilder::new().max_rect(strip), |ui| result_strip::show(self, ui));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let dropped: Vec<PathBuf> =
            ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if let Some(f) = dropped.into_iter().next() {
            self.load(f);
        }

        toolbar::show(self, ui);
        settings::show(self, ui);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG).inner_margin(12)).show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            if self.dwg.is_none() {
                empty::show(self, ui);
            } else {
                self.workbench(ui);
            }
            notice::show(self, ui, area);
        });
        if self.export_warning.is_some() {
            export_dialog::show(self, ui.ctx());
        }
    }
}
