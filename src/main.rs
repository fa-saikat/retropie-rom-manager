mod assets;
mod browse;
mod cli;
mod config;
mod detect;
mod launch;
mod library;
mod scraper;
mod skyscraper_setup;
mod systems;
mod theme;
mod ui;

use gpui_kit::component::{dialog, Theme, ThemeMode};
use gpui_kit::{size, AppContext, Bounds, KeyBinding, NoAction, Size, TitlebarOptions, WindowBounds, WindowOptions};
use ui::nav;
use ui::scale::{self, px};

/// The library's search field (kit inputs use the `Input` context).
const SEARCH: &str = "LibrarySearch > Input";
const ADD_SYSTEM_SEARCH: &str = "AddSystemSearch > Input";

fn main() {
    let options = match cli::parse(std::env::args().skip(1)) {
        Ok(cli::Command::Run(options)) => options,
        Ok(cli::Command::Help) => {
            println!("{}", cli::help());
            return;
        }
        Ok(cli::Command::Version) => {
            println!("{}", cli::version());
            return;
        }
        Err(err) => {
            eprintln!("error: {err}\n\n{}\n\nFor more information, try '--help'.", cli::USAGE);
            std::process::exit(2);
        }
    };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,wgpu_hal=off")).init();
    log::info!("{}", cli::version());

    gpui_kit::application()
        .with_assets(assets::AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            scale::init(cx);
            let mode = match options.theme {
                Some(cli::Theme::Dark) => ThemeMode::Dark,
                Some(cli::Theme::Light) | None => ThemeMode::Light,
            };
            Theme::change(mode, None, cx);
            scale::apply_to_theme(cx);
            // We keep the native title bar, so sheets can use the full height.
            Theme::update(cx, |t| t.sheet.margin_top = px(0.));
            cx.bind_keys([
                KeyBinding::new("/", ui::FocusSearch, Some(ui::KEY_CONTEXT)),
                KeyBinding::new("ctrl-q", ui::Quit, None),
                // Gamepads mapped to keys often have Backspace but no Esc, so
                // it closes dialogs too. Text fields still get it first.
                KeyBinding::new("backspace", dialog::Cancel, Some("Dialog")),
                KeyBinding::new("up", ui::MoveUp, Some(ui::BROWSER_KEY_CONTEXT)),
                KeyBinding::new("down", ui::MoveDown, Some(ui::BROWSER_KEY_CONTEXT)),
                KeyBinding::new("enter", ui::Activate, Some(ui::BROWSER_KEY_CONTEXT)),
                KeyBinding::new("backspace", ui::Back, Some(ui::BROWSER_KEY_CONTEXT)),
                KeyBinding::new("left", ui::FocusPlaces, Some(ui::BROWSER_KEY_CONTEXT)),
                KeyBinding::new("right", ui::FocusFiles, Some(ui::BROWSER_KEY_CONTEXT)),
                KeyBinding::new("escape", ui::Close, Some(ui::BROWSER_KEY_CONTEXT)),
                // The main window, while it has focus itself (#42).
                KeyBinding::new("up", nav::Up, Some(nav::KEY_CONTEXT)),
                KeyBinding::new("down", nav::Down, Some(nav::KEY_CONTEXT)),
                KeyBinding::new("left", nav::Left, Some(nav::KEY_CONTEXT)),
                KeyBinding::new("right", nav::Right, Some(nav::KEY_CONTEXT)),
                KeyBinding::new("enter", nav::Activate, Some(nav::KEY_CONTEXT)),
                KeyBinding::new("backspace", nav::Back, Some(nav::KEY_CONTEXT)),
                KeyBinding::new("p", nav::Play, Some(nav::KEY_CONTEXT)),
                KeyBinding::new("s", nav::Scrape, Some(nav::KEY_CONTEXT)),
                KeyBinding::new("x", nav::Remove, Some(nav::KEY_CONTEXT)),
                // The search field: Down and Enter go on to the results,
                // Backspace on an empty field and Esc go back.
                KeyBinding::new("down", nav::Leave, Some(SEARCH)),
                KeyBinding::new("enter", nav::Leave, Some(SEARCH)),
                KeyBinding::new("backspace", nav::Back, Some(SEARCH)),
                KeyBinding::new("escape", dialog::Cancel, Some(SEARCH)),
                // Buttons in dialogs and the sheet. Enter presses the one
                // with focus, instead of the kit's "Enter is OK".
                KeyBinding::new("up", nav::Up, Some(nav::GROUP_CONTEXT)),
                KeyBinding::new("down", nav::Down, Some(nav::GROUP_CONTEXT)),
                KeyBinding::new("left", nav::Left, Some(nav::GROUP_CONTEXT)),
                KeyBinding::new("right", nav::Right, Some(nav::GROUP_CONTEXT)),
                KeyBinding::new("enter", NoAction, Some(nav::GROUP_CONTEXT)),
                KeyBinding::new("p", nav::Play, Some(nav::SHEET_CONTEXT)),
                KeyBinding::new("s", nav::Scrape, Some(nav::SHEET_CONTEXT)),
                KeyBinding::new("x", nav::Remove, Some(nav::SHEET_CONTEXT)),
                KeyBinding::new("backspace", dialog::Cancel, Some("Sheet")),
                KeyBinding::new("down", nav::Leave, Some(ADD_SYSTEM_SEARCH)),
                KeyBinding::new("enter", nav::Leave, Some(ADD_SYSTEM_SEARCH)),
                KeyBinding::new("backspace", nav::Back, Some(ADD_SYSTEM_SEARCH)),
            ]);
            cx.on_action(|_: &ui::Quit, cx| ui::request_quit(cx));

            // Below this the sidebar + card grid start wrapping badly.
            let min_size = scale::fit_display(size(px(1024.), px(680.)), 1., cx);
            let wanted = scale::fit_display(size(px(1280.), px(800.)), 0.9, cx);
            let window_size = Size {
                width: wanted.width.max(min_size.width),
                height: wanted.height.max(min_size.height),
            };

            let window_options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, window_size, cx))),
                window_min_size: Some(min_size),
                titlebar: Some(TitlebarOptions {
                    title: Some("ROM Manager".into()),
                    ..Default::default()
                }),
                app_id: Some("retropie-rom-manager".into()),
                ..Default::default()
            };
            gpui_kit::open_window(window_options, cx, move |window, cx| {
                // Opening with fullscreen bounds isn't honoured on X11, so
                // ask the window manager once the window exists. The app
                // offers no way to toggle it afterwards.
                if options.fullscreen {
                    window.toggle_fullscreen();
                }
                cx.new(|cx| {
                    let mut view = ui::RootView::new(window, cx);
                    // No mouse is expected in fullscreen, so show where the keys are.
                    if options.fullscreen {
                        view.show_focus();
                    }
                    view
                })
            })
            .expect("failed to open window");

            cx.activate(true);
        });
}
