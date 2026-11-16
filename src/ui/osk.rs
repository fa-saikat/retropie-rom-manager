//! The on-screen keyboard, for searching with a gamepad in fullscreen (#56).
//!
//! Enter on the search field opens it instead of the text cursor. The arrow
//! keys move between keys, Enter presses one, Backspace erases, Space types a
//! space and Esc closes it, so a pad drives it like everything else: A types,
//! B closes, and the pad sends Backspace for X and Space for Y here (see
//! `gamepad.rs`). A real keyboard can still type straight in.
//!
//! Search ignores case, so there's no Shift.

use gpui_kit::component::{h_flex, v_flex, ActiveTheme, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::root::{Page, RootView};
use super::scale::px;

gpui_kit::actions!(osk, [Up, Down, Left, Right, Press, Erase, Space, Close]);

/// Active while the keyboard has focus.
pub const KEY_CONTEXT: &str = "OnScreenKeyboard";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Space,
    Erase,
    Clear,
    Done,
}

/// Every row is this many units wide.
const UNITS: usize = 10;

const CHAR_ROWS: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl-", "zxcvbnm'.&"];

/// The bottom row: each key with its width in units.
const BOTTOM: [(Key, usize); 4] = [(Key::Space, 4), (Key::Erase, 2), (Key::Clear, 2), (Key::Done, 2)];

const ROWS: usize = CHAR_ROWS.len() + 1;

/// The keys of row `row`, each with its first unit and width.
fn row_keys(row: usize) -> Vec<(Key, usize, usize)> {
    match CHAR_ROWS.get(row) {
        Some(chars) => chars.chars().enumerate().map(|(i, c)| (Key::Char(c), i, 1)).collect(),
        None => {
            let mut start = 0;
            BOTTOM
                .iter()
                .map(|&(key, width)| {
                    let key = (key, start, width);
                    start += width;
                    key
                })
                .collect()
        }
    }
}

/// Where the keyboard's highlight is: a row, and the unit it's over, so
/// moving between rows of different widths keeps to the same column.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Osk {
    row: usize,
    unit: usize,
}

impl Osk {
    pub fn key(&self) -> Key {
        self.cell().0
    }

    fn cell(&self) -> (Key, usize, usize) {
        row_keys(self.row)
            .into_iter()
            .find(|(_, start, width)| (*start..start + width).contains(&self.unit))
            .expect("every unit has a key")
    }

    /// Up and down stop at the edges; left and right wrap around the row.
    pub fn step(&mut self, dx: i32, dy: i32) {
        if dy < 0 {
            self.row = self.row.saturating_sub(1);
        } else if dy > 0 {
            self.row = (self.row + 1).min(ROWS - 1);
        }
        if dx != 0 {
            let (_, start, width) = self.cell();
            self.unit = if dx < 0 { (start + UNITS - 1) % UNITS } else { (start + width) % UNITS };
            // Land on the start of a wide key, so Right leaves it next time.
            self.unit = self.cell().1;
        }
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
        match key {
            Key::Char(c) => self.osk_edit(|t| t.push(c), window, cx),
            Key::Space => self.osk_edit(|t| t.push(' '), window, cx),
            Key::Erase => self.osk_edit(|t| { t.pop(); }, window, cx),
            Key::Clear => self.osk_edit(String::clear, window, cx),
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
        let (unit_w, gap) = (64., 8.);
        let placeholder = match self.page {
            Page::System => "Search games",
            Page::Emulators => "Search systems",
        };
        let text = self.osk_text();
        let shown = if text.is_empty() {
            div().text_color(t.muted_foreground).child(placeholder)
        } else {
            div().child(format!("{text}▏"))
        };

        let rows = (0..ROWS).map(|row| {
            h_flex().gap(px(gap)).children(row_keys(row).into_iter().map(|(key, start, width)| {
                let focused = osk.row == row && (start..start + width).contains(&osk.unit);
                let label: SharedString = match key {
                    Key::Char(c) => c.to_string().into(),
                    Key::Space => "Space".into(),
                    Key::Erase => "⌫ Erase".into(),
                    Key::Clear => "Clear".into(),
                    Key::Done => "Done".into(),
                };
                div()
                    .id(("osk-key", row * UNITS + start))
                    .w(px(unit_w * width as f32 + gap * (width as f32 - 1.)))
                    .h(px(56.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(t.radius)
                    .text_xl()
                    .font_medium()
                    .cursor_pointer()
                    .map(|k| {
                        if focused {
                            k.bg(t.primary).text_color(t.primary_foreground)
                        } else if key == Key::Done {
                            k.bg(t.secondary).text_color(t.primary)
                        } else {
                            k.bg(t.secondary).text_color(t.secondary_foreground)
                        }
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(osk) = &mut this.osk {
                            *osk = Osk { row, unit: start };
                        }
                        this.osk_press_key(key, window, cx);
                    }))
            }))
        });

        let hint = |button: &'static str, what: &'static str| {
            h_flex()
                .gap_1p5()
                .child(div().px_1p5().rounded(px(4.)).border_1().border_color(t.border).child(button))
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
            // A real keyboard types straight in.
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let m = &event.keystroke.modifiers;
                if m.control || m.platform || m.alt {
                    return;
                }
                if let Some(c) = event.keystroke.key_char.as_deref().and_then(single_char) {
                    this.osk_press_key(Key::Char(c.to_ascii_lowercase()), window, cx);
                    cx.stop_propagation();
                }
            }))
            .p(px(20.))
            .gap(px(gap))
            .rounded(t.radius_lg)
            .border_1()
            .border_color(t.border)
            .bg(t.popover)
            .shadow_lg()
            .child(
                div()
                    .h(px(48.))
                    .px(px(16.))
                    .mb(px(8.))
                    .flex()
                    .items_center()
                    .rounded(t.radius)
                    .border_1()
                    .border_color(t.ring)
                    .text_xl()
                    .child(shown),
            )
            .children(rows)
            .child(
                h_flex()
                    .mt(px(8.))
                    .gap(px(20.))
                    .justify_center()
                    .text_sm()
                    .text_color(t.muted_foreground)
                    .child(hint("A", "Type"))
                    .child(hint("X", "Erase"))
                    .child(hint("Y", "Space"))
                    .child(hint("B", "Close")),
            );

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

    fn at(row: usize, unit: usize) -> Osk {
        Osk { row, unit }
    }

    #[test]
    fn rows_fill_the_width() {
        for row in 0..ROWS {
            let keys = row_keys(row);
            let (_, start, width) = keys.last().copied().unwrap();
            assert_eq!(start + width, UNITS, "row {row}");
        }
    }

    #[test]
    fn starts_on_1_and_moves_by_key() {
        let mut osk = Osk::default();
        assert_eq!(osk.key(), Key::Char('1'));
        osk.step(0, 1);
        assert_eq!(osk.key(), Key::Char('q'));
        osk.step(1, 0);
        assert_eq!(osk.key(), Key::Char('w'));
        osk.step(0, -1);
        osk.step(0, -1);
        assert_eq!(osk.key(), Key::Char('2'));
    }

    #[test]
    fn left_and_right_wrap() {
        let mut osk = at(1, 0);
        osk.step(-1, 0);
        assert_eq!(osk.key(), Key::Char('p'));
        osk.step(1, 0);
        assert_eq!(osk.key(), Key::Char('q'));
    }

    #[test]
    fn the_bottom_row_keeps_the_column() {
        // Down from "b" (unit 4) is Erase; up again is back over "b".
        let mut osk = at(3, 4);
        osk.step(0, 1);
        assert_eq!(osk.key(), Key::Erase);
        osk.step(0, -1);
        assert_eq!(osk.key(), Key::Char('b'));
        // Down from "z" is Space, and Right steps over Space in one go.
        let mut osk = at(3, 0);
        osk.step(0, 1);
        assert_eq!(osk.key(), Key::Space);
        osk.step(1, 0);
        assert_eq!(osk.key(), Key::Erase);
        osk.step(1, 0);
        osk.step(1, 0);
        assert_eq!(osk.key(), Key::Done);
        osk.step(1, 0);
        assert_eq!(osk.key(), Key::Space);
        osk.step(-1, 0);
        assert_eq!(osk.key(), Key::Done);
        // Up from Done lands over its first unit.
        osk.step(0, -1);
        assert_eq!(osk.key(), Key::Char('.'));
    }

    #[test]
    fn stops_at_the_top_and_bottom() {
        let mut osk = at(0, 3);
        osk.step(0, -1);
        assert_eq!(osk, at(0, 3));
        let mut osk = at(ROWS - 1, 8);
        osk.step(0, 1);
        assert_eq!(osk.key(), Key::Done);
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
