//! UI scale for displays bigger than 1080p.
//!
//! GPUI already follows the desktop's own scale setting, but a 4K screen
//! left at 1x reports 3840×2160 logical pixels, and a UI sized for 1080p
//! ends up a quarter of the screen. At startup we pick a factor from the
//! display height, with 1080 as 1x, and apply it to the theme's base sizes
//! (which GPUI Kit's rem-sized components follow) and to every fixed size
//! in our own UI.
//!
//! UI code imports this module's `px` in place of GPUI's, so `px(240.)`
//! means 240 pixels at 1080p and grows with the display.

use std::sync::OnceLock;

use gpui_kit::component::{ActiveTheme, Theme};
use gpui_kit::{size, App, Pixels, Size};

const BASE_HEIGHT: f32 = 1080.;
const MAX_FACTOR: f32 = 3.;

static FACTOR: OnceLock<f32> = OnceLock::new();
static BASE_THEME: OnceLock<ThemeSizes> = OnceLock::new();

/// The theme's sizes before scaling, so scaling can be re-applied after a
/// light/dark switch reloads them.
#[derive(Clone)]
struct ThemeSizes {
    font_size: Pixels,
    mono_font_size: Pixels,
    radius: Pixels,
    radius_lg: Pixels,
    notification_width: Pixels,
    /// Top, right, bottom, left.
    notification_margins: [Pixels; 4],
}

/// Pick the factor for this session. `ROM_MANAGER_SCALE=1.5` overrides it.
pub fn init(cx: &App) -> f32 {
    let requested = std::env::var("ROM_MANAGER_SCALE")
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|f| f.is_finite() && *f > 0.)
        .map(|f| f.clamp(0.5, MAX_FACTOR));
    let display = display_size(cx);
    let factor = requested.unwrap_or_else(|| factor_for(display.map(|d| d.height.as_f32())));
    log::info!("UI scale {factor:.2} (display {display:?})");
    *FACTOR.get_or_init(|| factor)
}

/// 1080 or less is 1x; taller displays scale up in proportion.
fn factor_for(display_height: Option<f32>) -> f32 {
    display_height.map_or(1., |h| (h / BASE_HEIGHT).clamp(1., MAX_FACTOR))
}

pub fn factor() -> f32 {
    FACTOR.get().copied().unwrap_or(1.)
}

/// A size given in 1080p pixels, scaled for this display.
pub fn px(value: f32) -> Pixels {
    gpui_kit::px(value * factor())
}

/// The display the window opens on, in logical pixels. Wayland has no
/// primary display, so take the first one it lists.
pub fn display_size(cx: &App) -> Option<Size<Pixels>> {
    cx.primary_display()
        .or_else(|| cx.displays().into_iter().next())
        .map(|d| d.bounds().size)
}

/// `wanted`, shrunk to fit within `share` of the display.
pub fn fit_display(wanted: Size<Pixels>, share: f32, cx: &App) -> Size<Pixels> {
    match display_size(cx) {
        Some(d) => size(wanted.width.min(d.width * share), wanted.height.min(d.height * share)),
        None => wanted,
    }
}

/// Scale the theme's base sizes. Call again after anything that reloads
/// the theme, such as switching between light and dark.
pub fn apply_to_theme(cx: &mut App) {
    let base = BASE_THEME
        .get_or_init(|| {
            let t = cx.theme();
            ThemeSizes {
                font_size: t.font_size,
                mono_font_size: t.mono_font_size,
                radius: t.radius,
                radius_lg: t.radius_lg,
                notification_width: t.notification.width,
                notification_margins: {
                    let m = &t.notification.margins;
                    [m.top, m.right, m.bottom, m.left]
                },
            }
        })
        .clone();
    let f = factor();
    Theme::update(cx, |t| {
        t.font_size = base.font_size * f;
        t.mono_font_size = base.mono_font_size * f;
        t.radius = base.radius * f;
        t.radius_lg = base.radius_lg * f;
        t.notification.width = base.notification_width * f;
        let [top, right, bottom, left] = base.notification_margins;
        let m = &mut t.notification.margins;
        (m.top, m.right, m.bottom, m.left) = (top * f, right * f, bottom * f, left * f);
    });
}

#[cfg(test)]
mod tests {
    use super::factor_for;

    #[test]
    fn factor_follows_display_height() {
        assert_eq!(factor_for(Some(1080.)), 1.);
        assert_eq!(factor_for(Some(2160.)), 2.);
        assert!((factor_for(Some(1440.)) - 4. / 3.).abs() < 1e-6);
    }

    #[test]
    fn small_or_unknown_displays_stay_at_1x() {
        assert_eq!(factor_for(Some(720.)), 1.);
        assert_eq!(factor_for(None), 1.);
    }

    #[test]
    fn factor_is_capped() {
        assert_eq!(factor_for(Some(8640.)), 3.);
    }
}
