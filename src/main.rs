#![cfg_attr(windows, windows_subsystem = "windows")]

use cli_batch_runner::{app::BatchRunner, config};
use gpui_kit::assets::AllAssets;
use gpui_kit::{AppContext, Bounds, WindowBounds, WindowOptions, point, px, size};

fn main() {
    gpui_kit::application().with_assets(AllAssets).run(|cx| {
        gpui_kit::init(cx);
        let (mut app_config, warning, save_allowed) = config::load();
        app_config.normalize();
        cli_batch_runner::theme::load_builtin_themes(cx)
            .expect("bundled GPUI Kit themes must parse successfully");
        app_config.theme = cli_batch_runner::theme::resolve_theme_name(&app_config.theme, cx);
        cli_batch_runner::theme::apply(&app_config.theme, cx);

        let default_size = size(px(700.), px(540.));
        let bounds = if let (Some(x), Some(y)) = (app_config.window_x, app_config.window_y) {
            Bounds {
                origin: point(px(x), px(y)),
                size: default_size,
            }
        } else {
            Bounds::centered(None, default_size, cx)
        };

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_resizable: true,
            window_min_size: Some(size(px(650.), px(420.))),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| BatchRunner::new(app_config, warning, save_allowed, true, window, cx))
        })
        .expect("could not open CLI Batch Runner window");
    });
}
