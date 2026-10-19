// Runs a route for one file in a scratch folder and moves the results next to the original.

use crate::engine::Step;
use crate::formats;
use crate::route::{Hop, Planner};
use anyhow::{Context, anyhow};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn scratch() -> anyhow::Result<PathBuf> {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("westron-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `dir/name`, or `dir/stem (2).ext` and up when taken.
pub fn free_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let ext = formats::extension_of(Path::new(name));
    let stem = if ext.is_empty() || first.is_dir() {
        name.to_string()
    } else {
        name[..name.len() - ext.len() - 1].to_string()
    };
    (2..)
        .map(|i| {
            dir.join(if ext.is_empty() {
                format!("{stem} ({i})")
            } else {
                format!("{stem} ({i}).{ext}")
            })
        })
        .find(|p| !p.exists())
        .unwrap()
}

/// Converts `input` to format `to`; results go to `out_dir` (default: next to the input).
pub fn convert(
    planner: &Planner,
    input: &Path,
    to: &str,
    out_dir: Option<&Path>,
    progress: &(dyn Fn(f32) + Sync),
) -> anyhow::Result<Vec<PathBuf>> {
    let input = &std::path::absolute(input)?;
    let from =
        formats::detect(input).ok_or_else(|| anyhow!("unknown format: {}", input.display()))?;
    let route = planner
        .plan(from.id, to)
        .ok_or_else(|| anyhow!("no route from {} to {to}", from.name))?;
    let dest = match out_dir {
        Some(d) => d.to_path_buf(),
        None => input.parent().context("no parent folder")?.to_path_buf(),
    };
    let work = scratch()?;
    let stem = formats::stem_of(input);
    let result = run_route(&route, input, &stem, &work, progress);
    let moved = result.and_then(|files| {
        files
            .into_iter()
            .map(|f| {
                let name = f
                    .file_name()
                    .context("no name")?
                    .to_string_lossy()
                    .into_owned();
                let target = free_path(&dest, &name);
                std::fs::rename(&f, &target)
                    .with_context(|| format!("move to {}", target.display()))?;
                Ok(target)
            })
            .collect()
    });
    let _ = std::fs::remove_dir_all(&work);
    moved
}

fn run_route(
    route: &[Hop],
    input: &Path,
    stem: &str,
    work: &Path,
    progress: &(dyn Fn(f32) + Sync),
) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = vec![input.to_path_buf()];
    let n = route.len() as f32;
    for (i, hop) in route.iter().enumerate() {
        let out_dir = work.join(i.to_string());
        std::fs::create_dir_all(&out_dir)?;
        let mut next = vec![];
        let count = files.len() as f32;
        for (j, f) in files.iter().enumerate() {
            // Several files from one hop (PDF pages) keep their own names into the next.
            let own = if files.len() > 1 {
                formats::stem_of(f)
            } else {
                stem.to_string()
            };
            let report = |p: f32| progress((i as f32 + (j as f32 + p) / count) / n);
            let step = Step {
                input: f,
                from: hop.from,
                to: hop.to,
                out_dir: &out_dir,
                stem: &own,
                progress: &report,
            };
            next.extend(
                hop.engine
                    .convert(&step)
                    .with_context(|| format!("{} → {} ({})", hop.from, hop.to, hop.engine.id()))?,
            );
        }
        files = next;
    }
    progress(1.0);
    Ok(files)
}

/// Converts `files` in parallel; one result per file, in order.
pub fn convert_many(
    planner: &Planner,
    files: &[PathBuf],
    to: &str,
    out_dir: Option<&Path>,
) -> Vec<anyhow::Result<Vec<PathBuf>>> {
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(files.len().max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<_> = files.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(f) = files.get(i) else { break };
                    *results[i].lock().unwrap() = Some(convert(planner, f, to, out_dir, &|_| {}));
                }
            });
        }
    });
    results
        .into_iter()
        .map(|r| r.into_inner().unwrap().unwrap())
        .collect()
}
