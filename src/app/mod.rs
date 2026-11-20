//! The DXF Cleaner window: the Workbench layout of toolbar, settings rail, Before/After
//! views and result strip, plus the empty state and the open-path export warning.

mod canvas;
mod empty;
mod export_dialog;
mod icons;
mod logo;
mod notice;
mod result_strip;
mod settings;
pub mod theme;
mod toolbar;
mod widgets;

use canvas::View;
use dxf_cleaner::geom::{BBox, P};
use dxf_cleaner::*;
use eframe::egui::{self, Rect, UiBuilder, Vec2};
use export_dialog::ExportWarning;
use notice::Notice;
use std::path::PathBuf;

pub const ICON_PNG: &[u8] = include_bytes!("../../assets/icon-256.png");

pub struct App {
    /// The open source file and its drawing.
    job: Option<Job>,
    /// What the Before view shows of `job`.
    before: Before,
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
    logo: logo::Logo,
}

impl App {
    pub fn new() -> Self {
        App {
            job: None,
            before: Before::default(),
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
            logo: logo::Logo::default(),
        }
    }

    pub fn load(&mut self, path: PathBuf) {
        match Job::open(path) {
            Ok(job) => {
                self.before = job.before();
                self.job = Some(job);
                self.result = None;
                self.result_stale = false;
                self.view = None;
                self.fit_to = None;
                self.notice = None;
            }
            // Job::open's message already names the file.
            Err(e) => self.notice = Some(Notice::error(e)),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(f) = rfd::FileDialog::new().add_filter("DXF drawing", &["dxf", "DXF"]).pick_file() {
            self.load(f);
        }
    }

    fn run_clean(&mut self) {
        let Some(job) = &self.job else { return };
        self.opt.put_everything_on_layer_0(self.one_layer);
        self.result = Some(job.clean(&self.opt));
        self.result_stale = false;
        self.result_tolerance = self.opt.tolerance;
        if self.opt.move_to_origin {
            self.view = None; // geometry moved, refit the view
        }
    }

    /// A Clean Up setting changed: the result no longer matches the settings.
    fn settings_changed(&mut self) {
        if self.result.is_some() {
            self.result_stale = true;
        }
    }

    /// The result Export DXF may write, or why there is none. The toolbar button and
    /// the export path both ask here.
    fn exportable(&self) -> Result<&CleanResult, NotExportable> {
        match &self.result {
            None => Err(NotExportable::NoResult),
            Some(_) if self.result_stale => Err(NotExportable::Stale),
            Some(res) => Ok(res),
        }
    }

    /// Export DXF: first warns about open paths, if there are any.
    fn export(&mut self) {
        let Ok(res) = self.exportable() else { return };
        if res.stats.open_paths > 0 {
            self.export_warning = Some(ExportWarning::new());
        } else {
            self.save_dialog();
        }
    }

    /// The DXF that Export DXF writes, in the chosen export format.
    fn export_text(&self) -> Result<String, NotExportable> {
        let res = self.exportable()?;
        let job = self.job.as_ref().ok_or(NotExportable::NoResult)?;
        Ok(job.dxf_text(res, self.mode))
    }

    fn save_dialog(&mut self) {
        let Some(job) = &self.job else { return };
        if self.exportable().is_err() {
            return;
        }
        let default = job.default_output();
        let name = default.file_name().unwrap_or_default().to_string_lossy();
        let mut dlg = rfd::FileDialog::new().add_filter("DXF drawing", &["dxf"]).set_file_name(name);
        if let Some(d) = default.parent() {
            dlg = dlg.set_directory(d);
        }
        if let Some(out) = dlg.save_file() {
            let Ok(text) = self.export_text() else { return };
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

/// Why Export DXF cannot write the result.
#[derive(Debug, PartialEq)]
enum NotExportable {
    /// Clean Up has not run on the open file.
    NoResult,
    /// A Clean Up setting changed after Clean Up ran.
    Stale,
}

impl NotExportable {
    /// What hovering the disabled Export DXF button says.
    fn hover_text(&self) -> &'static str {
        match self {
            NotExportable::NoResult => "Click Clean Up first.",
            NotExportable::Stale => STALE_TEXT,
        }
    }
}

/// Shown while the result no longer matches the Clean Up settings.
const STALE_TEXT: &str = "Settings changed. Clean Up again.";

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
            if self.job.is_none() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn cleaned() -> App {
        let mut app = App::new();
        app.load(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/illustrator_polluted.dxf"));
        app.run_clean();
        app
    }

    #[test]
    fn a_changed_setting_blocks_export_until_clean_up_runs_again() {
        let mut app = cleaned();
        assert!(app.exportable().is_ok());
        app.settings_changed();
        assert_eq!(app.exportable().err(), Some(NotExportable::Stale));
        app.run_clean();
        assert!(app.exportable().is_ok());
    }

    #[test]
    fn the_export_path_refuses_a_stale_result() {
        let mut app = cleaned();
        app.settings_changed();
        assert_eq!(app.export_text().err(), Some(NotExportable::Stale));
    }

    #[test]
    fn after_clean_up_runs_again_the_export_matches_the_current_settings() {
        let mut app = cleaned();
        let old = app.export_text().unwrap();
        app.opt.move_to_origin = !app.opt.move_to_origin;
        app.settings_changed();
        app.run_clean();
        let fresh = Job::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/illustrator_polluted.dxf")).unwrap();
        let opt = Options { move_to_origin: !Options::default().move_to_origin, ..Options::default() };
        let expected = fresh.dxf_text(&fresh.clean(&opt), OutputMode::Polylines);
        assert_eq!(app.export_text().unwrap(), expected);
        assert_ne!(old, expected, "moving to the origin must change the sample for this test to mean anything");
    }

    #[test]
    fn switching_the_export_format_leaves_export_enabled() {
        let mut app = cleaned();
        app.mode = OutputMode::LinesAndArcs;
        assert!(app.exportable().is_ok());
        app.mode = OutputMode::Polylines;
        assert!(app.exportable().is_ok());
    }

    #[test]
    fn opening_a_file_blocks_export_until_clean_up_runs() {
        let mut app = cleaned();
        app.settings_changed();
        app.load(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/manual_fix.dxf"));
        assert_eq!(app.exportable().err(), Some(NotExportable::NoResult));
        app.run_clean();
        assert!(app.exportable().is_ok());
    }

    #[test]
    fn a_stale_result_says_to_clean_up_again() {
        assert_eq!(NotExportable::Stale.hover_text(), "Settings changed. Clean Up again.");
    }
}
