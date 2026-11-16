//! Gamepads, read directly with [gilrs](https://gitlab.com/gilrs-project/gilrs).
//!
//! Buttons go by position, as on consoles, whatever is printed on them;
//! gilrs maps common pads through SDL's controller database (and honours
//! `SDL_GAMECONTROLLERCONFIG`):
//!
//! | Pad                 | Key          |
//! |---------------------|--------------|
//! | D-pad, left stick   | Arrow keys   |
//! | South (A / ✕)       | Enter        |
//! | East (B / ○)        | Escape       |
//! | West (X / □)        | X (Delete)   |
//! | North (Y / △)       | S (Scrape)   |
//!
//! as Big Picture's button hints show. The pad presses and releases those
//! keys in the window, so every screen keeps the key bindings it already
//! has and works the same with either. Directions repeat while held.
//!
//! On the on-screen keyboard X and Y send Backspace and Space instead, and
//! RT and Start work its Shift and Done keys; they do nothing anywhere else.
//! The guide button, Select and the other shoulder buttons do nothing here:
//! a shell running the app may use them, and while AntiMicroX is in use it
//! maps the shoulders to keys of its own.
//!
//! A thread waits on gilrs and sends inputs to the UI thread, which hands
//! them on only while this app's window is the active one. Pads are read
//! without grabbing them, so while a game is in front the pad is the game's.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, Gilrs};
use gpui_kit::{App, KeyDownEvent, KeyUpEvent, Keystroke, PlatformInput, Window};

use crate::ui::{nav, osk};

/// Hold a direction this long before it starts repeating...
const REPEAT_DELAY: Duration = Duration::from_millis(350);
/// ...then repeat this often.
const REPEAT_INTERVAL: Duration = Duration::from_millis(110);
/// The left stick counts as pushed past this, and as let go only once both
/// axes are back under `STICK_RELEASE`, so it doesn't flicker at the edge.
const STICK_PRESS: f32 = 0.6;
const STICK_RELEASE: f32 = 0.35;
/// A pad button and a key giving the same input this close together are
/// one press seen twice: AntiMicroX turns the button into the key.
const ECHO_WINDOW: Duration = Duration::from_millis(100);

/// One input from the pad.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PadInput {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Back,
    Delete,
    Scrape,
    /// RT: Shift on the on-screen keyboard.
    Shift,
    /// Start: Done on the on-screen keyboard.
    Done,
}

impl PadInput {
    /// The key it stands for, if any.
    fn key(self) -> Option<&'static str> {
        Some(match self {
            PadInput::Up => "up",
            PadInput::Down => "down",
            PadInput::Left => "left",
            PadInput::Right => "right",
            PadInput::Accept => "enter",
            PadInput::Back => "escape",
            PadInput::Delete => "x",
            PadInput::Scrape => "s",
            PadInput::Shift | PadInput::Done => return None,
        })
    }

    /// The input a key (GPUI's name for it, no modifiers) is a copy of.
    /// Gamepad profiles send Escape or BackSpace for Back.
    fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "up" => PadInput::Up,
            "down" => PadInput::Down,
            "left" => PadInput::Left,
            "right" => PadInput::Right,
            "enter" => PadInput::Accept,
            "escape" | "backspace" => PadInput::Back,
            "x" => PadInput::Delete,
            "s" => PadInput::Scrape,
            _ => return None,
        })
    }

    fn repeats(self) -> bool {
        matches!(self, PadInput::Up | PadInput::Down | PadInput::Left | PadInput::Right)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Msg {
    Press(PadInput),
    Repeat(PadInput),
    Release(PadInput),
}

/// Start reading gamepads. Call once, after the key bindings are set up.
pub fn start(cx: &mut App) {
    let (tx, rx) = async_channel::unbounded::<Msg>();
    if let Err(err) = std::thread::Builder::new().name("gamepad".into()).spawn(move || read_pads(tx)) {
        log::warn!("gamepad: no reader thread: {err}");
        return;
    }

    let echoes = Rc::new(RefCell::new(Echoes::default()));
    // Set while the pad's own key events are in flight, so they aren't
    // taken for AntiMicroX's copies of themselves.
    let from_pad = Rc::new(Cell::new(false));

    // Remove with AntiMicroX (shopno-shell #55): drop the keys it makes
    // from a button the pad already gave us, before they reach any action.
    {
        let (echoes, from_pad) = (echoes.clone(), from_pad.clone());
        cx.intercept_keystrokes(move |event, _, cx| {
            let k = &event.keystroke;
            if from_pad.get() || k.modifiers.modified() {
                return;
            }
            if let Some(input) = PadInput::from_key(&k.key) {
                if !echoes.borrow_mut().key(input, Instant::now()) {
                    cx.stop_propagation();
                }
            }
        })
        .detach();
    }

    cx.spawn(async move |cx| {
        while let Ok(msg) = rx.recv().await {
            if !echoes.borrow_mut().pad(msg, Instant::now()) {
                continue;
            }
            let from_pad = from_pad.clone();
            cx.update(|cx| {
                let Some(window) = cx.active_window() else { return };
                let _ = window.update(cx, |_, window, cx| {
                    // The pad is the game's (or another app's) while it's in front.
                    if window.is_window_active() {
                        from_pad.set(true);
                        send(msg, window, cx);
                        from_pad.set(false);
                    }
                });
            });
        }
    })
    .detach();
}

/// Press, hold or release the input's key in the window.
fn send(msg: Msg, window: &mut Window, cx: &mut App) {
    // RT and Start have no key: they only work the on-screen keyboard.
    if let Msg::Press(input @ (PadInput::Shift | PadInput::Done)) = msg {
        if window.context_stack().iter().any(|c| c.contains(osk::KEY_CONTEXT)) {
            let action: Box<dyn gpui_kit::Action> =
                if input == PadInput::Shift { Box::new(osk::Shift) } else { Box::new(osk::Done) };
            window.dispatch_action(action, cx);
        }
        return;
    }
    let (Msg::Press(input) | Msg::Repeat(input) | Msg::Release(input)) = msg;
    if input.key().is_none() {
        return;
    }
    let (input, event) = match msg {
        Msg::Press(input) | Msg::Repeat(input) => {
            let held = matches!(msg, Msg::Repeat(_));
            (input, PlatformInput::KeyDown(KeyDownEvent { keystroke: keystroke(input), is_held: held, prefer_character_input: false }))
        }
        Msg::Release(input) => (input, PlatformInput::KeyUp(KeyUpEvent { keystroke: keystroke(input) })),
    };
    if matches!(input, PadInput::Delete | PadInput::Scrape) {
        let in_context = |name: &str| window.context_stack().iter().any(|c| c.contains(name));
        // On the on-screen keyboard X erases and Y types a space.
        if in_context(osk::KEY_CONTEXT) {
            let key = if input == PadInput::Delete { "backspace" } else { "space" };
            let keystroke = Keystroke::parse(key).expect("a plain key name");
            let event = match event {
                PlatformInput::KeyDown(down) => PlatformInput::KeyDown(KeyDownEvent { keystroke, ..down }),
                _ => PlatformInput::KeyUp(KeyUpEvent { keystroke }),
            };
            window.dispatch_event(event, cx);
            return;
        }
        // Elsewhere X and S would type into the search field: send them
        // only where they're bound.
        if !in_context(nav::KEY_CONTEXT) && !in_context(nav::SHEET_CONTEXT) {
            return;
        }
    }
    window.dispatch_event(event, cx);
}

fn keystroke(input: PadInput) -> Keystroke {
    Keystroke::parse(input.key().expect("only inputs with keys get here")).expect("a plain key name")
}

fn read_pads(tx: async_channel::Sender<Msg>) {
    let mut gilrs = match Gilrs::new() {
        Ok(gilrs) => gilrs,
        Err(err) => {
            log::warn!("gamepad: can't read gamepads: {err}");
            return;
        }
    };
    if gilrs.gamepads().next().is_none() {
        log::info!("gamepad: none connected yet");
    }
    for (_, pad) in gilrs.gamepads() {
        log::info!("gamepad: {} ({:?} mapping)", pad.name(), pad.mapping_source());
    }

    let mut pad = PadState::default();
    let mut out = Vec::new();
    loop {
        let now = Instant::now();
        pad.tick(now, &mut out);
        let timeout = pad.next_wake().map(|at| at.saturating_duration_since(now));
        if let Some(event) = gilrs.next_event_blocking(timeout) {
            let now = Instant::now();
            match event.event {
                EventType::Connected => {
                    let p = gilrs.gamepad(event.id);
                    log::info!("gamepad connected: {} ({:?} mapping)", p.name(), p.mapping_source());
                }
                EventType::Disconnected => {
                    log::info!("gamepad disconnected: {}", gilrs.gamepad(event.id).name());
                    pad.release_all(&mut out);
                }
                EventType::ButtonPressed(button, _) => pad.button(button, true, now, &mut out),
                EventType::ButtonReleased(button, _) => pad.button(button, false, now, &mut out),
                EventType::AxisChanged(axis, value, _) => pad.axis(axis, value, now, &mut out),
                _ => {}
            }
        }
        for msg in out.drain(..) {
            if tx.send_blocking(msg).is_err() {
                return;
            }
        }
    }
}

/// What the pads are doing, turned into presses, repeats and releases.
#[derive(Default)]
pub(crate) struct PadState {
    /// Inputs held down by buttons.
    buttons: HashSet<PadInput>,
    /// Where the left stick is pushed, if anywhere.
    stick: Option<PadInput>,
    stick_x: f32,
    stick_y: f32,
    /// The held direction that repeats, and when it next does.
    repeat: Option<(PadInput, Instant)>,
}

impl PadState {
    pub(crate) fn button(&mut self, button: Button, pressed: bool, now: Instant, out: &mut Vec<Msg>) {
        let input = match button {
            Button::DPadUp => PadInput::Up,
            Button::DPadDown => PadInput::Down,
            Button::DPadLeft => PadInput::Left,
            Button::DPadRight => PadInput::Right,
            Button::South => PadInput::Accept,
            Button::East => PadInput::Back,
            Button::West => PadInput::Delete,
            Button::North => PadInput::Scrape,
            Button::RightTrigger2 => PadInput::Shift,
            Button::Start => PadInput::Done,
            // Guide, Select, the shoulders and LT.
            _ => return,
        };
        if pressed {
            if self.buttons.insert(input) {
                self.press(input, now, out);
            }
        } else if self.buttons.remove(&input) {
            self.release(input, out);
        }
    }

    pub(crate) fn axis(&mut self, axis: Axis, value: f32, now: Instant, out: &mut Vec<Msg>) {
        match axis {
            Axis::LeftStickX => self.stick_x = value,
            Axis::LeftStickY => self.stick_y = value,
            _ => return,
        }
        let (x, y) = (self.stick_x, self.stick_y);
        let push = x.abs().max(y.abs());
        let wanted = if push >= STICK_PRESS {
            // The further-pushed axis wins. gilrs has up as positive Y.
            Some(match (x.abs() >= y.abs(), x > 0., y > 0.) {
                (true, true, _) => PadInput::Right,
                (true, false, _) => PadInput::Left,
                (false, _, true) => PadInput::Up,
                (false, _, false) => PadInput::Down,
            })
        } else if push < STICK_RELEASE {
            None
        } else {
            self.stick
        };
        if wanted != self.stick {
            if let Some(old) = self.stick.take() {
                self.release(old, out);
            }
            if let Some(new) = wanted {
                self.stick = Some(new);
                self.press(new, now, out);
            }
        }
    }

    /// A pad went away: let go of everything it held.
    pub(crate) fn release_all(&mut self, out: &mut Vec<Msg>) {
        for input in std::mem::take(&mut self.buttons) {
            self.release(input, out);
        }
        if let Some(input) = self.stick.take() {
            self.release(input, out);
        }
        self.stick_x = 0.;
        self.stick_y = 0.;
    }

    fn press(&mut self, input: PadInput, now: Instant, out: &mut Vec<Msg>) {
        out.push(Msg::Press(input));
        if input.repeats() {
            self.repeat = Some((input, now + REPEAT_DELAY));
        }
    }

    fn release(&mut self, input: PadInput, out: &mut Vec<Msg>) {
        out.push(Msg::Release(input));
        if self.repeat.is_some_and(|(held, _)| held == input) {
            self.repeat = None;
        }
    }

    /// When [`tick`](Self::tick) next has something to do.
    pub(crate) fn next_wake(&self) -> Option<Instant> {
        self.repeat.map(|(_, at)| at)
    }

    pub(crate) fn tick(&mut self, now: Instant, out: &mut Vec<Msg>) {
        if let Some((input, at)) = self.repeat {
            if now >= at {
                out.push(Msg::Repeat(input));
                self.repeat = Some((input, now + REPEAT_INTERVAL));
            }
        }
    }
}

/// Remove with AntiMicroX (shopno-shell #55). While it also turns pad
/// buttons into keys, one press arrives twice; whichever copy comes first
/// is kept.
#[derive(Default)]
pub(crate) struct Echoes {
    /// Inputs the pad holds and whose press was sent on.
    pad_held: HashSet<PadInput>,
    last_pad: HashMap<PadInput, Instant>,
    last_key: HashMap<PadInput, Instant>,
}

impl Echoes {
    /// Whether a pad input should be sent on.
    pub(crate) fn pad(&mut self, msg: Msg, now: Instant) -> bool {
        match msg {
            Msg::Press(input) => {
                self.last_pad.insert(input, now);
                let first = !recent(self.last_key.get(&input), now);
                if first {
                    self.pad_held.insert(input);
                }
                first
            }
            Msg::Repeat(input) => self.pad_held.contains(&input),
            // Released only if its press went through.
            Msg::Release(input) => {
                self.last_pad.insert(input, now);
                self.pad_held.remove(&input)
            }
        }
    }

    /// Whether a key should reach the actions. While the pad holds the same
    /// input, the key (and X's auto-repeat of it) is the pad's echo.
    pub(crate) fn key(&mut self, input: PadInput, now: Instant) -> bool {
        if self.pad_held.contains(&input) || recent(self.last_pad.get(&input), now) {
            return false;
        }
        self.last_key.insert(input, now);
        true
    }
}

fn recent(at: Option<&Instant>, now: Instant) -> bool {
    at.is_some_and(|at| now.saturating_duration_since(*at) < ECHO_WINDOW)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn setup() -> (PadState, Vec<Msg>, Instant) {
        (PadState::default(), Vec::new(), Instant::now())
    }

    #[test]
    fn press_and_release() {
        let (mut pad, mut out, t) = setup();
        pad.button(Button::South, true, t, &mut out);
        pad.button(Button::South, false, t, &mut out);
        assert_eq!(out, [Msg::Press(PadInput::Accept), Msg::Release(PadInput::Accept)]);
        assert_eq!(pad.next_wake(), None, "Accept doesn't repeat");
    }

    #[test]
    fn face_buttons_go_by_position() {
        let (mut pad, mut out, t) = setup();
        for button in [Button::South, Button::East, Button::West, Button::North] {
            pad.button(button, true, t, &mut out);
        }
        assert_eq!(
            out,
            [Msg::Press(PadInput::Accept), Msg::Press(PadInput::Back), Msg::Press(PadInput::Delete), Msg::Press(PadInput::Scrape)]
        );
        assert_eq!(pad.next_wake(), None, "only directions repeat");
    }

    #[test]
    fn held_direction_repeats_after_a_delay() {
        let (mut pad, mut out, t) = setup();
        pad.button(Button::DPadRight, true, t, &mut out);
        pad.tick(t + ms(349), &mut out);
        assert_eq!(out, [Msg::Press(PadInput::Right)]);
        pad.tick(t + REPEAT_DELAY, &mut out);
        assert_eq!(pad.next_wake(), Some(t + REPEAT_DELAY + REPEAT_INTERVAL));
        pad.tick(t + REPEAT_DELAY + ms(109), &mut out);
        pad.tick(t + REPEAT_DELAY + REPEAT_INTERVAL, &mut out);
        assert_eq!(out[1..], [Msg::Repeat(PadInput::Right), Msg::Repeat(PadInput::Right)]);
        pad.button(Button::DPadRight, false, t + ms(900), &mut out);
        assert_eq!(out.last(), Some(&Msg::Release(PadInput::Right)));
        assert_eq!(pad.next_wake(), None);
    }

    #[test]
    fn stick_has_hysteresis() {
        let (mut pad, mut out, t) = setup();
        pad.axis(Axis::LeftStickX, 0.55, t, &mut out);
        assert!(out.is_empty(), "under the press threshold");
        pad.axis(Axis::LeftStickX, 0.7, t, &mut out);
        pad.axis(Axis::LeftStickX, 0.4, t, &mut out);
        assert_eq!(out, [Msg::Press(PadInput::Right)], "still held between the thresholds");
        pad.axis(Axis::LeftStickX, 0.3, t, &mut out);
        assert_eq!(out[1], Msg::Release(PadInput::Right));
    }

    #[test]
    fn stick_up_is_positive_y_and_switches_cleanly() {
        let (mut pad, mut out, t) = setup();
        pad.axis(Axis::LeftStickY, 0.9, t, &mut out);
        // Rolled straight round to the left without passing the centre.
        pad.axis(Axis::LeftStickX, -0.95, t, &mut out);
        assert_eq!(out, [Msg::Press(PadInput::Up), Msg::Release(PadInput::Up), Msg::Press(PadInput::Left)]);
    }

    #[test]
    fn disconnect_releases_everything() {
        let (mut pad, mut out, t) = setup();
        pad.button(Button::DPadUp, true, t, &mut out);
        pad.button(Button::South, true, t, &mut out);
        pad.axis(Axis::LeftStickX, -0.9, t, &mut out);
        out.clear();
        pad.release_all(&mut out);
        out.sort_by_key(|m| format!("{m:?}"));
        assert_eq!(out, [Msg::Release(PadInput::Accept), Msg::Release(PadInput::Left), Msg::Release(PadInput::Up)]);
        assert_eq!(pad.next_wake(), None);
    }

    #[test]
    fn rt_and_start_are_shift_and_done_and_dont_repeat() {
        let (mut pad, mut out, t) = setup();
        pad.button(Button::RightTrigger2, true, t, &mut out);
        pad.tick(t + ms(1000), &mut out);
        pad.button(Button::RightTrigger2, false, t + ms(1000), &mut out);
        pad.button(Button::Start, true, t + ms(1100), &mut out);
        pad.button(Button::Start, false, t + ms(1200), &mut out);
        assert_eq!(
            out,
            [
                Msg::Press(PadInput::Shift),
                Msg::Release(PadInput::Shift),
                Msg::Press(PadInput::Done),
                Msg::Release(PadInput::Done)
            ]
        );
    }

    #[test]
    fn guide_select_shoulders_and_lt_do_nothing() {
        let (mut pad, mut out, t) = setup();
        for button in [
            Button::Mode,
            Button::Select,
            Button::LeftTrigger,
            Button::RightTrigger,
            Button::LeftTrigger2,
        ] {
            pad.button(button, true, t, &mut out);
            pad.tick(t + ms(5000), &mut out);
            pad.button(button, false, t + ms(5000), &mut out);
        }
        assert!(out.is_empty());
    }

    #[test]
    fn antimicrox_key_after_the_pad_is_dropped() {
        let (mut e, t) = (Echoes::default(), Instant::now());
        assert!(e.pad(Msg::Press(PadInput::Left), t));
        assert!(!e.key(PadInput::Left, t + ms(5)), "echo while held");
        assert!(!e.key(PadInput::Left, t + ms(500)), "X's auto-repeat while held");
        assert!(e.pad(Msg::Repeat(PadInput::Left), t + ms(500)));
        assert!(e.pad(Msg::Release(PadInput::Left), t + ms(600)));
        assert!(!e.key(PadInput::Left, t + ms(650)), "echo just after the release");
        assert!(e.key(PadInput::Left, t + ms(900)), "a real key later");
    }

    #[test]
    fn key_before_the_pad_wins() {
        let (mut e, t) = (Echoes::default(), Instant::now());
        assert!(e.key(PadInput::Accept, t));
        assert!(!e.pad(Msg::Press(PadInput::Accept), t + ms(10)));
        assert!(!e.pad(Msg::Release(PadInput::Accept), t + ms(80)), "its release goes with it");
    }

    #[test]
    fn backspace_is_an_echo_of_back() {
        let (mut e, t) = (Echoes::default(), Instant::now());
        assert!(e.pad(Msg::Press(PadInput::Back), t));
        assert!(!e.key(PadInput::from_key("backspace").unwrap(), t + ms(5)));
        assert!(!e.key(PadInput::from_key("escape").unwrap(), t + ms(5)));
    }

    #[test]
    fn other_inputs_are_independent() {
        let (mut e, t) = (Echoes::default(), Instant::now());
        assert!(e.pad(Msg::Press(PadInput::Left), t));
        assert!(e.key(PadInput::Back, t + ms(5)));
    }
}
