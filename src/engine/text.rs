// Subtitles (SRT, WebVTT, ASS/SSA) and Markdown → HTML.

use super::{Edge, Engine, Step, edge};
use anyhow::{Context, bail};
use std::path::PathBuf;

pub struct Subtitles(Vec<Edge>);

impl Subtitles {
    pub fn new() -> Self {
        let mut e = vec![];
        for f in ["srt", "vtt", "ass", "ssa"] {
            for t in ["srt", "vtt", "ass"] {
                if f != t {
                    e.push(edge(f, t, 10));
                }
            }
        }
        Subtitles(e)
    }
}

/// Start and end in milliseconds, text with '\n' between lines.
#[derive(Debug, PartialEq)]
pub struct Cue(u64, u64, String);

/// "01:02:03,456", "02:03.456" (VTT) or "1:02:03.45" (ASS).
fn time(s: &str) -> Option<u64> {
    let s = s.trim().replace(',', ".");
    let (hms, frac) = s.split_once('.').unwrap_or((&s, "0"));
    let ms = match frac.len() {
        1 => frac.parse::<u64>().ok()? * 100,
        2 => frac.parse::<u64>().ok()? * 10,
        _ => frac.get(..3)?.parse().ok()?,
    };
    let parts: Vec<u64> = hms
        .split(':')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let secs = parts.iter().fold(0, |acc, p| acc * 60 + p);
    Some(secs * 1000 + ms)
}

fn stamp(ms: u64, sep: char) -> String {
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

pub fn parse(text: &str, from: &str) -> Vec<Cue> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    if from == "ass" || from == "ssa" {
        let mut fields: Vec<String> = vec![];
        let mut cues = vec![];
        for line in text.lines() {
            if let Some(f) = line.strip_prefix("Format:") {
                fields = f.split(',').map(|x| x.trim().to_lowercase()).collect();
            } else if let Some(d) = line.strip_prefix("Dialogue:") {
                let at = |name: &str| fields.iter().position(|f| f == name);
                let (Some(s), Some(e), Some(t)) = (at("start"), at("end"), at("text")) else {
                    continue;
                };
                let cols: Vec<&str> = d.splitn(fields.len(), ',').collect();
                let (Some(s), Some(e), Some(raw)) = (
                    cols.get(s).and_then(|v| time(v)),
                    cols.get(e).and_then(|v| time(v)),
                    cols.get(t),
                ) else {
                    continue;
                };
                // Drop {\override} tags; \N and \n are line breaks.
                let mut txt = String::new();
                let mut depth = 0;
                for c in raw.trim().chars() {
                    match c {
                        '{' => depth += 1,
                        '}' if depth > 0 => depth -= 1,
                        c if depth == 0 => txt.push(c),
                        _ => {}
                    }
                }
                cues.push(Cue(s, e, txt.replace("\\N", "\n").replace("\\n", "\n")));
            }
        }
        return cues;
    }
    // SRT and VTT: blocks with a "start --> end" line; VTT's header and NOTE blocks have none.
    text.split("\n\n")
        .filter_map(|block| {
            let mut lines = block.lines().skip_while(|l| !l.contains("-->"));
            let (a, b) = lines.next()?.split_once("-->")?;
            let end = b.split_whitespace().next()?;
            Some(Cue(
                time(a)?,
                time(end)?,
                lines.collect::<Vec<_>>().join("\n"),
            ))
        })
        .collect()
}

pub fn render(cues: &[Cue], to: &str) -> String {
    match to {
        "vtt" => {
            let body: Vec<String> = cues
                .iter()
                .map(|Cue(s, e, t)| format!("{} --> {}\n{t}\n", stamp(*s, '.'), stamp(*e, '.')))
                .collect();
            format!("WEBVTT\n\n{}", body.join("\n"))
        }
        "ass" => {
            let cs = |ms: u64| {
                format!(
                    "{}:{:02}:{:02}.{:02}",
                    ms / 3_600_000,
                    ms / 60_000 % 60,
                    ms / 1000 % 60,
                    ms % 1000 / 10
                )
            };
            let mut out = String::from(
                "[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, OutlineColour, BorderStyle, Outline, Shadow, Alignment, MarginV\nStyle: Default,Arial,48,&H00FFFFFF,&H00000000,1,2,0,2,40\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
            );
            for Cue(s, e, t) in cues {
                out.push_str(&format!(
                    "Dialogue: 0,{},{},Default,,0,0,0,,{}\n",
                    cs(*s),
                    cs(*e),
                    t.replace('\n', "\\N")
                ));
            }
            out
        }
        _ => cues
            .iter()
            .enumerate()
            .map(|(i, Cue(s, e, t))| {
                format!(
                    "{}\n{} --> {}\n{t}\n",
                    i + 1,
                    stamp(*s, ','),
                    stamp(*e, ',')
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

impl Engine for Subtitles {
    fn id(&self) -> &'static str {
        "subtitles"
    }

    fn edges(&self) -> &[Edge] {
        &self.0
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let bytes =
            std::fs::read(step.input).with_context(|| format!("read {}", step.input.display()))?;
        let cues = parse(&String::from_utf8_lossy(&bytes), step.from);
        if cues.is_empty() {
            bail!("no subtitles found in {}", step.input.display());
        }
        let out = step.output("");
        std::fs::write(&out, render(&cues, step.to))?;
        (step.progress)(1.0);
        Ok(vec![out])
    }
}

pub struct Markdown(Vec<Edge>);

impl Markdown {
    pub fn new() -> Self {
        // 9: beats Pandoc's direct edge; textutil takes the HTML on to DOCX, RTF, ODT.
        Markdown(vec![edge("md", "html", 9)])
    }
}

impl Engine for Markdown {
    fn id(&self) -> &'static str {
        "markdown"
    }

    fn edges(&self) -> &[Edge] {
        &self.0
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        use pulldown_cmark::{Options, Parser, html};
        let md = std::fs::read_to_string(step.input)?;
        let mut body = String::new();
        html::push_html(
            &mut body,
            Parser::new_ext(
                &md,
                Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
            ),
        );
        let title = step.stem.replace('&', "&amp;").replace('<', "&lt;");
        let out = step.output("");
        std::fs::write(
            &out,
            format!(
                "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{title}</title></head>\n<body>\n{body}</body></html>\n"
            ),
        )?;
        (step.progress)(1.0);
        Ok(vec![out])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitles_round_trip() {
        let srt = "1\n00:00:01,500 --> 00:00:03,000\nHello\nworld\n\n2\n00:01:02,000 --> 00:01:04,250\nBye\n";
        let cues = parse(srt, "srt");
        assert_eq!(cues[0], Cue(1500, 3000, "Hello\nworld".into()));
        for to in ["vtt", "ass", "srt"] {
            assert_eq!(parse(&render(&cues, to), to), cues, "{to}");
        }
        let vtt = "WEBVTT\n\nNOTE x\n\n00:05.000 --> 00:06.000 align:start\nHi\n";
        assert_eq!(parse(vtt, "vtt"), vec![Cue(5000, 6000, "Hi".into())]);
    }
}
