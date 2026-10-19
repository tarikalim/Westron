mod app;
mod cli;
mod donut;
mod engine;
mod formats;
mod jobs;
mod mac;
mod mcp;
mod mem;
mod route;

use futures::StreamExt as _;
use gpui_kit::*;
use std::path::PathBuf;

gpui_kit::actions!(westron, [Quit, CloseWindow]);

struct MainWindow(Option<AnyWindowHandle>);
impl Global for MainWindow {}

/// Opens the window, or brings it to the front; `files` become the ones to convert.
pub fn show_window(files: Option<Vec<PathBuf>>, cx: &mut App) {
    if let Some(files) = files {
        app::converter(cx).update(cx, |c, cx| c.set_pending(files, cx));
    }
    let existing = cx.global::<MainWindow>().0;
    if let Some(w) = existing
        && w.update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        cx.activate(true);
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(560.), px(620.)), cx)),
        titlebar: Some(TitlebarOptions {
            title: Some("Westron".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let w = gpui_kit::open_window(options, cx, |window, cx| {
        gpui_kit::component::Theme::sync_system_appearance(Some(window), cx);
        cx.new(app::AppView::new)
    })
    .ok()
    .map(|(w, _)| w);
    cx.set_global(MainWindow(w));
    cx.activate(true);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(cmd) = args.get(1).filter(|a| !a.starts_with("-psn")) {
        let r = match cmd.as_str() {
            "mcp" => mcp::run(),
            // Internal: one rich text → PDF conversion, on this process's main thread.
            "__pdf" if args.len() == 4 => engine::print::render(args[2].as_ref(), args[3].as_ref()),
            "__pdftext" if args.len() == 4 => {
                engine::print::extract(args[2].as_ref(), args[3].as_ref())
            }
            _ => cli::run(cmd, &args[2..]),
        };
        if let Err(e) = r {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    mem::ensure_space_efficient_malloc();
    let application = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    // Closing the window keeps the app in the menu bar, watching for ⌥-drags.
    application.on_reopen(|cx| show_window(None, cx));
    application.run(|cx| {
        gpui_kit::init(cx);
        // Menu bar app: no Dock icon; Quit is in the menu bar item.
        cx.set_activation_policy(ActivationPolicy::Accessory);
        mem::cap_glyph_cache(0);
        app::init(cx);
        cx.set_global(MainWindow(None));

        cx.bind_keys([
            KeyBinding::new("secondary-q", Quit, None),
            KeyBinding::new("secondary-w", CloseWindow, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &CloseWindow, cx| {
            if let Some(w) = cx.active_window() {
                let _ = w.update(cx, |_, window, _| window.remove_window());
            }
        });
        cx.set_menus(vec![Menu {
            name: "Westron".into(),
            items: vec![
                MenuItem::action("Close Window", CloseWindow),
                MenuItem::action("Quit", Quit),
            ],
            disabled: false,
        }]);

        let mtm = objc2::MainThreadMarker::new().expect("main thread");
        let mut events = mac::start(mtm);
        cx.spawn(async move |cx: &mut AsyncApp| {
            let mut donut: Option<AnyWindowHandle> = None;
            while let Some(e) = events.next().await {
                cx.update(|cx| match e {
                    mac::Event::DragStart(files, at) => {
                        if let Some(w) = donut.take() {
                            let _ = w.update(cx, |_, window, _| window.remove_window());
                        }
                        donut = donut::open(files, at, cx);
                    }
                    mac::Event::DragEnd => {
                        // After the drop reaches the ring; a missed drop just closes it.
                        if let Some(w) = donut.take() {
                            cx.spawn(async move |cx: &mut AsyncApp| {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(400))
                                    .await;
                                let _ = cx.update(|cx| {
                                    w.update(cx, |_, window, _| window.remove_window())
                                });
                            })
                            .detach();
                        }
                    }
                    mac::Event::OpenWindow => show_window(None, cx),
                    mac::Event::Quit => cx.quit(),
                });
            }
        })
        .detach();

        if std::env::var_os("WESTRON_HIDDEN").is_none() {
            show_window(None, cx);
        }
    });
}
