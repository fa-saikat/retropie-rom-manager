//! Shape scale for the app's design system.
//!
//! The design system follows Material 3's roles and structure, with one
//! deliberate change: corners stay tight. Material 3 rounds controls into
//! pills and cards up to 28px; here every corner is 3 or 4 pixels.
//!
//! - [`sm`] (3px): controls and small elements. Buttons, inputs, rows,
//!   tags, key caps, thumbnails and small system tiles.
//! - [`md`] (4px): containers. Cards, dialogs, sheets, notifications and
//!   large system tiles.
//!
//! Fully round shapes are only for true circles, such as status dots.
//! Never use them for pills.
//!
//! Both values are 1080p pixels and grow with the UI scale, like every
//! other size from [`super::scale::px`].

use gpui_kit::component::tag::Tag;
use gpui_kit::component::Sizable;
use gpui_kit::Pixels;

use super::scale::px;

/// Controls and small elements, in 1080p pixels.
pub const SM: f32 = 3.;
/// Containers, in 1080p pixels.
pub const MD: f32 = 4.;

/// Corner radius for controls and small elements.
pub fn sm() -> Pixels {
    px(SM)
}

/// Corner radius for containers.
pub fn md() -> Pixels {
    px(MD)
}

/// A small kit tag on the shape scale. The kit rounds small tags at half
/// the control radius, which would fall below 3px.
pub fn small_tag(tag: Tag) -> Tag {
    tag.small().rounded(sm())
}
