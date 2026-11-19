//! Colours and egui style of the Workbench design.

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle, Theme, Vec2};

pub const BG: Color32 = Color32::from_rgb(0x15, 0x17, 0x1B);
pub const PANEL: Color32 = Color32::from_rgb(0x1C, 0x1F, 0x24);
pub const RAIL: Color32 = Color32::from_rgb(0x1A, 0x1D, 0x22);
pub const CANVAS: Color32 = Color32::from_rgb(0x11, 0x13, 0x16);
pub const GRID: Color32 = Color32::from_rgb(0x1E, 0x21, 0x26);
pub const BORDER: Color32 = Color32::from_rgb(0x2A, 0x2E, 0x35);
pub const CONTROL: Color32 = Color32::from_rgb(0x23, 0x27, 0x2D);
pub const CONTROL_BORDER: Color32 = Color32::from_rgb(0x35, 0x3A, 0x42);
pub const SELECTED: Color32 = Color32::from_rgb(0x2A, 0x2F, 0x37);
pub const DIALOG: Color32 = Color32::from_rgb(0x1F, 0x23, 0x29);
pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xE8, 0xEB);
pub const TEXT_2: Color32 = Color32::from_rgb(0xC9, 0xCE, 0xD6);
pub const MUTED: Color32 = Color32::from_rgb(0x9A, 0xA1, 0xAB);
pub const DISABLED: Color32 = Color32::from_rgb(0x8A, 0x91, 0x9B);
pub const ACCENT: Color32 = Color32::from_rgb(0x5A, 0xAA, 0xFF);
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x0B, 0x12, 0x20);

// Geometry colours, shared by the canvases, the legend and the export dialog.
pub const C_BEFORE: Color32 = Color32::from_rgb(200, 200, 205);
pub const C_NODE_BEFORE: Color32 = Color32::from_rgb(255, 80, 80);
pub const C_LINE: Color32 = Color32::from_rgb(90, 170, 255);
pub const C_ARC: Color32 = Color32::from_rgb(255, 170, 60);
pub const C_CIRCLE: Color32 = Color32::from_rgb(110, 220, 120);
pub const C_NODE_AFTER: Color32 = Color32::from_rgb(255, 255, 255);
/// Open path ends, and everything that warns about them.
pub const C_OPEN: Color32 = Color32::from_rgb(0xFF, 0xD4, 0x3B);
pub const OPEN_BG: Color32 = Color32::from_rgb(0x3A, 0x32, 0x16);
pub const OPEN_TEXT: Color32 = Color32::from_rgb(0xFF, 0xE2, 0x7A);
pub const ON_OPEN: Color32 = Color32::from_rgb(0x1A, 0x15, 0x00);

// Errors: a dark red ground and a red icon, with normal text (no pink).
pub const ERROR_BG: Color32 = Color32::from_rgb(0x36, 0x1C, 0x1C);
pub const ERROR_BORDER: Color32 = Color32::from_rgb(0x7A, 0x2E, 0x2E);
pub const ERROR_ICON: Color32 = Color32::from_rgb(0xFF, 0x5F, 0x56);
pub const INFO_BG: Color32 = Color32::from_rgb(0x1F, 0x2A, 0x36);
pub const INFO_BORDER: Color32 = Color32::from_rgb(0x2E, 0x4A, 0x66);

/// Height of large buttons, such as Open DXF in the empty drop zone.
pub const BUTTON_H: f32 = 44.0;
/// Height of the toolbar's buttons; the app icon is drawn to match.
pub const TOOLBAR_H: f32 = 36.0;

/// Dark Workbench look, whatever the Windows theme is.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);

    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = DIALOG;
    v.window_stroke = Stroke::new(1.0, CONTROL_BORDER);
    v.extreme_bg_color = BG;
    v.faint_bg_color = RAIL;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);

    let r = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = r;
    w.inactive.bg_fill = CONTROL;
    w.inactive.weak_bg_fill = CONTROL;
    w.inactive.bg_stroke = Stroke::new(1.0, CONTROL_BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = SELECTED;
    w.hovered.weak_bg_fill = SELECTED;
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x4A, 0x51, 0x5B));
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.hovered.corner_radius = r;
    w.active.bg_fill = Color32::from_rgb(0x30, 0x36, 0x3F);
    w.active.weak_bg_fill = Color32::from_rgb(0x30, 0x36, 0x3F);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(2.0, TEXT);
    w.active.corner_radius = r;
    w.open = w.active;
    ctx.set_visuals_of(Theme::Dark, v);

    ctx.style_mut_of(Theme::Dark, |s| {
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.interact_size.y = 24.0;
        s.spacing.icon_width = 16.0;
        s.spacing.icon_width_inner = 10.0;
        s.text_styles.insert(TextStyle::Body, FontId::proportional(13.5));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(13.5));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(12.5));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(16.0));
    });
}
