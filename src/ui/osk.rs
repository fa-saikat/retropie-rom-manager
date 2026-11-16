//! The on-screen keyboard, for searching with a gamepad in fullscreen (#56).
//!
//! Enter on the search field opens it instead of the text cursor. The arrow
//! keys move between keys, Enter presses one, Backspace erases, Space types a
//! space and Esc closes it, so a pad drives it like everything else: A types
//! and B closes. Other buttons are shortcuts, marked on their keys: X
//! deletes and Y types a space (the pad sends Backspace and Space for them
//! here), RT is Shift and Start is Done (sent as `Shift` and `Done`; see
//! `gamepad.rs`). A real keyboard can still type straight in.
//!
//! The layout is a staggered QWERTY with a digits row, Shift and Delete
//! around the bottom letters, and `?123` for a page of the symbols game
//! names use.

use gpui_kit::component::{h_flex, v_flex, ActiveTheme, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::root::{Page, RootView};
use super::scale::px;

gpui_kit::actions!(osk, [Up, Down, Left, Right, Press, Erase, Space, Close, Shift, Done]);

/// Active while the keyboard has focus.
pub const KEY_CONTEXT: &str = "OnScreenKeyboard";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    /// Capitalise the next letter.
    Shift,
    /// Switch between letters and symbols.
    Symbols,
    Space,
    Erase,
    Done,
}

/// Rows are this many half-key units wide, so the staggered rows can sit
/// half a key in.
const UNITS: usize = 20;

/// Each row's characters, on the letter page and the symbol page, and the
/// unit its first character starts at. Every character key is 2 units.
const CHAR_ROWS: [(&str, &str, usize); 4] = [
    ("1234567890", "1234567890", 0),
    ("qwertyuiop", "-/:;()&@\"'", 0),
    ("asdfghjkl", "_,?!+=#%*", 1),
    ("zxcvbnm", "~[]<>$^", 3),
];

const ROWS: usize = CHAR_ROWS.len() + 1;

/// The keys of row `row`, each with its first unit and width in units.
fn row_keys(row: usize, symbols: bool) -> Vec<(Key, usize, usize)> {
    if let Some(&(letters, others, start)) = CHAR_ROWS.get(row) {
        let chars = if symbols { others } else { letters };
        let mut keys: Vec<_> = chars.chars().enumerate().map(|(i, c)| (Key::Char(c), start + 2 * i, 2)).collect();
        // The bottom letter row has Shift before it and Delete after.
        if start == 3 {
            keys.insert(0, (Key::Shift, 0, 3));
            keys.push((Key::Erase, 17, 3));
        }
        return keys;
    }
    vec![(Key::Symbols, 0, 3), (Key::Space, 3, 11), (Key::Char('.'), 14, 2), (Key::Done, 16, 4)]
}

/// Where the keyboard's highlight is, and its Shift and symbol state.
/// `unit` is the middle of the highlighted key, so moving between rows of
/// different widths keeps to the same column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Osk {
    row: usize,
    unit: usize,
    shift: bool,
    symbols: bool,
}

impl Default for Osk {
    /// On "q", as on a phone.
    fn default() -> Self {
        Self { row: 1, unit: 1, shift: false, symbols: false }
    }
}

impl Osk {
    pub fn key(&self) -> Key {
        row_keys(self.row, self.symbols)[self.index()].0
    }

    /// The highlighted key's place in its row: the one under `unit`, or the
    /// nearest where the row is set in.
    fn index(&self) -> usize {
        let keys = row_keys(self.row, self.symbols);
        let distance = |&(_, start, width): &(Key, usize, usize)| {
            if self.unit < start {
                start - self.unit
            } else {
                self.unit.saturating_sub(start + width - 1)
            }
        };
        (0..keys.len()).min_by_key(|&i| distance(&keys[i])).expect("rows aren't empty")
    }

    /// Up and down stop at the edges; left and right wrap around the row.
    pub fn step(&mut self, dx: i32, dy: i32) {
        if dy != 0 {
            // Go from the key's middle, so a wide key leads to the key
            // under its middle.
            self.unit = self.middle(self.index());
            self.row = if dy < 0 { self.row.saturating_sub(1) } else { (self.row + 1).min(ROWS - 1) };
        }
        if dx != 0 {
            let len = row_keys(self.row, self.symbols).len();
            let i = (self.index() + if dx < 0 { len - 1 } else { 1 }) % len;
            self.unit = self.middle(i);
        }
    }

    fn middle(&self, index: usize) -> usize {
        let (_, start, width) = row_keys(self.row, self.symbols)[index];
        start + (width - 1) / 2
    }
}

impl RootView {
    /// Open the keyboard on the current page's search field.
    pub(super) fn open_osk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.osk = Some(Osk::default());
        window.focus(&self.osk_focus, cx);
        cx.notify();
    }

    /// Close the keyboard, leaving the ring on the search field.
    fn close_osk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.osk = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn osk_text(&self) -> &str {
        match self.page {
            Page::System => &self.query,
            Page::Emulators => &self.emu_query,
        }
    }

    fn osk_edit(&mut self, edit: impl FnOnce(&mut String), window: &mut Window, cx: &mut Context<Self>) {
        let mut text = self.osk_text().to_string();
        edit(&mut text);
        match self.page {
            Page::System => self.query = text.clone(),
            Page::Emulators => self.emu_query = text.clone(),
        }
        self.active_search().update(cx, |s, cx| s.set_value(text, window, cx));
        cx.notify();
    }

    fn osk_press_key(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        let Some(osk) = &mut self.osk else { return };
        match key {
            Key::Char(c) => {
                let c = if osk.shift { c.to_ascii_uppercase() } else { c };
                osk.shift = false;
                self.osk_edit(|t| t.push(c), window, cx);
            }
            Key::Shift => {
                osk.shift = !osk.shift;
                cx.notify();
            }
            Key::Symbols => {
                osk.symbols = !osk.symbols;
                cx.notify();
            }
            Key::Space => self.osk_edit(|t| t.push(' '), window, cx),
            Key::Erase => self.osk_edit(|t| _ = t.pop(), window, cx),
            Key::Done => {
                self.close_osk(window, cx);
                self.leave_search(window, cx);
            }
        }
    }

    fn osk_move(&mut self, dx: i32, dy: i32, cx: &mut Context<Self>) {
        if let Some(osk) = &mut self.osk {
            osk.step(dx, dy);
            cx.notify();
        }
    }

    pub(super) fn render_osk(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let osk = self.osk?;
        // A click elsewhere (say into the search field) took focus: close.
        if !self.osk_focus.is_focused(window) {
            self.osk = None;
            return None;
        }
        let t = cx.theme();
        // One unit is half a key.
        let (unit_w, gap) = (34., 8.);
        let (title, placeholder) = match self.page {
            Page::System => ("Search games", "Game name"),
            Page::Emulators => ("Search systems", "System or maker"),
        };
        let text = self.osk_text();
        let shown = if text.is_empty() {
            div().text_color(t.muted_foreground).child(placeholder)
        } else {
            div().child(format!("{text}▏"))
        };
        let focused_ix = osk.index();

        let rows = (0..ROWS).map(|row| {
            let keys = row_keys(row, osk.symbols);
            let indent = keys[0].1;
            h_flex().pl(px(unit_w * indent as f32)).gap(px(gap)).children(keys.into_iter().enumerate().map(
                |(i, (key, start, width))| {
                    let focused = osk.row == row && i == focused_ix;
                    let label: SharedString = match key {
                        Key::Char(c) if osk.shift && c.is_ascii_alphabetic() => c.to_ascii_uppercase().to_string().into(),
                        Key::Char(c) => c.to_string().into(),
                        Key::Shift => "Shift".into(),
                        Key::Symbols if osk.symbols => "abc".into(),
                        Key::Symbols => "?123".into(),
                        Key::Space => "Space".into(),
                        Key::Erase => "Delete".into(),
                        Key::Done => "Done".into(),
                    };
                    let word = !matches!(key, Key::Char(_));
                    let lit = key == Key::Shift && osk.shift;
                    // The pad button that's a shortcut for the key.
                    let shortcut = match key {
                        Key::Shift => Some("RT"),
                        Key::Erase => Some("X"),
                        Key::Space => Some("Y"),
                        Key::Done => Some("Start"),
                        _ => None,
                    };
                    div()
                        .id(("osk-key", row * UNITS + start))
                        .relative()
                        // Each unit's share of the gaps, less the key's own.
                        .w(px(unit_w * width as f32 - gap))
                        .h(px(54.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(t.radius)
                        .map(|k| if word { k.text_lg() } else { k.text_2xl() })
                        .cursor_pointer()
                        .map(|k| {
                            if focused {
                                k.bg(t.foreground).text_color(t.background).font_semibold()
                            } else if lit {
                                k.bg(t.primary).text_color(t.primary_foreground)
                            } else {
                                k.bg(t.secondary).text_color(t.secondary_foreground)
                            }
                        })
                        .child(label)
                        .when_some(shortcut, |k, button| {
                            k.child(
                                div()
                                    .absolute()
                                    .top(px(5.))
                                    .left(px(5.))
                                    .min_w(px(20.))
                                    .h(px(20.))
                                    .px(px(5.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(10.))
                                    .bg(t.foreground)
                                    .text_color(t.background)
                                    .text_xs()
                                    .font_semibold()
                                    .child(button),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(osk) = &mut this.osk {
                                osk.row = row;
                                osk.unit = start;
                            }
                            this.osk_press_key(key, window, cx);
                        }))
                },
            ))
        });

        let hint = |button: &'static str, what: &'static str| {
            h_flex()
                .gap_2()
                .child(
                    div()
                        .min_w(px(28.))
                        .px_1p5()
                        .flex()
                        .justify_center()
                        .rounded(px(4.))
                        .bg(t.foreground)
                        .text_color(t.background)
                        .font_semibold()
                        .child(button),
                )
                .child(what)
        };

        let panel = v_flex()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.osk_focus)
            .on_action(cx.listener(|this, _: &Up, _, cx| this.osk_move(0, -1, cx)))
            .on_action(cx.listener(|this, _: &Down, _, cx| this.osk_move(0, 1, cx)))
            .on_action(cx.listener(|this, _: &Left, _, cx| this.osk_move(-1, 0, cx)))
            .on_action(cx.listener(|this, _: &Right, _, cx| this.osk_move(1, 0, cx)))
            .on_action(cx.listener(|this, _: &Press, window, cx| {
                if let Some(osk) = this.osk {
                    this.osk_press_key(osk.key(), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Erase, window, cx| this.osk_press_key(Key::Erase, window, cx)))
            .on_action(cx.listener(|this, _: &Space, window, cx| this.osk_press_key(Key::Space, window, cx)))
            .on_action(cx.listener(|this, _: &Close, window, cx| this.close_osk(window, cx)))
            .on_action(cx.listener(|this, _: &Shift, window, cx| this.osk_press_key(Key::Shift, window, cx)))
            .on_action(cx.listener(|this, _: &Done, window, cx| this.osk_press_key(Key::Done, window, cx)))
            // A real keyboard types straight in.
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let m = &event.keystroke.modifiers;
                if m.control || m.platform || m.alt {
                    return;
                }
                if let Some(c) = event.keystroke.key_char.as_deref().and_then(single_char) {
                    this.osk_edit(|t| t.push(c), window, cx);
                    cx.stop_propagation();
                }
            }))
            .p(px(24.))
            .gap(px(gap))
            .rounded(t.radius_lg)
            .border_1()
            .border_color(t.border)
            .bg(t.popover)
            .shadow_lg()
            .child(div().text_xl().child(title))
            .child(
                div()
                    .h(px(54.))
                    .px(px(20.))
                    .my(px(10.))
                    .flex()
                    .items_center()
                    .rounded(t.radius)
                    .border_1()
                    .border_color(t.muted_foreground)
                    .bg(t.background)
                    .text_2xl()
                    .child(shown),
            )
            .children(rows)
            .child(
                h_flex()
                    .mt(px(16.))
                    .gap(px(32.))
                    .justify_center()
                    .text_lg()
                    .text_color(t.muted_foreground)
                    .child(hint("A", "Type"))
                    .child(hint("B", "Close")),
            );

        // At the bottom, so the results above stay in sight.
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(24.))
                .flex()
                .justify_center()
                .child(panel)
                .into_any_element(),
        )
    }
}

/// The character, if `text` is exactly one printable one.
fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_control() => Some(c),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings in GPUI's own `test` macro.
    use super::{row_keys, single_char, Key, Osk, ROWS, UNITS};

    fn on(key: Key) -> impl Fn(&Osk) -> bool {
        move |osk| osk.key() == key
    }

    #[test]
    fn rows_fit_the_width_without_overlapping() {
        for symbols in [false, true] {
            for row in 0..ROWS {
                let keys = row_keys(row, symbols);
                for pair in keys.windows(2) {
                    assert_eq!(pair[0].1 + pair[0].2, pair[1].1, "row {row}");
                }
                let (_, start, width) = *keys.last().unwrap();
                assert!(start + width <= UNITS, "row {row}");
            }
        }
    }

    #[test]
    fn starts_on_q_and_moves_like_a_staggered_keyboard() {
        let mut osk = Osk::default();
        assert!(on(Key::Char('q'))(&osk));
        osk.step(0, 1);
        assert!(on(Key::Char('a'))(&osk));
        // "a" sits over Shift, half a key in.
        osk.step(0, 1);
        assert!(on(Key::Shift)(&osk));
        osk.step(1, 0);
        assert!(on(Key::Char('z'))(&osk));
        osk.step(0, 1);
        assert!(on(Key::Space)(&osk));
        osk.step(1, 0);
        assert!(on(Key::Char('.'))(&osk));
        osk.step(-1, 0);
        // Up from Space's middle.
        osk.step(0, -1);
        assert!(on(Key::Char('c'))(&osk));
    }

    #[test]
    fn left_and_right_wrap() {
        let mut osk = Osk::default();
        osk.step(-1, 0);
        assert!(on(Key::Char('p'))(&osk));
        osk.step(1, 0);
        assert!(on(Key::Char('q'))(&osk));
        osk.step(0, 1);
        osk.step(0, 1);
        osk.step(-1, 0);
        assert!(on(Key::Erase)(&osk));
        osk.step(1, 0);
        assert!(on(Key::Shift)(&osk));
    }

    #[test]
    fn wide_keys_lead_to_the_key_under_their_middle() {
        let mut osk = Osk::default();
        osk.step(0, 1);
        osk.step(0, 1);
        osk.step(-1, 0);
        assert!(on(Key::Erase)(&osk));
        osk.step(0, 1);
        assert!(on(Key::Done)(&osk));
        osk.step(0, -1);
        assert!(on(Key::Erase)(&osk));
        osk.step(0, -1);
        assert!(on(Key::Char('l'))(&osk));
    }

    #[test]
    fn stops_at_the_top_and_bottom() {
        let mut osk = Osk::default();
        osk.step(0, -1);
        osk.step(0, -1);
        assert!(on(Key::Char('1'))(&osk));
        for _ in 0..6 {
            osk.step(0, 1);
        }
        assert!(on(Key::Symbols)(&osk));
    }

    #[test]
    fn the_symbol_page_has_the_same_shape() {
        let osk = Osk { symbols: true, ..Osk::default() };
        assert!(on(Key::Char('-'))(&osk));
    }

    #[test]
    fn only_single_printable_characters_type() {
        assert_eq!(single_char("a"), Some('a'));
        assert_eq!(single_char(" "), Some(' '));
        assert_eq!(single_char("\n"), None);
        assert_eq!(single_char("ab"), None);
        assert_eq!(single_char(""), None);
    }
}
