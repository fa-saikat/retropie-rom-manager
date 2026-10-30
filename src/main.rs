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
use gpui_kit::{size, AppContext, Bounds, KeyBinding, Size, TitlebarOptions, WindowBounds, WindowOptions};
use ui::scale::{self, px};

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
                cx.new(|cx| ui::RootView::new(window, cx))
            })
            .expect("failed to open window");

            cx.activate(true);
        });
}
