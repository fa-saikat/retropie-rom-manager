//! Game details sheet, the delete / wrong-system confirmations, the
//! add / remove emulator dialogs and the About dialog.
//!
//! GPUI Kit re-runs a sheet/dialog builder on every render, so each one
//! works from a snapshot taken when it opens and talks back to `RootView`
//! through a weak handle.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::description_list::DescriptionList;
use gpui_kit::component::dialog::AlertDialog;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::rating::Rating;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, Sizable, WindowExt, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::PathBuf;
use std::cell::Cell;
use std::rc::Rc;

use super::nav::{self, focus_button};
use super::root::RootView;
use super::scale::px;
use super::sidebar::{system_glyph, system_tile};
use crate::assets;
use crate::detect;
use crate::library::{self, GameEntry};
use crate::scraper::{self, GameMeta, MediaState, ScrapeStatus};
use crate::systems::{SystemDef, MAKERS, SYSTEMS};
use crate::theme;

// GPUI Kit's dialog width and padding are fixed pixels. Restating them in
// our scaled `px` makes dialogs grow with the rest of the UI.
const DIALOG_WIDTH: f32 = 448.;
const DIALOG_PADDING: f32 = 16.;

/// Owned copy of everything the sheet shows.
#[derive(Clone)]
struct Snapshot {
    entry: GameEntry,
    system: &'static SystemDef,
    meta: Option<GameMeta>,
    no_match: bool,
    sizes: Vec<u64>,
    media: Vec<scraper::MediaSlot>,
    scrape_source: String,
    can_scrape: bool,
    /// The system this game's contents say it's really for.
    misplaced: Option<&'static SystemDef>,
    /// ...and it isn't in the sidebar yet, so moving adds it.
    move_adds: bool,
    /// RetroPie has an emulator for this system.
    can_run: bool,
    /// Another game is already running.
    running: bool,
}

pub(super) fn open_details(view: &mut RootView, entry: &GameEntry, window: &mut Window, cx: &mut Context<RootView>) {
    let status = view.status_of(entry);
    let meta = match status {
        ScrapeStatus::Scraped(m) => Some(m.clone()),
        _ => None,
    };
    let snapshot = Snapshot {
        entry: entry.clone(),
        system: view.selected,
        media: meta.as_ref().map(|m| scraper::media_slots(m, &view.settings)).unwrap_or_default(),
        meta,
        no_match: status == ScrapeStatus::NoMatch,
        sizes: entry.file_sizes(),
        scrape_source: view.settings.scrape_source.clone(),
        can_scrape: view.skyscraper_installed && view.scrape.is_none(),
        // The background audit may not have reached this game yet; one
        // file is cheap enough to check here.
        misplaced: view.misplaced_as(entry).or_else(|| {
            let file = detect::representative(view.selected, entry)?;
            detect::detect(&file).filter(|found| found.id != view.selected.id)
        }),
        move_adds: false,
        can_run: view.can_run(),
        running: view.running.is_some(),
    };
    let snapshot = Snapshot { move_adds: snapshot.misplaced.is_some_and(|to| !view.is_enabled(to)), ..snapshot };
    let this = cx.entity().downgrade();
    let keys = SheetKeys::new(cx);
    // Play (or Move) when it can be pressed, else the first button that can.
    let first = if snapshot.misplaced.is_some() || (snapshot.can_run && !snapshot.running) {
        &keys.main
    } else if snapshot.misplaced.is_none() && snapshot.can_scrape {
        &keys.scrape
    } else {
        &keys.delete
    };
    nav::focus_when_open(first.clone(), window);
    window.open_sheet(cx, move |sheet, window, cx| {
        sheet
            .size(px(440.))
            .child(render_body(&snapshot, cx))
            .footer(render_footer(&snapshot, &keys, this.clone(), window, cx))
    });
}

/// Focus handles for the sheet's buttons.
#[derive(Clone)]
struct SheetKeys {
    folder: FocusHandle,
    scrape: FocusHandle,
    delete: FocusHandle,
    /// Play, or Move for a game in the wrong system.
    main: FocusHandle,
}

impl SheetKeys {
    fn new(cx: &mut App) -> Self {
        Self { folder: cx.focus_handle(), scrape: cx.focus_handle(), delete: cx.focus_handle(), main: cx.focus_handle() }
    }
}

fn section(label: &'static str, aside: Option<String>, body: impl IntoElement, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    v_flex()
        .gap_2()
        .child(
            h_flex()
                .justify_between()
                .text_xs()
                .child(div().font_semibold().text_color(t.muted_foreground).child(label))
                .children(aside.map(|a| div().text_color(t.muted_foreground.opacity(0.8)).child(a))),
        )
        .child(body)
}

fn render_body(s: &Snapshot, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    let (title, tags) = s.entry.title_and_tags();
    let meta = s.meta.as_ref();
    let screenshot = meta.and_then(|m| m.screenshot.clone()).filter(|p| p.is_file());
    let cover = meta.and_then(|m| m.cover.clone()).filter(|p| p.is_file());
    let has_banner = screenshot.is_some();

    let banner = screenshot.map(|path| {
        div()
            .h(px(180.))
            .rounded(t.radius_lg)
            .overflow_hidden()
            .bg(t.muted)
            .child(img(path).size_full().object_fit(ObjectFit::Cover))
    });

    let box_art = div()
        .w(px(96.))
        .h(px(128.))
        .flex_shrink_0()
        .rounded(t.radius)
        .overflow_hidden()
        .border_3()
        .border_color(t.background)
        .bg(t.muted)
        .shadow_md()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(30.))
        .font_bold()
        .text_color(t.muted_foreground)
        .map(|b| match cover {
            Some(path) => b.child(img(path).size_full().object_fit(ObjectFit::Cover)),
            None => b.child(s.entry.monogram()),
        });

    let byline = match meta {
        Some(m) => [m.developer.clone(), m.year.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · "),
        None if s.no_match => "No match on ScreenScraper".into(),
        None => "Not scraped yet".into(),
    };

    let head = h_flex()
        .gap_4()
        .items_end()
        .when(has_banner, |h| h.mt(px(-56.)).pl_3())
        .child(box_art)
        .child(
            v_flex()
                .min_w_0()
                .pb_1()
                .gap_1()
                .child(div().text_xl().font_semibold().line_height(relative(1.2)).child(title))
                .child(div().text_sm().text_color(t.muted_foreground).child(byline))
                .child(h_flex().gap_1().children(tags.into_iter().map(|tag| Tag::secondary().small().child(tag)))),
        );

    let misplaced = s.misplaced.map(|other| {
        gpui_kit::component::alert::Alert::warning(
            "misplaced",
            format!(
                "It's in ~/RetroPie/roms/{}, so EmulationStation will try to start it with the {} emulator and it won't run. Move it to {}{}, or delete it.",
                s.system.folder,
                s.system.display_name,
                other.display_name,
                if s.move_adds { " (it'll be added to your list)" } else { "" },
            ),
        )
        .title(format!("This looks like a {} game", other.display_name))
    });

    let about = match meta {
        Some(m) => {
            let desc = match &m.desc {
                Some(d) => div().text_sm().line_height(relative(1.6)).text_color(t.muted_foreground).child(d.clone()),
                None => div().text_sm().italic().text_color(t.muted_foreground).child("No description in gamelist.xml."),
            };
            let mut details = DescriptionList::new().columns(1).label_width(px(96.)).bordered(false);
            for (label, value) in [
                ("Released", m.year.clone()),
                ("Developer", m.developer.clone()),
                ("Publisher", m.publisher.clone()),
                ("Genre", m.genre.clone()),
                ("Players", m.players.clone().map(|p| p.replace('-', "–"))),
            ] {
                if let Some(value) = value {
                    details = details.item(label, value, 1);
                }
            }
            if let Some(rating) = m.rating {
                details = details.item(
                    "Rating",
                    h_flex()
                        .gap_2()
                        .child(
                            Rating::new("detail-rating")
                                .value((rating * 5.0).round() as usize)
                                .small()
                                .disabled(true),
                        )
                        .child(div().text_color(t.muted_foreground).child(format!("{:.1} / 5", rating * 5.0)))
                        .into_any_element(),
                    1,
                );
            }
            v_flex()
                .gap_5()
                .child(desc)
                .child(section("Details", Some(pretty_source(&s.scrape_source)), details, cx))
                .child(section(
                    "Media",
                    Some(format!(
                        "{} of {} saved",
                        s.media.iter().filter(|m| matches!(m.state, MediaState::Saved(_))).count(),
                        s.media.len()
                    )),
                    render_media(s, cx),
                    cx,
                ))
                .into_any_element()
        }
        None => {
            let (heading, body) = if s.no_match {
                ("No match on ScreenScraper.", "Renaming the file to its No-Intro/Redump name usually fixes this.")
            } else {
                ("No artwork yet.", "Scrape to fetch box art, screenshots and a description.")
            };
            gpui_kit::component::alert::Alert::info("no-meta", body)
                .title(heading)
                .into_any_element()
        }
    };

    let total: u64 = s.sizes.iter().sum();
    let files = v_flex()
        .rounded(t.radius)
        .border_1()
        .border_color(t.border)
        .overflow_hidden()
        .text_xs()
        .children(s.entry.files.iter().zip(&s.sizes).map(|(file, size)| {
            h_flex()
                .gap_2()
                .px_2p5()
                .py_1p5()
                .border_b_1()
                .border_color(t.border)
                .child(Icon::new(IconName::File).xsmall().text_color(t.muted_foreground))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(t.mono_font_family.clone())
                        .child(file.file_name().unwrap_or_default().to_string_lossy().to_string()),
                )
                .child(div().text_color(t.muted_foreground).child(library::format_size(*size)))
        }))
        .child(
            h_flex()
                .gap_2()
                .px_2p5()
                .py_1p5()
                .bg(t.muted.opacity(0.5))
                .text_color(t.muted_foreground)
                .child(Icon::new(IconName::Folder).xsmall())
                .child(
                    div()
                        .font_family(t.mono_font_family.clone())
                        .child(format!("~/RetroPie/roms/{}/", s.system.folder)),
                ),
        );

    v_flex()
        .gap_5()
        .pb_4()
        .children(banner)
        .child(head)
        .children(misplaced)
        .child(about)
        .child(section(
            "Files",
            Some(format!(
                "{} {} · {}",
                s.entry.files.len(),
                if s.entry.files.len() == 1 { "file" } else { "files" },
                library::format_size(total)
            )),
            files,
            cx,
        ))
}

fn pretty_source(source: &str) -> String {
    match source {
        "screenscraper" => "ScreenScraper".into(),
        "thegamesdb" => "TheGamesDB".into(),
        "arcadedb" => "ArcadeDB".into(),
        "mobygames" => "MobyGames".into(),
        other => other.to_string(),
    }
}

/// One tile per media type: the image if Skyscraper saved it, otherwise a
/// dashed box saying whether it's switched off or just wasn't found.
fn render_media(s: &Snapshot, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    h_flex().gap_2().items_start().children(s.media.iter().map(|slot| {
        let (tile, note) = match &slot.state {
            MediaState::Saved(path) => (
                div()
                    .rounded(t.radius)
                    .overflow_hidden()
                    .border_1()
                    .border_color(t.border)
                    .bg(t.muted)
                    .child(img(path.clone()).size_full().object_fit(ObjectFit::Cover)),
                "Saved",
            ),
            other => (
                div()
                    .rounded(t.radius)
                    .border_1()
                    .border_dashed()
                    .border_color(t.border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(if slot.label == "Video" { IconName::VideoOff } else { IconName::ImageOff })
                            .small()
                            .text_color(t.muted_foreground),
                    ),
                if *other == MediaState::Disabled { "Off in skyscraper.cfg" } else { "Not found" },
            ),
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(tile.h(px(64.)).w_full())
            .child(div().text_xs().child(slot.label))
            .child(div().text_xs().text_color(t.muted_foreground).line_height(relative(1.2)).child(note))
    }))
}

/// No folder button in fullscreen: the file manager would open behind the app.
/// The arrow keys move between the buttons, and P, S and X work here as they
/// do on a card.
fn render_footer(s: &Snapshot, keys: &SheetKeys, this: WeakEntity<RootView>, window: &Window, cx: &App) -> impl IntoElement {
    let fullscreen = window.is_fullscreen();
    let folder = library::roms_root().join(s.system.folder);
    let play_tooltip = if !s.can_run {
        format!("RetroPie has no emulator set up for {}", s.system.display_name)
    } else if s.running {
        "Another game is running".to_string()
    } else {
        "Play without opening EmulationStation".to_string()
    };
    let can_play = s.can_run && !s.running;

    let play = {
        let (this, entry) = (this.clone(), s.entry.clone());
        Rc::new(move |window: &mut Window, cx: &mut App| {
            this.update(cx, |view, cx| view.run_game(&entry, window, cx)).ok();
        })
    };
    let scrape = {
        let (this, entry) = (this.clone(), s.entry.clone());
        Rc::new(move |window: &mut Window, cx: &mut App| {
            window.close_sheet(cx);
            this.update(cx, |view, cx| view.scrape_key(&entry, window, cx)).ok();
        })
    };
    let delete = {
        let (this, entry) = (this.clone(), s.entry.clone());
        Rc::new(move |window: &mut Window, cx: &mut App| {
            this.update(cx, |view, cx| confirm_delete_dialog(view, entry.clone(), window, cx)).ok();
        })
    };

    // The buttons the arrow keys stop at: the ones that can be pressed.
    let mut order = Vec::new();
    if !fullscreen {
        order.push(keys.folder.clone());
    }
    // Scraping as the wrong system would only fetch the wrong game.
    if s.misplaced.is_none() && s.can_scrape {
        order.push(keys.scrape.clone());
    }
    order.push(keys.delete.clone());
    if s.misplaced.is_some() || can_play {
        order.push(keys.main.clone());
    }

    let (p, sc, x) = (play.clone(), scrape.clone(), delete.clone());
    let (play_ok, scrape_ok) = (can_play && s.misplaced.is_none(), s.can_scrape && s.misplaced.is_none());
    let keys_wrap = div()
        .w_full()
        .key_context(nav::SHEET_CONTEXT)
        .on_action(move |_: &nav::Play, window, cx| {
            if play_ok {
                p(window, cx)
            }
        })
        .on_action(move |_: &nav::Scrape, window, cx| {
            if scrape_ok {
                sc(window, cx)
            }
        })
        .on_action(move |_: &nav::Remove, window, cx| x(window, cx));
    let buttons = nav::group(h_flex(), vec![order], None)
        .w_full()
        .gap_2()
        .when(!fullscreen, |footer| {
            footer.child(focus_button(
                "sheet-folder",
                &keys.folder,
                Button::new("sheet-folder-button")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::FolderOpen))
                    .tooltip("Show in folder"),
                true,
                move |_, cx| {
                    let _ = std::fs::create_dir_all(&folder);
                    cx.open_with_system(&folder);
                },
                window,
                cx,
            ))
        })
        .child(div().flex_1())
        .when(s.misplaced.is_none(), |footer| {
            footer.child(focus_button(
                "sheet-scrape",
                &keys.scrape,
                Button::new("sheet-scrape-button")
                    .outline()
                    .small()
                    .icon(Icon::new(if s.meta.is_some() { IconName::RefreshCw } else { IconName::ImageDown }))
                    .label(if s.meta.is_some() { "Re-scrape" } else { "Scrape" }),
                s.can_scrape,
                move |window, cx| scrape(window, cx),
                window,
                cx,
            ))
        })
        .child(focus_button(
            "sheet-delete",
            &keys.delete,
            Button::new("sheet-delete-button").danger().outline().small().icon(Icon::new(IconName::Trash)).label("Delete"),
            true,
            move |window, cx| delete(window, cx),
            window,
            cx,
        ))
        .map(|footer| match s.misplaced {
            // It won't run here, so moving it is the main action.
            Some(to) => {
                let (this, entry) = (this.clone(), s.entry.clone());
                footer.child(focus_button(
                    "sheet-move",
                    &keys.main,
                    Button::new("sheet-move-button")
                        .primary()
                        .small()
                        .icon(Icon::new(IconName::FolderInput))
                        .label(if s.move_adds { format!("Add {} & move", to.short_name) } else { format!("Move to {}", to.short_name) }),
                    true,
                    move |window, cx| {
                        this.update(cx, |view, cx| view.move_game(entry.clone(), to, window, cx)).ok();
                    },
                    window,
                    cx,
                ))
            }
            None => footer.child(focus_button(
                "sheet-play",
                &keys.main,
                Button::new("sheet-play-button")
                    .primary()
                    .small()
                    .icon(Icon::new(IconName::Play))
                    .label("Play")
                    .tooltip(play_tooltip.clone()),
                can_play,
                move |window, cx| play(window, cx),
                window,
                cx,
            )),
        });
    keys_wrap.child(buttons)
}

/// Our own dialog footer: the buttons in order, each focusable, so the
/// arrow keys move between them and Enter presses the one with focus.
fn dialog_footer(buttons: Vec<(FocusHandle, AnyElement)>) -> Div {
    let order = buttons.iter().map(|(h, _)| h.clone()).collect();
    nav::group(h_flex().w_full().gap_2().justify_end(), vec![order], None)
        .children(buttons.into_iter().map(|(_, button)| button))
}

/// Cancel and a confirm button, with Cancel focused when the dialog opens.
struct Confirm {
    cancel: FocusHandle,
    ok: FocusHandle,
}

impl Confirm {
    fn open(window: &mut Window, cx: &mut App) -> Self {
        let keys = Self { cancel: cx.focus_handle(), ok: cx.focus_handle() };
        nav::focus_when_open(keys.cancel.clone(), window);
        keys
    }

    fn footer(
        &self,
        cancel_label: &'static str,
        ok: Button,
        on_ok: impl Fn(&mut Window, &mut App) + 'static,
        window: &Window,
        cx: &App,
    ) -> Div {
        dialog_footer(vec![
            (
                self.cancel.clone(),
                focus_button("dialog-cancel", &self.cancel, Button::new("dialog-cancel-button").label(cancel_label), true, |window, cx| window.close_dialog(cx), window, cx)
                    .into_any_element(),
            ),
            (
                self.ok.clone(),
                focus_button(
                    "dialog-ok",
                    &self.ok,
                    ok,
                    true,
                    move |window, cx| {
                        window.close_dialog(cx);
                        on_ok(window, cx);
                    },
                    window,
                    cx,
                )
                .into_any_element(),
            ),
        ])
    }
}

pub(super) fn confirm_delete_dialog(
    view: &mut RootView,
    entry: GameEntry,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    let this = cx.entity().downgrade();
    let folder = view.selected.folder;
    let keys = Confirm::open(window, cx);
    window.open_alert_dialog(cx, move |alert: AlertDialog, window, cx| {
        let t = cx.theme();
        let (title, _) = entry.title_and_tags();
        let n = entry.files.len();
        let files = v_flex()
            .max_h(px(140.))
            .overflow_hidden()
            .rounded(t.radius)
            .border_1()
            .border_color(t.border)
            .px_2p5()
            .py_1p5()
            .gap_0p5()
            .text_xs()
            .font_family(t.mono_font_family.clone())
            .text_color(t.muted_foreground)
            .children(entry.files.iter().take(6).map(|f| {
                div().truncate().child(f.file_name().unwrap_or_default().to_string_lossy().to_string())
            }))
            .when(n > 6, |list| list.child(format!("+ {} more", n - 6)));
        let this = this.clone();
        let entry = entry.clone();
        alert
            .width(px(DIALOG_WIDTH))
            .p(px(DIALOG_PADDING))
            .title(format!("Delete {title}?"))
            .description(format!(
                "{n} {} will be permanently removed from ~/RetroPie/roms/{folder}, along with its scraped artwork. This can't be undone.",
                if n == 1 { "file" } else { "files" }
            ))
            .child(files)
            .footer(keys.footer(
                "Cancel",
                Button::new("delete-confirm").danger().label("Delete files"),
                move |window, cx| {
                    this.update(cx, |view, cx| view.confirm_delete(entry.clone(), window, cx)).ok();
                },
                window,
                cx,
            ))
    });
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().to_string()
}

/// One "thing → system" line, shared by the wrong-system dialogs. `adds`
/// marks a system that isn't in the sidebar yet.
fn destination_row(label: String, to: &'static SystemDef, adds: bool, cx: &App) -> Div {
    let t = cx.theme();
    h_flex()
        .gap_2()
        .px_2p5()
        .py_1p5()
        .border_b_1()
        .border_color(t.border)
        .text_xs()
        .child(div().flex_1().min_w_0().truncate().font_family(t.mono_font_family.clone()).child(label))
        .child(Icon::new(IconName::ArrowRight).xsmall().text_color(t.muted_foreground))
        .child(
            h_flex()
                .gap_1()
                .flex_shrink_0()
                .font_medium()
                .child(system_glyph(to, px(11.), t.foreground))
                .child(to.short_name),
        )
        .when(adds, |row| row.child(Tag::info().small().child("new")))
}

/// "Game Boy and SNES will be added to your list." for systems not listed.
fn added_note(new: &[&'static SystemDef]) -> Option<String> {
    let names: Vec<&str> = new.iter().map(|s| s.display_name).collect();
    let list = match names.as_slice() {
        [] => return None,
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    Some(format!(" {list} will be added to your list."))
}

fn destination_list(rows: impl IntoIterator<Item = Div>, cx: &App) -> Div {
    let t = cx.theme();
    v_flex()
        .max_h(px(180.))
        .overflow_hidden()
        .rounded(t.radius)
        .border_1()
        .border_color(t.border)
        .children(rows)
}

/// Some of the files being installed into `here` are, by their contents,
/// games for other systems. Offer to put them where they belong.
pub(super) fn confirm_wrong_system(
    view: &mut RootView,
    here: &'static SystemDef,
    files: Vec<(PathBuf, &'static SystemDef)>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    let this = cx.entity().downgrade();
    let n = files.len();
    let title = match files.as_slice() {
        [(path, other)] => format!("{} looks like a {} game", file_name(path), other.display_name),
        _ => format!("{n} files look like games for other systems"),
    };
    let mut targets: Vec<&'static SystemDef> = Vec::new();
    for (_, to) in &files {
        if !targets.iter().any(|t| t.id == to.id) {
            targets.push(to);
        }
    }
    let new: Vec<&'static SystemDef> = targets.iter().copied().filter(|t| !view.is_enabled(t)).collect();
    let go_label = match targets.as_slice() {
        [only] if new.is_empty() => format!("Install to {}", only.display_name),
        [only] => format!("Add {} & install", only.display_name),
        _ => "Install where they belong".to_string(),
    };
    let note = format!(
        "You're adding to {}. EmulationStation would start {} with the {} emulator, and {} won't run there.{}",
        here.display_name,
        if n == 1 { "it" } else { "them" },
        here.display_name,
        if n == 1 { "it" } else { "they" },
        added_note(&new).unwrap_or_default(),
    );
    // "Anyway" only makes sense if this system takes at least one of them.
    let can_stay = files
        .iter()
        .any(|(p, _)| here.extensions.contains(&library::ext_lower(p).as_str()));
    let (skip_key, stay_key, go_key) = (cx.focus_handle(), cx.focus_handle(), cx.focus_handle());
    // Nothing is lost either way, so start on the suggested choice.
    nav::focus_when_open(go_key.clone(), window);
    window.open_dialog(cx, move |dialog, window, cx| {
        let t = cx.theme();
        let (go, stay) = (this.clone(), this.clone());
        let go_files = files.clone();
        let stay_files: Vec<(PathBuf, &'static SystemDef)> = files.iter().map(|(p, _)| (p.clone(), here)).collect();
        let rows = files.iter().map(|(p, to)| {
            destination_row(file_name(p), to, new.iter().any(|s| s.id == to.id), cx)
        });
        dialog
            .title(title.clone())
            .w(px(480.))
            .p(px(DIALOG_PADDING))
            .child(
                v_flex()
                    .gap_3()
                    .child(div().text_sm().text_color(t.muted_foreground).child(note.clone()))
                    .child(destination_list(rows, cx)),
            )
            .footer({
                let mut order = vec![skip_key.clone()];
                if can_stay {
                    order.push(stay_key.clone());
                }
                order.push(go_key.clone());
                nav::group(h_flex().w_full().gap_2(), vec![order], None)
                    .child(focus_button(
                        "wrong-skip",
                        &skip_key,
                        Button::new("wrong-skip-button").ghost().label("Skip"),
                        true,
                        |window, cx| window.close_dialog(cx),
                        window,
                        cx,
                    ))
                    .child(div().flex_1())
                    .when(can_stay, |footer| {
                        footer.child(focus_button(
                            "wrong-stay",
                            &stay_key,
                            Button::new("wrong-stay-button").outline().label(format!("Add to {} anyway", here.short_name)),
                            true,
                            move |window, cx| {
                                window.close_dialog(cx);
                                stay.update(cx, |view, cx| view.install_into(stay_files.clone(), window, cx)).ok();
                            },
                            window,
                            cx,
                        ))
                    })
                    .child(focus_button(
                        "wrong-go",
                        &go_key,
                        Button::new("wrong-go-button").primary().label(go_label.clone()),
                        true,
                        move |window, cx| {
                            window.close_dialog(cx);
                            go.update(cx, |view, cx| view.install_elsewhere(go_files.clone(), window, cx)).ok();
                        },
                        window,
                        cx,
                    ))
            })
    });
}

/// Confirm moving every flagged game in the selected system.
pub(super) fn confirm_move_misplaced(view: &mut RootView, window: &mut Window, cx: &mut Context<RootView>) {
    let this = cx.entity().downgrade();
    let games: Vec<(String, &'static SystemDef, bool)> = view
        .misplaced_games()
        .into_iter()
        .map(|m| (m.entry.title_and_tags().0, m.to, !view.is_enabled(m.to)))
        .collect();
    let mut new: Vec<&'static SystemDef> = Vec::new();
    for (_, to, adds) in &games {
        if *adds && !new.iter().any(|s| s.id == to.id) {
            new.push(to);
        }
    }
    let description = format!(
        "Each one goes from ~/RetroPie/roms/{} to the folder for the system it was made for. Artwork scraped for the wrong system is removed.{}",
        view.selected.folder,
        added_note(&new).unwrap_or_default(),
    );
    let keys = Confirm::open(window, cx);
    window.open_alert_dialog(cx, move |alert: AlertDialog, window, cx| {
        let n = games.len();
        let this = this.clone();
        alert
            .width(px(DIALOG_WIDTH))
            .p(px(DIALOG_PADDING))
            .title(format!("Move {n} {}?", if n == 1 { "game" } else { "games" }))
            .description(description.clone())
            .child(destination_list(
                games.iter().map(|(title, to, adds)| destination_row(title.clone(), to, *adds, cx)),
                cx,
            ))
            .footer(keys.footer(
                "Cancel",
                Button::new("move-confirm").primary().label("Move games"),
                move |window, cx| {
                    this.update(cx, |view, cx| view.move_all_misplaced(window, cx)).ok();
                },
                window,
                cx,
            ))
    });
}

/// One system the "Add emulator" picker offers.
#[derive(Clone)]
struct Choice {
    system: &'static SystemDef,
    photo: Option<PathBuf>,
    /// Flagged games elsewhere that belong to it.
    waiting: usize,
    /// RetroPie has an emulator set up for it.
    installed: bool,
    focus: FocusHandle,
}

impl Choice {
    fn matches(&self, query: &str) -> bool {
        let s = self.system;
        query.is_empty()
            || [s.display_name, s.short_name, s.maker, s.id]
                .iter()
                .any(|field| field.to_lowercase().contains(query))
    }
}

/// The flat grey the system photos were composited onto, so the photo
/// well and the image read as one surface in both themes.
const PHOTO_WELL: u32 = 0xECECEF;

/// Searchable grid of photo cards for every system not in the sidebar,
/// grouped by maker, with the ones that have games waiting for them first.
pub(super) fn open_add_system(view: &mut RootView, window: &mut Window, cx: &mut Context<RootView>) {
    let this = cx.entity().downgrade();
    let choices: Vec<Choice> = SYSTEMS
        .iter()
        .filter(|s| !view.is_enabled(s))
        .map(|system| Choice {
            system,
            photo: assets::system_photo(system),
            waiting: view.waiting_for(system).len(),
            installed: view.runnable.contains(system.id),
            focus: cx.focus_handle(),
        })
        .collect();
    let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search systems"));
    // Once the dialog has opened and taken focus for itself.
    let focus = search.clone();
    window.on_next_frame(move |window, cx| focus.update(cx, |s, cx| s.focus(window, cx)));
    // Down or Enter in the search field goes on to the first match.
    let first = {
        let (choices, search) = (choices.clone(), search.clone());
        Rc::new(move |window: &mut Window, cx: &mut App| {
            let query = search.read(cx).value().trim().to_lowercase();
            let groups = choice_groups(&choices, &query);
            if let Some(choice) = groups.first().and_then(|(_, items)| items.first()) {
                window.focus(&choice.focus, cx);
            }
        })
    };
    let scroll = ScrollHandle::new();
    window.open_dialog(cx, move |dialog, window, cx| {
        let t = cx.theme();
        let query = search.read(cx).value().trim().to_lowercase();
        let visible: Vec<&Choice> = choices.iter().filter(|c| c.matches(&query)).collect();
        let groups = choice_groups(&choices, &query);
        // Three cards a row, a group at a time.
        let rows: Vec<Vec<FocusHandle>> = groups
            .iter()
            .flat_map(|(_, items)| items.chunks(3).map(|row| row.iter().map(|c| c.focus.clone()).collect()))
            .collect();
        let to_search = {
            let search = search.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| search.update(cx, |s, cx| s.focus(window, cx)))
                as nav::Handler
        };

        let body = if choices.is_empty() {
            empty_note("Every system the app knows is already in your list.", cx)
        } else if visible.is_empty() {
            empty_note(&format!("No system matches “{query}”."), cx)
        } else {
            nav::group(v_flex(), rows, Some(to_search)).gap_6().pb_1().children(groups.into_iter().map(|(heading, items)| {
                let count = items.len();
                let focused = items.iter().position(|c| c.focus.is_focused(window));
                let scroll = scroll.clone();
                v_flex()
                    .gap_2p5()
                    .child(
                        h_flex()
                            .gap_2()
                            .text_xs()
                            .font_semibold()
                            .text_color(t.muted_foreground)
                            .child(heading)
                            .child(div().font_normal().opacity(0.7).child(count.to_string())),
                    )
                    .child(
                        div()
                            .grid()
                            .grid_cols(3)
                            .gap_3()
                            .when_some(focused, |grid, ix| {
                                grid.on_children_prepainted(move |bounds, window, _| {
                                    nav::reveal_child(&scroll, &Cell::new(true), bounds.get(ix).copied(), window)
                                })
                            })
                            .children(items.into_iter().map(|choice| choice_card(choice, this.clone(), window, cx))),
                    )
            }))
        };

        dialog
            .title("Add an emulator")
            .w(px(760.))
            .p(px(DIALOG_PADDING))
            .child(
                v_flex()
                    .gap_4()
                    .child(
                        h_flex()
                            .gap_4()
                            .items_center()
                            .child(div().flex_1().min_w_0().text_sm().text_color(t.muted_foreground).child(
                                "Pick a system to add it to your list.",
                            ))
                            .child({
                                let (leave, back) = (first.clone(), search.clone());
                                div()
                                    .key_context(ADD_SYSTEM_SEARCH)
                                    .on_action(move |_: &nav::Leave, window, cx| leave(window, cx))
                                    // Backspace on an empty search closes, like elsewhere in the dialog.
                                    .on_action(move |_: &nav::Back, window, cx| {
                                        if back.read(cx).value().is_empty() {
                                            window.close_dialog(cx);
                                        } else {
                                            cx.propagate();
                                        }
                                    })
                                    .w(px(220.))
                                    .flex_shrink_0()
                                    .child(
                                        Input::new(&search)
                                            .prefix(Icon::new(IconName::Search).small().text_color(t.muted_foreground))
                                            .cleanable(true),
                                    )
                            }),
                    )
                    .child(div().id("add-system-scroll").h(px(500.)).overflow_y_scroll().track_scroll(&scroll).pr_1().child(body))
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.muted_foreground.opacity(0.8))
                            .child("Photos: Wikimedia Commons, mostly by Evan-Amos (public domain). Full credits in assets/systems/CREDITS.md."),
                    ),
            )
    });
}

/// Wraps the Add emulator search field, for Down, Enter and Backspace in it.
pub const ADD_SYSTEM_SEARCH: &str = "AddSystemSearch";

/// The picker's groups: systems with games waiting first, then by maker.
fn choice_groups<'a>(choices: &'a [Choice], query: &str) -> Vec<(String, Vec<&'a Choice>)> {
    let visible: Vec<&Choice> = choices.iter().filter(|c| c.matches(query)).collect();
    let mut groups: Vec<(String, Vec<&Choice>)> = Vec::new();
    let suggested: Vec<&Choice> = visible.iter().copied().filter(|c| c.waiting > 0).collect();
    if !suggested.is_empty() {
        groups.push(("Suggested for games you already have".into(), suggested));
    }
    for maker in MAKERS {
        let items: Vec<&Choice> = visible.iter().copied().filter(|c| c.system.maker == *maker && c.waiting == 0).collect();
        if !items.is_empty() {
            groups.push((maker.to_string(), items));
        }
    }
    groups
}

fn empty_note(text: &str, cx: &App) -> Div {
    div()
        .py_10()
        .flex()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_string())
}

fn choice_card(choice: &Choice, this: WeakEntity<RootView>, window: &Window, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    let system = choice.system;
    let accent = theme::accent(system.accent);
    let group = SharedString::from(format!("choice-{}", system.id));

    let photo = div()
        .relative()
        .h(px(128.))
        .bg(rgb(PHOTO_WELL))
        .flex()
        .items_center()
        .justify_center()
        .map(|well| match &choice.photo {
            Some(path) => well.child(img(path.clone()).size_full().object_fit(ObjectFit::Contain)),
            None => well.child(system_tile(system, px(56.), px(28.), px(14.))),
        })
        .when(choice.waiting > 0, |well| {
            well.child(
                div().absolute().left(px(8.)).top(px(8.)).child(
                    Tag::danger()
                        .small()
                        .child(format!("{} {} waiting", choice.waiting, if choice.waiting == 1 { "game" } else { "games" })),
                ),
            )
        })
        // "Add" pill fades in on hover, in the system's own colour.
        .child(
            h_flex()
                .absolute()
                .right(px(8.))
                .top(px(8.))
                .gap_1()
                .px_2()
                .py(px(3.))
                .rounded_full()
                .bg(accent)
                .text_xs()
                .font_medium()
                .text_color(white())
                .invisible()
                .group_hover(group.clone(), |s| s.visible())
                .child(Icon::new(IconName::Plus).xsmall())
                .child("Add"),
        );

    let (dot, status) = if choice.installed {
        (t.success, "Emulator installed")
    } else {
        (t.muted_foreground.opacity(0.5), "Emulator not installed yet")
    };
    let caption = h_flex()
        .gap_2p5()
        .px_3()
        .py_2p5()
        .child(system_tile(system, px(26.), px(14.), px(7.)))
        .child(
            v_flex()
                .min_w_0()
                .child(div().text_sm().font_semibold().truncate().child(system.display_name))
                .child(
                    h_flex()
                        .gap_1p5()
                        .text_xs()
                        .text_color(t.muted_foreground)
                        .child(div().size(px(6.)).flex_shrink_0().rounded_full().bg(dot))
                        .child(div().truncate().child(status)),
                ),
        );

    let focused = choice.focus.is_focused(window);
    v_flex()
        .id(SharedString::from(format!("add-{}", system.id)))
        .track_focus(&choice.focus)
        .relative()
        .group(group)
        .rounded(t.radius_lg)
        .overflow_hidden()
        .border_1()
        .border_color(if choice.waiting > 0 { t.danger.opacity(0.6) } else { t.border })
        .bg(theme::card_bg(cx))
        .cursor_pointer()
        .hover(|s| s.border_color(accent))
        .child(photo)
        .child(caption)
        .when(focused, |card| card.child(nav::ring(t.radius_lg, accent)))
        .on_click(move |_, window, cx| {
            window.close_dialog(cx);
            this.update(cx, |view, cx| view.add_system(system, window, cx)).ok();
        })
}

/// Removing a system deletes its games, so say exactly what goes.
pub(super) fn confirm_remove_system(
    system: &'static SystemDef,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    let this = cx.entity().downgrade();
    let games = library::list_installed_games(system).unwrap_or_default();
    let n = games.len();
    let files: usize = games.iter().map(|g| g.files.len()).sum();
    let bytes: u64 = games.iter().flat_map(|g| g.file_sizes()).sum();
    let keys = Confirm::open(window, cx);
    window.open_alert_dialog(cx, move |alert: AlertDialog, window, cx| {
        let this = this.clone();
        let description = if n == 0 {
            "It has no games, so nothing is deleted. You can add it back from Add emulator any time.".to_string()
        } else {
            format!(
                "This permanently deletes its {n} {} ({files} {}, {}) from ~/RetroPie/roms/{}, along with {} scraped artwork. This can't be undone. RetroPie's emulator itself stays installed.",
                if n == 1 { "game" } else { "games" },
                if files == 1 { "file" } else { "files" },
                library::format_size(bytes),
                system.folder,
                if n == 1 { "its" } else { "their" },
            )
        };
        alert
            .width(px(DIALOG_WIDTH))
            .p(px(DIALOG_PADDING))
            .title(format!("Remove {}?", system.display_name))
            .description(description)
            .footer(keys.footer(
                "Cancel",
                Button::new("remove-confirm")
                    .label(if n == 0 { "Remove".to_string() } else { format!("Delete {n} {} & remove", if n == 1 { "game" } else { "games" }) })
                    .with_variant(if n == 0 { ButtonVariant::Primary } else { ButtonVariant::Danger }),
                move |window, cx| {
                    this.update(cx, |view, cx| view.remove_system(system, window, cx)).ok();
                },
                window,
                cx,
            ))
    });
}

/// Quitting always asks, and lists anything quitting would interrupt.
/// Cancel has focus when it opens, so a stray Enter (a gamepad's A button,
/// say) just closes it.
pub(super) fn confirm_quit(view: &RootView, window: &mut Window, cx: &mut Context<RootView>) {
    let mut busy = Vec::new();
    if let Some(game) = &view.running {
        busy.push(format!("{} is still running. It keeps running after ROM Manager closes.", game.title));
    }
    if let Some(job) = &view.scrape {
        busy.push(format!(
            "Scraping {} stops, with {} of {} games done. The rest stay without artwork.",
            job.system.display_name, job.done, job.total
        ));
    }
    if view.setup.is_some() {
        busy.push("Skyscraper is still being installed. Stopping RetroPie-Setup now can leave it half-installed.".to_string());
    }
    if view.installing > 0 {
        busy.push("ROMs are still being copied. Quitting now can leave incomplete files in ~/RetroPie/roms.".to_string());
    }
    let this = cx.entity().downgrade();
    let keys = Confirm::open(window, cx);
    window.open_alert_dialog(cx, move |alert: AlertDialog, window, cx| {
        let t = cx.theme();
        let (closed, cancelled) = (this.clone(), this.clone());
        alert
            .width(px(DIALOG_WIDTH))
            .p(px(DIALOG_PADDING))
            .title("Quit ROM Manager?")
            .when(!busy.is_empty(), |alert| {
                alert.child(
                    v_flex()
                        .gap_2()
                        .text_sm()
                        .text_color(t.muted_foreground)
                        .children(busy.iter().map(|line| div().child(line.clone()))),
                )
            })
            .on_close(move |_, _, cx| {
                closed.update(cx, |view, _| view.quit_open = false).ok();
            })
            .footer(dialog_footer(vec![
                (
                    keys.cancel.clone(),
                    focus_button(
                        "quit-cancel",
                        &keys.cancel,
                        Button::new("quit-cancel-button").label("Cancel"),
                        true,
                        move |window, cx| {
                            // `close_dialog` skips `on_close`, so clear the flag here too.
                            cancelled.update(cx, |view, _| view.quit_open = false).ok();
                            window.close_dialog(cx);
                        },
                        window,
                        cx,
                    )
                    .into_any_element(),
                ),
                (
                    keys.ok.clone(),
                    focus_button(
                        "quit-confirm",
                        &keys.ok,
                        Button::new("quit-confirm-button")
                            .label("Quit")
                            .with_variant(if busy.is_empty() { ButtonVariant::Primary } else { ButtonVariant::Danger }),
                        true,
                        |_, cx| cx.quit(),
                        window,
                        cx,
                    )
                    .into_any_element(),
                ),
            ]))
    });
}

pub(super) fn confirm_install_skyscraper(window: &mut Window, cx: &mut Context<RootView>) {
    let this = cx.entity().downgrade();
    let keys = Confirm::open(window, cx);
    window.open_alert_dialog(cx, move |alert: AlertDialog, window, cx| {
        let this = this.clone();
        alert
            .width(px(DIALOG_WIDTH))
            .p(px(DIALOG_PADDING))
            .title("Install Skyscraper?")
            .description(
                "This runs RetroPie-Setup's Skyscraper package, the same as Manage packages → opt → skyscraper. \
                 It needs administrator rights, so you may be asked for your password, and it can take \
                 10–20 minutes on a Raspberry Pi while it builds. You can keep using the app meanwhile.",
            )
            .footer(keys.footer(
                "Not now",
                Button::new("install-confirm").primary().label("Install"),
                move |window, cx| {
                    this.update(cx, |view, cx| view.install_skyscraper(window, cx)).ok();
                },
                window,
                cx,
            ))
    });
}

pub(super) fn open_about(view: &mut RootView, window: &mut Window, cx: &mut Context<RootView>) {
    let scraper_line = if view.skyscraper_installed {
        format!("Skyscraper ({})", pretty_source(&view.settings.scrape_source))
    } else {
        "Skyscraper not installed".to_string()
    };
    let system = view.selected;
    let close = cx.focus_handle();
    nav::focus_when_open(close.clone(), window);
    window.open_dialog(cx, move |dialog, window, cx| {
        let t = cx.theme();
        dialog
            .title("RetroPie ROM Manager")
            .w(px(420.))
            .p(px(DIALOG_PADDING))
            .child(
                v_flex()
                    .gap_4()
                    .child(
                        h_flex()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(40.))
                                    .flex_shrink_0()
                                    .rounded(px(10.))
                                    .bg(t.foreground)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(system_glyph(system, px(20.), t.background)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .text_color(t.muted_foreground)
                                    .child("Install, browse and remove RetroPie ROMs, with box art fetched by RetroPie's own scraper."),
                            ),
                    )
                    .child(
                        DescriptionList::new()
                            .columns(1)
                            .label_width(px(110.))
                            .item("Version", env!("CARGO_PKG_VERSION"), 1)
                            .item("ROMs folder", "~/RetroPie/roms", 1)
                            .item("Artwork", scraper_line.clone(), 1)
                            .item("Developers", "Fahim A Saikat, Ukay K Marma", 1)
                            .item("Copyright", "© 2026 JaduPc", 1)
                            .item("License", "MIT", 1),
                    ),
            )
            .footer(dialog_footer(vec![(
                close.clone(),
                focus_button("about-close", &close, Button::new("about-close-button").label("Close"), true, |window, cx| window.close_dialog(cx), window, cx)
                    .into_any_element(),
            )]))
    });
}

