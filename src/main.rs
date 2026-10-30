mod assets;
mod config;
mod detect;
mod launch;
mod library;
mod scraper;
mod skyscraper_setup;
mod systems;
mod theme;
mod ui;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{size, AppContext, Bounds, KeyBinding, Size, TitlebarOptions, WindowBounds, WindowOptions};
use ui::scale::{self, px};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,wgpu_hal=off")).init();
    log::info!("retropie-rom-manager {}", env!("CARGO_PKG_VERSION"));

    gpui_kit::application()
        .with_assets(assets::AppAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            scale::init(cx);
            Theme::change(ThemeMode::Light, None, cx);
            scale::apply_to_theme(cx);
            // We keep the native title bar, so sheets can use the full height.
            Theme::update(cx, |t| t.sheet.margin_top = px(0.));
            cx.bind_keys([KeyBinding::new("/", ui::FocusSearch, Some(ui::KEY_CONTEXT))]);

            // Below this the sidebar + card grid start wrapping badly.
            let min_size = scale::fit_display(size(px(1024.), px(680.)), 1., cx);
            let wanted = scale::fit_display(size(px(1280.), px(800.)), 0.9, cx);
            let window_size = Size {
                width: wanted.width.max(min_size.width),
                height: wanted.height.max(min_size.height),
            };

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, window_size, cx))),
                window_min_size: Some(min_size),
                titlebar: Some(TitlebarOptions {
                    title: Some("ROM Manager".into()),
                    ..Default::default()
                }),
                app_id: Some("retropie-rom-manager".into()),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::RootView::new(window, cx))
            })
            .expect("failed to open window");

            cx.activate(true);
        });
}
