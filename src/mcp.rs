//! `westron mcp`: MCP server over stdio (newline-delimited JSON-RPC 2.0), no auth.
//! The `westron` skill (skills/westron) is served through the Skills extension.

use crate::{cli::format_id, engine, formats, jobs, route::Planner};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::io::{BufRead as _, Write as _};
use std::path::{Path, PathBuf};

const PROTOCOL: &str = "2025-06-18";
const SUPPORTED: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const SKILLS: &str = "io.modelcontextprotocol/skills";

/// The skill's files: (path inside the skill, text).
const SKILL_FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../skills/westron/SKILL.md")),
    (
        "references/recipes.md",
        include_str!("../skills/westron/references/recipes.md"),
    ),
];

const INSTRUCTIONS: &str = "Local file conversion on this Mac: images, PDF, archives, and (when installed) \
video/audio (FFmpeg), office documents (LibreOffice), markup (Pandoc), e-books (calibre), EPS/PS (Ghostscript). \
Results are written next to the input unless out_dir is given. Call `formats` to see what a file can become. The `westron` skill explains the tools.";

pub fn run() -> Result<()> {
    let planner = Planner::new(&engine::available());
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle(&planner, &line) {
            writeln!(out, "{resp}")?;
            out.flush()?;
        }
    }
    Ok(())
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// One JSON-RPC message in, the response out (None for notifications).
pub fn handle(planner: &Planner, line: &str) -> Option<Value> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Some(rpc_error(Value::Null, -32700, &format!("parse error: {e}"))),
    };
    let id = msg.get("id").cloned();
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        return match id {
            Some(id) if msg.get("result").is_none() && msg.get("error").is_none() => {
                Some(rpc_error(id, -32600, "invalid request"))
            }
            _ => None,
        };
    };
    let id = id?;
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    let info = json!({ "name": "westron", "version": env!("CARGO_PKG_VERSION") });
    let capabilities =
        json!({ "tools": { "listChanged": false }, "resources": {}, "extensions": { SKILLS: {} } });
    let cache = |mut v: Value| {
        v["resultType"] = json!("complete");
        v["ttlMs"] = json!(300_000);
        v["cacheScope"] = json!("public");
        v
    };
    let result = match method {
        "initialize" => {
            let version = params["protocolVersion"]
                .as_str()
                .filter(|v| SUPPORTED.contains(v))
                .unwrap_or(PROTOCOL);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": capabilities,
                "serverInfo": info,
                "instructions": INSTRUCTIONS
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if tools().iter().any(|t| t["name"] == name) {
                let (text, is_error) = match call(planner, name, &args) {
                    Ok(t) => (t, false),
                    Err(e) => (format!("{e:#}"), true),
                };
                Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
            } else {
                Err((-32602, format!("unknown tool: {name}")))
            }
        }
        "server/discover" => Ok(
            json!({ "capabilities": capabilities, "serverInfo": info, "instructions": INSTRUCTIONS }),
        ),
        "skills/list" => Ok(cache(json!({ "skills": [skill_entry()] }))),
        "skills/get" => match params["uri"].as_str() {
            Some(uri) if uri == skill_uri("SKILL.md") => {
                Ok(cache(json!({ "skill": skill_entry() })))
            }
            _ => Err((-32602, "unknown skill".into())),
        },
        "resources/list" => Ok(
            json!({ "resources": SKILL_FILES.iter().map(|(path, text)| json!({
            "uri": skill_uri(path), "name": format!("westron/{path}"), "mimeType": "text/markdown", "size": text.len()
        })).collect::<Vec<_>>() }),
        ),
        "resources/read" => match SKILL_FILES
            .iter()
            .find(|(path, _)| params["uri"].as_str() == Some(&skill_uri(path)))
        {
            Some((path, text)) => Ok(cache(
                json!({ "contents": [{ "uri": skill_uri(path), "mimeType": "text/markdown", "text": text }] }),
            )),
            None => Err((-32602, "unknown resource".into())),
        },
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, m)) => rpc_error(id, code, &m),
    })
}

fn skill_uri(path: &str) -> String {
    format!("skill://westron/{path}")
}

/// The skills/list entry: frontmatter and a manifest of every file with its digest and size.
fn skill_entry() -> Value {
    let mut frontmatter = serde_json::Map::new();
    for line in SKILL_FILES[0]
        .1
        .split("---")
        .nth(1)
        .unwrap_or_default()
        .lines()
    {
        if let Some((k, v)) = line.split_once(':') {
            frontmatter.insert(k.trim().into(), json!(v.trim()));
        }
    }
    json!({
        "uri": skill_uri("SKILL.md"),
        "frontmatter": frontmatter,
        "resources": SKILL_FILES.iter().map(|(path, text)| json!({
            "uri": skill_uri(path),
            "digest": sha256(text),
            "size": text.len()
        })).collect::<Vec<_>>()
    })
}

/// `sha256:<hex>` of a file's bytes, as the Skills manifest wants it.
fn sha256(text: &str) -> String {
    let hex: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}")
}

fn tool(name: &str, description: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": props, "required": required, "additionalProperties": false },
        "annotations": { "readOnlyHint": read_only, "destructiveHint": false, "openWorldHint": false }
    })
}

fn tools() -> Vec<Value> {
    let out_dir = json!({ "type": "string", "description": "Folder for the results; default: next to each input" });
    vec![
        tool(
            "convert",
            "Convert files to a format (id or extension: jpg, webp, pdf, mp4, mp3, docx, zip, folder, …). \
             Files run in parallel; routes of up to three engines are found automatically. \
             Returns each input with the paths written, or its error. Existing files are never overwritten: \
             a free name like `photo (2).jpg` is used. `merge: true` with `to: pdf` joins PDFs into one.",
            json!({
                "files": { "type": "array", "items": { "type": "string" }, "description": "Absolute paths" },
                "to": { "type": "string" },
                "out_dir": out_dir,
                "merge": { "type": "boolean" }
            }),
            &["files", "to"],
            false,
        ),
        tool(
            "split_pdf",
            "Split a PDF into one PDF per page (`<name>-1.pdf`, …). Returns the paths.",
            json!({ "file": { "type": "string" }, "out_dir": out_dir }),
            &["file"],
            false,
        ),
        tool(
            "formats",
            "What a file (path) or a format (id or extension) can be converted to, cheapest first, with the engines on the way.",
            json!({ "of": { "type": "string" } }),
            &["of"],
            true,
        ),
        tool(
            "engines",
            "Which engines can run on this Mac, and how to install the missing ones.",
            json!({}),
            &[],
            true,
        ),
    ]
}

fn str_arg<'a>(a: &'a Value, k: &str) -> Result<&'a str> {
    a[k].as_str().with_context(|| format!("missing '{k}'"))
}

fn call(planner: &Planner, name: &str, a: &Value) -> Result<String> {
    let out_dir = a["out_dir"].as_str().map(PathBuf::from);
    match name {
        "convert" => {
            let to = format_id(&str_arg(a, "to")?.to_lowercase())?;
            let files: Vec<PathBuf> = a["files"]
                .as_array()
                .context("missing 'files'")?
                .iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect();
            if files.is_empty() {
                bail!("'files' is empty");
            }
            if a["merge"].as_bool() == Some(true) {
                if to != "pdf" {
                    bail!("merge writes one PDF; use to: pdf");
                }
                let dir = out_dir
                    .unwrap_or_else(|| files[0].parent().unwrap_or(Path::new(".")).to_path_buf());
                let out = jobs::free_path(&dir, &format!("{}.pdf", formats::stem_of(&files[0])));
                engine::pdf::merge(&files, &out)?;
                return Ok(out.display().to_string());
            }
            let results = jobs::convert_many(planner, &files, to, out_dir.as_deref());
            let lines: Vec<String> = files
                .iter()
                .zip(results)
                .map(|(f, r)| match r {
                    Ok(outs) => format!(
                        "{} → {}",
                        f.display(),
                        outs.iter()
                            .map(|o| o.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    Err(e) => format!("{} ✗ {e:#}", f.display()),
                })
                .collect();
            Ok(lines.join("\n"))
        }
        "split_pdf" => {
            let f = PathBuf::from(str_arg(a, "file")?);
            let dir = out_dir.unwrap_or_else(|| f.parent().unwrap_or(Path::new(".")).to_path_buf());
            let outs = engine::pdf::split(&f, &dir, &formats::stem_of(&f))?;
            Ok(outs
                .iter()
                .map(|o| o.display().to_string())
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "formats" => {
            let of = str_arg(a, "of")?;
            let from = formats::detect(Path::new(of))
                .map(|f| f.id)
                .map_or_else(|| format_id(&of.to_lowercase()), Ok)?;
            let lines: Vec<String> = planner
                .targets(from)
                .into_iter()
                .map(|to| {
                    let via: Vec<_> = planner
                        .plan(from, to)
                        .unwrap_or_default()
                        .iter()
                        .map(|h| h.engine.id())
                        .collect();
                    format!("{to} ({})", via.join(" → "))
                })
                .collect();
            Ok(if lines.is_empty() {
                format!("nothing reachable from {from}")
            } else {
                lines.join("\n")
            })
        }
        "engines" => Ok(engine::all()
            .iter()
            .map(|e| match e.unavailable() {
                None => format!("✓ {}", e.id()),
                Some(why) => format!("✗ {}: {why}", e.id()),
            })
            .collect::<Vec<_>>()
            .join("\n")),
        _ => bail!("unknown tool {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rpc(method: &str, params: Value) -> Value {
        let p = Planner::new(&engine::available());
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        handle(&p, &req.to_string()).unwrap()
    }

    #[test]
    fn skill_is_served_with_a_matching_manifest() {
        let init = rpc("initialize", json!({ "protocolVersion": "2025-06-18" }));
        assert!(init["result"]["capabilities"]["extensions"][SKILLS].is_object());
        let skill = &rpc("skills/list", json!({}))["result"]["skills"][0];
        assert_eq!(skill["frontmatter"]["name"], "westron");
        for file in skill["resources"].as_array().unwrap() {
            let read = rpc("resources/read", json!({ "uri": file["uri"] }));
            let text = read["result"]["contents"][0]["text"].as_str().unwrap();
            assert_eq!(file["size"], text.len());
            assert_eq!(file["digest"], sha256(text));
        }
        assert_eq!(
            rpc("skills/get", json!({ "uri": "skill://nope/SKILL.md" }))["error"]["code"],
            -32602
        );
        assert_eq!(
            sha256("hello"),
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn protocol_and_tools() {
        let p = Planner::new(&engine::available());
        let init = handle(&p, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
        assert!(
            handle(
                &p,
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            )
            .is_none()
        );
        let list = handle(&p, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).unwrap();
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 4);
        let f = handle(&p, r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"formats","arguments":{"of":"heic"}}}"#).unwrap();
        assert!(
            f["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("jpg (image)")
        );
    }
}
