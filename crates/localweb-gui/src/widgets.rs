//! Shared interface components built on the design tokens.

use crate::{icons::Icon, theme};
use eframe::egui::{self, Color32, FontId, Margin, Response, RichText, Sense, Stroke, Ui, Vec2};

/// Semibold text used for titles and strong labels.
#[must_use]
pub fn strong(text: impl Into<String>) -> RichText {
    RichText::new(text).family(theme::strong_family())
}

/// Medium text used for form labels and secondary emphasis.
#[must_use]
pub fn medium(text: impl Into<String>) -> RichText {
    RichText::new(text).family(theme::medium_family())
}

/// Visual weight of a square control.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PillKind {
    /// Mint primary action.
    Primary,
    /// Red destructive action.
    Danger,
    /// Neutral outlined action.
    Secondary,
    /// Text-only action without a resting container.
    Ghost,
    /// Text-only destructive action.
    GhostDanger,
}

/// A flat square control with optional leading icon.
pub struct Pill<'a> {
    icon: Option<Icon>,
    label: &'a str,
    kind: PillKind,
    enabled: bool,
    min_width: f32,
    height: f32,
}

impl<'a> Pill<'a> {
    #[must_use]
    pub fn new(label: &'a str, kind: PillKind) -> Self {
        Self {
            icon: None,
            label,
            kind,
            enabled: true,
            min_width: 0.0,
            height: 38.0,
        }
    }

    #[must_use]
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    #[must_use]
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    #[must_use]
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one paint routine keeps every pill state consistent"
    )]
    pub fn show(self, ui: &mut Ui) -> Response {
        let enabled = self.enabled && ui.is_enabled();
        let font = FontId::new(13.5, theme::medium_family());
        let base_text = match self.kind {
            PillKind::Danger => Color32::WHITE,
            PillKind::Primary | PillKind::Secondary | PillKind::Ghost => theme::INK,
            PillKind::GhostDanger => theme::DANGER,
        };
        let text_color = if enabled {
            base_text
        } else {
            base_text.gamma_multiply(0.65)
        };
        let pad = match self.kind {
            PillKind::Primary | PillKind::Danger => 17.0,
            PillKind::Secondary => 15.0,
            PillKind::Ghost | PillKind::GhostDanger => 10.0,
        };
        let galley = ui
            .painter()
            .layout_no_wrap(self.label.to_owned(), font, text_color);
        let icon_size = (self.height * 0.42).clamp(13.0, 16.0);
        let icon_w = if self.icon.is_some() {
            icon_size + 7.0
        } else {
            0.0
        };
        let width = (galley.size().x + icon_w + pad * 2.0).max(self.min_width);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, self.height), sense);
        if ui.is_rect_visible(rect) {
            let hovered = enabled && response.hovered();
            let pressed = enabled && response.is_pointer_button_down_on();
            let t = ui
                .ctx()
                .animate_bool_with_time(response.id.with("hover"), hovered, 0.12);
            let (fill, stroke_color, stroke_width) = match self.kind {
                PillKind::Primary => {
                    let target = if pressed {
                        theme::ACCENT_PRESSED
                    } else {
                        theme::ACCENT_HOVER
                    };
                    let t = if pressed { 1.0 } else { t };
                    if enabled {
                        (
                            theme::ACCENT.lerp_to_gamma(target, t),
                            Color32::TRANSPARENT,
                            0.0,
                        )
                    } else {
                        (
                            theme::ACCENT.gamma_multiply(0.55),
                            Color32::TRANSPARENT,
                            0.0,
                        )
                    }
                }
                PillKind::Danger => {
                    let target = if pressed {
                        theme::DANGER
                    } else {
                        theme::DANGER_HOVER
                    };
                    if enabled {
                        (
                            theme::DANGER.lerp_to_gamma(target, t),
                            Color32::TRANSPARENT,
                            0.0,
                        )
                    } else {
                        (
                            theme::DANGER.gamma_multiply(0.55),
                            Color32::TRANSPARENT,
                            0.0,
                        )
                    }
                }
                PillKind::Secondary => {
                    if enabled {
                        if pressed {
                            (theme::ACCENT_SOFT_PRESSED, theme::FOCUS, 1.0)
                        } else {
                            (
                                theme::SURFACE.lerp_to_gamma(theme::SURFACE_HOVER, t),
                                theme::BORDER.lerp_to_gamma(theme::BORDER_STRONG, t),
                                1.0,
                            )
                        }
                    } else {
                        (theme::SURFACE, theme::BORDER.gamma_multiply(0.6), 1.0)
                    }
                }
                PillKind::Ghost | PillKind::GhostDanger => {
                    if enabled {
                        if pressed {
                            match self.kind {
                                PillKind::GhostDanger => (theme::DANGER_SOFT, theme::DANGER, 1.0),
                                _ => (theme::ACCENT_SOFT_PRESSED, Color32::TRANSPARENT, 0.0),
                            }
                        } else {
                            let tint = match self.kind {
                                PillKind::GhostDanger => theme::DANGER_SOFT,
                                _ => theme::ACCENT_SOFT,
                            };
                            (tint.gamma_multiply(t), Color32::TRANSPARENT, 0.0)
                        }
                    } else {
                        (Color32::TRANSPARENT, Color32::TRANSPARENT, 0.0)
                    }
                }
            };
            let painter = ui.painter();
            let radius: egui::CornerRadius = theme::RADIUS_CONTROL.into();
            painter.rect_filled(rect, radius, fill);
            if stroke_width > 0.0 {
                painter.rect_stroke(
                    rect,
                    radius,
                    Stroke::new(stroke_width, stroke_color),
                    egui::StrokeKind::Inside,
                );
            }
            if response.has_focus() {
                focus_ring(ui, rect, theme::FOCUS);
            }
            let start_x =
                f32::midpoint(rect.center().x - icon_w, rect.center().x - galley.size().x);
            if let Some(icon) = self.icon {
                let icon_color = if enabled {
                    match self.kind {
                        PillKind::Primary => theme::INK,
                        PillKind::Danger => Color32::WHITE,
                        PillKind::GhostDanger => theme::DANGER,
                        PillKind::Secondary | PillKind::Ghost => theme::INK_MUTED,
                    }
                } else {
                    text_color
                };
                let icon_rect = egui::Rect::from_center_size(
                    egui::pos2(start_x + icon_size / 2.0, rect.center().y),
                    Vec2::splat(icon_size),
                );
                crate::icons::paint(
                    painter,
                    icon_rect,
                    icon,
                    icon_color,
                    (self.height * 0.095).clamp(1.3, 1.75),
                );
            }
            painter.galley(
                egui::pos2(start_x + icon_w, rect.center().y - galley.size().y / 2.0),
                galley,
                text_color,
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, self.label.to_owned())
        });
        response
    }
}

/// Content card at rest: flat surface plane with a hairline border.
pub fn card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(theme::RADIUS_CARD)
        .inner_margin(Margin::same(18))
}

/// Foreground content card used for the banner and focused surfaces.
pub fn raised_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
        .corner_radius(theme::RADIUS_CARD)
        .inner_margin(Margin::same(18))
}

/// Filled square with a centered icon, used for state marks.
pub fn icon_tile(ui: &mut Ui, icon: Icon, background: Color32, foreground: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect_filled(rect, 0, background);
    crate::icons::paint(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), Vec2::splat(size * 0.46)),
        icon,
        foreground,
        (size * 0.06).clamp(1.5, 2.6),
    );
}

/// Status mark: solid square, always beside a text label.
pub fn status_mark(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
    ui.painter().rect_filled(
        egui::Rect::from_center_size(rect.center(), Vec2::splat(7.0)),
        0,
        color,
    );
}

/// Two-pixel square focus ring drawn outside the target.
pub fn focus_ring(ui: &Ui, rect: egui::Rect, color: Color32) {
    ui.painter().rect_stroke(
        rect.expand(2.0),
        egui::CornerRadius::ZERO,
        Stroke::new(2.0, color),
        egui::StrokeKind::Outside,
    );
}

/// Small filled chip used for observed project state.
pub fn state_chip(ui: &mut Ui, running: bool, active_text: &str, idle_text: &str) {
    let (fill, color, label) = if running {
        (theme::ACCENT_SOFT, theme::ACCENT_TEXT, active_text)
    } else {
        (theme::SURFACE_MUTED, theme::INK_MUTED, idle_text)
    };
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(theme::RADIUS_CONTROL)
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                status_mark(
                    ui,
                    if running {
                        theme::SUCCESS
                    } else {
                        theme::STATUS_OFF
                    },
                );
                ui.label(medium(label).size(12.0).color(color));
            });
        });
}

/// Compact labelled state used for read-only presets.
pub fn service_chip(ui: &mut Ui, title: &str, value: &str, color: Color32) {
    egui::Frame::NONE
        .fill(theme::SURFACE_MUTED)
        .corner_radius(theme::RADIUS_CONTROL)
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_min_width(128.0);
            ui.horizontal(|ui| {
                status_mark(ui, color);
                ui.vertical(|ui| {
                    ui.label(medium(title).size(12.0).color(theme::INK));
                    ui.label(RichText::new(value).size(12.0).color(theme::INK_MUTED));
                });
            });
        });
}

/// Page title block.
pub fn page_header(ui: &mut Ui, title: &str) {
    ui.label(strong(title).size(30.0).color(theme::INK));
}

/// Section heading inside a page.
pub fn section_heading(ui: &mut Ui, title: &str, description: &str) {
    ui.label(strong(title).size(18.0).color(theme::INK));
    ui.label(
        RichText::new(description)
            .size(13.0)
            .color(theme::INK_MUTED),
    );
}

/// Visible form label.
pub fn form_label(ui: &mut Ui, label: &str) -> Response {
    ui.label(medium(label).size(13.0).color(theme::INK))
}

/// Tinted inline plane with a leading icon for persistent conditions.
pub fn callout(ui: &mut Ui, icon: Icon, content: impl FnOnce(&mut Ui)) {
    let (fill, color) = (theme::DANGER_SOFT, theme::DANGER);
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(theme::RADIUS_CARD)
        .inner_margin(Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                crate::icons::paint(ui.painter(), rect, icon, color, 1.7);
                ui.add_space(2.0);
                content(ui);
            });
        });
}

/// Small square icon button used for dismiss and similar affordances.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let pressed = response.is_pointer_button_down_on();
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    if ui.is_rect_visible(rect) {
        if pressed {
            ui.painter()
                .rect_filled(rect, theme::RADIUS_CONTROL, theme::ACCENT_SOFT_PRESSED);
        } else if t > 0.0 {
            ui.painter().rect_filled(
                rect,
                theme::RADIUS_CONTROL,
                theme::ACCENT_SOFT.gamma_multiply(t),
            );
        }
        let color = if pressed {
            theme::INK
        } else {
            theme::INK_MUTED.lerp_to_gamma(theme::INK, t)
        };
        crate::icons::paint(
            ui.painter(),
            egui::Rect::from_center_size(rect.center(), Vec2::splat(size * 0.58)),
            icon,
            color,
            1.6,
        );
    }
    if response.has_focus() {
        focus_ring(ui, rect, theme::FOCUS);
    }
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, String::new()));
    response
}

/// Checkbox with a full-width 36 px clickable row.
#[must_use]
pub fn check_row(ui: &mut Ui, checked: &mut bool, label: &str) -> Response {
    let width = ui.available_width();
    let galley = ui.painter().layout(
        label.to_owned(),
        FontId::new(13.0, theme::medium_family()),
        theme::INK,
        (width - 32.0).max(80.0),
    );
    let height = (galley.size().y + 12.0).max(36.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let pressed = response.is_pointer_button_down_on();
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let activate = response.clicked()
        || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Space)));
    if activate {
        *checked = !*checked;
    }
    let box_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 8.0, rect.center().y - 8.0),
        Vec2::splat(16.0),
    );
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if pressed {
            painter.rect_filled(rect, theme::RADIUS_CONTROL, theme::ACCENT_SOFT_PRESSED);
        } else if t > 0.0 {
            painter.rect_filled(
                rect,
                theme::RADIUS_CONTROL,
                theme::SURFACE_HOVER.gamma_multiply(t),
            );
        }
        painter.rect_filled(box_rect, 0, theme::SURFACE);
        painter.rect_stroke(
            box_rect,
            0,
            Stroke::new(
                1.0,
                if *checked {
                    theme::ACCENT
                } else {
                    theme::BORDER_STRONG
                },
            ),
            egui::StrokeKind::Inside,
        );
        if *checked {
            painter.rect_filled(box_rect, 0, theme::ACCENT);
            crate::icons::paint(painter, box_rect, Icon::Check, theme::INK, 1.9);
        }
        painter.galley(
            egui::pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0),
            galley,
            theme::INK,
        );
    }
    if response.has_focus() {
        focus_ring(ui, box_rect.expand(1.0), theme::FOCUS);
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *checked, label.to_owned())
    });
    response
}

/// Centered empty state with an optional primary action inside the card.
///
/// Returns true when the action was clicked.
pub fn empty_card(
    ui: &mut Ui,
    icon: Icon,
    title: &str,
    body: &str,
    action: Option<(&str, Icon)>,
) -> bool {
    let mut clicked = false;
    raised_frame()
        .inner_margin(Margin::symmetric(28, 30))
        .show(ui, |ui| {
            let width = (ui.available_width() - 56.0).max(240.0);
            ui.set_min_width(width);
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
                crate::icons::paint(
                    ui.painter(),
                    rect,
                    icon,
                    theme::INK_MUTED.gamma_multiply(0.8),
                    2.2,
                );
                ui.add_space(10.0);
                ui.label(strong(title).size(17.0).color(theme::INK));
                ui.label(RichText::new(body).size(13.5).color(theme::INK_MUTED));
                if let Some((label, action_icon)) = action {
                    ui.add_space(14.0);
                    clicked = Pill::new(label, PillKind::Primary)
                        .icon(action_icon)
                        .show(ui)
                        .clicked();
                }
            });
        });
    clicked
}

/// Navigation rail item: icon plus label, selected state and hover.
pub fn rail_item(ui: &mut Ui, label: &str, icon: Icon, selected: bool) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::click());
    let pressed = response.is_pointer_button_down_on() && !selected;
    let t = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() && !selected,
        0.12,
    );
    if ui.is_rect_visible(rect) {
        let radius: egui::CornerRadius = theme::RADIUS_CONTROL.into();
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, radius, theme::ACCENT);
        } else if pressed {
            painter.rect_filled(rect, radius, theme::NAV_PRESSED);
        } else if t > 0.0 {
            painter.rect_filled(rect, radius, theme::NAV_HOVER.gamma_multiply(t));
        }
        let text_color = if selected {
            theme::INK
        } else if pressed {
            Color32::WHITE
        } else {
            theme::NAV_MUTED.lerp_to_gamma(Color32::WHITE, t)
        };
        crate::icons::paint(
            painter,
            egui::Rect::from_center_size(
                egui::pos2(rect.left() + 22.0, rect.center().y),
                Vec2::splat(16.0),
            ),
            icon,
            text_color,
            1.6,
        );
        let family = if selected {
            theme::strong_family()
        } else {
            theme::medium_family()
        };
        let galley =
            painter.layout_no_wrap(label.to_owned(), FontId::new(14.0, family), text_color);
        painter.galley(
            egui::pos2(rect.left() + 42.0, rect.center().y - galley.size().y / 2.0),
            galley,
            text_color,
        );
    }
    if response.has_focus() {
        if selected {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                theme::RADIUS_CONTROL,
                Stroke::new(2.0, theme::INK),
                egui::StrokeKind::Inside,
            );
        } else {
            focus_ring(ui, rect, theme::NAV_MUTED);
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label.to_owned())
    });
    response
}

/// Keeps the tail of a value and elides the middle for fixed-width cells.
#[must_use]
pub fn truncate_middle(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars || max_chars == 0 {
        return text.to_owned();
    }
    let mut out = String::from("…");
    out.extend(text.chars().skip(count - (max_chars - 1)));
    out
}
