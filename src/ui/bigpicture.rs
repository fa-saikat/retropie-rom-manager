//! Big Picture: a TV mode laid out like the PS5 home screen, started with
//! `--big-picture`. The systems in your list run across the top, the
//! selected system's games run in one row with the focused tile anchored at
//! the left, and under it the focused game gets a large title, a wide Play
//! button and cards with its details. The focused game's cover fills the
//! background.
//!
//! It shares `RootView`'s state (systems, games, gamelist, running game), so
//! Play, Delete and Scrape do exactly what they do in the desktop window.
//! Everything is reachable with the arrow keys, Enter and Backspace, plus Q/E
//! or Page Up/Down to switch systems (L1/R1 on a gamepad mapped to keys).
//! Sizes are 1080p pixels through `scale::px`, so a 4K TV scales up.

use gpui_kit::assets::IconName;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, Sizable, StyledExt, WindowExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::details;
use super::root::RootView;
use super::scale::px;
use super::shape;
use super::sidebar::logo_plate;
use crate::assets;
use crate::library::GameEntry;
use crate::scraper::ScrapeStatus;
use crate::theme;

/// The screen's side margins, in 1080p pixels.
const EDGE: f32 = 96.;
const TILE: f32 = 160.;
const TILE_FOCUSED: f32 = 240.;
const TILE_GAP: f32 = 20.;
/// The Play button.
const PLAY_WIDTH: f32 = 440.;
const PLAY_HEIGHT: f32 = 72.;

/// Where the keys are on the Big Picture screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// The system tabs across the top.
    Tabs,
    /// The leave button, top right.
    Exit,
    /// The row of games.
    Tiles,
    /// The Play button.
    Play,
    /// The detail cards along the bottom.
    Info,
}

pub struct BigPicture {
    pub zone: Zone,
    /// Focused game, an index into the selected system's games.
    pub tile: usize,
    /// Focused detail card.
    pub info: usize,
}

impl BigPicture {
    pub fn new() -> Self {
        Self { zone: Zone::Tiles, tile: 0, info: 0 }
    }
}

/// One of the detail cards under the hero.
struct InfoCard {
    label: &'static str,
    value: String,
    /// File names and folders use the detail font.
    detail: bool,
}

/// A stand-in cover for a game without art: the system's accent, turned a
/// little per title so neighbouring tiles differ.
fn placeholder_fill(accent: Hsla, title: &str) -> Background {
    let hash = title.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let shift = ((hash % 70) as f32 - 35.) / 360.;
    let hue = |d: f32| (accent.h + shift + d).rem_euclid(1.);
    let s = accent.s.clamp(0.35, 0.75);
    linear_gradient(
        150.,
        linear_color_stop(hsla(hue(0.), s, 0.56, 1.), 0.),
        linear_color_stop(hsla(hue(30. / 360.), s, 0.28, 1.), 1.),
    )
}

/// A white ring drawn just outside a focused element. The parent needs
/// `.relative()`.
fn focus_ring(radius: Pixels) -> Div {
    let gap = px(6.);
    div()
        .absolute()
        .top(-gap)
        .left(-gap)
        .right(-gap)
        .bottom(-gap)
        .rounded(radius + gap)
        .border(px(4.))
        .border_color(white())
}

/// A controller button hint: a round key and what it does.
fn hint(key: &'static str, label: &'static str, round: bool, muted: Hsla) -> Div {
    h_flex()
        .gap(px(10.))
        .text_size(px(20.))
        .text_color(muted)
        .child(
            div()
                .h(px(32.))
                .when(round, |k| k.w(px(32.)).rounded_full())
                .when(!round, |k| k.px(px(8.)).rounded(shape::sm()))
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0., 0., 0.92, 1.))
                .text_color(hsla(0., 0., 0.07, 1.))
                .text_size(px(15.))
                .font_bold()
                .child(key),
        )
        .child(label)
}

impl RootView {
    pub(super) fn enter_big_picture(&mut self) {
        self.big_picture = Some(BigPicture::new());
    }

    fn bp(&self) -> &BigPicture {
        self.big_picture.as_ref().expect("Big Picture is on")
    }

    fn bp_mut(&mut self) -> &mut BigPicture {
        self.big_picture.as_mut().expect("Big Picture is on")
    }

    /// The focused game, if the system has any.
    fn bp_game(&self) -> Option<&GameEntry> {
        self.games.get(self.bp().tile.min(self.games.len().saturating_sub(1)))
    }

    fn bp_tab(&self) -> usize {
        self.enabled.iter().position(|s| s.id == self.selected.id).unwrap_or(0)
    }

    fn bp_info_cards(&self) -> Vec<InfoCard> {
        let Some(entry) = self.bp_game() else { return Vec::new() };
        let meta = match self.status_of(entry) {
            ScrapeStatus::Scraped(m) => Some(m),
            _ => None,
        };
        let about = meta
            .and_then(|m| m.desc.clone())
            .unwrap_or_else(|| "Not scraped yet. Press S to fetch box art, a description and release details with Skyscraper.".into());
        let released = meta
            .map(|m| [m.year.clone(), m.genre.clone(), m.developer.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · "))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Unknown".into());
        let file = entry
            .files
            .first()
            .and_then(|f| f.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        vec![
            InfoCard { label: "About", value: about, detail: false },
            InfoCard { label: "Released", value: released, detail: false },
            InfoCard { label: if entry.files.len() > 1 { "Files" } else { "File" }, value: file, detail: true },
            InfoCard { label: "Folder", value: format!("~/RetroPie/roms/{}", self.selected.folder), detail: true },
        ]
    }

    // ── Keys ────────────────────────────────────────────────────────────

    pub(super) fn bp_switch(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.enabled.is_empty() {
            return;
        }
        let n = self.enabled.len() as isize;
        let next = (self.bp_tab() as isize + delta).rem_euclid(n) as usize;
        let system = self.enabled[next];
        self.select_system(system, window, cx);
        let bp = self.bp_mut();
        bp.tile = 0;
        bp.info = 0;
        if matches!(bp.zone, Zone::Play | Zone::Info) {
            bp.zone = Zone::Tiles;
        }
        cx.notify();
    }

    pub(super) fn bp_move(&mut self, dx: isize, dy: isize, window: &mut Window, cx: &mut Context<Self>) {
        let games = self.games.len();
        let has_info = games > 0;
        let zone = self.bp().zone;
        if dx != 0 {
            match zone {
                Zone::Tabs => {
                    if dx > 0 && self.bp_tab() + 1 == self.enabled.len() {
                        self.bp_mut().zone = Zone::Exit;
                    } else if !(dx < 0 && self.bp_tab() == 0) {
                        self.bp_switch(dx, window, cx);
                        self.bp_mut().zone = Zone::Tabs;
                    }
                }
                Zone::Exit => {
                    if dx < 0 {
                        self.bp_mut().zone = Zone::Tabs;
                    }
                }
                Zone::Tiles => {
                    let bp = self.bp_mut();
                    bp.tile = (bp.tile as isize + dx).clamp(0, games.saturating_sub(1) as isize) as usize;
                }
                Zone::Play => {}
                Zone::Info => {
                    let cards = self.bp_info_cards().len();
                    let bp = self.bp_mut();
                    bp.info = (bp.info as isize + dx).clamp(0, cards.saturating_sub(1) as isize) as usize;
                }
            }
        }
        if dy != 0 {
            let order: Vec<Zone> = [Zone::Tabs, Zone::Tiles, Zone::Play, Zone::Info]
                .into_iter()
                .filter(|z| has_info || matches!(z, Zone::Tabs))
                .collect();
            let at = order.iter().position(|z| *z == if zone == Zone::Exit { Zone::Tabs } else { zone }).unwrap_or(0);
            let next = (at as isize + dy).clamp(0, order.len() as isize - 1) as usize;
            let bp = self.bp_mut();
            if order[next] != zone {
                bp.zone = order[next];
                bp.info = 0;
            }
        }
        cx.notify();
    }

    pub(super) fn bp_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.bp().zone {
            Zone::Tabs => {
                self.bp_mut().zone = Zone::Tiles;
                cx.notify();
            }
            Zone::Exit => self.request_quit(window, cx),
            Zone::Tiles | Zone::Play => self.bp_play(window, cx),
            Zone::Info => {}
        }
    }

    pub(super) fn bp_back(&mut self, cx: &mut Context<Self>) {
        let bp = self.bp_mut();
        bp.zone = match bp.zone {
            Zone::Info | Zone::Play => Zone::Tiles,
            _ => Zone::Tabs,
        };
        cx.notify();
    }

    pub(super) fn bp_play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.bp_game().cloned() else { return };
        if self.running.is_some() {
            return;
        }
        if !self.can_run() {
            window.push_notification(
                Notification::warning(format!("Install the {} emulator with RetroPie-Setup first.", self.selected.display_name))
                    .title("No emulator for this system"),
                cx,
            );
            return;
        }
        if self.misplaced_as(&entry).is_some() {
            return;
        }
        self.run_game(&entry, window, cx);
    }

    pub(super) fn bp_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.bp_game().cloned() {
            details::confirm_delete_dialog(self, entry, window, cx);
        }
    }

    pub(super) fn bp_scrape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.bp_game().cloned() {
            self.scrape_key(&entry, window, cx);
        }
    }

    // ── Drawing ─────────────────────────────────────────────────────────

    pub(super) fn render_big_picture(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let t = cx.theme();
        let bg = t.background;
        let (fg, muted) = (t.foreground, t.muted_foreground);
        let accent = theme::accent(self.selected.accent);
        let bp = self.bp();
        let game = self.bp_game();
        let cover = |e: &GameEntry| match self.status_of(e) {
            ScrapeStatus::Scraped(m) => m.cover.clone().or(m.screenshot.clone()).filter(|p| p.is_file()),
            _ => None,
        };

        // Background: the cover, dimmed, or a glow in the system's colour
        // with the game's initials (or the system's name) as a watermark.
        let backdrop = div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .bg(linear_gradient(
                120.,
                linear_color_stop(hsla(accent.h, (accent.s * 0.6).min(0.6), 0.10, 1.), 0.),
                linear_color_stop(hsla(accent.h, (accent.s * 0.8).min(0.7), 0.30, 1.), 1.),
            ))
            .map(|d| match game.and_then(cover) {
                Some(path) => d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .opacity(0.38)
                        .child(img(path).size_full().object_fit(ObjectFit::Cover)),
                ),
                None => d.child(
                    div()
                        .absolute()
                        .right(px(110.))
                        .top(px(150.))
                        .text_size(px(if game.is_some() { 480. } else { 340. }))
                        .font_bold()
                        .text_color(white().opacity(0.07))
                        .child(game.map(|g| g.monogram()).unwrap_or_else(|| self.selected.short_name.to_string())),
                ),
            })
            // Scrims, so text on the left and bottom always reads.
            .child(div().absolute().inset_0().bg(linear_gradient(
                90.,
                linear_color_stop(bg.opacity(0.97), 0.),
                linear_color_stop(bg.opacity(0.), 0.75),
            )))
            .child(div().absolute().inset_0().bg(linear_gradient(
                0.,
                linear_color_stop(bg.opacity(0.98), 0.),
                linear_color_stop(bg.opacity(0.), 0.5),
            )));

        // Top bar: L1, the system tabs, R1, and the leave button.
        let bumper = |label: &'static str| {
            div()
                .px(px(8.))
                .py(px(3.))
                .rounded(shape::sm())
                .border_1()
                .border_color(white().opacity(0.2))
                .text_size(px(14.))
                .font_semibold()
                .text_color(t.muted_foreground)
                .child(label)
        };
        let tabs = h_flex().gap(px(4.)).children(self.enabled.iter().enumerate().map(|(i, system)| {
            let current = system.id == self.selected.id;
            let focused = current && bp.zone == Zone::Tabs;
            let system = *system;
            h_flex()
                .id(("bp-tab", i))
                .relative()
                .h(px(48.))
                .px(px(20.))
                .gap(px(12.))
                .rounded(shape::sm())
                .text_size(px(22.))
                .cursor_pointer()
                .when(current, |tab| tab.bg(white().opacity(0.08)).font_semibold().text_color(t.foreground))
                .when(!current, |tab| tab.font_medium().text_color(t.muted_foreground))
                .child(div().size(px(10.)).rounded_full().bg(theme::accent(system.accent)))
                .child(system.short_name)
                .when(focused, |tab| tab.child(focus_ring(shape::sm())))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_system(system, window, cx);
                    let bp = this.bp_mut();
                    bp.tile = 0;
                    bp.zone = Zone::Tiles;
                }))
        }));
        let exit = div()
            .id("bp-exit")
            .relative()
            .size(px(48.))
            .rounded(shape::sm())
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(if bp.zone == Zone::Exit { t.foreground } else { t.muted_foreground })
            .child(Icon::empty().path(assets::QUIT_ICON).with_size(px(28.)))
            .when(bp.zone == Zone::Exit, |b| b.child(focus_ring(shape::sm())))
            .on_click(cx.listener(|this, _, window, cx| this.request_quit(window, cx)));
        let top = h_flex()
            .h(px(56.))
            .gap(px(8.))
            .child(bumper("L1"))
            .child(tabs)
            .child(bumper("R1"))
            .child(div().flex_1())
            .child(exit);

        let content: AnyElement = match game {
            Some(_) => self.bp_library(window, cx).into_any_element(),
            None => self.bp_empty(cx).into_any_element(),
        };

        // Controller hints, bottom right.
        let hints = h_flex()
            .gap(px(28.))
            .child(hint("A", "Select", true, muted))
            .child(hint("B", "Back", true, muted))
            .when(game.is_some(), |h| h.child(hint("X", "Delete", true, muted)).child(hint("Y", "Scrape", true, muted)))
            .child(hint("L1 R1", "Systems", false, muted));

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(bg)
            .text_color(fg)
            .child(backdrop)
            .child(
                v_flex()
                    .absolute()
                    .inset_0()
                    .px(px(EDGE))
                    .pt(px(48.))
                    .pb(px(36.))
                    .child(top)
                    .child(div().flex_1().min_h_0().child(content))
                    .child(h_flex().justify_end().child(hints)),
            )
    }

    /// The game row, the hero with Play, and the detail cards.
    fn bp_library(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let t = cx.theme();
        let bp = self.bp();
        let accent = theme::accent(self.selected.accent);
        let focus = bp.tile.min(self.games.len().saturating_sub(1));
        let entry = &self.games[focus];
        let in_tiles = bp.zone == Zone::Tiles;

        // Only the focused tile and the ones after it: the row scrolls so
        // the focused tile sits at the left edge, like the PS5.
        let width = window.viewport_size().width.as_f32() / super::scale::factor();
        let fit = ((width - 2. * EDGE - TILE_FOCUSED - 560.) / (TILE + TILE_GAP)).ceil().max(1.) as usize + 1;
        let tiles = h_flex().gap(px(TILE_GAP)).items_start().children(
            self.games.iter().enumerate().skip(focus).take(fit + 1).map(|(i, game)| {
                let focused = i == focus;
                let size = px(if focused { TILE_FOCUSED } else { TILE });
                let (title, _) = game.title_and_tags();
                let status = self.status_of(game);
                let cover = match status {
                    ScrapeStatus::Scraped(m) => m.cover.clone().or(m.screenshot.clone()).filter(|p| p.is_file()),
                    _ => None,
                };
                let tile = div()
                    .id(("bp-tile", i))
                    .relative()
                    .flex_shrink_0()
                    .size(size)
                    .rounded(shape::md())
                    .when(!focused && !in_tiles, |t| t.opacity(0.55))
                    .cursor_pointer()
                    .child(
                        div()
                            .size_full()
                            .rounded(shape::md())
                            .overflow_hidden()
                            .map(|d| match cover {
                                Some(path) => d.bg(black()).child(img(path).size_full().object_fit(ObjectFit::Cover)),
                                None => d
                                    .bg(placeholder_fill(accent, &title))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(if focused { 76. } else { 52. }))
                                    .font_bold()
                                    .text_color(white().opacity(0.92))
                                    .child(game.monogram()),
                            }),
                    )
                    .when(!matches!(status, ScrapeStatus::Scraped(_)), |tile| {
                        tile.child(
                            div()
                                .absolute()
                                .left(px(10.))
                                .top(px(10.))
                                .px(px(7.))
                                .py(px(3.))
                                .rounded(shape::sm())
                                .bg(hsla(45. / 360., 0.9, 0.82, 1.))
                                .text_color(hsla(35. / 360., 0.6, 0.22, 1.))
                                .text_size(px(13.))
                                .font_semibold()
                                .child("No art"),
                        )
                    })
                    .when(focused && in_tiles, |tile| tile.child(focus_ring(shape::md())))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.bp().tile == i {
                            this.bp_play(window, cx);
                        } else {
                            let bp = this.bp_mut();
                            bp.tile = i;
                            bp.zone = Zone::Tiles;
                            cx.notify();
                        }
                    }));
                let label = focused.then(|| {
                    let sub = match status {
                        ScrapeStatus::Scraped(m) => {
                            [m.year.clone(), m.genre.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ")
                        }
                        _ => "Not scraped".into(),
                    };
                    v_flex()
                        .self_end()
                        .max_w(px(520.))
                        .pb(px(6.))
                        .pr(px(12.))
                        .child(div().truncate().text_size(px(30.)).font_semibold().child(title.clone()))
                        .child(div().text_size(px(18.)).text_color(t.muted_foreground).mt(px(6.)).child(sub))
                });
                h_flex().gap(px(TILE_GAP)).child(tile).children(label).into_any_element()
            }),
        );

        // The hero: title, chips, and Play.
        let (title, _) = entry.title_and_tags();
        let status = self.status_of(entry);
        let chip = |text: String, detail: bool| {
            div()
                .h(px(34.))
                .px(px(14.))
                .flex()
                .items_center()
                .rounded(shape::sm())
                .bg(white().opacity(0.1))
                .text_size(px(18.))
                .font_medium()
                .when(detail, |c| c.font_family(t.mono_font_family.clone()).font_normal())
                .child(text)
        };
        let ext = entry
            .files
            .first()
            .and_then(|f| f.extension())
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .unwrap_or_default();
        let meta = h_flex()
            .gap(px(10.))
            .map(|row| match status {
                ScrapeStatus::Scraped(m) => row
                    .when_some(m.year.clone(), |r, y| r.child(chip(y, false)))
                    .when_some(m.genre.clone(), |r, g| r.child(chip(g, false)))
                    .when_some(m.rating, |r, rating| {
                        let stars = (rating * 5.).round().clamp(0., 5.) as usize;
                        r.child(
                            div()
                                .text_size(px(22.))
                                .text_color(t.primary)
                                .child("★".repeat(stars) + &"☆".repeat(5 - stars)),
                        )
                    }),
                _ => row.child(
                    chip("No art yet".into(), false).bg(hsla(45. / 360., 0.9, 0.82, 1.)).text_color(hsla(35. / 360., 0.6, 0.22, 1.)),
                ),
            })
            .child(chip(ext, true));

        let can_play = self.can_run() && self.misplaced_as(entry).is_none();
        let play_focused = bp.zone == Zone::Play;
        let play = h_flex()
            .id("bp-play")
            .relative()
            .w(px(PLAY_WIDTH))
            .h(px(PLAY_HEIGHT))
            .gap(px(14.))
            .justify_center()
            .rounded(shape::sm())
            .cursor_pointer()
            .text_size(px(26.))
            .font_semibold()
            .map(|b| {
                if can_play {
                    b.bg(t.primary).text_color(t.primary_foreground)
                } else {
                    b.bg(white().opacity(0.1)).text_color(t.muted_foreground)
                }
            })
            .when(self.running.is_some(), |b| b.opacity(0.6))
            .child(Icon::new(IconName::Play).with_size(px(26.)))
            .child(match (&self.running, can_play) {
                (Some(_), _) => "Playing…".to_string(),
                (None, true) => "Play".to_string(),
                (None, false) => "No emulator installed".to_string(),
            })
            .when(play_focused, |b| b.child(focus_ring(shape::sm())))
            .on_click(cx.listener(|this, _, window, cx| this.bp_play(window, cx)));

        let hero = v_flex()
            .gap(px(20.))
            .max_w(px(1000.))
            .child(div().text_size(px(64.)).line_height(px(68.)).font_bold().line_clamp(2).child(title))
            .child(meta)
            .child(div().mt(px(10.)).child(play));

        // Detail cards.
        let cards = h_flex().gap(px(20.)).children(self.bp_info_cards().into_iter().enumerate().map(|(i, card)| {
            let focused = bp.zone == Zone::Info && i == bp.info;
            v_flex()
                .relative()
                .flex_1()
                .min_w_0()
                .h(px(150.))
                .px(px(24.))
                .py(px(20.))
                .gap(px(8.))
                .rounded(shape::md())
                .bg(white().opacity(if focused { 0.12 } else { 0.07 }))
                .child(
                    div()
                        .text_size(px(16.))
                        .font_semibold()
                        .text_color(t.muted_foreground)
                        .child(card.label.to_uppercase()),
                )
                .child(
                    div()
                        .text_size(px(if card.detail { 20. } else { 22. }))
                        .line_height(px(30.))
                        .line_clamp(2)
                        .when(card.detail, |v| v.font_family(t.mono_font_family.clone()))
                        .child(card.value),
                )
                .when(focused, |c| c.child(focus_ring(shape::md())))
        }));

        v_flex()
            .size_full()
            .child(
                div()
                    .mt(px(40.))
                    .text_size(px(20.))
                    .font_medium()
                    .text_color(t.muted_foreground)
                    .child(format!(
                        "{} · {} {}",
                        self.selected.display_name,
                        self.games.len(),
                        if self.games.len() == 1 { "game" } else { "games" }
                    )),
            )
            .child(div().mt(px(16.)).h(px(TILE_FOCUSED)).child(tiles))
            .child(div().mt(px(64.)).child(hero))
            .child(div().flex_1())
            .child(div().mb(px(24.)).child(cards))
    }

    /// A system with no games: its logo and where ROMs go.
    fn bp_empty(&self, cx: &App) -> Div {
        let t = cx.theme();
        let system = self.selected;
        v_flex()
            .size_full()
            .justify_center()
            .gap(px(20.))
            .children(logo_plate(system, size(px(320.), px(110.)), shape::md(), cx))
            .child(
                div()
                    .text_size(px(64.))
                    .font_bold()
                    .child(format!("No {} games yet", system.short_name)),
            )
            .child(div().text_size(px(24.)).text_color(t.muted_foreground).child(format!(
                "Add ROMs from the desktop window, or copy them into ~/RetroPie/roms/{}.",
                system.folder
            )))
    }
}
