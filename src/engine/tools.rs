// Engines that run a program installed on the Mac: FFmpeg, LibreOffice, Pandoc, calibre, Ghostscript.
// Cost 25 = fallback that built-in engines beat.

use super::{Edge, Engine, Engines, Step, edge};
use crate::formats::{Category, KNOWN};
use anyhow::{Context, bail};
use std::io::BufRead as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};

pub fn all() -> Engines {
    vec![
        Arc::new(Ffmpeg::default()),
        Arc::new(LibreOffice::default()),
        Arc::new(Pandoc::default()),
        Arc::new(Calibre::default()),
        Arc::new(Ghostscript::default()),
    ]
}

/// First existing path, else the program on PATH (Homebrew's bins included, a GUI app has no shell PATH).
fn locate(name: &str, app_paths: &[&str]) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    app_paths
        .iter()
        .map(PathBuf::from)
        .chain(
            path.split(':')
                .chain(["/opt/homebrew/bin", "/usr/local/bin"])
                .map(|d| Path::new(d).join(name)),
        )
        .find(|p| p.is_file())
}

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    let r = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run {:?}", cmd.get_program()))?;
    if !r.status.success() {
        let err = String::from_utf8_lossy(&r.stderr);
        bail!(
            "{}",
            err.lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("failed")
                .trim()
        );
    }
    Ok(())
}

fn ids(c: Category) -> impl Iterator<Item = &'static str> {
    KNOWN.iter().filter(move |f| f.category == c).map(|f| f.id)
}

fn cross(edges: &mut Vec<Edge>, from: &[&'static str], to: &[&'static str], cost: u32) {
    for &f in from {
        for &t in to {
            if f != t {
                edges.push(edge(f, t, cost));
            }
        }
    }
}

macro_rules! located {
    ($ty:ident, $name:literal, [$($p:literal),*]) => {
        #[derive(Default)]
        pub struct $ty {
            bin: OnceLock<Option<PathBuf>>,
            edges: OnceLock<Vec<Edge>>,
        }
        impl $ty {
            fn bin(&self) -> Option<&PathBuf> {
                self.bin.get_or_init(|| locate($name, &[$($p),*])).as_ref()
            }
        }
    };
}

// ---- FFmpeg ----

/// Target → (muxer, video codec, audio codec).
const VIDEO: &[(&str, &str, &str, &str)] = &[
    ("mp4", "mp4", "libx264", "aac"),
    ("m4v", "mp4", "libx264", "aac"),
    ("mov", "mov", "libx264", "aac"),
    ("mkv", "matroska", "libx264", "aac"),
    ("flv", "flv", "libx264", "aac"),
    ("ts", "mpegts", "libx264", "aac"),
    ("m2ts", "mpegts", "libx264", "aac"),
    ("3gp", "3gp", "libx264", "aac"),
    ("3g2", "3g2", "libx264", "aac"),
    ("webm", "webm", "libvpx-vp9", "libopus"),
    ("avi", "avi", "mpeg4", "libmp3lame"),
    ("wmv", "asf", "wmv2", "wmav2"),
    ("mpg", "vob", "mpeg2video", "mp2"),
    ("ogv", "ogg", "libtheora", "libvorbis"),
    ("mxf", "mxf", "mpeg2video", "pcm_s16le"),
];
const AUDIO: &[(&str, &str, &str)] = &[
    ("mp3", "mp3", "libmp3lame"),
    ("m4a", "ipod", "aac"),
    ("m4b", "ipod", "aac"),
    ("aac", "adts", "aac"),
    ("wav", "wav", "pcm_s16le"),
    ("flac", "flac", "flac"),
    ("ogg", "ogg", "libvorbis"),
    ("opus", "opus", "libopus"),
    ("wma", "asf", "wmav2"),
    ("aiff", "aiff", "pcm_s16be"),
    ("ac3", "ac3", "ac3"),
    ("au", "au", "pcm_s16be"),
    ("caf", "caf", "pcm_s16be"),
    ("weba", "webm", "libopus"),
    ("mka", "matroska", "libopus"),
    ("mp2", "mp2", "mp2"),
    ("wv", "wv", "wavpack"),
    ("tta", "tta", "tta"),
    ("w64", "w64", "pcm_s16le"),
    ("eac3", "eac3", "eac3"),
];
const SUBS: &[(&str, &str)] = &[("srt", "srt"), ("ass", "ass"), ("vtt", "webvtt")];
const GIF_FILTER: &str =
    "fps=12,scale='min(640,iw)':-2:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse";

located!(Ffmpeg, "ffmpeg", []);

impl Engine for Ffmpeg {
    fn id(&self) -> &'static str {
        "ffmpeg"
    }

    fn edges(&self) -> &[Edge] {
        self.edges.get_or_init(|| {
            let videos: Vec<_> = ids(Category::Video).collect();
            let audios: Vec<_> = ids(Category::Audio).collect();
            let v: Vec<_> = VIDEO.iter().map(|t| t.0).collect();
            let a: Vec<_> = AUDIO.iter().map(|t| t.0).collect();
            let mut e = vec![];
            cross(&mut e, &videos, &v, 10);
            cross(&mut e, &videos, &a, 10);
            cross(&mut e, &audios, &a, 10);
            cross(&mut e, &videos, &["srt"], 10);
            cross(&mut e, &videos, &["ass", "vtt"], 11);
            cross(&mut e, &videos, &["gif", "apng"], 15);
            cross(&mut e, &["gif", "apng"], &["mp4", "webm", "mov"], 10);
            e
        })
    }

    fn unavailable(&self) -> Option<String> {
        self.bin()
            .is_none()
            .then(|| "FFmpeg is not installed (brew install ffmpeg)".into())
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let ffmpeg = self.bin().context("FFmpeg is not installed")?;
        let out = step.output("");
        let mut cmd = Command::new(ffmpeg);
        cmd.args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostats",
            "-progress",
            "pipe:1",
            "-i",
        ])
        .arg(step.input);
        if let Some((_, mux, vc, ac)) = VIDEO.iter().find(|t| t.0 == step.to) {
            // Even sizes for 4:2:0 encoders; yuv420p plays everywhere.
            cmd.args([
                "-c:v",
                vc,
                "-c:a",
                ac,
                "-vf",
                "scale=trunc(iw/2)*2:trunc(ih/2)*2",
                "-pix_fmt",
                "yuv420p",
            ]);
            if step.to == "mp4" || step.to == "m4v" || step.to == "mov" {
                cmd.args(["-movflags", "+faststart"]);
            }
            cmd.args(["-f", mux]);
        } else if let Some((_, mux, ac)) = AUDIO.iter().find(|t| t.0 == step.to) {
            cmd.args(["-vn", "-c:a", ac, "-f", mux]);
        } else if let Some((_, mux)) = SUBS.iter().find(|t| t.0 == step.to) {
            cmd.args(["-map", "0:s:0", "-f", mux]);
        } else if step.to == "gif" {
            cmd.args(["-an", "-filter_complex", GIF_FILTER, "-loop", "0"]);
        } else if step.to == "apng" {
            cmd.args(["-an", "-plays", "0", "-f", "apng"]);
        }
        cmd.arg(&out);
        let duration = duration_us(ffmpeg, step.input);
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        for line in std::io::BufReader::new(child.stdout.take().unwrap())
            .lines()
            .map_while(Result::ok)
        {
            if let (Some(us), Some(total)) = (
                line.strip_prefix("out_time_us=")
                    .and_then(|v| v.parse::<f64>().ok()),
                duration,
            ) {
                (step.progress)((us / total).clamp(0.0, 1.0) as f32);
            }
        }
        let r = child.wait_with_output()?;
        if !r.status.success() {
            let _ = std::fs::remove_file(&out);
            let err = String::from_utf8_lossy(&r.stderr);
            bail!("ffmpeg: {}", err.lines().last().unwrap_or("failed"));
        }
        Ok(vec![out])
    }
}

fn duration_us(ffmpeg: &Path, input: &Path) -> Option<f64> {
    let probe = ffmpeg.with_file_name("ffprobe");
    let r = Command::new(probe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(input)
        .output()
        .ok()?;
    String::from_utf8_lossy(&r.stdout)
        .trim()
        .parse::<f64>()
        .ok()
        .map(|s| s * 1e6)
}

// ---- LibreOffice ----

const WRITER: &[&str] = &["docx", "doc", "odt", "rtf", "fodt", "ott"];
const CALC: &[&str] = &["xlsx", "xls", "ods", "csv", "xlt", "ots"];
const IMPRESS: &[&str] = &["pptx", "ppt", "odp", "otp"];
const WRITER_IMPORT: &[&str] = &[
    "dot", "wps", "wpd", "lwp", "abw", "pages", "sdw", "cwk", "mcw", "602", "psw", "docm", "dotx",
    "dotm",
];
const CALC_IMPORT: &[&str] = &["et", "numbers", "sdc", "fods", "xlsm", "xltx", "tsv"];
const IMPRESS_IMPORT: &[&str] = &["key", "dps", "sda", "fodp", "pptm", "potx", "ppsx"];
const DRAW: &[&str] = &["pub", "cdr", "vsd", "odg", "fodg", "emf", "wmf", "cgm"];

located!(
    LibreOffice,
    "soffice",
    ["/Applications/LibreOffice.app/Contents/MacOS/soffice"]
);

impl Engine for LibreOffice {
    fn id(&self) -> &'static str {
        "libreoffice"
    }

    fn edges(&self) -> &[Edge] {
        self.edges.get_or_init(|| {
            let mut e = vec![];
            let cat = |a: &[&'static str], b: &[&'static str]| [a, b].concat();
            cross(&mut e, WRITER, &cat(WRITER, &["txt", "html", "pdf"]), 25);
            cross(&mut e, CALC, &cat(CALC, &["html", "pdf"]), 25);
            cross(&mut e, IMPRESS, &cat(IMPRESS, &["pdf"]), 25);
            cross(&mut e, &["txt", "html"], &["docx", "odt", "pdf"], 28);
            cross(
                &mut e,
                WRITER_IMPORT,
                &cat(WRITER, &["pdf", "txt", "html"]),
                25,
            );
            cross(&mut e, CALC_IMPORT, &cat(CALC, &["pdf", "html"]), 25);
            cross(&mut e, IMPRESS_IMPORT, &cat(IMPRESS, &["pdf"]), 25);
            cross(
                &mut e,
                DRAW,
                &["pdf", "odg", "svg", "png", "emf", "wmf"],
                25,
            );
            cross(&mut e, &["svg"], &["odg", "emf", "wmf"], 25);
            cross(&mut e, &["pdf"], &["odg"], 25);
            e
        })
    }

    fn unavailable(&self) -> Option<String> {
        self.bin()
            .is_none()
            .then(|| "LibreOffice is not installed (brew install --cask libreoffice)".into())
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let soffice = self.bin().context("LibreOffice is not installed")?;
        // Own profile per run: an open LibreOffice is untouched and runs can overlap.
        let profile = step.out_dir.join(".lo-profile");
        let filter = match step.to {
            "txt" => "txt:Text (encoded):UTF8",
            "csv" => "csv:Text - txt - csv (StarCalc):44,34,76",
            t => t,
        };
        let work = step.out_dir.join(".lo-out");
        run(Command::new(soffice)
            .arg(format!(
                "-env:UserInstallation=file://{}",
                profile.display()
            ))
            .args([
                "--headless",
                "--norestore",
                "--convert-to",
                filter,
                "--outdir",
            ])
            .arg(&work)
            .arg(step.input))?;
        let ext = step.output("").extension().unwrap_or_default().to_owned();
        let made =
            work.join(Path::new(step.input.file_stem().unwrap_or_default()).with_extension(ext));
        let out = step.output("");
        std::fs::rename(&made, &out).with_context(|| "LibreOffice wrote nothing")?;
        let _ = std::fs::remove_dir_all(&profile);
        (step.progress)(1.0);
        Ok(vec![out])
    }
}

// ---- Pandoc ----

const READERS: &[(&str, &str)] = &[
    ("md", "markdown"),
    ("html", "html"),
    ("docx", "docx"),
    ("odt", "odt"),
    ("rtf", "rtf"),
    ("rst", "rst"),
    ("tex", "latex"),
    ("org", "org"),
    ("adoc", "asciidoc"),
    ("typ", "typst"),
    ("mediawiki", "mediawiki"),
    ("dbk", "docbook"),
    ("ipynb", "ipynb"),
    ("textile", "textile"),
    ("man", "man"),
    ("opml", "opml"),
];
const WRITERS: &[(&str, &str)] = &[
    ("md", "gfm"),
    ("html", "html"),
    ("docx", "docx"),
    ("odt", "odt"),
    ("rtf", "rtf"),
    ("txt", "plain"),
    ("rst", "rst"),
    ("tex", "latex"),
    ("epub", "epub"),
    ("org", "org"),
    ("adoc", "asciidoc"),
    ("typ", "typst"),
    ("mediawiki", "mediawiki"),
    ("dbk", "docbook5"),
    ("ipynb", "ipynb"),
    ("textile", "textile"),
    ("man", "man"),
    ("opml", "opml"),
    ("pptx", "pptx"),
];
const MARKUP: &[&str] = &[
    "rst",
    "tex",
    "org",
    "adoc",
    "typ",
    "mediawiki",
    "dbk",
    "ipynb",
    "textile",
    "man",
    "opml",
];
const SLIDES: &[&str] = &[
    "md",
    "rst",
    "org",
    "adoc",
    "typ",
    "mediawiki",
    "textile",
    "ipynb",
];

located!(Pandoc, "pandoc", []);

impl Engine for Pandoc {
    fn id(&self) -> &'static str {
        "pandoc"
    }

    fn edges(&self) -> &[Edge] {
        self.edges.get_or_init(|| {
            let mut e = vec![];
            cross(&mut e, &["docx", "odt", "html", "rtf"], &["md"], 10);
            cross(
                &mut e,
                &["md"],
                &["docx", "odt", "html", "rtf", "txt", "epub"],
                10,
            );
            cross(&mut e, &["html"], &["docx", "odt"], 14);
            cross(&mut e, &["docx", "odt"], &["html"], 14);
            cross(
                &mut e,
                MARKUP,
                &["md", "html", "docx", "odt", "rtf", "epub", "txt"],
                10,
            );
            cross(
                &mut e,
                &[&["md", "html", "docx", "odt"][..], MARKUP].concat(),
                MARKUP,
                11,
            );
            cross(&mut e, SLIDES, &["pptx"], 30);
            e
        })
    }

    fn unavailable(&self) -> Option<String> {
        self.bin()
            .is_none()
            .then(|| "Pandoc is not installed (brew install pandoc)".into())
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let pandoc = self.bin().context("Pandoc is not installed")?;
        let r = READERS
            .iter()
            .find(|r| r.0 == step.from)
            .context("no Pandoc reader")?
            .1;
        let w = WRITERS
            .iter()
            .find(|w| w.0 == step.to)
            .context("no Pandoc writer")?
            .1;
        let out = step.output("");
        let mut cmd = Command::new(pandoc);
        cmd.args(["-f", r, "-t", w, "-o"]).arg(&out).arg(step.input);
        if matches!(step.to, "html" | "tex" | "dbk" | "man" | "opml" | "typ") {
            cmd.arg("--standalone");
        }
        run(&mut cmd)?;
        (step.progress)(1.0);
        Ok(vec![out])
    }
}

// ---- calibre ----

const CAL_IN: &[&str] = &[
    "lit", "lrf", "chm", "pdb", "pml", "rb", "snb", "tcr", "oeb", "mobi", "azw3", "azw", "azw4",
    "prc", "fb2", "htmlz", "txtz", "cbz", "cbr", "cb7",
];
const CAL_OUT: &[&str] = &[
    "mobi", "azw3", "lit", "lrf", "pdb", "pml", "rb", "snb", "tcr", "fb2", "htmlz", "txtz",
];

located!(
    Calibre,
    "ebook-convert",
    ["/Applications/calibre.app/Contents/MacOS/ebook-convert"]
);

impl Engine for Calibre {
    fn id(&self) -> &'static str {
        "calibre"
    }

    fn edges(&self) -> &[Edge] {
        self.edges.get_or_init(|| {
            let mut e = vec![];
            cross(&mut e, CAL_IN, &["epub"], 20);
            cross(&mut e, &["epub"], CAL_OUT, 20);
            cross(&mut e, &[&["epub"][..], CAL_IN].concat(), &["pdf"], 22);
            e
        })
    }

    fn unavailable(&self) -> Option<String> {
        self.bin()
            .is_none()
            .then(|| "calibre is not installed (brew install --cask calibre)".into())
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let bin = self.bin().context("calibre is not installed")?;
        let out = step.output("");
        let mut cmd = Command::new(bin);
        cmd.arg(step.input).arg(&out);
        if step.to == "pdf" {
            cmd.args(["--paper-size", "a4", "--pdf-page-numbers"]);
        }
        run(&mut cmd)?;
        (step.progress)(1.0);
        Ok(vec![out])
    }
}

// ---- Ghostscript ----

located!(Ghostscript, "gs", []);

impl Engine for Ghostscript {
    fn id(&self) -> &'static str {
        "ghostscript"
    }

    fn edges(&self) -> &[Edge] {
        self.edges.get_or_init(|| {
            let mut e = vec![];
            cross(&mut e, &["eps", "ps", "ai"], &["pdf"], 10);
            cross(&mut e, &["pdf"], &["eps", "ps"], 10);
            e
        })
    }

    fn unavailable(&self) -> Option<String> {
        self.bin()
            .is_none()
            .then(|| "Ghostscript is not installed (brew install ghostscript)".into())
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let gs = self.bin().context("Ghostscript is not installed")?;
        let device = match step.to {
            "eps" => "eps2write",
            "ps" => "ps2write",
            _ => "pdfwrite",
        };
        let out = step.output("");
        run(Command::new(gs)
            .args(["-dSAFER", "-dBATCH", "-dNOPAUSE", "-dQUIET", "-dEPSCrop"])
            .arg(format!("-sDEVICE={device}"))
            .arg(format!("-sOutputFile={}", out.display()))
            .arg(step.input))?;
        (step.progress)(1.0);
        Ok(vec![out])
    }
}
