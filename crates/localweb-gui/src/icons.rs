//! Interface icons from the Lucide icon font.
//!
//! Every symbol comes from one provider, [Lucide](https://lucide.dev), embedded
//! as a font under the ISC licence. The font maps each icon to a private-use
//! codepoint, so the interface draws real vector outlines at any size instead
//! of hand-authored approximations.

use crate::theme::icon_family;
use eframe::egui::{Align2, Color32, FontId, Painter, Rect};

/// Icons used across the interface.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    /// Install or download.
    Download,
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
    /// Edit a project.
    Pencil,
    /// MCP server or command line.
    Terminal,
    /// Search fields.
    Search,
}

impl Icon {
    /// Every icon in the set, used by the coverage test.
    #[cfg(test)]
    const ALL: [Self; 24] = [
        Self::Overview,
        Self::Folder,
        Self::Plus,
        Self::Refresh,
        Self::External,
        Self::Mail,
        Self::Trash,
        Self::Warning,
        Self::Play,
        Self::Stop,
        Self::Globe,
        Self::Close,
        Self::Check,
        Self::Download,
        Self::Minus,
        Self::ChevronDown,
        Self::ChevronLeft,
        Self::ChevronRight,
        Self::ChevronUp,
        Self::Copy,
        Self::Settings,
        Self::Pencil,
        Self::Terminal,
        Self::Search,
    ];

    /// Private-use codepoint assigned by the pinned Lucide font.
    const fn codepoint(self) -> char {
        match self {
            Self::Overview => '\u{E1C1}',     // layout-dashboard
            Self::Folder => '\u{E0D7}',       // folder
            Self::Plus => '\u{E13D}',         // plus
            Self::Refresh => '\u{E145}',      // refresh-cw
            Self::External => '\u{E0B9}',     // external-link
            Self::Mail => '\u{E10F}',         // mail
            Self::Trash => '\u{E18E}',        // trash-2
            Self::Warning => '\u{E193}',      // triangle-alert
            Self::Play => '\u{E13C}',         // play
            Self::Stop => '\u{E167}',         // square
            Self::Globe => '\u{E0E8}',        // globe
            Self::Close => '\u{E1B2}',        // x
            Self::Check => '\u{E06C}',        // check
            Self::Download => '\u{E0B2}',     // download
            Self::Minus => '\u{E11C}',        // minus
            Self::ChevronDown => '\u{E06D}',  // chevron-down
            Self::ChevronLeft => '\u{E06E}',  // chevron-left
            Self::ChevronRight => '\u{E06F}', // chevron-right
            Self::ChevronUp => '\u{E070}',    // chevron-up
            Self::Copy => '\u{E09E}',         // copy
            Self::Settings => '\u{E154}',     // settings
            Self::Pencil => '\u{E1F9}',       // pencil
            Self::Terminal => '\u{E181}',     // terminal
            Self::Search => '\u{E151}',       // search
        }
    }
}

/// Paints `icon` centered inside `rect`.
///
/// The glyph is scaled to the smaller edge of `rect`; the Lucide outline keeps
/// its native 24-unit grid and two-unit stroke at every size.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height());
    if size <= 0.0 {
        return;
    }
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon.codepoint(),
        FontId::new(size, icon_family()),
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use skrifa::MetadataProvider as _;

    const LUCIDE: &[u8] = include_bytes!("../assets/fonts/Lucide.ttf");

    #[test]
    fn every_icon_has_a_distinct_lucide_glyph() {
        let font = skrifa::FontRef::new(LUCIDE).expect("the embedded Lucide font parses");
        let charmap = font.charmap();
        let mut codepoints = std::collections::BTreeSet::new();
        for icon in Icon::ALL {
            let codepoint = icon.codepoint();
            assert!(
                codepoints.insert(codepoint),
                "{icon:?} reuses U+{:04X}",
                u32::from(codepoint)
            );
            assert!(
                charmap.map(codepoint).is_some(),
                "{icon:?} is missing from the embedded Lucide font"
            );
        }
    }
}
