//! Keyboard navigation: the arrow keys, Enter and Backspace reach every
//! control, so the app works with no mouse (and with a gamepad mapped to
//! those keys).
//!
//! The main window keeps its own idea of what has focus (`Focus`) and draws
//! the ring itself, like the file browser does: the sidebar, the header and
//! the library are lists the keys walk through. Dialogs and the details sheet
//! use real GPUI focus instead, through `focus_button` and `group`, so Enter
//! presses whichever button has focus rather than always OK.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, Disableable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::details;
use super::root::{RootView, ViewMode};
use super::scale::px;
use crate::library::{self, GameEntry};
use crate::systems::{self, SystemDef};

gpui_kit::actions!(nav, [Up, Down, Left, Right, Activate, Back, Play, Scrape, Remove, Leave]);

/// Active while the main window itself has focus, so the letter keys never
/// fire while typing in the search field.
pub const KEY_CONTEXT: &str = "Nav";
/// A row or grid of focusable buttons in a dialog or the sheet.
pub const GROUP_CONTEXT: &str = "NavGroup";
/// The details sheet's buttons, which also take P, S and X.
pub const SHEET_CONTEXT: &str = "GameSheet";
/// Wraps the library's search field, for Down, Enter, Backspace and Esc in it.
pub const SEARCH_CONTEXT: &str = "LibrarySearch";

/// Where the keyboard is in the main window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar(usize),
    Header(usize),
    Library(usize),
}

pub struct Nav {
    pub(super) focus: Focus,
    /// The ring shows once a key is pressed and hides on a mouse click, so
    /// it never sits on something nobody picked.
    pub(super) visible: bool,
    /// Header item to come back to from the library.
    last_header: Option<usize>,
    pub(super) scroll: ScrollHandle,
    /// Scroll the focused card into view on the next paint.
    pub(super) reveal: Rc<Cell<bool>>,
}

impl Nav {
    pub(super) fn new() -> Self {
        Self {
            focus: Focus::Library(0),
            visible: false,
            last_header: None,
            scroll: ScrollHandle::new(),
            reveal: Rc::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SideItem {
    System(&'static SystemDef),
    AddEmulator,
    RomsFolder,
    About,
    Theme,
    Quit,
}

impl PartialEq for SideItem {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (SideItem::System(a), SideItem::System(b)) => a.id == b.id,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderItem {
    Folder,
    Scrape,
    AddRom,
    /// The button on the notice under the header, when it has one.
    Notice,
    Search,
    /// The grid / list switch: Enter flips it.
    View,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibItem {
    /// "Add your first ROM" in an empty system.
    AddFirst,
    /// The dashed Add ROM tile at the start of the grid.
    AddTile,
    /// Index into `visible_games`.
    Game(usize),
}

impl RootView {
    pub(super) fn sidebar_items(&self, fullscreen: bool) -> Vec<SideItem> {
        let mut items: Vec<SideItem> = self.enabled.iter().map(|s| SideItem::System(s)).collect();
        if systems::SYSTEMS.iter().any(|s| !self.is_enabled(s)) {
            items.push(SideItem::AddEmulator);
        }
        if !fullscreen {
            items.push(SideItem::RomsFolder);
        }
        items.extend([SideItem::About, SideItem::Theme, SideItem::Quit]);
        items
    }

    pub(super) fn header_items(&self, fullscreen: bool) -> Vec<HeaderItem> {
        let mut items = Vec::new();
        if !fullscreen {
            items.push(HeaderItem::Folder);
        }
        if self.skyscraper_installed && !self.games.is_empty() {
            items.push(HeaderItem::Scrape);
        }
        items.push(HeaderItem::AddRom);
        if self.notice_action().is_some() {
            items.push(HeaderItem::Notice);
        }
        items.extend([HeaderItem::Search, HeaderItem::View]);
        items
    }

    pub(super) fn library_items(&self) -> Vec<LibItem> {
        if self.games.is_empty() {
            return vec![LibItem::AddFirst];
        }
        let games = (0..self.visible_games().len()).map(LibItem::Game);
        if self.view_mode == ViewMode::Grid && self.query.trim().is_empty() {
            std::iter::once(LibItem::AddTile).chain(games).collect()
        } else {
            games.collect()
        }
    }

    /// Columns in the library as it's drawn now: the grid's, or 1 for the
    /// list and the empty state.
    pub(super) fn library_columns(&self, window: &Window) -> usize {
        if self.view_mode == ViewMode::List || self.games.is_empty() {
            return 1;
        }
        super::library::grid_columns(window) as usize
    }

    // What has the ring, for drawing it.

    pub(super) fn focused_side(&self, fullscreen: bool) -> Option<SideItem> {
        match self.nav.focus {
            Focus::Sidebar(i) if self.nav.visible => self.sidebar_items(fullscreen).get(i).copied(),
            _ => None,
        }
    }

    pub(super) fn focused_header(&self, fullscreen: bool) -> Option<HeaderItem> {
        match self.nav.focus {
            Focus::Header(i) if self.nav.visible => self.header_items(fullscreen).get(i).copied(),
            _ => None,
        }
    }

    pub(super) fn focused_library(&self) -> Option<LibItem> {
        match self.nav.focus {
            Focus::Library(i) if self.nav.visible => self.library_items().get(i).copied(),
            _ => None,
        }
    }

    fn focused_game(&self) -> Option<GameEntry> {
        match self.focused_library()? {
            LibItem::Game(i) => self.visible_games().get(i).map(|g| (*g).clone()),
            _ => None,
        }
    }

    // Moving.

    fn current_system_index(&self, fullscreen: bool) -> usize {
        let items = self.sidebar_items(fullscreen);
        items.iter().position(|i| *i == SideItem::System(self.selected)).unwrap_or(0)
    }

    fn focus_sidebar(&mut self, fullscreen: bool) {
        self.nav.focus = Focus::Sidebar(self.current_system_index(fullscreen));
    }

    fn focus_header(&mut self, fullscreen: bool) {
        let items = self.header_items(fullscreen);
        let add = items.iter().position(|i| *i == HeaderItem::AddRom).unwrap_or(0);
        let ix = self.nav.last_header.filter(|i| *i < items.len()).unwrap_or(add);
        self.nav.focus = Focus::Header(ix);
        self.nav.scroll.set_offset(point(px(0.), px(0.)));
    }

    /// The first card, or the search field when nothing matches.
    pub(super) fn focus_library(&mut self, fullscreen: bool) {
        if self.library_items().is_empty() {
            let items = self.header_items(fullscreen);
            self.nav.last_header = items.iter().position(|i| *i == HeaderItem::Search);
            self.focus_header(fullscreen);
        } else {
            self.set_library_focus(0);
        }
    }

    fn set_library_focus(&mut self, ix: usize) {
        self.nav.focus = Focus::Library(ix);
        self.nav.reveal.set(true);
    }

    /// Show the ring if it was hidden. The first key press only does that,
    /// so nothing happens to an item nobody could see was picked.
    fn wake(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        let fullscreen = window.is_fullscreen();
        // Lists change under the focus (a game deleted, a view switched).
        let len = match self.nav.focus {
            Focus::Sidebar(_) => self.sidebar_items(fullscreen).len(),
            Focus::Header(_) => self.header_items(fullscreen).len(),
            Focus::Library(_) => self.library_items().len(),
        };
        match &mut self.nav.focus {
            Focus::Sidebar(i) | Focus::Header(i) | Focus::Library(i) => *i = (*i).min(len.saturating_sub(1)),
        }
        if matches!(self.nav.focus, Focus::Library(_)) && len == 0 {
            self.focus_library(fullscreen);
        }
        cx.notify();
        if self.nav.visible {
            return true;
        }
        self.nav.visible = true;
        self.nav.reveal.set(true);
        false
    }

    /// A mouse click hides the ring until the next key press.
    pub(super) fn hide_ring(&mut self, cx: &mut Context<Self>) {
        if self.nav.visible {
            self.nav.visible = false;
            cx.notify();
        }
    }

    /// Moving onto a system opens it, as clicking it does.
    fn sidebar_moved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Focus::Sidebar(i) = self.nav.focus else { return };
        if let Some(SideItem::System(system)) = self.sidebar_items(window.is_fullscreen()).get(i).copied() {
            if system.id != self.selected.id {
                self.select_system(system, window, cx);
                self.nav.scroll.set_offset(point(px(0.), px(0.)));
            }
        }
    }

    fn nav_up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wake(window, cx) {
            return;
        }
        match self.nav.focus {
            Focus::Sidebar(i) => {
                self.nav.focus = Focus::Sidebar(i.saturating_sub(1));
                self.sidebar_moved(window, cx);
            }
            Focus::Header(_) => {}
            Focus::Library(i) => {
                let columns = self.library_columns(window);
                if i < columns {
                    self.focus_header(window.is_fullscreen());
                } else {
                    self.set_library_focus(i - columns);
                }
            }
        }
    }

    fn nav_down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wake(window, cx) {
            return;
        }
        match self.nav.focus {
            Focus::Sidebar(i) => {
                let last = self.sidebar_items(window.is_fullscreen()).len().saturating_sub(1);
                self.nav.focus = Focus::Sidebar((i + 1).min(last));
                self.sidebar_moved(window, cx);
            }
            Focus::Header(_) => {
                if !self.library_items().is_empty() {
                    self.set_library_focus(0);
                }
            }
            Focus::Library(i) => {
                let columns = self.library_columns(window);
                let len = self.library_items().len();
                if i + columns < len {
                    self.set_library_focus(i + columns);
                } else if i / columns < (len - 1) / columns {
                    // A short last row: land on its last card.
                    self.set_library_focus(len - 1);
                }
            }
        }
    }

    fn nav_left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wake(window, cx) {
            return;
        }
        let fullscreen = window.is_fullscreen();
        match self.nav.focus {
            Focus::Sidebar(_) => {}
            Focus::Header(0) => self.focus_sidebar(fullscreen),
            Focus::Header(i) => {
                self.nav.focus = Focus::Header(i - 1);
                self.nav.last_header = Some(i - 1);
            }
            Focus::Library(i) => {
                if i % self.library_columns(window) == 0 {
                    self.focus_sidebar(fullscreen);
                } else {
                    self.set_library_focus(i - 1);
                }
            }
        }
    }

    fn nav_right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wake(window, cx) {
            return;
        }
        let fullscreen = window.is_fullscreen();
        match self.nav.focus {
            Focus::Sidebar(_) => self.focus_library(fullscreen),
            Focus::Header(i) => {
                if i + 1 < self.header_items(fullscreen).len() {
                    self.nav.focus = Focus::Header(i + 1);
                    self.nav.last_header = Some(i + 1);
                }
            }
            Focus::Library(i) => {
                let columns = self.library_columns(window);
                if i % columns + 1 < columns && i + 1 < self.library_items().len() {
                    self.set_library_focus(i + 1);
                }
            }
        }
    }

    fn nav_activate(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wake(window, cx) {
            return;
        }
        let fullscreen = window.is_fullscreen();
        if let Some(item) = self.focused_side(fullscreen) {
            match item {
                SideItem::System(system) => {
                    if system.id != self.selected.id {
                        self.select_system(system, window, cx);
                    }
                    self.focus_library(fullscreen);
                }
                SideItem::AddEmulator => details::open_add_system(self, window, cx),
                SideItem::RomsFolder => self.open_folder(library::roms_root(), cx),
                SideItem::About => details::open_about(self, window, cx),
                SideItem::Theme => self.toggle_theme(window, cx),
                SideItem::Quit => self.request_quit(window, cx),
            }
        } else if let Some(item) = self.focused_header(fullscreen) {
            match item {
                HeaderItem::Folder => self.open_folder(self.system_folder(), cx),
                HeaderItem::Scrape => self.start_scrape(Vec::new(), window, cx),
                HeaderItem::AddRom => self.pick_and_install(window, cx),
                HeaderItem::Notice => self.run_notice_action(window, cx),
                HeaderItem::Search => self.search.update(cx, |s, cx| s.focus(window, cx)),
                HeaderItem::View => {
                    let mode = if self.view_mode == ViewMode::Grid { ViewMode::List } else { ViewMode::Grid };
                    self.set_view_mode(mode, cx);
                }
            }
        } else if let Some(item) = self.focused_library() {
            match item {
                LibItem::AddFirst | LibItem::AddTile => self.pick_and_install(window, cx),
                LibItem::Game(_) => {
                    if let Some(entry) = self.focused_game() {
                        details::open_details(self, &entry, window, cx);
                    }
                }
            }
        }
    }

    fn nav_back(&mut self, _: &Back, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wake(window, cx) {
            return;
        }
        let fullscreen = window.is_fullscreen();
        match self.nav.focus {
            Focus::Sidebar(_) => {}
            Focus::Header(_) if !self.library_items().is_empty() => self.set_library_focus(0),
            Focus::Header(_) | Focus::Library(_) => self.focus_sidebar(fullscreen),
        }
    }

    fn nav_play(&mut self, _: &Play, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wake(window, cx) {
            return;
        }
        if let Some(entry) = self.focused_game() {
            if self.can_run() && self.misplaced_as(&entry).is_none() && self.running.is_none() {
                self.run_game(&entry, window, cx);
            }
        }
    }

    fn nav_scrape(&mut self, _: &Scrape, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wake(window, cx) {
            return;
        }
        if let Some(entry) = self.focused_game() {
            self.scrape_key(&entry, window, cx);
        }
    }

    /// S on a game: scrape it, or offer to install Skyscraper first.
    pub(super) fn scrape_key(&mut self, entry: &GameEntry, window: &mut Window, cx: &mut Context<Self>) {
        // Scraping as the wrong system would only fetch the wrong game.
        if self.misplaced_as(entry).is_some() {
            return;
        }
        if !self.skyscraper_installed && self.can_install_skyscraper {
            details::confirm_install_skyscraper(window, cx);
        } else {
            self.scrape_one(entry, window, cx);
        }
    }

    fn nav_remove(&mut self, _: &Remove, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wake(window, cx) {
            return;
        }
        if let Some(entry) = self.focused_game() {
            details::confirm_delete_dialog(self, entry, window, cx);
        } else if let Some(SideItem::System(system)) = self.focused_side(window.is_fullscreen()) {
            // The last system can't go, same as its hidden button.
            if self.enabled.len() > 1 {
                details::confirm_remove_system(system, window, cx);
            }
        }
    }

    // The search field.

    /// Down or Enter in the search field: on to the first match.
    pub(super) fn leave_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.nav.visible = true;
        self.focus_library(window.is_fullscreen());
        cx.notify();
    }

    fn nav_search_leave(&mut self, _: &Leave, window: &mut Window, cx: &mut Context<Self>) {
        self.leave_search(window, cx);
    }

    /// Backspace in the search field goes back once it's empty.
    fn nav_search_back(&mut self, _: &Back, window: &mut Window, cx: &mut Context<Self>) {
        if self.query.is_empty() {
            self.leave_search(window, cx);
        } else {
            cx.propagate();
        }
    }

    /// Esc clears the search and goes back.
    fn nav_search_cancel(&mut self, _: &gpui_kit::component::dialog::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
        self.leave_search(window, cx);
    }

    /// The main window's key handlers, for its root element.
    pub(super) fn nav_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(Self::nav_up))
            .on_action(cx.listener(Self::nav_down))
            .on_action(cx.listener(Self::nav_left))
            .on_action(cx.listener(Self::nav_right))
            .on_action(cx.listener(Self::nav_activate))
            .on_action(cx.listener(Self::nav_back))
            .on_action(cx.listener(Self::nav_play))
            .on_action(cx.listener(Self::nav_scrape))
            .on_action(cx.listener(Self::nav_remove))
    }

    /// The search field, wrapped so its own keys reach `RootView`.
    pub(super) fn search_keys(&self, field: impl IntoElement, cx: &mut Context<Self>) -> Div {
        div()
            .key_context(SEARCH_CONTEXT)
            .on_action(cx.listener(Self::nav_search_leave))
            .on_action(cx.listener(Self::nav_search_back))
            .on_action(cx.listener(Self::nav_search_cancel))
            .child(field)
    }
}

/// Keep the focused card in view: called with the bounds of the library's
/// children, in item order, after they're laid out.
pub(super) fn reveal_child(scroll: &ScrollHandle, reveal: &Cell<bool>, child: Option<Bounds<Pixels>>, window: &mut Window) {
    if !reveal.replace(false) {
        return;
    }
    let Some(child) = child else { return };
    let view = scroll.bounds();
    let margin = px(16.);
    let mut offset = scroll.offset();
    if child.top() < view.top() + margin {
        offset.y += view.top() + margin - child.top();
    } else if child.bottom() > view.bottom() - margin {
        offset.y -= child.bottom() - (view.bottom() - margin);
    } else {
        return;
    }
    offset.y = offset.y.min(px(0.));
    scroll.set_offset(offset);
    // A refresh asked for while painting is dropped, so ask after it.
    window.on_next_frame(|window, _| window.refresh());
}

fn ring_border(mut el: Div, width: Pixels) -> Div {
    let width: AbsoluteLength = width.into();
    el.style().border_widths = EdgesRefinement {
        top: Some(width),
        right: Some(width),
        bottom: Some(width),
        left: Some(width),
    };
    el
}

/// The ring, laid over the inside edge of a card or row. The parent needs
/// `.relative()`.
pub(super) fn ring(radius: Pixels, color: Hsla) -> Div {
    ring_border(div().absolute().top_0().left_0().size_full().rounded(radius), px(2.)).border_color(color)
}

/// A control (a kit button, the search field) with the ring drawn just
/// outside it when `focused`.
pub(super) fn ringed(control: impl IntoElement, focused: bool, radius: Pixels, color: Hsla) -> Div {
    let gap = px(3.);
    div().relative().flex_shrink_0().child(control).when(focused, |wrap| {
        wrap.child(
            ring_border(div().absolute().top(-gap).left(-gap).right(-gap).bottom(-gap).rounded(radius + gap), px(2.))
                .border_color(color),
        )
    })
}

/// A kit button that takes real keyboard focus, for dialogs and the sheet.
/// The wrapper holds the focus and answers Enter and Space; the button
/// itself keeps the mouse click and stays out of the tab order.
pub(super) fn focus_button(
    id: impl Into<ElementId>,
    handle: &FocusHandle,
    button: Button,
    enabled: bool,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let focused = handle.is_focused(window);
    let color = cx.theme().ring;
    let radius = cx.theme().radius;
    let on_press = Rc::new(on_press);
    let key_press = on_press.clone();
    let button = button
        .tab_stop(false)
        .disabled(!enabled)
        .when(enabled, |b| b.on_click(move |_, window, cx| on_press(window, cx)));
    ringed(button, focused, radius, color)
        .id(id)
        .when(enabled, |wrap| {
            wrap.track_focus(handle).on_click(move |event, window, cx| {
                if event.is_keyboard() {
                    key_press(window, cx);
                }
            })
        })
}

/// Something a key does, shared between handlers.
pub(super) type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

/// Arrow keys between the focusable buttons in `rows`, for the element that
/// holds them. ← → go through them in order, ↑ ↓ to the row above or
/// below. `above` runs on ↑ from the top row.
pub(super) fn group(
    el: Div,
    rows: Vec<Vec<FocusHandle>>,
    above: Option<Handler>,
) -> Div {
    let rows = Rc::new(rows);
    let find = |rows: &[Vec<FocusHandle>], window: &Window| {
        rows.iter().enumerate().find_map(|(r, row)| {
            row.iter().position(|h| h.is_focused(window)).map(|c| (r, c))
        })
    };
    let flat = |rows: &[Vec<FocusHandle>]| rows.iter().flatten().cloned().collect::<Vec<_>>();
    let step = {
        let rows = rows.clone();
        move |delta: isize, window: &mut Window, cx: &mut App| {
            let all = flat(&rows);
            let Some(at) = all.iter().position(|h| h.is_focused(window)) else { return };
            let next = at as isize + delta;
            if let Some(handle) = usize::try_from(next).ok().and_then(|n| all.get(n)) {
                window.focus(handle, cx);
            }
        }
    };
    let vertical = {
        let rows = rows.clone();
        move |down: bool, window: &mut Window, cx: &mut App| {
            let Some((r, c)) = find(&rows, window) else { return };
            let target = if down { rows.get(r + 1) } else { r.checked_sub(1).and_then(|r| rows.get(r)) };
            match target {
                Some(row) => {
                    if let Some(handle) = row.get(c.min(row.len().saturating_sub(1))) {
                        window.focus(handle, cx);
                    }
                }
                None if !down => {
                    if let Some(above) = &above {
                        above(window, cx);
                    }
                }
                None => {}
            }
        }
    };
    let step = Rc::new(step);
    let (left, right) = (step.clone(), step);
    let vertical = Rc::new(vertical);
    let (up, down) = (vertical.clone(), vertical);
    el.key_context(GROUP_CONTEXT)
        .on_action(move |_: &Left, window, cx| left(-1, window, cx))
        .on_action(move |_: &Right, window, cx| right(1, window, cx))
        .on_action(move |_: &Up, window, cx| up(false, window, cx))
        .on_action(move |_: &Down, window, cx| down(true, window, cx))
}

/// Focus `handle` once the dialog or sheet it's in has opened and taken
/// focus for itself.
pub(super) fn focus_when_open(handle: FocusHandle, window: &mut Window) {
    window.on_next_frame(move |window, cx| window.focus(&handle, cx));
}

