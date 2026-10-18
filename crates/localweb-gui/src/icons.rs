//! Authored geometry icons.
//!
//! The interface never uses Unicode glyphs as icons; every symbol is drawn on a
//! 16 by 16 grid with the egui painter so weight, alignment and color stay under
//! our control at any size.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

/// Icons used across the interface.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Dashboard overview.
    Overview,
    /// Project folder.
    Folder,
    /// Create or add.
    Plus,
    /// Refresh or retry.
    Refresh,
    /// Open external destination.
    External,
    /// Mailpit or mail surfaces.
    Mail,
    /// Remove a project.
    Trash,
    /// Warning state.
    Warning,
    /// Start action.
    Play,
    /// Stop action.
    Stop,
    /// Domains or the gateway.
    Globe,
    /// Dismiss a floating surface.
    Close,
    /// Successful state.
    Check,
    /// Inactive or stopped state.
    Minus,
    /// Expand or collapse.
    ChevronDown,
    /// Navigate back.
    ChevronLeft,
    /// Navigate forward.
    ChevronRight,
    /// Navigate to the parent folder.
    ChevronUp,
    /// Copy a value.
    Copy,
    /// Settings and preferences.
    Settings,
    /// MCP server or command line.
    Terminal,
    /// Search fields.
    Search,
}

/// Paints `icon` centered inside `rect`.
#[expect(
    clippy::too_many_lines,
    reason = "each icon is a short drawing routine in one dispatch table"
)]
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32, width: f32) {
    let k = rect.width().min(rect.height()) / 16.0;
    let origin = rect.center() - Vec2::splat(8.0 * k);
    let at = |x: f32, y: f32| origin + Vec2::new(x * k, y * k);
    let stroke = Stroke::new(width, color);
    match icon {
        Icon::Overview => {
            for (x, y) in [(1.8f32, 1.8f32), (9.2, 1.8), (1.8, 9.2), (9.2, 9.2)] {
                painter.rect_stroke(
                    Rect::from_min_size(at(x, y), Vec2::splat(5.0 * k)),
                    eframe::egui::CornerRadius::ZERO,
                    stroke,
                    eframe::egui::StrokeKind::Inside,
                );
            }
        }
        Icon::Folder => {
            painter.add(Shape::closed_line(
                vec![
                    at(2.0, 12.9),
                    at(2.0, 3.6),
                    at(6.2, 3.6),
                    at(7.6, 5.6),
                    at(14.0, 5.6),
                    at(14.0, 12.9),
                ],
                stroke,
            ));
        }
        Icon::Plus => {
            painter.line_segment([at(8.0, 3.2), at(8.0, 12.8)], stroke);
            painter.line_segment([at(3.2, 8.0), at(12.8, 8.0)], stroke);
        }
        Icon::Refresh => {
            let center = at(8.0, 8.0);
            let radius = 5.3 * k;
            let points: Vec<Pos2> = (0..=26u8)
                .map(|i| {
                    let t = 0.62 + (5.30 - 0.62) * f32::from(i) / 26.0;
                    center + Vec2::new(t.cos() * radius, t.sin() * radius)
                })
                .collect();
            painter.add(Shape::line(points, stroke));
            let tip = center + Vec2::new(0.28f32.cos(), 0.28f32.sin()) * radius;
            let base_outer = center + Vec2::new(0.74f32.cos(), 0.74f32.sin()) * (radius + 2.1 * k);
            let base_inner = center + Vec2::new(0.74f32.cos(), 0.74f32.sin()) * (radius - 2.1 * k);
            painter.add(Shape::convex_polygon(
                vec![tip, base_outer, base_inner],
                color,
                Stroke::NONE,
            ));
        }
        Icon::External => {
            painter.rect_stroke(
                Rect::from_min_size(at(2.4, 5.4), Vec2::new(7.8 * k, 8.2 * k)),
                eframe::egui::CornerRadius::ZERO,
                stroke,
                eframe::egui::StrokeKind::Inside,
            );
            painter.line_segment([at(8.2, 7.8), at(13.7, 2.3)], stroke);
            painter.add(Shape::line(
                vec![at(9.5, 2.3), at(13.7, 2.3), at(13.7, 6.5)],
                stroke,
            ));
        }
        Icon::Mail => {
            painter.rect_stroke(
                Rect::from_min_size(at(1.8, 3.8), Vec2::new(12.4 * k, 8.4 * k)),
                eframe::egui::CornerRadius::ZERO,
                stroke,
                eframe::egui::StrokeKind::Inside,
            );
            painter.add(Shape::line(
                vec![at(1.9, 5.0), at(8.0, 9.4), at(14.1, 5.0)],
                stroke,
            ));
        }
        Icon::Trash => {
            painter.line_segment([at(2.8, 4.4), at(13.2, 4.4)], stroke);
            painter.add(Shape::line(
                vec![at(6.2, 4.4), at(6.2, 2.6), at(9.8, 2.6), at(9.8, 4.4)],
                stroke,
            ));
            painter.rect_stroke(
                Rect::from_min_size(at(3.9, 6.4), Vec2::new(8.2 * k, 7.4 * k)),
                eframe::egui::CornerRadius::ZERO,
                stroke,
                eframe::egui::StrokeKind::Inside,
            );
            painter.line_segment([at(6.6, 8.6), at(6.6, 11.8)], stroke);
            painter.line_segment([at(9.4, 8.6), at(9.4, 11.8)], stroke);
        }
        Icon::Warning => {
            painter.add(Shape::closed_line(
                vec![at(8.0, 2.4), at(14.6, 13.4), at(1.4, 13.4)],
                stroke,
            ));
            painter.line_segment([at(8.0, 6.4), at(8.0, 10.0)], stroke);
            painter.rect_filled(
                Rect::from_center_size(at(8.0, 11.9), Vec2::splat(1.9 * k)),
                0,
                color,
            );
        }
        Icon::Play => {
            painter.add(Shape::convex_polygon(
                vec![at(5.2, 3.0), at(13.0, 8.0), at(5.2, 13.0)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            painter.rect_filled(
                Rect::from_min_size(at(4.6, 4.6), Vec2::splat(6.8 * k)),
                eframe::egui::CornerRadius::ZERO,
                color,
            );
        }
        Icon::Globe => {
            let center = at(8.0, 8.0);
            painter.circle_stroke(center, 5.5 * k, stroke);
            let ellipse: Vec<Pos2> = (0..=32u8)
                .map(|i| {
                    let t = std::f32::consts::TAU * f32::from(i) / 32.0;
                    center + Vec2::new(3.0 * k * t.cos(), 5.5 * k * t.sin())
                })
                .collect();
            painter.add(Shape::closed_line(ellipse, stroke));
            painter.line_segment([at(2.5, 8.0), at(13.5, 8.0)], stroke);
        }
        Icon::Close => {
            painter.line_segment([at(4.8, 4.8), at(11.2, 11.2)], stroke);
            painter.line_segment([at(11.2, 4.8), at(4.8, 11.2)], stroke);
        }
        Icon::Check => {
            painter.add(Shape::line(
                vec![at(3.4, 8.6), at(6.8, 11.7), at(12.7, 4.9)],
                stroke,
            ));
        }
        Icon::Minus => {
            painter.line_segment([at(4.6, 8.0), at(11.4, 8.0)], stroke);
        }
        Icon::ChevronDown => {
            painter.add(Shape::line(
                vec![at(4.6, 6.2), at(8.0, 9.6), at(11.4, 6.2)],
                stroke,
            ));
        }
        Icon::ChevronLeft => {
            painter.add(Shape::line(
                vec![at(9.8, 4.6), at(6.4, 8.0), at(9.8, 11.4)],
                stroke,
            ));
        }
        Icon::ChevronRight => {
            painter.add(Shape::line(
                vec![at(6.2, 4.6), at(9.6, 8.0), at(6.2, 11.4)],
                stroke,
            ));
        }
        Icon::ChevronUp => {
            painter.add(Shape::line(
                vec![at(4.6, 9.8), at(8.0, 6.4), at(11.4, 9.8)],
                stroke,
            ));
        }
        Icon::Copy => {
            painter.rect_stroke(
                Rect::from_min_size(at(6.0, 2.4), Vec2::new(7.6 * k, 7.4 * k)),
                eframe::egui::CornerRadius::ZERO,
                stroke,
                eframe::egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_min_size(at(2.4, 6.2), Vec2::new(7.6 * k, 7.4 * k)),
                eframe::egui::CornerRadius::ZERO,
                stroke,
                eframe::egui::StrokeKind::Inside,
            );
        }
        Icon::Settings => {
            for (y, knob) in [(4.2f32, 6.2f32), (8.0, 10.2), (11.8, 5.4)] {
                painter.line_segment([at(2.4, y), at(13.6, y)], stroke);
                painter.rect_filled(
                    Rect::from_center_size(at(knob, y), Vec2::splat(2.7 * k)),
                    0,
                    color,
                );
            }
        }
        Icon::Terminal => {
            painter.rect_stroke(
                Rect::from_min_size(at(1.8, 2.6), Vec2::new(12.4 * k, 10.8 * k)),
                0,
                stroke,
                eframe::egui::StrokeKind::Inside,
            );
            painter.add(Shape::line(
                vec![at(4.8, 6.2), at(7.0, 8.0), at(4.8, 9.8)],
                stroke,
            ));
            painter.line_segment([at(8.8, 10.2), at(11.4, 10.2)], stroke);
        }
        Icon::Search => {
            let center = at(7.0, 7.0);
            painter.circle_stroke(center, 4.4 * k, stroke);
            painter.line_segment([at(10.3, 10.3), at(13.6, 13.6)], stroke);
        }
    }
}
