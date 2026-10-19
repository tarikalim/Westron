// The window: drop zone, the formats the dropped files can become, and the conversions.

use crate::formats::{self, Category};
use crate::route::Planner;
use crate::{engine, jobs};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, button::Button, h_flex, v_flex};
use gpui_kit::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

enum State {
    Running,
    Done(Vec<PathBuf>),
    Failed(String),
}

struct Job {
    name: String,
    to: &'static str,
    progress: Arc<AtomicU32>,
    state: State,
}

/// Conversions and the planner; one per app, shared by the window and the donut.
pub struct Converter {
    pub planner: Arc<Planner>,
    pending: Vec<PathBuf>,
    jobs: Vec<Job>,
    ticking: bool,
}

struct ConverterGlobal(Entity<Converter>);
impl Global for ConverterGlobal {}

pub fn init(cx: &mut App) {
    let planner = Arc::new(Planner::new(&engine::available()));
    let c = cx.new(|_| Converter {
        planner,
        pending: vec![],
        jobs: vec![],
        ticking: false,
    });
    cx.set_global(ConverterGlobal(c));
}

pub fn converter(cx: &App) -> Entity<Converter> {
    cx.global::<ConverterGlobal>().0.clone()
}

/// Formats every one of `files` can become, cheapest first for the first file.
pub fn common_targets(planner: &Planner, files: &[PathBuf]) -> Vec<&'static str> {
    let mut sets = files.iter().map(|f| {
        formats::detect(f)
            .map(|fmt| planner.targets(fmt.id))
            .unwrap_or_default()
    });
    let first = sets.next().unwrap_or_default();
    let rest: Vec<Vec<_>> = sets.collect();
    first
        .into_iter()
        .filter(|t| rest.iter().all(|r| r.contains(t)))
        .collect()
}

impl Converter {
    pub fn set_pending(&mut self, files: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.pending = files;
        cx.notify();
    }

    pub fn start(&mut self, files: Vec<PathBuf>, to: &'static str, cx: &mut Context<Self>) {
        for file in files {
            let progress = Arc::new(AtomicU32::new(0));
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.jobs.insert(
                0,
                Job {
                    name,
                    to,
                    progress: progress.clone(),
                    state: State::Running,
                },
            );
            let planner = self.planner.clone();
            let task = cx.background_spawn(async move {
                jobs::convert(&planner, &file, to, None, &|p| {
                    progress.store(p.to_bits(), Ordering::Relaxed)
                })
            });
            let job_progress = self.jobs[0].progress.clone();
            cx.spawn(async move |this, cx| {
                let r = task.await;
                this.update(cx, |this, cx| {
                    if let Some(job) = this
                        .jobs
                        .iter_mut()
                        .find(|j| Arc::ptr_eq(&j.progress, &job_progress))
                    {
                        job.state = match r {
                            Ok(files) => State::Done(files),
                            Err(e) => State::Failed(format!("{e:#}")),
                        };
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        self.jobs.truncate(50);
        self.tick(cx);
        cx.notify();
    }

    /// Redraws progress while something runs; stops when idle.
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.ticking {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let running = this
                    .update(cx, |this, cx| {
                        cx.notify();
                        let r = this.jobs.iter().any(|j| matches!(j.state, State::Running));
                        this.ticking = r;
                        r
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
    }
}

pub struct AppView {
    converter: Entity<Converter>,
}

impl AppView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let converter = converter(cx);
        cx.observe(&converter, |_, _, cx| cx.notify()).detach();
        AppView { converter }
    }
}

const CATEGORY_ORDER: &[Category] = &[
    Category::Image,
    Category::Pdf,
    Category::Document,
    Category::Spreadsheet,
    Category::Presentation,
    Category::Text,
    Category::Ebook,
    Category::Video,
    Category::Audio,
    Category::Subtitle,
    Category::Archive,
    Category::Vector,
    Category::Hwp,
    Category::Data,
    Category::Font,
    Category::Cad,
    Category::Model3D,
];

impl Render for AppView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let c = self.converter.read(cx);
        let targets = common_targets(&c.planner, &c.pending);
        let pending = c.pending.clone();

        let drop_zone = div()
            .id("drop")
            .h(px(140.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radius * 2.)
            .border_2()
            .border_color(theme.border)
            .text_color(theme.muted_foreground)
            .drag_over::<ExternalPaths>(|s, _, _, cx| {
                s.bg(cx.theme().drop_target).border_color(cx.theme().ring)
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.converter
                    .update(cx, |c, cx| c.set_pending(paths.paths().to_vec(), cx));
            }))
            .child(if pending.is_empty() {
                "Drop files here, or hold ⌥ while dragging anywhere".to_string()
            } else {
                let kinds: Vec<_> = pending
                    .iter()
                    .filter_map(|p| formats::detect(p))
                    .map(|f| f.name)
                    .collect();
                format!("{} file(s) · {}", pending.len(), dedup(kinds).join(", "))
            });

        let mut picker = v_flex().gap_2();
        if !pending.is_empty() && targets.is_empty() {
            picker = picker.child(
                div()
                    .text_color(theme.muted_foreground)
                    .child("No conversion for these files with the installed engines."),
            );
        }
        for cat in CATEGORY_ORDER {
            let in_cat: Vec<_> = targets
                .iter()
                .copied()
                .filter(|t| formats::by_id(t).is_some_and(|f| f.category == *cat))
                .collect();
            if in_cat.is_empty() {
                continue;
            }
            let mut row = h_flex().flex_wrap().gap_1();
            for to in in_cat {
                let name = formats::by_id(to).map_or(to, |f| f.name);
                let files = pending.clone();
                row = row.child(Button::new(to).label(name).small().outline().on_click(
                    cx.listener(move |this, _, _, cx| {
                        let files = files.clone();
                        this.converter.update(cx, |c, cx| {
                            c.pending.clear();
                            c.start(files, to, cx);
                        });
                    }),
                ));
            }
            picker = picker.child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("{cat:?}")),
                    )
                    .child(row),
            );
        }

        let mut list = v_flex().gap_1();
        for (i, job) in c.jobs.iter().enumerate() {
            let to = formats::by_id(job.to).map_or(job.to, |f| f.name);
            let (status, color, files) = match &job.state {
                State::Running => {
                    let p = f32::from_bits(job.progress.load(Ordering::Relaxed));
                    (format!("{:.0}%", p * 100.), theme.muted_foreground, vec![])
                }
                State::Done(files) => ("✓".into(), theme.green, files.clone()),
                State::Failed(e) => (e.clone(), theme.red, vec![]),
            };
            list = list.child(
                h_flex()
                    .id(("job", i))
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded(theme.radius)
                    .hover(|s| s.bg(cx.theme().secondary))
                    .when(!files.is_empty(), |el| el.cursor_pointer())
                    .on_click(move |_, _, _| reveal(&files))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .child(format!("{} → {to}", job.name)),
                    )
                    .child(
                        div()
                            .max_w(px(260.))
                            .truncate()
                            .text_color(color)
                            .child(status),
                    ),
            );
        }

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .p_4()
            .gap_3()
            .child(drop_zone)
            .child(picker)
            .child(div().id("jobs").flex_1().overflow_y_scroll().child(list))
    }
}

fn dedup(mut v: Vec<&'static str>) -> Vec<&'static str> {
    let mut seen = std::collections::HashSet::new();
    v.retain(|x| seen.insert(*x));
    v
}

/// Selects the results in Finder.
pub fn reveal(files: &[PathBuf]) {
    if !files.is_empty() {
        let _ = std::process::Command::new("/usr/bin/open")
            .arg("-R")
            .args(files)
            .spawn();
    }
}

use gpui_kit::prelude::FluentBuilder as _;
