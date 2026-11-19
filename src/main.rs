#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 820.0])
        .with_min_inner_size([900.0, 560.0])
        .with_title("DXF Cleaner")
        .with_drag_and_drop(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(app::ICON_PNG) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let start_file = std::env::args().nth(1).map(PathBuf::from);
    eframe::run_native(
        "DXF Cleaner",
        options,
        Box::new(move |cc| {
            app::theme::apply(&cc.egui_ctx);
            let mut app = app::App::new(&cc.egui_ctx);
            if let Some(f) = start_file {
                app.load(f);
            }
            Ok(Box::new(app))
        }),
    )
}
