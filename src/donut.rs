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
    /// Slot under the cursor: a target index, or `targets.len()` for the centre.
    hovered: Option<usize>,
}

impl Donut {
    /// The whole wedge around a format picks it, not only its pill; `targets.len()` is the centre.
    fn slot(&self, p: Point<Pixels>) -> Option<usize> {
        let (dx, dy) = (f32::from(p.x) - SIZE / 2., f32::from(p.y) - SIZE / 2.);
        let r = dx.hypot(dy);
        if r <= 55. {
            return Some(self.targets.len());
        }
        let n = self.targets.len() as f32;
        let a = (dy.atan2(dx) + std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU);
        Some((a / (std::f32::consts::TAU / n)).round() as usize % self.targets.len())
    }
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
    cx.open_window(options, |_, cx| {
        cx.new(|_| Donut {
            targets,
            count,
            hovered: None,
        })
    })
    .ok()
    .map(Into::into)
}

impl Render for Donut {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let n = self.targets.len() as f32;
        let centre = self.targets.len();
        let hot = |s: Div, cx: &App| {
            s.bg(cx.theme().primary)
                .text_color(cx.theme().primary_foreground)
        };
        let mut ring = div()
            .size(px(SIZE))
            .relative()
            .on_drag_move(
                cx.listener(|this, e: &DragMoveEvent<ExternalPaths>, _, cx| {
                    let slot = this.slot(e.event.position);
                    if slot != this.hovered {
                        this.hovered = slot;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                let files = paths.paths().to_vec();
                match this.slot(window.mouse_position()) {
                    Some(i) if i == this.targets.len() => crate::show_window(Some(files), cx),
                    Some(i) => {
                        let to = this.targets[i];
                        converter(cx).update(cx, |c, cx| c.start(files, to, cx));
                    }
                    None => return,
                }
                window.remove_window();
            }))
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
                    .bg(if self.hovered == Some(centre) {
                        theme.drop_target
                    } else {
                        theme.background
                    })
                    .child(format!("{} file(s) · more…", self.count)),
            );
        for (i, &to) in self.targets.iter().enumerate() {
            let a = std::f32::consts::TAU * i as f32 / n - std::f32::consts::FRAC_PI_2;
            let (x, y) = (SIZE / 2. + RING * a.cos(), SIZE / 2. + RING * a.sin());
            let name = formats::by_id(to).map_or(to, |f| f.name);
            let pill = div()
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
                .child(name);
            ring = ring.child(if self.hovered == Some(i) {
                hot(pill, cx)
            } else {
                pill
            });
        }
        ring
    }
}

#[cfg(test)]
mod tests {
    use super::{Donut, SIZE};
    use gpui_kit::{point, px};

    #[test]
    fn slot_picks_the_whole_wedge() {
        let d = Donut {
            targets: vec!["png", "jpg", "webp", "pdf"],
            count: 1,
            hovered: None,
        };
        let at = |x: f32, y: f32| d.slot(point(px(SIZE / 2. + x), px(SIZE / 2. + y)));
        assert_eq!(at(0., 0.), Some(4)); // centre
        assert_eq!(at(0., -150.), Some(0)); // top, past the pill
        assert_eq!(at(40., -80.), Some(0)); // just right of top
        assert_eq!(at(70., -20.), Some(1)); // right
        assert_eq!(at(0., 80.), Some(2)); // bottom, inside the pill
        assert_eq!(at(-150., 0.), Some(3)); // left
        assert_eq!(at(169., -169.), Some(1)); // window corner
    }
}
