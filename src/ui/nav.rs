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
use super::root::{Page, RootView, ViewMode};
use super::scale::px;
use crate::library::{self, GameEntry};
use crate::systems::{self, SystemDef};

gpui_kit::actions!(nav, [Up, Down, Left, Right, Activate, Back, Play, Scrape, Remove, Leave, PrevSystem, NextSystem]);

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
    pub(super) scroll: ScrollHandle,
    /// Scroll the focused card into view on the next paint.
    pub(super) reveal: Rc<Cell<bool>>,
}

impl Nav {
    pub(super) fn new() -> Self {
        Self {
            focus: Focus::Library(0),
            visible: false,
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
    /// A maker filter chip on the emulator library (`None` is "All").
    Maker(Option<&'static str>),
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

/// Cards in groups, each group starting its own rows of `columns`. Moves
/// return the card to go to, or `None` at an edge.
struct Grid {
    groups: Vec<usize>,
    columns: usize,
}

/// Where a card is: its group, the group's first card, its row in the
/// group and its column.
struct Spot {
    group: usize,
    start: usize,
    row: usize,
    col: usize,
}

impl Grid {
    fn locate(&self, ix: usize) -> Option<Spot> {
        let mut start = 0;
        for (group, len) in self.groups.iter().enumerate() {
            if ix < start + len {
                let at = ix - start;
                return Some(Spot { group, start, row: at / self.columns, col: at % self.columns });
            }
            start += len;
        }
        None
    }

    /// Card `col` of row `row` in a group of `len` cards from `start`, or
    /// the row's last card when it's shorter.
    fn at(&self, start: usize, len: usize, row: usize, col: usize) -> usize {
        start + (row * self.columns + col).min(len - 1)
    }

    fn up(&self, ix: usize) -> Option<usize> {
        let c = self.locate(ix)?;
        if c.row > 0 {
            return Some(self.at(c.start, self.groups[c.group], c.row - 1, c.col));
        }
        // The last row of the group above.
        let group = c.group.checked_sub(1)?;
        let len = self.groups[group];
        Some(self.at(c.start - len, len, (len - 1) / self.columns, c.col))
    }

    fn down(&self, ix: usize) -> Option<usize> {
        let c = self.locate(ix)?;
        let len = self.groups[c.group];
        if c.row < (len - 1) / self.columns {
            return Some(self.at(c.start, len, c.row + 1, c.col));
        }
        // The first row of the next group with cards in it.
        let next = *self.groups.get(c.group + 1)?;
        (next > 0).then(|| self.at(c.start + len, next, 0, c.col))
    }

    fn left(&self, ix: usize) -> Option<usize> {
        let c = self.locate(ix)?;
        (c.col > 0).then(|| ix - 1)
    }

    fn right(&self, ix: usize) -> Option<usize> {
        let c = self.locate(ix)?;
        (c.col + 1 < self.columns && ix + 1 < c.start + self.groups[c.group]).then_some(ix + 1)
    }
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

    /// The header's items, row by row as they're drawn: the title row's
    /// buttons, the notice's button, then search and the view switch just
    /// above the games. Up and Down move between rows (#58).
    pub(super) fn header_rows(&self, fullscreen: bool) -> Vec<Vec<HeaderItem>> {
        if self.page == Page::Emulators {
            return vec![std::iter::once(HeaderItem::Maker(None))
                .chain(systems::MAKERS.iter().map(|m| HeaderItem::Maker(Some(*m))))
                .chain([HeaderItem::Search])
                .collect()];
        }
        let mut title = Vec::new();
        if !fullscreen {
            title.push(HeaderItem::Folder);
        }
        if self.skyscraper_installed && !self.games.is_empty() {
            title.push(HeaderItem::Scrape);
        }
        title.push(HeaderItem::AddRom);
        let mut rows = vec![title];
        if self.notice_action().is_some() {
            rows.push(vec![HeaderItem::Notice]);
        }
        rows.push(vec![HeaderItem::Search, HeaderItem::View]);
        rows
    }

    /// All header items in one list; `Focus::Header` indexes into it.
    pub(super) fn header_items(&self, fullscreen: bool) -> Vec<HeaderItem> {
        self.header_rows(fullscreen).concat()
    }

    pub(super) fn library_items(&self) -> Vec<LibItem> {
        if self.page == Page::Emulators {
            return (0..self.emulator_cards().len()).map(LibItem::Game).collect();
        }
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
        if self.page == Page::Emulators {
            return super::library::columns_for(window, super::emulators::CARD_MIN_WIDTH) as usize;
        }
        if self.view_mode == ViewMode::List || self.games.is_empty() {
            return 1;
        }
        super::library::grid_columns(window) as usize
    }

    /// The library's cards as the keys see them: the emulator page's
    /// categories each start their own rows (#59).
    fn library_grid(&self, window: &Window) -> Grid {
        let groups = if self.page == Page::Emulators {
            self.emulator_groups().iter().map(|g| g.systems.len()).collect()
        } else {
            vec![self.library_items().len()]
        };
        Grid { groups, columns: self.library_columns(window).max(1) }
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
        if self.page == Page::Emulators {
            return None;
        }
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

    /// Focus header row `row`, coming from the row below or above: Scrape
    /// (or Add ROM without it) on the title row, otherwise the row's first
    /// item, which is search on the row above the games.
    fn focus_header_row(&mut self, row: usize, fullscreen: bool) {
        let rows = self.header_rows(fullscreen);
        let Some(items) = rows.get(row) else { return };
        let start: usize = rows[..row].iter().map(Vec::len).sum();
        let col = [HeaderItem::Scrape, HeaderItem::AddRom]
            .iter()
            .find_map(|wanted| items.iter().position(|i| i == wanted))
            .unwrap_or(0);
        self.nav.focus = Focus::Header(start + col);
        self.nav.scroll.set_offset(point(px(0.), px(0.)));
    }

    /// The header row nearest the games.
    fn focus_header(&mut self, fullscreen: bool) {
        let last = self.header_rows(fullscreen).len().saturating_sub(1);
        self.focus_header_row(last, fullscreen);
    }

    /// Focus a header item by what it is, if it's shown.
    fn focus_header_item(&mut self, item: HeaderItem, fullscreen: bool) -> bool {
        match self.header_items(fullscreen).iter().position(|i| *i == item) {
            Some(ix) => {
                self.nav.focus = Focus::Header(ix);
                self.nav.scroll.set_offset(point(px(0.), px(0.)));
                true
            }
            None => false,
        }
    }

    /// Row and column of header item `ix`.
    fn header_pos(&self, ix: usize, fullscreen: bool) -> Option<(usize, usize, usize)> {
        let mut start = 0;
        for (row, items) in self.header_rows(fullscreen).iter().enumerate() {
            if ix < start + items.len() {
                return Some((row, ix - start, items.len()));
            }
            start += items.len();
        }
        None
    }

    /// The first card, or the search field when nothing matches.
    pub(super) fn focus_library(&mut self, fullscreen: bool) {
        if self.library_items().is_empty() {
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
        if self.big_picture.is_some() {
            return self.bp_move(0, -1, window, cx);
        }
        if !self.wake(window, cx) {
            return;
        }
        match self.nav.focus {
            Focus::Sidebar(i) => {
                self.nav.focus = Focus::Sidebar(i.saturating_sub(1));
                self.sidebar_moved(window, cx);
            }
            Focus::Header(i) => {
                let fullscreen = window.is_fullscreen();
                if let Some((row, _, _)) = self.header_pos(i, fullscreen) {
                    if row > 0 {
                        self.focus_header_row(row - 1, fullscreen);
                    }
                }
            }
            Focus::Library(i) => match self.library_grid(window).up(i) {
                Some(ix) => self.set_library_focus(ix),
                None => self.focus_header(window.is_fullscreen()),
            },
        }
    }

    fn nav_down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        if self.big_picture.is_some() {
            return self.bp_move(0, 1, window, cx);
        }
        if !self.wake(window, cx) {
            return;
        }
        match self.nav.focus {
            Focus::Sidebar(i) => {
                let last = self.sidebar_items(window.is_fullscreen()).len().saturating_sub(1);
                self.nav.focus = Focus::Sidebar((i + 1).min(last));
                self.sidebar_moved(window, cx);
            }
            Focus::Header(i) => {
                let fullscreen = window.is_fullscreen();
                let rows = self.header_rows(fullscreen).len();
                match self.header_pos(i, fullscreen) {
                    Some((row, _, _)) if row + 1 < rows => self.focus_header_row(row + 1, fullscreen),
                    _ if self.library_items().is_empty() => {}
                    // The view switch sits above the top row's last card.
                    _ if self.focused_header(fullscreen) == Some(HeaderItem::View) => {
                        let columns = self.library_columns(window);
                        self.set_library_focus(columns.min(self.library_items().len()) - 1);
                    }
                    _ => self.set_library_focus(0),
                }
            }
            Focus::Library(i) => {
                if let Some(ix) = self.library_grid(window).down(i) {
                    self.set_library_focus(ix);
                }
            }
        }
    }

    fn nav_left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        if self.big_picture.is_some() {
            return self.bp_move(-1, 0, window, cx);
        }
        if !self.wake(window, cx) {
            return;
        }
        let fullscreen = window.is_fullscreen();
        match self.nav.focus {
            Focus::Sidebar(_) => {}
            Focus::Header(i) => match self.header_pos(i, fullscreen) {
                Some((_, col, _)) if col > 0 => self.nav.focus = Focus::Header(i - 1),
                _ => self.focus_sidebar(fullscreen),
            },
            Focus::Library(i) => match self.library_grid(window).left(i) {
                Some(ix) => self.set_library_focus(ix),
                None => self.focus_sidebar(fullscreen),
            },
        }
    }

    fn nav_right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        if self.big_picture.is_some() {
            return self.bp_move(1, 0, window, cx);
        }
        if !self.wake(window, cx) {
            return;
        }
        let fullscreen = window.is_fullscreen();
        match self.nav.focus {
            Focus::Sidebar(_) => self.focus_library(fullscreen),
            Focus::Header(i) => {
                if let Some((_, col, len)) = self.header_pos(i, fullscreen) {
                    if col + 1 < len {
                        self.nav.focus = Focus::Header(i + 1);
                    }
                }
            }
            Focus::Library(i) => {
                let grid = self.library_grid(window);
                if let Some(ix) = grid.right(i) {
                    self.set_library_focus(ix);
                } else if grid.up(i).is_none() {
                    // The top row's last card: on to the view switch above it.
                    self.focus_header_item(HeaderItem::View, fullscreen);
                }
            }
        }
    }

    fn nav_activate(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        if self.big_picture.is_some() {
            return self.bp_activate(window, cx);
        }
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
                SideItem::AddEmulator => self.open_emulators(window, cx),
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
                // No keyboard is expected in fullscreen: type on screen (#56).
                HeaderItem::Search if fullscreen => self.open_osk(window, cx),
                HeaderItem::Search => self.active_search().update(cx, |s, cx| s.focus(window, cx)),
                HeaderItem::View => {
                    let mode = if self.view_mode == ViewMode::Grid { ViewMode::List } else { ViewMode::Grid };
                    self.set_view_mode(mode, cx);
                }
                HeaderItem::Maker(maker) => self.set_maker_filter(maker, cx),
            }
        } else if let Some(item) = self.focused_library() {
            if self.page == Page::Emulators {
                if let LibItem::Game(i) = item {
                    self.activate_emulator(i, window, cx);
                }
                return;
            }
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
        if self.big_picture.is_some() {
            return self.bp_back(cx);
        }
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
        if self.big_picture.is_some() {
            return self.bp_play(window, cx);
        }
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
        if self.big_picture.is_some() {
            return self.bp_scrape(window, cx);
        }
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
        if self.big_picture.is_some() {
            return self.bp_delete(window, cx);
        }
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
        let empty = match self.page {
            Page::System => self.query.is_empty(),
            Page::Emulators => self.emu_query.is_empty(),
        };
        if empty {
            self.leave_search(window, cx);
        } else {
            cx.propagate();
        }
    }

    /// Esc clears the search and goes back.
    fn nav_search_cancel(&mut self, _: &gpui_kit::component::dialog::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_search(window, cx);
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
            .on_action(cx.listener(|this, _: &PrevSystem, window, cx| {
                if this.big_picture.is_some() {
                    this.bp_switch(-1, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextSystem, window, cx| {
                if this.big_picture.is_some() {
                    this.bp_switch(1, window, cx);
                }
            }))
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


#[cfg(test)]
mod tests {
    use super::Grid;

    /// The example from #59: Nintendo's 9 cards then Sega's 7, six a row.
    fn nintendo_sega() -> Grid {
        Grid { groups: vec![9, 7], columns: 6 }
    }

    #[test]
    fn down_stays_in_the_group_while_it_has_rows() {
        let g = nintendo_sega();
        assert_eq!(g.down(0), Some(6));
        assert_eq!(g.down(2), Some(8));
        // Nothing right under the 4th to 6th: the second row's last card.
        assert_eq!(g.down(3), Some(8));
        assert_eq!(g.down(5), Some(8));
    }

    #[test]
    fn down_from_a_groups_last_row_goes_to_the_next_group() {
        let g = nintendo_sega();
        assert_eq!(g.down(6), Some(9));
        assert_eq!(g.down(8), Some(11));
        // Sega's second row has one card.
        assert_eq!(g.down(14), Some(15));
        assert_eq!(g.down(15), None);
    }

    #[test]
    fn up_goes_to_the_last_row_of_the_group_above() {
        let g = nintendo_sega();
        assert_eq!(g.up(9), Some(6));
        assert_eq!(g.up(11), Some(8));
        // Under the 4th to 6th Nintendo column there's no second-row card.
        assert_eq!(g.up(13), Some(8));
        assert_eq!(g.up(15), Some(9));
        assert_eq!(g.up(6), Some(0));
        assert_eq!(g.up(3), None);
    }

    #[test]
    fn left_and_right_stop_at_row_and_group_ends() {
        let g = nintendo_sega();
        assert_eq!(g.right(4), Some(5));
        assert_eq!(g.right(5), None);
        assert_eq!(g.right(8), None);
        assert_eq!(g.left(6), None);
        assert_eq!(g.left(9), None);
        assert_eq!(g.left(10), Some(9));
    }

    #[test]
    fn one_group_is_the_plain_grid() {
        let g = Grid { groups: vec![7], columns: 5 };
        assert_eq!(g.down(1), Some(6));
        // A short last row: its last card.
        assert_eq!(g.down(4), Some(6));
        assert_eq!(g.down(6), None);
        assert_eq!(g.up(6), Some(1));
        assert_eq!(g.up(4), None);
    }
}
