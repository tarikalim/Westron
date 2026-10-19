// Archives through macOS's bsdtar (libarchive): extract to a folder, pack a folder.

use super::{Edge, Engine, Step, edge};
use anyhow::bail;
use std::path::PathBuf;
use std::process::Command;

/// What libarchive reads on macOS.
const READS: &[&str] = &[
    "zip", "7z", "rar", "tar", "tgz", "tbz2", "txz", "jar", "cab", "cpio", "iso", "lha", "rpm",
    "deb",
];
/// What it writes (`tar -a` picks the format from the extension).
const WRITES: &[&str] = &["zip", "7z", "tar", "tgz", "tbz2", "txz"];
/// Single-file compressors, by their own tools.
const STREAMS: &[(&str, &str)] = &[("gz", "/usr/bin/gzip"), ("bz2", "/usr/bin/bzip2")];

pub struct ArchiveEngine {
    edges: Vec<Edge>,
}

impl ArchiveEngine {
    pub fn new() -> Self {
        let mut edges: Vec<Edge> = READS.iter().map(|f| edge(f, "folder", 10)).collect();
        edges.extend(STREAMS.iter().map(|(f, _)| edge(f, "folder", 10)));
        edges.extend(WRITES.iter().map(|t| edge("folder", t, 10)));
        ArchiveEngine { edges }
    }
}

impl Engine for ArchiveEngine {
    fn id(&self) -> &'static str {
        "archive"
    }

    fn edges(&self) -> &[Edge] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let out = if step.to == "folder" {
            step.out_dir.join(step.stem)
        } else {
            step.output("")
        };
        let r = if step.to == "folder" {
            std::fs::create_dir_all(&out)?;
            if let Some((_, tool)) = STREAMS.iter().find(|(f, _)| *f == step.from) {
                let file = std::fs::File::create(out.join(step.stem))?;
                Command::new(tool)
                    .arg("-dc")
                    .arg(step.input)
                    .stdout(file)
                    .output()?
            } else {
                Command::new("/usr/bin/tar")
                    .arg("-xf")
                    .arg(step.input)
                    .arg("-C")
                    .arg(&out)
                    .output()?
            }
        } else if step.to == "zip" {
            // As Finder's Compress, without the __MACOSX metadata entries.
            Command::new("/usr/bin/ditto")
                .args(["-c", "-k", "--norsrc", "--keepParent"])
                .arg(step.input)
                .arg(&out)
                .output()?
        } else {
            // The folder itself is the archive's top entry.
            let parent = step.input.parent().unwrap_or(step.input);
            let name = step.input.file_name().unwrap_or_default();
            Command::new("/usr/bin/tar")
                .arg("-acf")
                .arg(&out)
                .arg("-C")
                .arg(parent)
                .arg(name)
                .output()?
        };
        if !r.status.success() {
            bail!("{}", String::from_utf8_lossy(&r.stderr).trim());
        }
        // An archive of one folder extracts to that folder, not <stem>/<folder>.
        if step.to == "folder" {
            let entries: Vec<_> = std::fs::read_dir(&out)?
                .flatten()
                .map(|e| e.path())
                .collect();
            if let [only] = &entries[..]
                && only.is_dir()
            {
                let lifted = step.out_dir.join(".lifted");
                std::fs::rename(only, &lifted)?;
                std::fs::remove_dir(&out)?;
                std::fs::rename(&lifted, &out)?;
            }
        }
        (step.progress)(1.0);
        Ok(vec![out])
    }
}
