//! App-level theming on top of GPUI Kit's Default Light/Dark themes.
//!
//! The design system follows Material 3's colour roles, generated from the
//! selected system's accent the way Material You derives a scheme from a
//! seed colour: the accent becomes the primary colour, and every neutral
//! (surfaces, borders, muted text) is tinted faintly with its hue. Depth
//! comes from the surface tiers, not shadows. Everything is applied flat:
//! no gradients or glows.
//!
//! The kit's own components read these roles through its theme, so a kit
//! button or tag picks up the scheme without any per-widget work.

use gpui_kit::component::{ActiveTheme, Colorize, Theme, ThemeMode};
use gpui_kit::{hsla, rgb, white, App, Hsla, Window};

/// All interface text.
pub const FONT: &str = "Inter";
/// Detail text: key hints, file names, extensions. Stands in for the kit's
/// monospace slot, which this app only uses for such labels.
pub const DETAIL_FONT: &str = "Poppins";

pub fn accent(color: u32) -> Hsla {
    rgb(color).into()
}

/// Material 3 colour roles for one accent, in one mode.
pub struct Scheme {
    pub primary: Hsla,
    pub on_primary: Hsla,
    /// Selected rows, tonal buttons.
    pub secondary_container: Hsla,
    pub on_secondary_container: Hsla,
    /// Window background.
    pub surface: Hsla,
    /// Dialogs and popovers.
    pub surface_container_lowest: Hsla,
    /// Sidebar, cards.
    pub surface_container_low: Hsla,
    /// Table heads, chips.
    pub surface_container: Hsla,
    /// Search fields, hover.
    pub surface_container_high: Hsla,
    pub surface_container_highest: Hsla,
    pub on_surface: Hsla,
    pub on_surface_variant: Hsla,
    pub outline: Hsla,
    pub outline_variant: Hsla,
}

impl Scheme {
    /// Derive the roles from `accent`, Material-style: the primary keeps the
    /// accent, every neutral takes its hue at low saturation.
    pub fn new(accent: Hsla, dark: bool) -> Self {
        let h = accent.h;
        let s = accent.s;
        // Tinted neutral at a lightness, keeping saturation low so surfaces
        // read as grey with a hint of the accent.
        let neutral = |l: f32, sat: f32| hsla(h, (s * sat).min(0.3), l, 1.);
        if dark {
            Self {
                primary: hsla(h, (s * 0.75).min(0.8), 0.72, 1.),
                on_primary: hsla(h, (s * 0.6).min(0.6), 0.14, 1.),
                secondary_container: hsla(h, (s * 0.35).min(0.35), 0.27, 1.),
                on_secondary_container: hsla(h, (s * 0.2).min(0.2), 0.92, 1.),
                surface: neutral(0.115, 0.12),
                surface_container_lowest: neutral(0.09, 0.12),
                surface_container_low: neutral(0.145, 0.12),
                surface_container: neutral(0.175, 0.12),
                surface_container_high: neutral(0.21, 0.12),
                surface_container_highest: neutral(0.245, 0.12),
                on_surface: neutral(0.93, 0.15),
                on_surface_variant: neutral(0.72, 0.15),
                outline: neutral(0.5, 0.12),
                outline_variant: neutral(0.27, 0.1),
            }
        } else {
            Self {
                primary: accent,
                on_primary: white(),
                secondary_container: hsla(h, (s * 0.5).min(0.5), 0.91, 1.),
                on_secondary_container: hsla(h, (s * 0.5).min(0.5), 0.22, 1.),
                surface: neutral(0.985, 0.25),
                surface_container_lowest: white(),
                surface_container_low: neutral(0.965, 0.25),
                surface_container: neutral(0.945, 0.22),
                surface_container_high: neutral(0.92, 0.2),
                surface_container_highest: neutral(0.895, 0.18),
                on_surface: neutral(0.17, 0.3),
                on_surface_variant: neutral(0.42, 0.2),
                outline: neutral(0.62, 0.15),
                outline_variant: neutral(0.86, 0.15),
            }
        }
    }
}

/// Apply everything this app layers over the kit theme: the UI scale, the
/// fonts and the colour roles for `accent`. Call after anything that
/// reloads the kit theme (startup, a light/dark switch).
pub fn apply(accent_color: u32, cx: &mut App) {
    crate::ui::scale::apply_to_theme(cx);
    Theme::update(cx, |t| {
        t.font_family = FONT.into();
        t.mono_font_family = DETAIL_FONT.into();
    });
    apply_accent(accent_color, cx);
}

/// Re-derive the colour roles from `color` and write them into the kit
/// theme. Cheap enough to run on every system switch.
pub fn apply_accent(color: u32, cx: &mut App) {
    let dark = cx.theme().is_dark();
    let c = Scheme::new(accent(color), dark);
    Theme::update(cx, |t| {
        // Primary
        t.primary = c.primary;
        t.primary_hover = c.primary.opacity(0.9);
        t.primary_active = c.primary.darken(0.1);
        t.primary_foreground = c.on_primary;
        t.button_primary = c.primary;
        t.button_primary_hover = c.primary.opacity(0.9);
        t.button_primary_active = c.primary.darken(0.1);
        t.button_primary_foreground = c.on_primary;
        t.ring = c.primary;
        t.progress_bar = c.primary;
        t.link = c.primary;
        t.caret = c.primary;
        t.selection = c.primary.opacity(0.25);

        // Surfaces
        t.background = c.surface;
        t.foreground = c.on_surface;
        t.popover = c.surface_container_lowest;
        t.popover_foreground = c.on_surface;
        t.sidebar = c.surface_container_low;
        t.sidebar_foreground = c.on_surface;
        t.sidebar_border = c.outline_variant;
        t.secondary = c.surface_container;
        t.secondary_hover = c.surface_container_high;
        t.secondary_active = c.surface_container_highest;
        t.secondary_foreground = c.on_surface;
        t.button_secondary = c.surface_container;
        t.button_secondary_hover = c.surface_container_high;
        t.button_secondary_active = c.surface_container_highest;
        t.button_secondary_foreground = c.on_surface;
        t.muted = c.surface_container_high;
        t.muted_foreground = c.on_surface_variant;
        t.input = c.outline;
        t.colors.list = c.surface;
        t.list_hover = c.surface_container_low;
        t.list_head = c.surface_container;
        t.table = c.surface;
        t.table_hover = c.surface_container_low;
        t.table_head = c.surface_container;
        t.table_head_foreground = c.on_surface_variant;
        t.table_row_border = c.outline_variant;
        t.skeleton = c.surface_container_high;
        t.scrollbar_thumb = c.outline.opacity(0.5);
        t.scrollbar_thumb_hover = c.outline;

        // Outlines
        t.border = c.outline_variant;

        // Selection: the sidebar's active row and the kit's accent (list
        // hover) take the secondary container, so a picked row is tinted
        // with the system's own hue.
        t.sidebar_accent = c.secondary_container;
        t.sidebar_accent_foreground = c.on_secondary_container;
        t.sidebar_primary = c.primary;
        t.sidebar_primary_foreground = c.on_primary;
        t.accent = c.secondary_container;
        t.accent_foreground = c.on_secondary_container;
        t.list_active = c.secondary_container;
        t.list_active_border = c.primary;
        t.table_active = c.secondary_container;
        t.table_active_border = c.primary;
        t.tab_active = c.secondary_container;
        t.tab_active_foreground = c.on_secondary_container;
    });
}

/// Flip light/dark. Changing mode reloads the kit's colours, sizes and
/// fonts, so everything is re-applied afterwards.
pub fn toggle_mode(accent_color: u32, window: &mut Window, cx: &mut App) {
    let mode = if cx.theme().is_dark() { ThemeMode::Light } else { ThemeMode::Dark };
    Theme::change(mode, Some(window), cx);
    apply(accent_color, cx);
}

/// Background for raised surfaces (game cards, the list view): the low
/// surface container, one step up from the window background.
pub fn card_bg(cx: &App) -> Hsla {
    cx.theme().sidebar
}

#[cfg(test)]
mod tests {
    use super::{accent, Scheme};

    /// Text must stay readable on every surface tier, in both modes.
    #[test]
    fn surfaces_keep_text_contrast() {
        for color in [0x6b46c1u32, 0xdc2626, 0x2c9a63, 0xb7791f, 0x4b5563] {
            for dark in [false, true] {
                let s = Scheme::new(accent(color), dark);
                let tiers = [s.surface, s.surface_container_low, s.surface_container, s.surface_container_high, s.surface_container_highest];
                for tier in tiers {
                    let gap = (tier.l - s.on_surface.l).abs();
                    assert!(gap > 0.6, "{color:06x} dark={dark}: on_surface l={} vs tier l={}", s.on_surface.l, tier.l);
                }
                assert!((s.primary.l - s.on_primary.l).abs() > 0.4, "{color:06x} dark={dark}: primary unreadable");
            }
        }
    }

    /// Lighter tiers really are lighter in light mode and darker in dark mode.
    #[test]
    fn surface_tiers_are_ordered() {
        for dark in [false, true] {
            let s = Scheme::new(accent(0x6b46c1), dark);
            let ls = [s.surface_container_lowest.l, s.surface.l, s.surface_container_low.l, s.surface_container.l, s.surface_container_high.l, s.surface_container_highest.l];
            let mut sorted = ls;
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if dark {
                assert_eq!(ls, sorted, "dark tiers should get lighter");
            } else {
                sorted.reverse();
                assert_eq!(ls, sorted, "light tiers should get darker");
            }
        }
    }
}
