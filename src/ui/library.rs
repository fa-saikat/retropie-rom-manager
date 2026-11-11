//! Main column: system header, notices, toolbar and the game grid / list.

use gpui_kit::assets::IconName;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants};
use gpui_kit::component::empty::{
    Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle,
};
use gpui_kit::component::input::Input;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::rating::Rating;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable, Icon, Selectable, Sizable, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::details;
use super::nav::{self, HeaderItem, LibItem};
use super::root::{Page, RootView, ViewMode};
use super::scale::{self, px};
use super::shape;
use super::sidebar::{logo_plate, system_glyph, system_tile};
use crate::covers;
use crate::library::GameEntry;
use crate::scraper::ScrapeStatus;
use crate::systems::SystemDef;
use crate::theme;

const SIDEBAR_WIDTH: f32 = 240.;
pub(super) const CONTENT_PADDING: f32 = 24.;
const CARD_MIN_WIDTH: f32 = 236.;
pub(super) const GRID_GAP: f32 = 16.;
/// The card's play button, on the art.
const PLAY_SIZE: f32 = 64.;

/// Rounded 0–1 rating to whole stars, as the kit's `Rating` draws them.
fn stars(rating: f32) -> usize {
    (rating * 5.0).round().clamp(0.0, 5.0) as usize
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// Initials on a neutral tile: the stand-in for missing, loading or
/// unreadable box art.
fn initials_tile(bg: Hsla, fg: Hsla, initials: String) -> Div {
    div()
        .size_full()
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(56.))
        .font_bold()
        .text_color(fg)
        .child(initials)
}

/// "Looks like a GBA game" chip for a game sitting in the wrong system.
fn wrong_system_tag(to: &'static SystemDef) -> Tag {
    shape::small_tag(Tag::danger()).child(format!("{} game?", to.short_name))
}

/// Width left for a grid next to the sidebar, in 1080p pixels.
fn grid_width(window: &Window) -> f32 {
    let width = window.viewport_size().width.as_f32() / super::scale::factor();
    width - SIDEBAR_WIDTH - 2. * CONTENT_PADDING
}

/// As many columns of at least `min_width` as fit, in 1080p pixels.
pub(super) fn columns_for(window: &Window, min_width: f32) -> u16 {
    ((grid_width(window) + GRID_GAP) / (min_width + GRID_GAP)).floor().max(1.) as u16
}

/// As many ≥236px columns as fit next to the sidebar.
pub(super) fn grid_columns(window: &Window) -> u16 {
    columns_for(window, CARD_MIN_WIDTH)
}

/// One grid column's width in 1080p pixels: the art is square, so this is
/// its height too.
pub(super) fn column_width(window: &Window, columns: u16) -> f32 {
    (grid_width(window) - GRID_GAP * (columns as f32 - 1.)) / columns as f32
}

/// What the notice's button does, when it has one.
#[derive(Clone)]
pub(super) enum NoticeAction {
    /// One game moves straight away (nothing is lost).
    MoveOne(GameEntry, &'static SystemDef),
    /// Several get a confirmation listing where each goes.
    MoveAll,
    InstallSkyscraper,
    ScrapeMissing,
}

/// First genre only — ScreenScraper often sends "Shooter / Run and gun".
fn short_genre(genre: &str) -> &str {
    genre.split(['/', ',']).next().unwrap_or(genre).trim()
}

impl RootView {
    pub(super) fn render_main(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .drag_over::<ExternalPaths>(|style, _, _, cx| style.bg(cx.theme().drop_target))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.install_paths(paths.paths().to_vec(), window, cx)
            }))
            .child(
                div()
                    .id("main-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.nav.scroll)
                    .vertical_scrollbar(&self.nav.scroll)
                    .child({
                        // Switching system or page fades the column in and
                        // lifts it a few pixels into place.
                        let key = match self.page {
                            Page::System => format!("page-system-{}", self.selected.id),
                            Page::Emulators => "page-emulators".to_string(),
                        };
                        match self.page {
                            Page::System => v_flex()
                                .p(px(CONTENT_PADDING))
                                .gap_5()
                                .child(self.render_header(window.is_fullscreen(), cx))
                                .children(self.render_notice(window.is_fullscreen(), cx))
                                .child(self.render_toolbar(window.is_fullscreen(), cx))
                                .child(self.render_library(window, cx)),
                            Page::Emulators => self.render_emulators(window, cx),
                        }
                        .relative()
                        .with_animation(SharedString::from(key), super::motion::enter(), |c, d| {
                            c.opacity(d).top(px(12. * (1. - d)))
                        })
                    }),
            )
    }

    fn render_header(&self, fullscreen: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let system = self.selected;
        let file_count: usize = self.games.iter().map(|g| g.files.len()).sum();
        let scraping = self.scrape.as_ref().filter(|job| job.system.id == system.id);

        let chip = |body: AnyElement| {
            h_flex()
                .h(px(28.))
                .px(px(12.))
                .gap_1p5()
                .rounded(shape::sm())
                .bg(t.secondary)
                .text_xs()
                .font_medium()
                .text_color(t.muted_foreground)
                .child(body)
        };
        let chips = h_flex()
            .flex_wrap()
            .gap_1p5()
            .child(chip(plural(self.games.len(), "game").into_any_element()))
            .child(chip(plural(file_count, "file").into_any_element()))
            .child(chip(
                h_flex()
                    .gap_1p5()
                    .child(Icon::new(IconName::Folder).xsmall())
                    .child(format!("roms/{}", system.folder))
                    .into_any_element(),
            ));

        let progress = scraping.map(|job| {
            v_flex()
                .w(px(380.))
                .gap_1p5()
                .child(
                    h_flex()
                        .justify_between()
                        .text_xs()
                        .text_color(t.muted_foreground)
                        .child("Scraping with Skyscraper")
                        .child(div().text_color(t.foreground).child(format!("{} / {}", job.done, job.total))),
                )
                .child(Progress::new("scrape-progress").value(job.done as f32 / job.total as f32 * 100.))
        });

        let folder = self.system_folder();
        let focused = self.focused_header(fullscreen);
        let ring = |item: HeaderItem, button: Button| nav::ringed(button, focused == Some(item), t.radius, t.ring);
        // Logo and title on one row with the actions at the right; the
        // description, chips and progress below, from the logo's left edge.
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_5()
                    .items_center()
                    .child(
                        logo_plate(system, size(px(200.), px(72.)), shape::md(), cx)
                            .unwrap_or_else(|| system_tile(system, px(56.), px(28.), shape::md())),
                    )
                    .child(div().flex_1().min_w_0().truncate().text_2xl().font_medium().child(system.display_name))
                    .child(
                        h_flex()
                            .gap_2()
                            .when(!fullscreen, |buttons| {
                                buttons.child(ring(
                                    HeaderItem::Folder,
                                    Button::new("open-system-folder")
                                        .outline()
                                        .icon(Icon::new(IconName::FolderOpen))
                                        .tooltip("Open in file manager")
                                        .on_click(cx.listener(move |this, _, _, cx| this.open_folder(folder.clone(), cx))),
                                ))
                            })
                            .child(ring(
                                HeaderItem::Scrape,
                                Button::new("scrape-all")
                                    .outline()
                                    .icon(Icon::new(IconName::ImageDown))
                                    .label(if scraping.is_some() { "Scraping…" } else { "Scrape" })
                                    .loading(self.scrape.is_some())
                                    .disabled(!self.skyscraper_installed || self.games.is_empty())
                                    .tooltip("Fetch box art & metadata for every game")
                                    .on_click(cx.listener(|this, _, window, cx| this.start_scrape(Vec::new(), window, cx))),
                            ))
                            .child(ring(
                                HeaderItem::AddRom,
                                Button::new("add-rom")
                                    .primary()
                                    .icon(Icon::new(IconName::Plus))
                                    .label("Add ROM")
                                    .on_click(cx.listener(|this, _, window, cx| this.pick_and_install(window, cx))),
                            )),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(t.muted_foreground)
                    .child("Drop files anywhere in the window, or pick them from disk. Zips are extracted automatically."),
            )
            .child(chips)
            .children(progress)
    }

    /// Icon tile + bold lead + muted sentence + optional action, tinted with
    /// `tone`. Shared by the notices that need a button (the kit's `Alert`
    /// has no action slot).
    fn notice_row(
        tone: Hsla,
        icon: impl Into<Icon>,
        lead: String,
        body: impl IntoElement,
        action: Option<AnyElement>,
        cx: &App,
    ) -> AnyElement {
        let t = cx.theme();
        h_flex()
            .gap_3()
            .px_4()
            .py_3()
            .rounded(t.radius_lg)
            .border_1()
            .border_color(tone.opacity(0.35))
            .bg(tone.opacity(0.08))
            .child(
                div()
                    .size(px(28.))
                    .flex_shrink_0()
                    .rounded(shape::sm())
                    .bg(tone)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon.into().small().text_color(white())),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .child(div().font_semibold().child(lead))
                    .child(div().text_color(t.muted_foreground).child(body)),
            )
            .children(action)
            .into_any_element()
    }

    /// The notice's button, if the notice showing has one. Follows the same
    /// order as `render_notice`.
    pub(super) fn notice_action(&self) -> Option<NoticeAction> {
        if self.setup.is_some() || self.running.is_some() {
            return None;
        }
        match self.misplaced_games().as_slice() {
            [] => {}
            [m] => return Some(NoticeAction::MoveOne(m.entry.clone(), m.to)),
            _ => return Some(NoticeAction::MoveAll),
        }
        if !self.skyscraper_installed {
            return self.can_install_skyscraper.then_some(NoticeAction::InstallSkyscraper);
        }
        if self.es_running || self.scrape.is_some() || self.unscraped_games().is_empty() {
            return None;
        }
        Some(NoticeAction::ScrapeMissing)
    }

    pub(super) fn run_notice_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.notice_action() {
            Some(NoticeAction::MoveOne(entry, to)) => self.move_game(entry, to, window, cx),
            Some(NoticeAction::MoveAll) => details::confirm_move_misplaced(self, window, cx),
            Some(NoticeAction::InstallSkyscraper) => details::confirm_install_skyscraper(window, cx),
            Some(NoticeAction::ScrapeMissing) => self.scrape_unscraped(window, cx),
            None => {}
        }
    }

    /// At most one notice, most important first.
    fn render_notice(&self, fullscreen: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = cx.theme();
        let focused = self.focused_header(fullscreen) == Some(HeaderItem::Notice);
        let ring = |button: Button| Some(nav::ringed(button, focused, t.radius, t.ring).into_any_element());
        let run = || cx.listener(|this: &mut RootView, _: &ClickEvent, window, cx| this.run_notice_action(window, cx));
        if let Some(job) = &self.setup {
            let secs = job.started.elapsed().as_secs();
            let body = h_flex()
                .gap_2()
                .child(Spinner::new().small())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(t.mono_font_family.clone())
                        .text_xs()
                        .child(job.line.clone()),
                );
            return Some(Self::notice_row(
                theme::accent(self.selected.accent),
                Icon::new(IconName::Download),
                format!(
                    "Installing Skyscraper · {}:{:02}  — this can take 10–20 minutes on a Raspberry Pi",
                    secs / 60,
                    secs % 60
                ),
                body,
                None,
                cx,
            ));
        }
        if let Some(game) = &self.running {
            return Some(Self::notice_row(
                theme::accent(self.selected.accent),
                Icon::new(IconName::Play),
                format!("Playing {}", game.title),
                format!("Running in {}. Quit the emulator to come back here.", game.emulator),
                None,
                cx,
            ));
        }
        // A game in the wrong folder won't even start, so this outranks
        // anything about artwork.
        let misplaced = self.misplaced_games();
        if !misplaced.is_empty() {
            let system = self.selected;
            let one = misplaced.len() == 1;
            let new = misplaced.iter().filter(|m| !self.is_enabled(m.to)).count();
            let (lead, label) = match misplaced.as_slice() {
                [m] => (
                    format!("“{}” looks like a {} game", m.entry.title_and_tags().0, m.to.display_name),
                    if new > 0 { format!("Add {} & move", m.to.short_name) } else { format!("Move to {}", m.to.short_name) },
                ),
                _ => (
                    format!("{} look like they're for other systems", plural(misplaced.len(), "game")),
                    format!("Move {}", misplaced.len()),
                ),
            };
            let body = match misplaced.as_slice() {
                [m] if new > 0 => format!(
                    "It won't run with the {} emulator. {} isn't in your list yet: add it and the game moves there, or delete the game.",
                    system.display_name, m.to.display_name
                ),
                _ => format!(
                    "EmulationStation starts everything in roms/{} with the {} emulator, so {}. Move {} where {}{}, or delete {}.",
                    system.folder,
                    system.display_name,
                    if one { "it won't run" } else { "they won't run" },
                    if one { "it" } else { "them" },
                    if one { "it belongs" } else { "they belong" },
                    if new > 0 { " (systems you don't have yet are added)" } else { "" },
                    if one { "it" } else { "them" },
                ),
            };
            return Some(Self::notice_row(
                t.danger,
                Icon::new(IconName::TriangleAlert),
                lead,
                body,
                ring(
                    Button::new("move-misplaced")
                        .primary()
                        .small()
                        .icon(Icon::new(IconName::FolderInput))
                        .label(label)
                        .on_click(run()),
                ),
                cx,
            ));
        }
        if !self.skyscraper_installed {
            if self.can_install_skyscraper {
                return Some(Self::notice_row(
                    t.warning,
                    Icon::new(IconName::ImageDown),
                    "Box art needs Skyscraper".into(),
                    "RetroPie's scraper isn't installed yet. Install it here with RetroPie-Setup — no API keys needed.",
                    ring(
                        Button::new("install-skyscraper")
                            .primary()
                            .small()
                            .icon(Icon::new(IconName::Download))
                            .label("Install Skyscraper")
                            .on_click(run()),
                    ),
                    cx,
                ));
            }
            return Some(
                Alert::warning(
                    "skyscraper-missing",
                    format!(
                        "RetroPie-Setup wasn't found in your home folder, so the app can't install it for you. On a RetroPie system, run `{}`.",
                        crate::skyscraper_setup::manual_command()
                    ),
                )
                .title("Box art needs Skyscraper")
                .into_any_element(),
            );
        }
        if self.es_running {
            return Some(
                Alert::warning(
                    "es-running",
                    "It rewrites gamelist.xml when it quits, so new artwork is cached now and applied after you exit it.",
                )
                .title("EmulationStation is running")
                .into_any_element(),
            );
        }
        let missing = self.unscraped_games().len();
        if missing == 0 || self.scrape.is_some() {
            return None;
        }
        Some(Self::notice_row(
            theme::accent(self.selected.accent),
            Icon::new(IconName::ImageDown),
            format!("{} without artwork", plural(missing, "game")),
            "Scrape them to get box art and descriptions here and in EmulationStation.",
            ring(
                Button::new("scrape-missing")
                    .primary()
                    .small()
                    .label(format!("Scrape {missing}"))
                    .on_click(run()),
            ),
            cx,
        ))
    }

    fn render_toolbar(&self, fullscreen: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focused_header(fullscreen);
        let (muted, radius, ring) = (cx.theme().muted_foreground, cx.theme().radius, cx.theme().ring);
        let search = self.search_keys(
            nav::ringed(
                Input::new(&self.search)
                    .w(px(240.))
                    .prefix(Icon::new(IconName::Search).small().text_color(muted))
                    .when_some(Keystroke::parse("/").ok(), |input, key| input.suffix(Kbd::new(key)))
                    .cleanable(true),
                focused == Some(HeaderItem::Search),
                radius,
                ring,
            ),
            cx,
        );
        let t = cx.theme();
        let count = if self.query.trim().is_empty() {
            self.games.len().to_string()
        } else {
            format!("{} of {}", self.visible_games().len(), self.games.len())
        };
        h_flex()
            .gap_2()
            .child(div().text_base().font_semibold().child("Library"))
            .when(!self.games.is_empty(), |el| el.child(div().text_color(t.muted_foreground).child(count)))
            .child(div().flex_1())
            .child(search)
            .child(nav::ringed(
                ButtonGroup::new("view-mode")
                    .outline()
                    .child(
                        Button::new("view-grid")
                            .icon(Icon::new(IconName::LayoutGrid))
                            .tooltip("Grid")
                            .selected(self.view_mode == ViewMode::Grid),
                    )
                    .child(
                        Button::new("view-list")
                            .icon(Icon::new(IconName::List))
                            .tooltip("List")
                            .selected(self.view_mode == ViewMode::List),
                    )
                    .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                        let mode = if selected.contains(&1) { ViewMode::List } else { ViewMode::Grid };
                        this.set_view_mode(mode, cx);
                    })),
                focused == Some(HeaderItem::View),
                t.radius,
                t.ring,
            ))
    }

    fn render_library(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.games.is_empty() {
            return self.render_empty(cx).into_any_element();
        }
        let games = self.visible_games();
        if games.is_empty() {
            return Empty::new()
                .header(
                    EmptyHeader::new()
                        .media(EmptyMedia::new().child(Icon::new(IconName::Search)))
                        .title(EmptyTitle::new().child("No matches"))
                        .description(EmptyDescription::new().child(format!(
                            "Nothing in {} matches “{}”.",
                            self.selected.display_name,
                            self.query.trim()
                        ))),
                )
                .into_any_element();
        }
        match self.view_mode {
            ViewMode::Grid => self.render_grid(&games, window, cx).into_any_element(),
            ViewMode::List => self.render_list(&games, cx).into_any_element(),
        }
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let system = self.selected;
        Empty::new()
            .header(
                EmptyHeader::new()
                    .media(EmptyMedia::new().child(
                        logo_plate(system, size(px(260.), px(96.)), shape::md(), cx)
                            .unwrap_or_else(|| system_tile(system, px(56.), px(28.), shape::md())),
                    ))
                    .title(EmptyTitle::new().child(format!("No {} games yet", system.display_name)))
                    .description(EmptyDescription::new().child(format!(
                        "Add a ROM and it lands in ~/RetroPie/roms/{}, ready for EmulationStation. Multi-file games show up as one entry.",
                        system.folder
                    ))),
            )
            .content(
                EmptyContent::new()
                    .child(nav::ringed(
                        Button::new("add-first")
                            .primary()
                            .icon(Icon::new(IconName::Plus))
                            .label("Add your first ROM")
                            .on_click(cx.listener(|this, _, window, cx| this.pick_and_install(window, cx))),
                        self.focused_library() == Some(LibItem::AddFirst),
                        t.radius,
                        t.ring,
                    ))
                    .child(
                        h_flex()
                            .mt_2()
                            .gap_1()
                            .justify_center()
                            .children(system.extensions.iter().map(|ext| {
                                div()
                                    .px_1()
                                    .rounded(shape::sm())
                                    .bg(t.muted)
                                    .text_xs()
                                    .font_family(t.mono_font_family.clone())
                                    .text_color(t.muted_foreground)
                                    .child(*ext)
                            })),
                    ),
            )
    }

    /// Keeps the focused card or row in view: the container's children are
    /// the library's items, in order.
    fn reveal_focused(&self, container: Div) -> Div {
        let Some(nav::Focus::Library(ix)) = self.nav.visible.then_some(self.nav.focus) else { return container };
        let (scroll, reveal) = (self.nav.scroll.clone(), self.nav.reveal.clone());
        container.on_children_prepainted(move |bounds, window, _| {
            nav::reveal_child(&scroll, &reveal, bounds.get(ix).copied(), window)
        })
    }

    fn render_grid(&self, games: &[&GameEntry], window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let columns = grid_columns(window);
        // Square art: as tall as the column is wide.
        let art = px(column_width(window, columns));
        let grid = div()
            .grid()
            .grid_cols(columns)
            .gap(px(GRID_GAP))
            .when(self.query.trim().is_empty(), |grid| grid.child(self.render_add_tile(art, cx)))
            .children(games.iter().enumerate().map(|(i, g)| self.render_card(i, g, art, cx)));
        self.reveal_focused(grid)
    }

    fn render_add_tile(&self, art: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let focused = self.focused_library() == Some(LibItem::AddTile);
        v_flex()
            .id("add-tile")
            .relative()
            .min_h(art + px(78.))
            .p_4()
            .gap_2()
            .items_center()
            .justify_center()
            .rounded(t.radius_lg)
            .border_1()
            .border_dashed()
            .border_color(t.border)
            .cursor_pointer()
            .hover(|s| s.bg(t.muted.opacity(0.4)).border_color(t.muted_foreground))
            .child(
                div()
                    .size(px(44.))
                    .rounded(shape::md())
                    .bg(theme::accent(self.selected.accent))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::Plus).with_size(px(20.)).text_color(white())),
            )
            .child(div().text_sm().font_semibold().child("Add ROM"))
            .child(div().text_xs().text_color(t.muted_foreground).child("Click or drop files"))
            .child(
                h_flex()
                    .flex_wrap()
                    .justify_center()
                    .gap_1()
                    .max_w(px(170.))
                    .children(self.selected.extensions.iter().map(|ext| {
                        div()
                            .px_1()
                            .rounded(shape::sm())
                            .bg(t.muted)
                            .text_xs()
                            .font_family(t.mono_font_family.clone())
                            .text_color(t.muted_foreground)
                            .child(*ext)
                    })),
            )
            .when(focused, |tile| tile.child(nav::ring(t.radius_lg, t.ring)))
            .on_click(cx.listener(|this, _, window, cx| this.pick_and_install(window, cx)))
    }

    /// Neutral stand-in for missing box art: initials plus a faint glyph.
    fn art_placeholder(&self, entry: &GameEntry, cx: &App) -> Div {
        let t = cx.theme();
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(t.muted)
            .child(
                div()
                    .absolute()
                    .right(px(-12.))
                    .top(px(-10.))
                    .opacity(0.08)
                    .child(system_glyph(self.selected, px(92.), t.foreground)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(56.))
                    .font_bold()
                    .text_color(t.muted_foreground)
                    .child(entry.monogram()),
            )
    }

    fn render_card(&self, index: usize, entry: &GameEntry, art_size: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let status = self.status_of(entry);
        let (title, tags) = entry.title_and_tags();
        let cover = match status {
            ScrapeStatus::Scraped(m) => m.cover.clone().or(m.screenshot.clone()).filter(|p| p.is_file()),
            _ => None,
        };
        let placeholder = self.art_placeholder(entry, cx);

        let misplaced = self.misplaced_as(entry);
        // A misplaced game would start in the wrong emulator.
        let can_play = self.can_run() && misplaced.is_none();

        // Chips on the art: what's wrong with it, if anything.
        let flags = h_flex()
            .absolute()
            .left(px(12.))
            .top(px(12.))
            .gap_1()
            .children(misplaced.map(wrong_system_tag))
            .map(|row| match status {
                ScrapeStatus::NoMatch => row.child(shape::small_tag(Tag::warning()).child("No match")),
                ScrapeStatus::NotScraped => row.child(shape::small_tag(Tag::warning()).child("No art")),
                ScrapeStatus::Scraped(_) => row,
            });

        let art = div()
            .relative()
            .h(art_size)
            .overflow_hidden()
            .map(|art| match cover {
                Some(path) => {
                    // While decoding, or if the file is unreadable, show the
                    // same initials tile as a game without art.
                    let (bg, fg, mono) = (t.muted, t.muted_foreground, entry.monogram());
                    let loading = mono.clone();
                    art.child(
                        img(covers::sized(&path, scale::small_art()))
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .with_loading(move || initials_tile(bg, fg, loading.clone()).into_any_element())
                            .with_fallback(move || initials_tile(bg, fg, mono.clone()).into_any_element()),
                    )
                }
                None => art.child(placeholder),
            })
            .child(flags)
            // Delete, top right, on hover.
            .child(
                div()
                    .absolute()
                    .right(px(8.))
                    .top(px(8.))
                    .rounded(shape::sm())
                    .bg(t.background.opacity(0.85))
                    .invisible()
                    .group_hover("card", |s| s.visible())
                    .child({
                        let entry = entry.clone();
                        Button::new(("delete", index))
                            .ghost()
                            .small()
                            .icon(Icon::new(IconName::Trash))
                            .tooltip("Delete")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                details::confirm_delete_dialog(this, entry.clone(), window, cx);
                            }))
                    }),
            )
            // Play, bottom right, on hover: the card's one big action.
            .when(can_play, |art| {
                let entry = entry.clone();
                art.child(
                    div()
                        .id(("play", index))
                        .absolute()
                        .right(px(16.))
                        .bottom(px(16.))
                        .size(px(PLAY_SIZE))
                        .rounded(shape::sm())
                        .bg(t.primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .invisible()
                        .group_hover("card", |s| s.visible())
                        .hover(|s| s.bg(t.primary_hover))
                        .when(self.running.is_some(), |b| b.opacity(0.5))
                        .child(Icon::new(IconName::Play).with_size(px(30.)).text_color(t.primary_foreground))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            if this.running.is_none() {
                                this.run_game(&entry, window, cx);
                            }
                        })),
                )
            });

        // Under the art: the title, then year and genre, or the ROM's tags.
        let chips = match status {
            ScrapeStatus::Scraped(m) => h_flex()
                .gap_1()
                .when_some(m.year.clone(), |row, year| row.child(shape::small_tag(Tag::secondary()).child(year)))
                .when_some(m.genre.clone(), |row, genre: String| {
                    row.child(shape::small_tag(Tag::secondary()).child(short_genre(&genre).to_string()))
                }),
            _ => h_flex()
                .gap_1()
                .children(tags.iter().take(2).map(|tag| shape::small_tag(Tag::secondary()).child(tag.clone())))
                .when(tags.is_empty(), |row| row.child(shape::small_tag(Tag::secondary()).child("Not scraped"))),
        };

        let open_entry = entry.clone();
        let focused = self.focused_library() == Some(LibItem::Game(index));
        v_flex()
            .id(("card", index))
            .group("card")
            .relative()
            .rounded(t.radius_lg)
            .overflow_hidden()
            .border_1()
            .border_color(if misplaced.is_some() { t.danger.opacity(0.6) } else { t.border })
            .bg(theme::card_bg(cx))
            .cursor_pointer()
            .hover(|s| s.bg(t.secondary))
            .child(art)
            .child(
                v_flex()
                    .px_4()
                    .pt_3()
                    .pb_4()
                    .gap_2()
                    .child(div().text_size(px(15.)).font_semibold().truncate().child(title))
                    .child(div().overflow_hidden().child(chips)),
            )
            .when(focused, |card| card.child(nav::ring(t.radius_lg, t.ring)))
            .on_click(cx.listener(move |this, _, window, cx| details::open_details(this, &open_entry, window, cx)))
    }

    fn render_list(&self, games: &[&GameEntry], cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let head = h_flex()
            .h(px(34.))
            .px_3()
            .gap_3()
            .bg(t.table_head)
            .text_xs()
            .font_medium()
            .text_color(t.muted_foreground)
            .child(div().w(px(36.)))
            .child(div().flex_1().child("Title"))
            .child(div().w(px(200.)).child("Details"))
            .child(div().w(px(56.)).child("Files"))
            .child(div().w(px(64.)));

        let rows = games.iter().enumerate().map(|(i, entry)| {
            let status = self.status_of(entry);
            let (title, tags) = entry.title_and_tags();
            let thumb_cover = match status {
                ScrapeStatus::Scraped(m) => m.cover.clone().filter(|p| p.is_file()),
                _ => None,
            };
            let byline = match status {
                ScrapeStatus::Scraped(m) => [m.year.clone(), m.developer.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · "),
                ScrapeStatus::NoMatch => "No match on ScreenScraper".into(),
                ScrapeStatus::NotScraped => "Not scraped yet".into(),
            };
            let details_cell = match status {
                ScrapeStatus::Scraped(m) => h_flex()
                    .gap_1()
                    .when_some(m.genre.clone(), |row, g| row.child(shape::small_tag(Tag::secondary()).child(short_genre(&g).to_string())))
                    .when_some(m.rating, |row, r| row.child(Rating::new(("list-rating", i)).value(stars(r)).xsmall().disabled(true))),
                _ => h_flex().gap_1().children(tags.iter().take(2).map(|tag| shape::small_tag(Tag::secondary()).child(tag.clone()))),
            };
            let details_cell = h_flex().gap_1().children(self.misplaced_as(entry).map(wrong_system_tag)).child(details_cell);
            let open_entry = (*entry).clone();
            let delete_entry = (*entry).clone();
            let play_entry = (*entry).clone();
            let can_play = self.can_run() && self.misplaced_as(entry).is_none();
            let focused = self.focused_library() == Some(LibItem::Game(i));
            h_flex()
                .id(("row", i))
                .relative()
                .h(px(54.))
                .px_3()
                .gap_3()
                .border_t_1()
                .border_color(t.border.opacity(0.7))
                .cursor_pointer()
                .hover(|s| s.bg(t.table_hover))
                .child(
                    div()
                        .size(px(36.))
                        .flex_shrink_0()
                        .rounded(shape::sm())
                        .overflow_hidden()
                        .bg(t.muted)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .font_bold()
                        .text_color(t.muted_foreground)
                        .map(|thumb| match thumb_cover {
                            Some(path) => thumb.child(img(covers::sized(&path, scale::small_art())).size_full().object_fit(ObjectFit::Cover)),
                            None => thumb.child(entry.monogram()),
                        }),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().font_medium().truncate().child(title))
                        .child(div().text_xs().text_color(t.muted_foreground).truncate().child(byline)),
                )
                .child(div().w(px(200.)).child(details_cell))
                .child(div().w(px(56.)).text_sm().text_color(t.muted_foreground).child(entry.files.len().to_string()))
                .child(
                    h_flex().w(px(64.)).justify_end().gap_1()
                    .when(can_play, |cell| cell.child(
                        Button::new(("list-play", i))
                            .ghost()
                            .small()
                            .icon(Icon::new(IconName::Play))
                            .tooltip("Play")
                            .disabled(self.running.is_some())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.run_game(&play_entry, window, cx);
                            })),
                    ))
                    .child(
                        Button::new(("list-delete", i))
                            .ghost()
                            .small()
                            .icon(Icon::new(IconName::Trash))
                            .tooltip("Delete")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                details::confirm_delete_dialog(this, delete_entry.clone(), window, cx);
                            })),
                    ),
                )
                .when(focused, |row| row.child(nav::ring(px(0.), t.ring)))
                .on_click(cx.listener(move |this, _, window, cx| details::open_details(this, &open_entry, window, cx)))
        });

        v_flex()
            .rounded(t.radius_lg)
            .overflow_hidden()
            .border_1()
            .border_color(t.border.opacity(0.7))
            .bg(theme::card_bg(cx))
            .child(head)
            .child(self.reveal_focused(v_flex().children(rows)))
    }
}
