// The ring of formats that opens at the cursor while files are dragged with ⌥ held.

use crate::app::{common_targets, converter};
use crate::formats;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use std::path::PathBuf;

const SIZE: f32 = 340.;
const RING: f32 = 120.;
const MAX: usize = 10;

pub struct Donut {
    targets: Vec<&'static str>,
    count: usize,
}

/// Opens the ring centred on `at`; returns its window to close on drag end.
pub fn open(files: Vec<PathBuf>, at: (f32, f32), cx: &mut App) -> Option<AnyWindowHandle> {
    let planner = converter(cx).read(cx).planner.clone();
    let mut targets = common_targets(&planner, &files);
    if targets.is_empty() {
        return None;
    }
    // Same kind first (image → images), then the rest, cheapest first within each.
    let cat = formats::detect(&files[0]).map(|f| f.category);
    targets.sort_by_key(|t| formats::by_id(t).map(|f| f.category) != cat);
    targets.truncate(MAX);
    let bounds = Bounds::new(
        point(px(at.0 - SIZE / 2.), px(at.1 - SIZE / 2.)),
        size(px(SIZE), px(SIZE)),
    );
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        kind: WindowKind::PopUp,
        focus: false,
        is_movable: false,
        is_resizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let count = files.len();
    cx.open_window(options, |_, cx| cx.new(|_| Donut { targets, count }))
        .ok()
        .map(Into::into)
}

impl Render for Donut {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let n = self.targets.len() as f32;
        let mut ring = div()
            .size(px(SIZE))
            .relative()
            .child(
                div()
                    .absolute()
                    .size(px(SIZE - 20.))
                    .left(px(10.))
                    .top(px(10.))
                    .rounded_full()
                    .bg(theme.popover.opacity(0.92))
                    .border_1()
                    .border_color(theme.border),
            )
            .child(
                // Centre: open the window to pick from every format.
                div()
                    .id("centre")
                    .absolute()
                    .size(px(110.))
                    .left(px(SIZE / 2. - 55.))
                    .top(px(SIZE / 2. - 55.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .bg(theme.background)
                    .drag_over::<ExternalPaths>(|s, _, _, cx| s.bg(cx.theme().drop_target))
                    .on_drop(|paths: &ExternalPaths, window, cx| {
                        let files = paths.paths().to_vec();
                        crate::show_window(Some(files), cx);
                        window.remove_window();
                    })
                    .child(format!("{} file(s) · more…", self.count)),
            );
        for (i, &to) in self.targets.iter().enumerate() {
            let a = std::f32::consts::TAU * i as f32 / n - std::f32::consts::FRAC_PI_2;
            let (x, y) = (SIZE / 2. + RING * a.cos(), SIZE / 2. + RING * a.sin());
            let name = formats::by_id(to).map_or(to, |f| f.name);
            ring = ring.child(
                div()
                    .id(i)
                    .absolute()
                    .left(px(x - 34.))
                    .top(px(y - 17.))
                    .w(px(68.))
                    .h(px(34.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .bg(theme.secondary)
                    .border_1()
                    .border_color(theme.border)
                    .drag_over::<ExternalPaths>(|s, _, _, cx| {
                        s.bg(cx.theme().primary)
                            .text_color(cx.theme().primary_foreground)
                    })
                    .on_drop(move |paths: &ExternalPaths, window, cx| {
                        let files = paths.paths().to_vec();
                        converter(cx).update(cx, |c, cx| c.start(files, to, cx));
                        window.remove_window();
                    })
                    .child(name),
            );
        }
        ring
    }
}
