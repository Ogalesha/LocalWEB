//! `LocalWeb` brand mark, shared by the interface and the window icon.

use crate::theme;
use eframe::egui::{self, Color32, Painter, Rect, Stroke, Vec2};

/// The icon grid coordinate space.
const VIEW: f32 = 128.0;
/// Tile behind the glyph, a shade darker than the navigation rail.
const TILE: Color32 = Color32::from_rgb(23, 42, 31);
/// Tile outline so the mark stays legible on the dark rail.
const OUTLINE: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 22);

fn origin(rect: Rect) -> (egui::Pos2, f32) {
    let scale = rect.width().min(rect.height()) / VIEW;
    (rect.center() - Vec2::splat(VIEW * scale / 2.0), scale)
}

fn area(rect: Rect, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    let (origin, scale) = origin(rect);
    Rect::from_min_max(
        origin + Vec2::new(x0 * scale, y0 * scale),
        origin + Vec2::new(x1 * scale, y1 * scale),
    )
}

fn paint_glyph(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0, TILE);
    painter.rect_filled(area(rect, 30.0, 32.0, 46.0, 96.0), 0, theme::ACCENT);
    painter.rect_filled(area(rect, 30.0, 82.0, 98.0, 96.0), 0, theme::ACCENT);
    let (origin, scale) = origin(rect);
    painter.circle_filled(
        origin + Vec2::new(85.0 * scale, 47.0 * scale),
        17.0 * scale,
        theme::SURFACE,
    );
}

/// Paints the application icon with a hairline outline for the dark rail.
pub fn app_icon(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter();
    paint_glyph(painter, rect);
    painter.rect_stroke(rect, 0, Stroke::new(1.0, OUTLINE), egui::StrokeKind::Inside);
}

fn inside_rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
    x >= x0 && x < x1 && y >= y0 && y < y1
}

fn sample(x: f32, y: f32) -> Color32 {
    if inside_rect(x, y, 30.0, 32.0, 46.0, 96.0) || inside_rect(x, y, 30.0, 82.0, 98.0, 96.0) {
        theme::ACCENT
    } else if (x - 85.0).powi(2) + (y - 47.0).powi(2) <= 17.0 * 17.0 {
        theme::SURFACE
    } else {
        TILE
    }
}

/// Rasterises the same geometry for the window icon.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a small fixed-size pixel grid is clearer as indexed loops"
)]
#[must_use]
pub fn icon_data() -> egui::IconData {
    const SIZE: u32 = 128;
    const SAMPLES: u32 = 4;
    let sample_count = f32::from(SAMPLES as u16);
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut total = [0u32; 3];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / sample_count;
                    let py = y as f32 + (sy as f32 + 0.5) / sample_count;
                    let color = sample(px, py);
                    total[0] += u32::from(color.r());
                    total[1] += u32::from(color.g());
                    total[2] += u32::from(color.b());
                }
            }
            let count = SAMPLES * SAMPLES;
            rgba.push((total[0] / count) as u8);
            rgba.push((total[1] / count) as u8);
            rgba.push((total[2] / count) as u8);
            rgba.push(255);
        }
    }
    egui::IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}
