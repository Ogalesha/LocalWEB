//! `LocalWeb` brand assets: the desktop/window icon and the horizontal logo.
//!
//! The PNG is the desktop and window icon on Linux and macOS. Windows uses the
//! multi-size ICO for the window and taskbar icon. The horizontal logo is shown
//! in Settings and in the repository README.

use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};
use image::{RgbaImage, imageops::FilterType};

const ICON_PNG: &[u8] = include_bytes!("../../../assets/brand/localweb-icon.png");
const ICON_ICO: &[u8] = include_bytes!("../../../assets/brand/localweb-icon.ico");
const LOGO_PNG: &[u8] = include_bytes!("../../../assets/brand/localweb-logo.png");

/// Largest edge of the window and tray icon handed to the platform.
const WINDOW_ICON_SIZE: u32 = 256;
/// Largest edge of the icon texture drawn inside the interface.
const UI_ICON_SIZE: u32 = 256;
/// Largest width of the logo texture drawn inside the interface.
const UI_LOGO_WIDTH: u32 = 691;

/// Returns the icon used by the window manager, taskbar and system tray.
///
/// Windows uses the ICO so the taskbar receives the intended multi-size
/// artwork; the other platforms use the PNG desktop icon.
#[must_use]
pub fn icon_data() -> egui::IconData {
    let bytes = if cfg!(windows) {
        ico_largest_frame(ICON_ICO).unwrap_or(ICON_PNG)
    } else {
        ICON_PNG
    };
    let image = decode(bytes, WINDOW_ICON_SIZE)
        .expect("the embedded LocalWeb icon is a valid PNG or ICO image");
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

/// Loads the icon texture used by the navigation rail.
#[must_use]
pub fn icon_texture(context: &egui::Context) -> Option<TextureHandle> {
    let image = decode(ICON_PNG, UI_ICON_SIZE)?;
    Some(context.load_texture("localweb-icon", color_image(&image), TextureOptions::LINEAR))
}

/// Loads the horizontal logo texture used in Settings.
#[must_use]
pub fn logo_texture(context: &egui::Context) -> Option<TextureHandle> {
    let image = decode(LOGO_PNG, UI_LOGO_WIDTH)?;
    Some(context.load_texture("localweb-logo", color_image(&image), TextureOptions::LINEAR))
}

/// Draws the brand icon at `size` points, falling back to an accent tile when
/// the texture could not be decoded.
pub fn app_icon(ui: &mut egui::Ui, texture: Option<&TextureHandle>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
    if let Some(texture) = texture {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().rect_filled(rect, 0, crate::theme::ACCENT);
    }
}

/// Draws the horizontal logo at the requested width, preserving its aspect
/// ratio. Does nothing when the texture is unavailable.
pub fn logo(ui: &mut egui::Ui, texture: Option<&TextureHandle>, width: f32) {
    let Some(texture) = texture else {
        return;
    };
    let source = texture.size_vec2();
    let size = egui::vec2(width, width * source.y / source.x);
    ui.image((texture.id(), size));
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "image dimensions are small positive integers that fit exactly in f32 and u32"
)]
fn decode(bytes: &[u8], max_size: u32) -> Option<RgbaImage> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    if width <= max_size && height <= max_size {
        return Some(image);
    }
    let scale = max_size as f32 / width.max(height) as f32;
    let width = (width as f32 * scale).round().max(1.0) as u32;
    let height = (height as f32 * scale).round().max(1.0) as u32;
    Some(image::imageops::resize(
        &image,
        width,
        height,
        FilterType::Lanczos3,
    ))
}

fn color_image(image: &RgbaImage) -> ColorImage {
    let (width, height) = image.dimensions();
    ColorImage::from_rgba_unmultiplied([width as usize, height as usize], image.as_raw())
}

/// Returns the largest PNG frame stored in an ICO file.
///
/// The bundled Windows icon stores every size as PNG, so the returned bytes can
/// be decoded by the PNG decoder already used for the other platforms.
fn ico_largest_frame(bytes: &[u8]) -> Option<&[u8]> {
    let count = u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?) as usize;
    let mut best: Option<(u32, &[u8])> = None;
    for index in 0..count {
        let start = 6 + index * 16;
        let entry = bytes.get(start..start + 16)?;
        let dimension = |value: u8| if value == 0 { 256 } else { u32::from(value) };
        let area = dimension(entry[0]) * dimension(entry[1]);
        let size = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
        let frame = bytes.get(offset..offset.checked_add(size)?)?;
        if best.is_none_or(|(best_area, _)| area > best_area) {
            best = Some((area, frame));
        }
    }
    best.map(|(_, frame)| frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_brand_assets_decode() {
        let icon = decode(ICON_PNG, WINDOW_ICON_SIZE).expect("the PNG icon decodes");
        assert_eq!(icon.dimensions(), (WINDOW_ICON_SIZE, WINDOW_ICON_SIZE));

        let logo = decode(LOGO_PNG, UI_LOGO_WIDTH).expect("the PNG logo decodes");
        assert_eq!(logo.width(), UI_LOGO_WIDTH);
        assert!(logo.height() > 0);

        let frame = ico_largest_frame(ICON_ICO).expect("the ICO has a frame");
        assert!(frame.starts_with(b"\x89PNG\r\n\x1a\n"));
        let windows_icon = decode(frame, WINDOW_ICON_SIZE).expect("the ICO frame decodes");
        assert_eq!(windows_icon.dimensions(), (256, 256));
    }

    #[test]
    fn window_icon_matches_the_platform_asset() {
        let icon = icon_data();
        assert_eq!(icon.width, WINDOW_ICON_SIZE);
        assert_eq!(icon.height, WINDOW_ICON_SIZE);
        assert_eq!(
            icon.rgba.len(),
            (WINDOW_ICON_SIZE * WINDOW_ICON_SIZE * 4) as usize
        );
    }
}
