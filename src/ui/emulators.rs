//! The emulator library: a page in the main column listing every system
//! RetroPie can run, grouped by maker, with the ones that have games
//! waiting for them first. Replaces the old "Add emulator" dialog.
//!
//! Adding a system from here keeps you on the page (the sidebar updates and
//! a notification confirms it); "Open" on a listed system goes to it.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, Sizable, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::library::{column_width, columns_for, GRID_GAP};
use super::nav::{self, HeaderItem, LibItem};
use super::root::RootView;
use super::scale::px;
use super::shape;
use super::sidebar::{logo_plate, system_tile};
use crate::assets;
use crate::systems::{SystemDef, MAKERS, SYSTEMS};
use crate::theme;

/// Narrowest card the grid lays out, in 1080p pixels.
pub(super) const CARD_MIN_WIDTH: f32 = 250.;
/// The flat grey the system photos were composited onto, so the photo
/// well and the image read as one surface in both themes.
const PHOTO_WELL: u32 = 0xECECEF;

/// One group on the page: heading, its systems, and whether it's the
/// "games waiting" group (drawn in red).
pub(super) struct Group {
    pub heading: String,
    pub systems: Vec<&'static SystemDef>,
    pub hot: bool,
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

impl RootView {
    fn emulator_matches(&self, system: &SystemDef) -> bool {
        let q = self.emu_query.trim().to_lowercase();
        let maker_ok = self.maker_filter.is_none_or(|m| m == system.maker);
        maker_ok
            && (q.is_empty()
                || [system.display_name, system.short_name, system.maker, system.id]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&q)))
    }

    /// The page's groups: systems with games waiting (and not listed yet)
    /// first, then every maker in its usual order.
    pub(super) fn emulator_groups(&self) -> Vec<Group> {
        let visible: Vec<&'static SystemDef> = SYSTEMS.iter().filter(|s| self.emulator_matches(s)).collect();
        let mut groups = Vec::new();
        let suggested: Vec<&'static SystemDef> =
            visible.iter().copied().filter(|s| !self.is_enabled(s) && !self.waiting_for(s).is_empty()).collect();
        if !suggested.is_empty() {
            groups.push(Group { heading: "Suggested for games you already have".into(), systems: suggested.clone(), hot: true });
        }
        for maker in MAKERS {
            let systems: Vec<&'static SystemDef> =
                visible.iter().copied().filter(|s| s.maker == *maker && !suggested.iter().any(|x| x.id == s.id)).collect();
            if !systems.is_empty() {
                groups.push(Group { heading: maker.to_string(), systems, hot: false });
            }
        }
        groups
    }

    /// Every card on the page in order, for the keyboard.
    pub(super) fn emulator_cards(&self) -> Vec<&'static SystemDef> {
        self.emulator_groups().into_iter().flat_map(|g| g.systems).collect()
    }

    pub(super) fn set_maker_filter(&mut self, maker: Option<&'static str>, cx: &mut Context<Self>) {
        self.maker_filter = maker;
        cx.notify();
    }

    /// Enter on a card, or a click on it: add the system, or open it if
    /// it's listed already.
    pub(super) fn activate_emulator(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(system) = self.emulator_cards().get(index).copied() else { return };
        if self.is_enabled(system) {
            self.select_system(system, window, cx);
        } else {
            self.add_system_here(system, window, cx);
        }
    }

    pub(super) fn render_emulators(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let fullscreen = window.is_fullscreen();
        let focused_header = self.focused_header(fullscreen);
        let installed = SYSTEMS.iter().filter(|s| self.runnable.contains(s.id)).count();

        let (muted, radius, ring) = (cx.theme().muted_foreground, cx.theme().radius, cx.theme().ring);
        let search = self.search_keys(
            nav::ringed(
                Input::new(&self.emu_search)
                    .w(px(240.))
                    .prefix(Icon::new(IconName::Search).small().text_color(muted))
                    .when_some(Keystroke::parse("/").ok(), |input, key| input.suffix(Kbd::new(key)))
                    .cleanable(true),
                focused_header == Some(HeaderItem::Search),
                radius,
                ring,
            ),
            cx,
        );
        let t = cx.theme();

        let chip = |label: String| {
            h_flex()
                .h(px(28.))
                .px(px(12.))
                .rounded(shape::sm())
                .bg(t.secondary)
                .text_xs()
                .font_medium()
                .text_color(t.muted_foreground)
                .child(label)
        };
        let header = v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .child(div().flex_1().min_w_0().text_2xl().font_medium().child("Emulator library"))
                    .child(search),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(t.muted_foreground)
                    .child("Every system RetroPie can run. Add one to put it in your sidebar and get a ROM folder for it."),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1p5()
                    .child(chip(plural(SYSTEMS.len(), "system")))
                    .child(chip(format!("{} in your list", self.enabled.len())))
                    .child(chip(format!("{installed} emulators installed"))),
            );

        // Maker filter chips: All, then each maker with its count.
        let makers: Vec<Option<&'static str>> = std::iter::once(None).chain(MAKERS.iter().copied().map(Some)).collect();
        let filters = h_flex().flex_wrap().gap_2().children(makers.into_iter().enumerate().map(|(i, maker)| {
            let selected = self.maker_filter == maker;
            let focused = focused_header == Some(HeaderItem::Maker(maker));
            let count = match maker {
                None => SYSTEMS.len(),
                Some(m) => SYSTEMS.iter().filter(|s| s.maker == m).count(),
            };
            h_flex()
                .id(("maker", i))
                .relative()
                .h(px(32.))
                .px(px(12.))
                .gap_1p5()
                .rounded(shape::sm())
                .border_1()
                .border_color(if selected { transparent_black() } else { t.border })
                .bg(if selected { t.sidebar_accent } else { transparent_black() })
                .text_color(if selected { t.sidebar_accent_foreground } else { t.muted_foreground })
                .text_sm()
                .font_medium()
                .cursor_pointer()
                .hover(|s| s.bg(t.secondary))
                .when(selected, |c| c.child(Icon::new(IconName::Check).xsmall()))
                .child(maker.unwrap_or("All"))
                .child(div().font_normal().opacity(0.7).child(count.to_string()))
                .when(focused, |c| c.child(nav::ring(shape::sm(), t.ring)))
                .on_click(cx.listener(move |this, _, _, cx| this.set_maker_filter(maker, cx)))
        }));

        let groups = self.emulator_groups();
        let columns = columns_for(window, CARD_MIN_WIDTH);
        let well = px(column_width(window, columns) * 0.6);
        let focused_card = match self.focused_library() {
            Some(LibItem::Game(i)) => Some(i),
            _ => None,
        };
        let mut start = 0;
        let body: Vec<AnyElement> = if groups.is_empty() {
            vec![div()
                .py_10()
                .flex()
                .justify_center()
                .text_sm()
                .text_color(t.muted_foreground)
                .child(format!("No system matches “{}”.", self.emu_query.trim()))
                .into_any_element()]
        } else {
            groups
                .into_iter()
                .map(|group| {
                    let first = start;
                    start += group.systems.len();
                    // Keep the focused card in view: this group's grid knows
                    // its own children, offset by where the group starts.
                    let reveal = focused_card.filter(|ix| (first..start).contains(ix)).map(|ix| ix - first);
                    let (scroll, flag) = (self.nav.scroll.clone(), self.nav.reveal.clone());
                    v_flex()
                        .gap_3()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_baseline()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_semibold()
                                        .text_color(if group.hot { t.danger } else { t.foreground })
                                        .child(group.heading),
                                )
                                .child(div().text_xs().font_medium().text_color(t.muted_foreground).child(group.systems.len().to_string())),
                        )
                        .child(
                            div()
                                .grid()
                                .grid_cols(columns)
                                .gap(px(GRID_GAP))
                                .when_some(reveal, |grid, ix| {
                                    grid.on_children_prepainted(move |bounds, window, _| {
                                        nav::reveal_child(&scroll, &flag, bounds.get(ix).copied(), window)
                                    })
                                })
                                .children(
                                    group
                                        .systems
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, system)| self.render_emulator_card(first + i, system, well, cx)),
                                ),
                        )
                        .into_any_element()
                })
                .collect()
        };

        v_flex()
            .p(px(super::library::CONTENT_PADDING))
            .gap_5()
            .child(header)
            .child(filters)
            .children(body)
            .child(div().text_xs().text_color(t.muted_foreground.opacity(0.8)).child(
                "Photos: Wikimedia Commons, mostly by Evan-Amos (public domain). Logos: Wikimedia Commons (public domain), trademarks of their owners. Credits in assets/.",
            ))
    }

    fn render_emulator_card(&self, index: usize, system: &'static SystemDef, well: Pixels, cx: &Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let accent = theme::accent(system.accent);
        let added = self.is_enabled(system);
        let installed = self.runnable.contains(system.id);
        let waiting = self.waiting_for(system).len();
        let suggested = waiting > 0 && !added;
        let group = SharedString::from(format!("emu-{}", system.id));
        let focused = self.focused_library() == Some(LibItem::Game(index));

        let badges = h_flex()
            .absolute()
            .left(px(8.))
            .top(px(8.))
            .gap_1()
            .when(added, |row| {
                row.child(
                    h_flex()
                        .h(px(22.))
                        .px(px(8.))
                        .gap_1()
                        .rounded(shape::sm())
                        .bg(white().opacity(0.92))
                        .text_xs()
                        .font_medium()
                        .text_color(rgb(0x1d1b20))
                        .child(Icon::new(IconName::Check).xsmall())
                        .child("In your list"),
                )
            })
            .when(suggested, |row| {
                row.child(shape::small_tag(Tag::danger()).child(format!("{} waiting", plural(waiting, "game"))))
            });

        // Add in the system's own colour, or Open when it's listed. Shown
        // on hover and while the card has keyboard focus.
        let action: AnyElement = if added {
            Button::new(("emu-open", index))
                .outline()
                .small()
                .label("Open")
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.select_system(system, window, cx);
                }))
                .into_any_element()
        } else {
            h_flex()
                .id(("emu-add", index))
                .h(px(32.))
                .px(px(12.))
                .gap_1()
                .rounded(shape::sm())
                .bg(accent)
                .text_sm()
                .font_medium()
                .text_color(white())
                .cursor_pointer()
                .hover(|s| s.opacity(0.9))
                .child(Icon::new(IconName::Plus).xsmall())
                .child("Add")
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.add_system_here(system, window, cx);
                }))
                .into_any_element()
        };

        let photo = div()
            .relative()
            .h(well)
            .bg(rgb(PHOTO_WELL))
            .flex()
            .items_center()
            .justify_center()
            .map(|w| match assets::system_photo(system) {
                Some(path) => w.child(img(path).size_full().object_fit(ObjectFit::Contain).when(added, |i| i.opacity(0.55))),
                None => w.child(system_tile(system, px(56.), px(28.), shape::md())),
            })
            .child(badges)
            .child(
                div()
                    .absolute()
                    .right(px(12.))
                    .bottom(px(12.))
                    .invisible()
                    .group_hover(group.clone(), |s| s.visible())
                    .when(focused, |s| s.visible())
                    .child(action),
            );

        let (dot, status) = if installed {
            (t.success, "Emulator installed")
        } else {
            (t.muted_foreground.opacity(0.5), "Emulator not installed yet")
        };
        let caption = h_flex()
            .gap_3()
            .p_3()
            .child(
                logo_plate(system, size(px(76.), px(36.)), shape::sm(), cx)
                    .unwrap_or_else(|| system_tile(system, px(26.), px(14.), shape::sm())),
            )
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

        v_flex()
            .id(("emu", index))
            .group(group)
            .relative()
            .rounded(t.radius_lg)
            .overflow_hidden()
            .border_1()
            .border_color(if suggested { t.danger.opacity(0.6) } else { t.border })
            .bg(if added { t.background } else { theme::card_bg(cx) })
            .cursor_pointer()
            .hover(|s| s.border_color(t.muted_foreground))
            .child(photo)
            .child(caption)
            .when(focused, |card| card.child(nav::ring(t.radius_lg, t.ring)))
            .on_click(cx.listener(move |this, _, window, cx| this.activate_emulator(index, window, cx)))
    }
}
