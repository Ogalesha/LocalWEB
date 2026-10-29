//! Design tokens, typography and `egui` style for `LocalWeb`.
//!
//! The visual language is flat, minimal and square: no corner rounding, no
//! shadows, hairline borders and a single sage accent. Every color and
//! font used by the interface lives here so the system stays reviewable in one
//! place.

use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};
use std::sync::Arc;

// Working surfaces.

/// Warm sage paper canvas behind every page.
pub const CANVAS: Color32 = Color32::from_rgb(242, 244, 232);
/// Content plane: cards, banner, controls.
pub const SURFACE: Color32 = Color32::from_rgb(251, 252, 246);
/// Recessed plane for grouped or inactive content.
pub const SURFACE_MUTED: Color32 = Color32::from_rgb(231, 235, 220);
/// Hover state for rows and neutral controls.
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(239, 245, 231);

// Text.

/// Primary text.
pub const INK: Color32 = Color32::from_rgb(22, 37, 28);
/// Supporting text and metadata.
pub const INK_MUTED: Color32 = Color32::from_rgb(67, 80, 72);

// Navigation rail.

/// Deep forest rail.
pub const NAV: Color32 = Color32::from_rgb(37, 61, 44);
/// Rail supporting text.
pub const NAV_MUTED: Color32 = Color32::from_rgb(210, 221, 210);
/// Hover plane on the dark rail.
pub const NAV_HOVER: Color32 = Color32::from_rgb(47, 75, 56);
/// Pressed plane on the dark rail.
pub const NAV_PRESSED: Color32 = Color32::from_rgb(58, 90, 69);

// Actions and state.

/// Sage primary action.
pub const ACCENT: Color32 = Color32::from_rgb(157, 186, 129);
/// Sage hover.
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(169, 199, 146);
/// Sage pressed.
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(143, 182, 116);
/// Tinted green plane for selection and active fields.
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(229, 240, 219);
/// Pressed tint for neutral and text-only controls.
pub const ACCENT_SOFT_PRESSED: Color32 = Color32::from_rgb(216, 230, 204);
/// Dark green for accent text and small marks on light planes.
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(43, 79, 53);
/// Accessible focus and link color.
pub const FOCUS: Color32 = Color32::from_rgb(43, 79, 53);
/// Successful state color.
pub const SUCCESS: Color32 = Color32::from_rgb(31, 97, 57);
/// Tinted success plane.
pub const SUCCESS_SOFT: Color32 = Color32::from_rgb(226, 239, 227);
/// Warning state color.
pub const WARNING: Color32 = Color32::from_rgb(128, 75, 6);
/// Tinted warning plane.
pub const WARNING_SOFT: Color32 = Color32::from_rgb(250, 242, 226);
/// Danger state color.
pub const DANGER: Color32 = Color32::from_rgb(168, 28, 41);
/// Danger hover.
pub const DANGER_HOVER: Color32 = Color32::from_rgb(176, 31, 45);
/// Tinted danger plane.
pub const DANGER_SOFT: Color32 = Color32::from_rgb(251, 235, 236);

// Structure.

/// Hairline structure.
pub const BORDER: Color32 = Color32::from_rgb(116, 136, 119);
/// Stronger hairline for hovered structure.
pub const BORDER_STRONG: Color32 = Color32::from_rgb(73, 96, 78);
/// Inactive status marks.
pub const STATUS_OFF: Color32 = Color32::from_rgb(111, 128, 117);

/// Cards and banners are square.
pub const RADIUS_CARD: u8 = 0;
/// Controls and chips are square.
pub const RADIUS_CONTROL: u8 = 0;
/// Width of the navigation rail.
pub const RAIL_WIDTH: f32 = 236.0;

/// Standard visual height for controls.
pub const CONTROL_HEIGHT: f32 = 38.0;
/// Compact visual height for dense rows.
pub const COMPACT_CONTROL_HEIGHT: f32 = 36.0;
/// Visual height for page-level primary commands.
pub const COMMAND_HEIGHT: f32 = 40.0;
/// Minimum pointer target for a pill control.
pub const PILL_TARGET_HEIGHT: f32 = 44.0;
/// Height shared by form fields and combo boxes.
pub const FIELD_HEIGHT: f32 = 38.0;

/// Page title size.
pub const PAGE_TITLE_SIZE: f32 = 26.0;
/// Section title size.
pub const SECTION_TITLE_SIZE: f32 = 18.0;
/// Banner title size.
pub const BANNER_TITLE_SIZE: f32 = 20.0;
/// Body text size.
pub const BODY_SIZE: f32 = 14.0;
/// Button and label size.
pub const LABEL_SIZE: f32 = 13.5;
/// Small metadata size.
pub const SMALL_SIZE: f32 = 12.0;
/// Monospace metadata size.
pub const MONO_SIZE: f32 = 12.5;

/// Semibold family used for titles and strong labels.
#[must_use]
pub fn strong_family() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Medium family used for form labels and buttons.
#[must_use]
pub fn medium_family() -> FontFamily {
    FontFamily::Name("medium".into())
}

/// Installs fonts, theme colors, spacing and text styles.
pub fn configure_style(context: &egui::Context) {
    install_fonts(context);
    context.set_theme(egui::Theme::Light);
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = CANVAS;
    visuals.window_fill = SURFACE;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = RADIUS_CARD.into();
    visuals.menu_corner_radius = RADIUS_CARD.into();
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow::NONE;
    visuals.extreme_bg_color = SURFACE;
    visuals.faint_bg_color = SURFACE_MUTED;
    visuals.selection.bg_fill = ACCENT_SOFT;
    visuals.selection.stroke = Stroke::new(1.0, FOCUS);
    visuals.hyperlink_color = FOCUS;
    visuals.override_text_color = Some(INK);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.noninteractive.corner_radius = RADIUS_CONTROL.into();
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.corner_radius = RADIUS_CONTROL.into();
    visuals.widgets.hovered.bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.hovered.corner_radius = RADIUS_CONTROL.into();
    visuals.widgets.active.bg_fill = SURFACE_MUTED;
    visuals.widgets.active.weak_bg_fill = SURFACE_MUTED;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, FOCUS);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.active.corner_radius = RADIUS_CONTROL.into();
    visuals.widgets.open.corner_radius = RADIUS_CONTROL.into();
    context.set_visuals(visuals);
    context.style_mut_of(egui::Theme::Light, |style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.spacing.interact_size.y = CONTROL_HEIGHT;
        style.visuals.override_text_color = Some(INK);
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(PAGE_TITLE_SIZE, strong_family()),
        );
        style.text_styles.insert(
            TextStyle::Body,
            FontId::new(BODY_SIZE, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Button, FontId::new(LABEL_SIZE, medium_family()));
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(SMALL_SIZE, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(MONO_SIZE, FontFamily::Monospace),
        );
    });
}

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "contrast tests stay beside the public tokens they audit"
)]
mod tests {
    use super::*;

    fn luminance(color: Color32) -> f32 {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (lighter, darker) = if luminance(a) >= luminance(b) {
            (luminance(a), luminance(b))
        } else {
            (luminance(b), luminance(a))
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn text_pairs_meet_wcag_aa() {
        for (foreground, background) in [
            (INK, CANVAS),
            (INK_MUTED, CANVAS),
            (INK, SURFACE),
            (NAV_MUTED, NAV),
            (INK, ACCENT),
            (Color32::WHITE, DANGER),
            (ACCENT_TEXT, ACCENT_SOFT),
            (DANGER, SURFACE),
            (DANGER, CANVAS),
            (DANGER, DANGER_SOFT),
            (WARNING, WARNING_SOFT),
            (FOCUS, CANVAS),
        ] {
            assert!(
                contrast(foreground, background) >= 4.5,
                "contrast ratio was {}",
                contrast(foreground, background)
            );
        }
    }

    #[test]
    fn control_borders_and_focus_meet_three_to_one() {
        assert!(contrast(BORDER, SURFACE) >= 3.0);
        assert!(contrast(BORDER_STRONG, SURFACE) >= 3.0);
        assert!(contrast(FOCUS, CANVAS) >= 3.0);
        assert!(contrast(NAV_MUTED, NAV) >= 3.0);
    }
}

fn install_fonts(context: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (name, bytes) in [
        (
            "inter",
            include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice(),
        ),
        (
            "inter-medium",
            include_bytes!("../assets/fonts/Inter-Medium.ttf").as_slice(),
        ),
        (
            "inter-semibold",
            include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    let mut cjk =
        FontData::from_static(include_bytes!("../assets/fonts/NotoSansCJK-Regular.ttc").as_slice());
    cjk.index = 2;
    fonts.font_data.insert("noto-cjk".to_owned(), Arc::new(cjk));
    fonts.font_data.insert(
        "noto-devanagari".to_owned(),
        Arc::new(FontData::from_static(
            include_bytes!("../assets/fonts/NotoSansDevanagari-Regular.ttf").as_slice(),
        )),
    );
    let extras = ["noto-devanagari", "noto-cjk"];
    let mut proportional = vec!["inter".to_owned()];
    proportional.extend(extras.iter().map(|name| (*name).to_owned()));
    proportional.extend(fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Proportional, proportional);
    for (family, font) in [("medium", "inter-medium"), ("semibold", "inter-semibold")] {
        let mut chain = vec![font.to_owned()];
        chain.extend(extras.iter().map(|name| (*name).to_owned()));
        chain.extend(fallback.iter().cloned());
        fonts
            .families
            .insert(FontFamily::Name(family.into()), chain);
    }
    context.set_fonts(fonts);
}
