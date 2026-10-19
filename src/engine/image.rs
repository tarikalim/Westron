// Raster images: macOS ImageIO through `sips`, and the `image` crate for what ImageIO does not write.

use super::{Edge, Engine, Step, edge};
use anyhow::{Context, bail};
use std::path::PathBuf;
use std::process::Command;

/// What ImageIO reads on macOS 14+ (`sips --formats`).
const READS: &[&str] = &[
    "png", "jpg", "webp", "tiff", "bmp", "gif", "ico", "avif", "heic", "jxl", "jp2", "psd", "psb",
    "tga", "icns", "exr", "hdr", "dcm", "raw", "sgi", "dds", "cur", "svg", "ppm",
];
/// ImageIO's writers; sips's `format` name when it differs from the id.
const WRITES: &[(&str, &str)] = &[
    ("png", "png"),
    ("jpg", "jpeg"),
    ("tiff", "tiff"),
    ("bmp", "bmp"),
    ("gif", "gif"),
    ("heic", "heic"),
    ("avif", "avif"),
    ("jp2", "jp2"),
    ("psd", "psd"),
    ("tga", "tga"),
    ("exr", "exr"),
    ("ico", "ico"),
    ("icns", "icns"),
    ("pdf", "pdf"),
];
/// The `image` crate: decoders it has, and the encoders ImageIO lacks.
const CRATE_READS: &[&str] = &[
    "png", "jpg", "gif", "bmp", "tiff", "tga", "ico", "qoi", "ppm", "webp",
];
const CRATE_WRITES: &[&str] = &["webp", "qoi", "ppm"];

pub struct ImageEngine {
    edges: Vec<Edge>,
}

impl ImageEngine {
    pub fn new() -> Self {
        let mut edges = vec![];
        for &from in READS {
            for &(to, _) in WRITES {
                if from != to {
                    edges.push(edge(from, to, 10));
                }
            }
        }
        for &from in CRATE_READS {
            for &to in CRATE_WRITES {
                if from != to {
                    edges.push(edge(from, to, 10));
                }
            }
        }
        // QOI has no ImageIO reader; the crate brings it to PNG for the rest.
        edges.push(edge("qoi", "png", 10));
        ImageEngine { edges }
    }
}

impl Engine for ImageEngine {
    fn id(&self) -> &'static str {
        "image"
    }

    fn edges(&self) -> &[Edge] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let out = step.output("");
        let sips = WRITES.iter().find(|(id, _)| *id == step.to);
        match sips {
            Some((_, format)) if READS.contains(&step.from) => {
                let mut cmd = Command::new("/usr/bin/sips");
                // ICO holds at most 256 px, ICNS wants a square: fit and pad.
                match step.to {
                    "ico" => cmd.args(["-Z", "256", "-p", "256", "256"]),
                    "icns" => cmd.args(["-Z", "512", "-p", "512", "512"]),
                    _ => &mut cmd,
                };
                let r = cmd
                    .args(["-s", "format", format])
                    .arg(step.input)
                    .arg("--out")
                    .arg(&out)
                    .output()?;
                // sips exits 0 on many failures; the written file is the proof.
                if !out.metadata().is_ok_and(|m| m.len() > 0) {
                    bail!("sips: {}", String::from_utf8_lossy(&r.stderr).trim());
                }
            }
            _ => {
                let img = ::image::open(step.input)
                    .with_context(|| format!("read {}", step.input.display()))?;
                // QOI and lossless WebP take RGBA8; PPM takes RGB8.
                let img = if step.to == "ppm" {
                    img.to_rgb8().into()
                } else {
                    ::image::DynamicImage::from(img.to_rgba8())
                };
                img.save(&out)
                    .with_context(|| format!("write {}", out.display()))?;
            }
        }
        (step.progress)(1.0);
        Ok(vec![out])
    }
}
