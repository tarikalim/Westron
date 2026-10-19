// The contract every conversion engine implements, and the list of engines.

pub mod archive;
pub mod data;
pub mod image;
pub mod native;
pub mod pdf;
pub mod print;
pub mod text;
pub mod tools;

use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Edge {
    pub from: &'static str,
    pub to: &'static str,
    pub cost: u32,
}

pub const fn edge(from: &'static str, to: &'static str, cost: u32) -> Edge {
    Edge { from, to, cost }
}

/// One conversion: `input` (of format `from`) into `to`, results written into `out_dir`.
pub struct Step<'a> {
    pub input: &'a Path,
    pub from: &'a str,
    pub to: &'a str,
    pub out_dir: &'a Path,
    /// The file name the results start with (the original's stem).
    pub stem: &'a str,
    pub progress: &'a (dyn Fn(f32) + Sync),
}

impl Step<'_> {
    /// `<out_dir>/<stem>.<ext>`; engines that write several files add a suffix.
    pub fn output(&self, suffix: &str) -> PathBuf {
        let ext = crate::formats::by_id(self.to)
            .and_then(|f| f.extensions.first())
            .copied()
            .unwrap_or(self.to);
        self.out_dir.join(format!("{}{suffix}.{ext}", self.stem))
    }
}

pub trait Engine: Send + Sync {
    fn id(&self) -> &'static str;
    fn edges(&self) -> &[Edge];
    /// None when it can run; else why not (not installed, ...).
    fn unavailable(&self) -> Option<String> {
        None
    }
    /// Converts and returns the files it wrote.
    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>>;
}

pub type Engines = Vec<Arc<dyn Engine>>;

pub fn all() -> Engines {
    let mut v: Engines = vec![
        Arc::new(image::ImageEngine::new()),
        Arc::new(pdf::PdfEngine::new()),
        Arc::new(archive::ArchiveEngine::new()),
        Arc::new(native::TextUtil::new()),
        Arc::new(native::AfConvert::new()),
        Arc::new(native::AvConvert::new()),
        Arc::new(data::DataEngine::new()),
        Arc::new(text::Subtitles::new()),
        Arc::new(text::Markdown::new()),
        Arc::new(print::RichPdf::new()),
    ];
    v.extend(tools::all());
    v
}

/// The engines that can run now.
pub fn available() -> Engines {
    all()
        .into_iter()
        .filter(|e| e.unavailable().is_none())
        .collect()
}
