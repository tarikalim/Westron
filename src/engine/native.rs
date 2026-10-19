// macOS's own converters: textutil (word processing), afconvert (audio), avconvert (video).
// Cost 9: they beat FFmpeg and LibreOffice for what they cover, with nothing to install.

use super::{Edge, Engine, Step, edge};
use anyhow::{Context, bail};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    let r = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run {:?}", cmd.get_program()))?;
    if !r.status.success() {
        bail!("{}", String::from_utf8_lossy(&r.stderr).trim());
    }
    Ok(())
}

fn all_pairs(formats: &[&'static str], cost: u32) -> Vec<Edge> {
    formats
        .iter()
        .flat_map(|&f| {
            formats
                .iter()
                .filter(move |&&t| t != f)
                .map(move |&t| edge(f, t, cost))
        })
        .collect()
}

/// Done when the file is there and not empty (the tools exit 0 on some failures).
fn written(step: &Step, out: PathBuf) -> anyhow::Result<Vec<PathBuf>> {
    if !out.metadata().is_ok_and(|m| m.len() > 0) {
        bail!("nothing written for {} → {}", step.from, step.to);
    }
    (step.progress)(1.0);
    Ok(vec![out])
}

pub struct TextUtil(Vec<Edge>);

impl TextUtil {
    pub fn new() -> Self {
        TextUtil(all_pairs(&["txt", "html", "rtf", "doc", "docx", "odt"], 9))
    }
}

impl Engine for TextUtil {
    fn id(&self) -> &'static str {
        "textutil"
    }

    fn edges(&self) -> &[Edge] {
        &self.0
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let out = step.output("");
        run(Command::new("/usr/bin/textutil")
            .args(["-convert", step.to, "-encoding", "UTF-8", "-output"])
            .arg(&out)
            .arg(step.input))?;
        written(step, out)
    }
}

/// Target → (afconvert file type, data format).
const AUDIO_OUT: &[(&str, &str, &str)] = &[
    ("wav", "WAVE", "LEI16"),
    ("aiff", "AIFF", "BEI16"),
    ("m4a", "m4af", "aac"),
    ("caf", "caff", "BEI16"),
    ("flac", "flac", "flac"),
    ("aac", "adts", "aac"),
];
/// What Core Audio reads.
const AUDIO_IN: &[&str] = &[
    "wav", "aiff", "m4a", "caf", "flac", "aac", "mp3", "au", "ac3", "m4b",
];

pub struct AfConvert(Vec<Edge>);

impl AfConvert {
    pub fn new() -> Self {
        let mut e = vec![];
        for &f in AUDIO_IN {
            for &(t, _, _) in AUDIO_OUT {
                if f != t {
                    e.push(edge(f, t, 9));
                }
            }
        }
        AfConvert(e)
    }
}

impl Engine for AfConvert {
    fn id(&self) -> &'static str {
        "afconvert"
    }

    fn edges(&self) -> &[Edge] {
        &self.0
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let (_, kind, data) = AUDIO_OUT
            .iter()
            .find(|t| t.0 == step.to)
            .context("no afconvert target")?;
        let out = step.output("");
        run(Command::new("/usr/bin/afconvert")
            .args(["-f", kind, "-d", data])
            .arg(step.input)
            .arg(&out))?;
        written(step, out)
    }
}

const VIDEO: &[&str] = &["mov", "mp4", "m4v"];

pub struct AvConvert(Vec<Edge>);

impl AvConvert {
    pub fn new() -> Self {
        let mut e = all_pairs(VIDEO, 9);
        e.extend(VIDEO.iter().map(|f| edge(f, "m4a", 9)));
        AvConvert(e)
    }
}

impl Engine for AvConvert {
    fn id(&self) -> &'static str {
        "avconvert"
    }

    fn edges(&self) -> &[Edge] {
        &self.0
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let preset = if step.to == "m4a" {
            "PresetAppleM4A"
        } else {
            "PresetHighestQuality"
        };
        let out = step.output("");
        run(Command::new("/usr/bin/avconvert")
            .args(["-p", preset, "-s"])
            .arg(step.input)
            .arg("-o")
            .arg(&out))?;
        written(step, out)
    }
}
