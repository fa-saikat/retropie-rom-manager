//! Motion for the design system: one entrance and one spring, so every
//! transition in the app moves the same way.
//!
//! - [`enter`]: a page, system or game coming into view fades in over
//!   260ms and settles quickly (Material's "emphasized decelerate" feel).
//!   Callers pair it with a small rise or slide, 12-24px.
//! - [`spring`]: things that follow the focus (Big Picture's row and tile
//!   sizes) chase their target on a critically damped spring, so a fast
//!   run of key presses glides instead of queueing up animations.
//!
//! Animations restart when their element id changes, so ids carry what the
//! transition is about: the system, the page or the focused game.

use std::time::Duration;

use gpui_kit::{ease_out_quint, Animation, SpringAnimation, SpringConfig};

/// The entrance: 260ms, fast start, gentle stop.
pub fn enter() -> Animation {
    Animation::new(Duration::from_millis(260)).with_easing(ease_out_quint())
}

/// A critically damped spring: quick, no overshoot.
pub fn spring() -> SpringAnimation {
    // Critical damping is 2 * sqrt(stiffness * mass) = 2 * sqrt(320) ≈ 35.8.
    SpringAnimation::new(SpringConfig::new(320., 36., 1.))
}
