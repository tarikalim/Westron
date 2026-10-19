// `westron convert|formats|engines|watch`: the same engines and routes as the app.

use crate::{engine, formats, jobs, route::Planner};
use anyhow::{Context, bail};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

const USAGE: &str = "\
usage:
  westron convert <files…> --to <format> [-o <dir>] [--merge]
  westron split <file.pdf> [-o <dir>]  one PDF per page
  westron formats <file|format>      formats a file can become
  westron engines                    which engines can run
  westron watch <dir> --to <format>  convert every new file in <dir>
  westron mcp                        MCP server over stdio";

struct Args {
    files: Vec<PathBuf>,
    to: Option<String>,
    out: Option<PathBuf>,
    merge: bool,
}

fn parse(args: &[String]) -> anyhow::Result<Args> {
    let mut a = Args {
        files: vec![],
        to: None,
        out: None,
        merge: false,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--to" | "-t" => a.to = Some(it.next().context("--to needs a format")?.to_lowercase()),
            "-o" | "--out" => a.out = Some(it.next().context("-o needs a folder")?.into()),
            "--merge" => a.merge = true,
            "-h" | "--help" => bail!("{USAGE}"),
            f => a.files.push(f.into()),
        }
    }
    Ok(a)
}

/// The format id a user typed: an id or any of its extensions.
pub fn format_id(s: &str) -> anyhow::Result<&'static str> {
    formats::KNOWN
        .iter()
        .find(|f| f.id == s || f.extensions.contains(&s))
        .map(|f| f.id)
        .with_context(|| format!("unknown format '{s}'"))
}

pub fn run(cmd: &str, args: &[String]) -> anyhow::Result<()> {
    match cmd {
        "convert" => convert(parse(args)?),
        "formats" => {
            let what = args.first().context(USAGE)?;
            let from = formats::detect(Path::new(what))
                .map(|f| f.id)
                .map_or_else(|| format_id(what), Ok)?;
            let planner = Planner::new(&engine::available());
            for to in planner.targets(from) {
                let route = planner.plan(from, to).unwrap();
                let via: Vec<_> = route
                    .iter()
                    .map(|h| format!("{}({})", h.to, h.engine.id()))
                    .collect();
                println!("{to:<10} {}", via.join(" → "));
            }
            Ok(())
        }
        "engines" => {
            for e in engine::all() {
                match e.unavailable() {
                    None => println!("✓ {:<12} {} edges", e.id(), e.edges().len()),
                    Some(why) => println!("✗ {:<12} {why}", e.id()),
                }
            }
            Ok(())
        }
        "watch" => watch(parse(args)?),
        "split" => {
            let a = parse(args)?;
            for f in &a.files {
                let dir = a
                    .out
                    .clone()
                    .unwrap_or_else(|| f.parent().unwrap_or(Path::new(".")).to_path_buf());
                for out in engine::pdf::split(f, &dir, &formats::stem_of(f))? {
                    println!("{}", out.display());
                }
            }
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

fn convert(a: Args) -> anyhow::Result<()> {
    let to = format_id(a.to.as_deref().context("--to is required")?)?;
    if a.files.is_empty() {
        bail!("{USAGE}");
    }
    if a.merge {
        if to != "pdf" {
            bail!("--merge writes one PDF; use --to pdf");
        }
        let dir = a
            .out
            .clone()
            .unwrap_or_else(|| a.files[0].parent().unwrap_or(Path::new(".")).to_path_buf());
        let out = jobs::free_path(&dir, &format!("{}.pdf", formats::stem_of(&a.files[0])));
        engine::pdf::merge(&a.files, &out)?;
        println!("{}", out.display());
        return Ok(());
    }
    let planner = Planner::new(&engine::available());
    let mut failed = 0;
    for (f, r) in a
        .files
        .iter()
        .zip(jobs::convert_many(&planner, &a.files, to, a.out.as_deref()))
    {
        match r {
            Ok(outs) => outs.iter().for_each(|o| println!("{}", o.display())),
            Err(e) => {
                eprintln!("{}: {e:#}", f.display());
                failed += 1;
            }
        }
    }
    match failed {
        0 => Ok(()),
        n => bail!("{n} of {} failed", a.files.len()),
    }
}

fn watch(a: Args) -> anyhow::Result<()> {
    let to = format_id(a.to.as_deref().context("--to is required")?)?;
    let dir = a.files.first().context(USAGE)?.clone();
    let planner = Planner::new(&engine::available());
    let list = || -> HashSet<PathBuf> {
        std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect()
    };
    let mut seen = list();
    eprintln!("watching {} → {to}", dir.display());
    loop {
        std::thread::sleep(Duration::from_secs(2));
        let now = list();
        let mut fresh = vec![];
        for f in now.difference(&seen) {
            let skip = f
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                || formats::detect(f).is_none_or(|fmt| fmt.id == to);
            // A file still being copied in grows between two looks; wait for the next round.
            if skip {
                continue;
            }
            if std::fs::metadata(f)
                .and_then(|m| m.modified())
                .is_ok_and(|t| t.elapsed().unwrap_or_default() < Duration::from_secs(2))
            {
                fresh.push(f.clone());
                continue;
            }
            match jobs::convert(&planner, f, to, a.out.as_deref(), &|_| {}) {
                Ok(outs) => outs.iter().for_each(|o| println!("{}", o.display())),
                Err(e) => eprintln!("{}: {e:#}", f.display()),
            }
        }
        // Results land in the folder too: count them as seen.
        seen = list();
        for f in fresh {
            seen.remove(&f);
        }
    }
}
