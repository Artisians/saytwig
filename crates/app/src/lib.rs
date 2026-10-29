//! Saytwig's compact native shell, backed by the matched GPUI Kit stack.
mod clipboard;
mod cloud;
mod model;
mod shell;
mod strings;
mod worker;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{px, rgb, size, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

/// Opens Saytwig without acquiring a microphone or downloading any model.
///
/// # Errors
/// Returns an error when Linux has no configured native desktop session.
pub fn run() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        anyhow::bail!(strings::t("desktop_required"));
    }
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::update(cx, |theme| {
            theme.font_size = px(14.);
            theme.radius = px(6.);
            theme.shadow = false;
            theme.background = rgb(0x101010).into();
            theme.foreground = rgb(0xf2f2f2).into();
            theme.border = rgb(0x303030).into();
            theme.input = rgb(0x191919).into();
            theme.muted = rgb(0x222222).into();
            theme.muted_foreground = rgb(0xb3b3b3).into();
            theme.accent = rgb(0x242424).into();
            theme.accent_foreground = rgb(0xf2f2f2).into();
            theme.button = rgb(0x191919).into();
            theme.button_hover = rgb(0x292929).into();
            theme.button_active = rgb(0x333333).into();
            theme.button_foreground = rgb(0xf2f2f2).into();
            theme.primary = rgb(0xeeeeee).into();
            theme.primary_foreground = rgb(0x111111).into();
            theme.primary_hover = rgb(0xffffff).into();
            theme.primary_active = rgb(0xd5d5d5).into();
            theme.button_primary = theme.primary;
            theme.button_primary_foreground = theme.primary_foreground;
            theme.button_primary_hover = theme.primary_hover;
            theme.button_primary_active = theme.primary_active;
            theme.ring = rgb(0xa5d6c3).into();
        });
        shell::bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(840.), px(590.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(640.), px(500.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Saytwig".into()),
                ..Default::default()
            }),
            app_id: Some("com.saytwig.desktop".into()),
            ..Default::default()
        };
        match gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| shell::Shell::new(window, cx))
        }) {
            Ok((window, view)) => {
                let main_window = window.window_id();
                cx.on_window_closed(move |cx, closed| {
                    if closed == main_window {
                        view.update(cx, |view, _| view.cancel());
                        cx.quit();
                    }
                })
                .detach();
                cx.activate(true);
            }
            Err(_) => {
                eprintln!("{}", strings::t("window_failed"));
                cx.quit();
            }
        }
    });
    Ok(())
}
