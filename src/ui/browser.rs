//! The built-in file browser for Add ROM (#44). One state, two layouts to
//! try side by side: a dialog for the mouse, and a full-window view for the
//! keyboard and a gamepad. The Layout switch in both is for choosing
//! between them and goes once one is picked.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::breadcrumb::{Breadcrumb, BreadcrumbItem};
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::{Dialog, DialogFooter};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable, Icon, Selectable, Sizable, StyledExt, WindowExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::root::RootView;
use super::scale::px;
use crate::browse::{self, Entry, Listing, Place, PlaceKind};
use crate::library;
use crate::systems::SystemDef;

gpui_kit::actions!(file_browser, [MoveUp, MoveDown, Activate, Back, FocusPlaces, FocusFiles, Close]);

/// Key context of the full-window layout.
pub const KEY_CONTEXT: &str = "FileBrowser";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Dialog,
    Full,
}

/// Where the keyboard is, in the full-window layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Places(usize),
    Files(usize),
    Add,
}

pub struct Browser {
    system: &'static SystemDef,
    pub(super) layout: Layout,
    places: Vec<Place>,
    place: usize,
    dir: PathBuf,
    listing: Listing,
    error: Option<String>,
    /// Picked files, kept while moving between folders.
    selected: BTreeSet<PathBuf>,
    show_all: bool,
    focus: Focus,
    mounting: bool,
    scroll: ScrollHandle,
}

impl Browser {
    fn new(system: &'static SystemDef, layout: Layout) -> Self {
        let places = browse::places();
        let place = places.iter().position(|p| p.name == "Downloads").unwrap_or(0);
        let dir = places.get(place).and_then(Place::root).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/"));
        let mut browser = Self {
            system,
            layout,
            places,
            place,
            dir,
            listing: Listing::default(),
            error: None,
            selected: BTreeSet::new(),
            show_all: false,
            focus: Focus::Files(0),
            mounting: false,
            scroll: ScrollHandle::new(),
        };
        browser.refresh();
        browser
    }

    fn refresh(&mut self) {
        match browse::list(&self.dir, self.system.extensions, self.show_all) {
            Ok(listing) => {
                self.listing = listing;
                self.error = None;
            }
            Err(err) => {
                self.listing = Listing::default();
                self.error = Some(format!("Couldn't open this folder: {err}"));
            }
        }
        if let Focus::Files(ix) = self.focus {
            self.focus = Focus::Files(ix.min(self.listing.entries.len().saturating_sub(1)));
        }
    }

    fn open_dir(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.refresh();
        self.set_focus(Focus::Files(0));
    }

    fn root(&self) -> Option<&Path> {
        self.places.get(self.place)?.root()
    }

    /// One folder up, unless already at the place's top.
    fn go_up(&mut self) -> bool {
        let Some(root) = self.root().map(Path::to_path_buf) else { return false };
        if self.dir == root || !self.dir.starts_with(&root) {
            return false;
        }
        let Some(parent) = self.dir.parent().map(Path::to_path_buf) else { return false };
        let came_from = std::mem::replace(&mut self.dir, parent);
        self.refresh();
        let ix = self.listing.entries.iter().position(|e| e.path == came_from).unwrap_or(0);
        self.set_focus(Focus::Files(ix));
        true
    }

    /// The place, then each folder below it.
    fn crumbs(&self) -> Vec<(String, PathBuf)> {
        let Some((place, root)) = self.places.get(self.place).and_then(|p| Some((p, p.root()?))) else {
            return vec![(self.dir.display().to_string(), self.dir.clone())];
        };
        let mut crumbs = vec![(place.name.clone(), root.to_path_buf())];
        let mut path = root.to_path_buf();
        for part in self.dir.strip_prefix(root).into_iter().flat_map(|rest| rest.components()) {
            path.push(part);
            crumbs.push((part.as_os_str().to_string_lossy().to_string(), path.clone()));
        }
        crumbs
    }

    fn toggle(&mut self, path: &Path) {
        if !self.selected.remove(path) {
            self.selected.insert(path.to_path_buf());
        }
    }

    fn selected_size(&self) -> u64 {
        self.selected.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum()
    }

    fn summary(&self) -> String {
        match self.selected.len() {
            0 => "Nothing selected".to_string(),
            n => format!("{n} selected · {}", library::format_size(self.selected_size())),
        }
    }

    fn add_label(&self) -> String {
        match self.selected.len() {
            0 | 1 => "Add file".to_string(),
            n => format!("Add {n} files"),
        }
    }

    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        if let Focus::Files(ix) = focus {
            self.scroll.scroll_to_item(ix);
        }
    }

    fn move_focus(&mut self, down: bool) {
        let files = self.listing.entries.len();
        let next = match self.focus {
            Focus::Places(i) if down => Focus::Places((i + 1).min(self.places.len().saturating_sub(1))),
            Focus::Places(i) => Focus::Places(i.saturating_sub(1)),
            Focus::Files(i) if down && i + 1 >= files => Focus::Add,
            Focus::Files(i) if down => Focus::Files(i + 1),
            Focus::Files(i) => Focus::Files(i.saturating_sub(1)),
            Focus::Add if down => Focus::Add,
            Focus::Add if files > 0 => Focus::Files(files - 1),
            Focus::Add => Focus::Add,
        };
        self.set_focus(next);
    }
}

impl RootView {
    /// Add ROM: opens in the full-window layout when the window is
    /// fullscreen, as a dialog otherwise.
    pub(super) fn open_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let layout = if window.is_fullscreen() { Layout::Full } else { Layout::Dialog };
        self.open_browser_in(layout, window, cx);
    }

    pub(super) fn open_browser_in(&mut self, layout: Layout, window: &mut Window, cx: &mut Context<Self>) {
        self.browser = Some(Browser::new(self.selected, layout));
        self.show_browser(window, cx);
    }

    fn show_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.browser.as_ref().map(|b| b.layout) {
            Some(Layout::Dialog) => open_dialog(window, cx),
            Some(Layout::Full) => window.focus(&self.browser_focus, cx),
            None => {}
        }
        cx.notify();
    }

    fn set_browser_layout(&mut self, layout: Layout, window: &mut Window, cx: &mut Context<Self>) {
        let Some(browser) = self.browser.as_mut() else { return };
        if browser.layout == layout {
            return;
        }
        if browser.layout == Layout::Dialog {
            // `close_dialog` skips `on_close`, so the browser stays open.
            window.close_dialog(cx);
        }
        browser.layout = layout;
        self.show_browser(window, cx);
    }

    fn close_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.browser.take().map(|b| b.layout) {
            Some(Layout::Dialog) => window.close_dialog(cx),
            Some(Layout::Full) => window.focus(&self.focus_handle, cx),
            None => {}
        }
        cx.notify();
    }

    fn browser_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(browser) = self.browser.as_ref() else { return };
        let paths: Vec<PathBuf> = browser.selected.iter().cloned().collect();
        if paths.is_empty() {
            return;
        }
        self.close_browser(window, cx);
        self.install_paths(paths, window, cx);
    }

    fn browser_use_native(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_browser(window, cx);
        self.pick_native(window, cx);
    }

    fn browser_open_place(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(browser) = self.browser.as_mut() else { return };
        let Some(place) = browser.places.get(ix).cloned() else { return };
        browser.place = ix;
        if let Some(root) = place.root() {
            browser.open_dir(root.to_path_buf());
            cx.notify();
            return;
        }
        let PlaceKind::Drive { device, .. } = place.kind else { return };
        browser.mounting = true;
        browser.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { browse::mount(&device) }).await;
            this.update(cx, |view, cx| {
                let Some(browser) = view.browser.as_mut() else { return };
                browser.mounting = false;
                match result {
                    Ok(path) => {
                        if let Some(PlaceKind::Drive { mount, .. }) = browser.places.get_mut(ix).map(|p| &mut p.kind) {
                            *mount = Some(path.clone());
                        }
                        if let Some(p) = browser.places.get_mut(ix) {
                            p.detail = "USB drive".to_string();
                        }
                        if browser.place == ix {
                            browser.open_dir(path);
                        }
                    }
                    Err(err) => browser.error = Some(format!("Couldn't open {}: {err}", place.name)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A click or Enter on a row: open a folder, tick a file.
    fn browser_activate_entry(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(browser) = self.browser.as_mut() else { return };
        let Some(entry) = browser.listing.entries.get(ix).cloned() else { return };
        if entry.is_dir {
            browser.open_dir(entry.path);
        } else {
            browser.toggle(&entry.path);
            browser.set_focus(Focus::Files(ix));
        }
        cx.notify();
    }

    fn browser_up(&mut self, cx: &mut Context<Self>) {
        if let Some(browser) = self.browser.as_mut() {
            browser.go_up();
        }
        cx.notify();
    }

    fn browser_show_all(&mut self, show_all: bool, cx: &mut Context<Self>) {
        if let Some(browser) = self.browser.as_mut() {
            browser.show_all = show_all;
            browser.refresh();
        }
        cx.notify();
    }

    fn browser_open_crumb(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(browser) = self.browser.as_mut() {
            browser.open_dir(path);
        }
        cx.notify();
    }

    // Keys, in the full-window layout.

    fn on_move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(browser) = self.browser.as_mut() {
            browser.move_focus(false);
        }
        cx.notify();
    }

    fn on_move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(browser) = self.browser.as_mut() {
            browser.move_focus(true);
        }
        cx.notify();
    }

    fn on_activate(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        let Some(focus) = self.browser.as_ref().map(|b| b.focus) else { return };
        match focus {
            Focus::Places(ix) => self.browser_open_place(ix, window, cx),
            Focus::Files(ix) => self.browser_activate_entry(ix, cx),
            Focus::Add => self.browser_add(window, cx),
        }
    }

    fn on_back(&mut self, _: &Back, window: &mut Window, cx: &mut Context<Self>) {
        let went_up = self.browser.as_mut().is_some_and(|b| b.go_up());
        if went_up {
            cx.notify();
        } else {
            self.close_browser(window, cx);
        }
    }

    fn on_focus_places(&mut self, _: &FocusPlaces, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(browser) = self.browser.as_mut() {
            browser.set_focus(Focus::Places(browser.place));
        }
        cx.notify();
    }

    fn on_focus_files(&mut self, _: &FocusFiles, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(browser) = self.browser.as_mut().filter(|b| matches!(b.focus, Focus::Places(_))) {
            browser.set_focus(Focus::Files(0));
        }
        cx.notify();
    }

    fn on_close(&mut self, _: &Close, window: &mut Window, cx: &mut Context<Self>) {
        self.close_browser(window, cx);
    }

    /// The full-window layout, in place of the sidebar and library.
    pub(super) fn render_full_browser(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(b) = self.browser.as_ref() else { return div().into_any_element() };
        let this = cx.entity().downgrade();
        let t = cx.theme();
        let accent = crate::theme::accent(b.system.accent);
        let places = v_flex()
            .w(px(280.))
            .h_full()
            .flex_shrink_0()
            .p(px(16.))
            .pt(px(28.))
            .gap(px(6.))
            .bg(t.sidebar)
            .border_r_1()
            .border_color(t.sidebar_border)
            .child(div().px(px(12.)).pb(px(8.)).text_xs().text_color(t.muted_foreground).child("ADD FROM"))
            .children(b.places.iter().enumerate().map(|(ix, place)| {
                let focused = b.focus == Focus::Places(ix);
                place_row(ix, place, ix == b.place, focused, Metrics::FULL, accent, &this, cx)
            }));

        let header = v_flex()
            .px(px(36.))
            .pt(px(28.))
            .pb(px(18.))
            .gap(px(10.))
            .child(
                h_flex()
                    .gap(px(14.))
                    .child(super::sidebar::system_tile(b.system, px(44.), px(22.), px(10.)))
                    .child(div().flex_1().text_2xl().font_semibold().child(format!("Add ROMs to {}", b.system.display_name)))
                    .child(layout_switch(b.layout, &this)),
            )
            .child(crumbs(b, Metrics::FULL, &this, cx));

        let list = div()
            .id("browser-full-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&b.scroll)
            .px(px(28.))
            .child(v_flex().gap(px(4.)).children(file_rows(b, Metrics::FULL, accent, &this, cx)));

        let hint = |key: &'static str, label: &'static str| {
            h_flex()
                .gap(px(8.))
                .child(
                    div()
                        .px(px(8.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().sidebar)
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(key),
                )
                .child(label)
        };
        let add_focused = b.focus == Focus::Add;
        let footer = h_flex()
            .px(px(36.))
            .py(px(16.))
            .gap(px(22.))
            .border_t_1()
            .border_color(t.border)
            .text_color(t.muted_foreground)
            .child(hint("↑ ↓", "Move"))
            .child(hint("Enter", "Open or select"))
            .child(hint("Backspace", "Back"))
            .child(hint("←", "Places"))
            .child(div().flex_1())
            .child(div().text_color(t.foreground).child(b.summary()))
            .child(
                div()
                    .rounded(px(10.))
                    .border_2()
                    .border_color(if add_focused { accent } else { transparent_black() })
                    .p(px(2.))
                    .child(
                        Button::new("browser-full-add")
                            .primary()
                            .large()
                            .label(b.add_label())
                            .disabled(b.selected.is_empty())
                            .on_click(on_click(&this, |view, window, cx| view.browser_add(window, cx))),
                    ),
            );

        h_flex()
            .id("file-browser")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.browser_focus)
            .on_action(cx.listener(Self::on_move_up))
            .on_action(cx.listener(Self::on_move_down))
            .on_action(cx.listener(Self::on_activate))
            .on_action(cx.listener(Self::on_back))
            .on_action(cx.listener(Self::on_focus_places))
            .on_action(cx.listener(Self::on_focus_files))
            .on_action(cx.listener(Self::on_close))
            .size_full()
            .child(places)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(header)
                    .child(list)
                    .child(footer),
            )
            .into_any_element()
    }
}

/// Sizes that differ between the layouts.
#[derive(Clone, Copy)]
struct Metrics {
    row: f32,
    place: f32,
    text: f32,
    icon: f32,
    full: bool,
}

impl Metrics {
    const DIALOG: Metrics = Metrics { row: 36., place: 32., text: 14., icon: 16., full: false };
    const FULL: Metrics = Metrics { row: 58., place: 52., text: 18., icon: 22., full: true };
}

/// A handler for any control (a click, a checkbox's new value) that runs
/// `f` on the root view.
fn on_click<E: ?Sized + 'static>(
    this: &WeakEntity<RootView>,
    f: impl Fn(&mut RootView, &mut Window, &mut Context<RootView>) + 'static,
) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    let this = this.clone();
    move |_, window, cx| {
        this.update(cx, |view, cx| f(view, window, cx)).ok();
    }
}

/// For trying both layouts; goes once one is picked.
fn layout_switch(layout: Layout, this: &WeakEntity<RootView>) -> impl IntoElement {
    let this = this.clone();
    ButtonGroup::new("browser-layout")
        .outline()
        .small()
        .child(Button::new("browser-layout-dialog").label("Dialog").selected(layout == Layout::Dialog))
        .child(Button::new("browser-layout-full").label("Full window").selected(layout == Layout::Full))
        .on_click(move |selected: &Vec<usize>, window, cx| {
            let layout = if selected.contains(&1) { Layout::Full } else { Layout::Dialog };
            this.update(cx, |view, cx| view.set_browser_layout(layout, window, cx)).ok();
        })
}

fn crumbs(b: &Browser, m: Metrics, this: &WeakEntity<RootView>, cx: &App) -> impl IntoElement {
    let crumbs = b.crumbs();
    let last = crumbs.len().saturating_sub(1);
    h_flex()
        .gap(px(8.))
        .text_size(px(if m.full { 16. } else { 13. }))
        .text_color(cx.theme().muted_foreground)
        .child(
            Button::new("browser-up")
                .outline()
                .small()
                .icon(Icon::new(IconName::ArrowUp))
                .tooltip("Up one folder")
                .disabled(last == 0)
                .on_click(on_click(this, |view, _, cx| view.browser_up(cx))),
        )
        .child(Breadcrumb::new().children(crumbs.into_iter().enumerate().map(|(ix, (label, path))| {
            let item = BreadcrumbItem::new(label);
            if ix == last {
                item
            } else {
                item.on_click(on_click(this, move |view, _, cx| view.browser_open_crumb(path.clone(), cx)))
            }
        })))
}

#[allow(clippy::too_many_arguments)]
fn place_row(
    ix: usize,
    place: &Place,
    active: bool,
    focused: bool,
    m: Metrics,
    accent: Hsla,
    this: &WeakEntity<RootView>,
    cx: &App,
) -> impl IntoElement {
    let t = cx.theme();
    let icon = match (&place.kind, place.name.as_str()) {
        (PlaceKind::Drive { .. }, _) => IconName::HardDrive,
        (_, "Downloads") => IconName::Download,
        _ => IconName::House,
    };
    let unmounted = matches!(place.kind, PlaceKind::Drive { mount: None, .. });
    h_flex()
        .id(("browser-place", ix))
        .h(px(m.place))
        .px(px(if m.full { 12. } else { 8. }))
        .gap(px(if m.full { 14. } else { 10. }))
        .rounded(px(if m.full { 8. } else { 6. }))
        .border_2()
        .border_color(if focused { accent } else { transparent_black() })
        .text_size(px(if m.full { 17. } else { 14. }))
        .cursor_pointer()
        .text_color(if unmounted { t.muted_foreground } else { t.sidebar_foreground })
        .when(active, |row| row.bg(t.sidebar_accent).font_medium())
        .hover(|s| s.bg(t.sidebar_accent))
        .child(Icon::new(icon).with_size(px(m.icon)))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().truncate().child(place.name.clone()))
                .when(!place.detail.is_empty(), |col| {
                    col.child(div().text_xs().text_color(t.muted_foreground).truncate().child(place.detail.clone()))
                }),
        )
        .on_click(on_click(this, move |view, window, cx| view.browser_open_place(ix, window, cx)))
}

/// Rows for the current folder, or the state that stands in for them.
fn file_rows(b: &Browser, m: Metrics, accent: Hsla, this: &WeakEntity<RootView>, cx: &App) -> Vec<AnyElement> {
    let t = cx.theme();
    let message = |title: String, detail: Option<String>| {
        v_flex()
            .py(px(48.))
            .items_center()
            .gap(px(10.))
            .text_center()
            .child(div().text_size(px(m.text)).font_medium().child(title))
            .when_some(detail, |col, d| col.child(div().text_sm().text_color(t.muted_foreground).child(d)))
            .into_any_element()
    };
    if b.mounting {
        let name = b.places.get(b.place).map(|p| p.name.clone()).unwrap_or_default();
        return vec![v_flex()
            .py(px(48.))
            .items_center()
            .gap(px(12.))
            .child(Spinner::new().with_size(px(24.)))
            .child(div().text_size(px(m.text)).font_medium().child(format!("Opening {name}…")))
            .child(div().text_sm().text_color(t.muted_foreground).child("Your system may ask for your password."))
            .into_any_element()];
    }
    if let Some(error) = &b.error {
        return vec![message(error.clone(), None)];
    }
    if b.listing.entries.is_empty() {
        let detail = (b.listing.hidden > 0).then(|| {
            format!("{} other files are here. Turn on Show all files to see them.", b.listing.hidden)
        });
        return vec![message(format!("No {} files here", b.system.display_name), detail)];
    }
    let mut rows: Vec<AnyElement> = b
        .listing
        .entries
        .iter()
        .enumerate()
        .map(|(ix, entry)| {
            let checked = b.selected.contains(&entry.path);
            let focused = m.full && b.focus == Focus::Files(ix);
            file_row(ix, entry, b.system, checked, focused, m, accent, this, cx).into_any_element()
        })
        .collect();
    if b.listing.hidden > 0 && !b.show_all {
        rows.push(
            div()
                .px(px(8.))
                .py(px(10.))
                .text_sm()
                .text_color(t.muted_foreground)
                .child(format!("{} other files hidden", b.listing.hidden))
                .into_any_element(),
        );
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn file_row(
    ix: usize,
    entry: &Entry,
    system: &'static SystemDef,
    checked: bool,
    focused: bool,
    m: Metrics,
    accent: Hsla,
    this: &WeakEntity<RootView>,
    cx: &App,
) -> impl IntoElement {
    let t = cx.theme();
    let other_type = !entry.is_dir && !browse::matches(&entry.name, system.extensions);
    let meta = if entry.is_dir {
        String::new()
    } else {
        library::format_size(entry.size)
    };
    let date = entry.modified.map(browse::format_date).unwrap_or_default();
    h_flex()
        .id(("browser-file", ix))
        .h(px(m.row))
        .flex_shrink_0()
        .px(px(if m.full { 14. } else { 8. }))
        .gap(px(if m.full { 16. } else { 10. }))
        .rounded(px(if m.full { 10. } else { 6. }))
        .border_2()
        .border_color(if focused { accent } else { transparent_black() })
        .text_size(px(m.text))
        .cursor_pointer()
        .when(checked, |row| row.bg(accent.opacity(0.1)))
        .when(!checked, |row| row.hover(|s| s.bg(t.muted)))
        .child(
            div().w(px(if m.full { 26. } else { 16. })).flex_shrink_0().when(!entry.is_dir, |cell| {
                cell.child(
                    Checkbox::new(("browser-check", ix))
                        .checked(checked)
                        .when(m.full, |c| c.large())
                        .on_click(on_click(this, move |view, _, cx| view.browser_activate_entry(ix, cx))),
                )
            }),
        )
        .child(
            Icon::new(if entry.is_dir { IconName::Folder } else { IconName::File })
                .with_size(px(m.icon))
                .text_color(t.muted_foreground),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(other_type, |name| name.text_color(t.muted_foreground))
                .child(entry.name.clone()),
        )
        .when(entry.is_dir, |row| {
            row.child(Icon::new(IconName::ChevronRight).with_size(px(m.icon * 0.8)).text_color(t.muted_foreground))
        })
        .when(!entry.is_dir, |row| {
            row.child(div().w(px(if m.full { 90. } else { 72. })).text_right().text_sm().text_color(t.muted_foreground).child(meta))
                .when(!m.full, |row| {
                    row.child(div().w(px(96.)).text_right().text_sm().text_color(t.muted_foreground).child(date))
                })
        })
        .on_click(on_click(this, move |view, _, cx| view.browser_activate_entry(ix, cx)))
}

fn open_dialog(window: &mut Window, cx: &mut Context<RootView>) {
    let this = cx.entity().downgrade();
    window.open_dialog(cx, move |dialog: Dialog, _, cx| {
        let closed = this.clone();
        let dialog = dialog
            .w(px(880.))
            .p(px(16.))
            // The kit makes Enter press OK, which would close the browser and
            // drop the selection. Only the Add button adds.
            .on_ok(|_, _, _| false)
            .on_close(move |_, _, cx| {
                // Closed by the user (Esc, the X). A layout switch closes it
                // without calling this.
                closed
                    .update(cx, |view, cx| {
                        view.browser = None;
                        cx.notify();
                    })
                    .ok();
            });
        let Some(view) = this.upgrade() else { return dialog };
        let view = view.read(cx);
        let Some(b) = view.browser.as_ref() else { return dialog };
        let t = cx.theme();
        let accent = crate::theme::accent(b.system.accent);
        let m = Metrics::DIALOG;

        let places = v_flex()
            .w(px(200.))
            .flex_shrink_0()
            .p(px(8.))
            .gap(px(2.))
            .bg(t.sidebar)
            .border_r_1()
            .border_color(t.border)
            .children(b.places.iter().enumerate().map(|(ix, place)| place_row(ix, place, ix == b.place, false, m, accent, &this, cx)));

        let table_head = h_flex()
            .h(px(32.))
            .px(px(10.))
            .gap(px(10.))
            .text_xs()
            .text_color(t.muted_foreground)
            .border_b_1()
            .border_color(t.border)
            .child(div().w(px(16.)))
            .child(div().w(px(16.)))
            .child(div().flex_1().child("Name"))
            .child(div().w(px(72.)).text_right().child("Size"))
            .child(div().w(px(96.)).text_right().child("Modified"));

        let files = v_flex()
            .flex_1()
            .min_w_0()
            .child(h_flex().h(px(44.)).px(px(10.)).border_b_1().border_color(t.border).child(crumbs(b, m, &this, cx)))
            .child(table_head)
            .child(
                div()
                    .id("browser-dialog-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&b.scroll)
                    .p(px(2.))
                    .child(v_flex().children(file_rows(b, m, accent, &this, cx))),
            );

        let show_all = this.clone();
        dialog
            .title(
                h_flex()
                    .w_full()
                    .gap(px(12.))
                    .pr(px(24.))
                    .child(div().flex_1().child(format!("Add ROMs to {}", b.system.display_name)))
                    .child(layout_switch(b.layout, &this)),
            )
            .child(
                h_flex()
                    .h(px(440.))
                    .items_stretch()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t.border)
                    .overflow_hidden()
                    .child(places)
                    .child(files),
            )
            .footer(
                DialogFooter::new()
                    .w_full()
                    .child(
                        Switch::new("browser-show-all")
                            .checked(b.show_all)
                            .label("Show all files")
                            .on_click(move |checked: &bool, _, cx| {
                                let checked = *checked;
                                show_all.update(cx, |view, cx| view.browser_show_all(checked, cx)).ok();
                            }),
                    )
                    .child(
                        Button::new("browser-native")
                            .ghost()
                            .small()
                            .label("Use system picker")
                            .on_click(on_click(&this, |view, window, cx| view.browser_use_native(window, cx))),
                    )
                    .child(div().flex_1())
                    .child(div().text_sm().text_color(t.muted_foreground).child(b.summary()))
                    .child(
                        Button::new("browser-cancel")
                            .label("Cancel")
                            .on_click(on_click(&this, |view, window, cx| view.close_browser(window, cx))),
                    )
                    .child(
                        Button::new("browser-add")
                            .primary()
                            .label(b.add_label())
                            .disabled(b.selected.is_empty())
                            .on_click(on_click(&this, |view, window, cx| view.browser_add(window, cx))),
                    ),
            )
    });
}
