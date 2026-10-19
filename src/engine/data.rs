// Structured data and tables: JSON, JSON Lines, YAML, TOML, XML, CSV, TSV; Excel and ODS sheets.
// Everything goes through a JSON value; tables are arrays of objects keyed by the header row.

use super::{Edge, Engine, Step, edge};
use anyhow::{Context, bail};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const READS: &[&str] = &[
    "json", "jsonl", "yaml", "toml", "xml", "csv", "tsv", "xlsx", "xlsm", "xls", "ods",
];
const WRITES: &[&str] = &["json", "jsonl", "yaml", "toml", "xml", "csv", "tsv", "xlsx"];

pub struct DataEngine(Vec<Edge>);

impl DataEngine {
    pub fn new() -> Self {
        let e = READS
            .iter()
            .flat_map(|&f| {
                WRITES
                    .iter()
                    .filter(move |&&t| t != f)
                    .map(move |&t| edge(f, t, 10))
            })
            .collect();
        DataEngine(e)
    }
}

impl Engine for DataEngine {
    fn id(&self) -> &'static str {
        "data"
    }

    fn edges(&self) -> &[Edge] {
        &self.0
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let value = read(step.input, step.from)?;
        let out = step.output("");
        write(&value, &out, step.to)?;
        (step.progress)(1.0);
        Ok(vec![out])
    }
}

pub fn read(path: &Path, from: &str) -> anyhow::Result<Value> {
    if matches!(from, "xlsx" | "xlsm" | "xls" | "ods") {
        return read_sheet(path);
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let text = text.trim_start_matches('\u{feff}');
    Ok(match from {
        "json" => serde_json::from_str(text)?,
        "jsonl" => Value::Array(
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .map(serde_json::from_str)
                .collect::<Result<_, _>>()?,
        ),
        "yaml" => serde_yaml_ng::from_str(text)?,
        "toml" => toml::from_str(text)?,
        "xml" => read_xml(text)?,
        "csv" => read_delimited(text, b',')?,
        "tsv" => read_delimited(text, b'\t')?,
        _ => bail!("cannot read {from}"),
    })
}

pub fn write(value: &Value, out: &Path, to: &str) -> anyhow::Result<()> {
    let text = match to {
        "json" => serde_json::to_string_pretty(value)? + "\n",
        "jsonl" => match value {
            Value::Array(items) => items.iter().map(|v| v.to_string() + "\n").collect(),
            v => v.to_string() + "\n",
        },
        "yaml" => serde_yaml_ng::to_string(value)?,
        // TOML's top level is a table; a list goes under "rows".
        "toml" => match value {
            Value::Object(_) => toml::to_string_pretty(value)?,
            v => toml::to_string_pretty(&serde_json::json!({ "rows": v }))?,
        },
        "xml" => write_xml(value),
        "csv" => write_delimited(value, b',')?,
        "tsv" => write_delimited(value, b'\t')?,
        "xlsx" => return write_xlsx(value, out),
        _ => bail!("cannot write {to}"),
    };
    std::fs::write(out, text)?;
    Ok(())
}

/// Rows as objects keyed by the header; a list of lists without one.
fn table(value: &Value) -> anyhow::Result<(Vec<String>, Vec<Vec<Value>>)> {
    // Unwrap one-key wrappers ({"rows": [...]}, <root><item>…) down to the list.
    let mut value = value;
    while let Value::Object(m) = value
        && m.len() == 1
        && m.values()
            .next()
            .is_some_and(|v| v.is_array() || v.is_object())
    {
        value = m.values().next().unwrap();
    }
    let rows = match value {
        Value::Array(rows) => rows.as_slice(),
        Value::Object(_) => std::slice::from_ref(value),
        _ => bail!("not a table: expected a list of records"),
    };
    let mut header: Vec<String> = vec![];
    for row in rows {
        if let Value::Object(m) = row {
            for k in m.keys() {
                if !header.contains(k) {
                    header.push(k.clone());
                }
            }
        }
    }
    let body = rows
        .iter()
        .map(|row| match row {
            Value::Object(m) => header
                .iter()
                .map(|k| m.get(k).cloned().unwrap_or(Value::Null))
                .collect(),
            Value::Array(cells) => cells.clone(),
            v => vec![v.clone()],
        })
        .collect();
    Ok((header, body))
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

fn read_delimited(text: &str, sep: u8) -> anyhow::Result<Value> {
    let mut r = csv::ReaderBuilder::new()
        .delimiter(sep)
        .flexible(true)
        .from_reader(text.as_bytes());
    let header: Vec<String> = r.headers()?.iter().map(str::to_string).collect();
    let mut rows = vec![];
    for rec in r.records() {
        let rec = rec?;
        let obj: Map<String, Value> = header
            .iter()
            .zip(rec.iter())
            .map(|(k, v)| (k.clone(), Value::String(v.to_string())))
            .collect();
        rows.push(Value::Object(obj));
    }
    Ok(Value::Array(rows))
}

fn write_delimited(value: &Value, sep: u8) -> anyhow::Result<String> {
    let (header, body) = table(value)?;
    let mut w = csv::WriterBuilder::new().delimiter(sep).from_writer(vec![]);
    if !header.is_empty() {
        w.write_record(&header)?;
    }
    for row in body {
        w.write_record(row.iter().map(cell_text))?;
    }
    Ok(String::from_utf8(w.into_inner()?)?)
}

fn read_sheet(path: &Path) -> anyhow::Result<Value> {
    use calamine::{Data, Reader as _};
    let mut book = calamine::open_workbook_auto(path)?;
    let range = book
        .worksheet_range_at(0)
        .context("the workbook has no sheet")??;
    let mut rows = range.rows();
    let header: Vec<String> = rows
        .next()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .unwrap_or_default();
    let cell = |c: &Data| match c {
        Data::Empty => Value::Null,
        Data::Int(i) => Value::from(*i),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => Value::from(*f as i64),
        Data::Float(f) => Value::from(*f),
        Data::Bool(b) => Value::from(*b),
        c => Value::String(c.to_string()),
    };
    Ok(Value::Array(
        rows.map(|r| Value::Object(header.iter().cloned().zip(r.iter().map(cell)).collect()))
            .collect(),
    ))
}

fn write_xlsx(value: &Value, out: &Path) -> anyhow::Result<()> {
    let (header, body) = table(value)?;
    let mut book = rust_xlsxwriter::Workbook::new();
    let sheet = book.add_worksheet();
    let bold = rust_xlsxwriter::Format::new().set_bold();
    for (c, h) in header.iter().enumerate() {
        sheet.write_string_with_format(0, c as u16, h, &bold)?;
    }
    let first = if header.is_empty() { 0 } else { 1 };
    for (r, row) in body.iter().enumerate() {
        let r = (r + first) as u32;
        for (c, v) in row.iter().enumerate() {
            let c = c as u16;
            match v {
                Value::Null => {}
                Value::Number(n) => {
                    sheet.write_number(r, c, n.as_f64().unwrap_or_default())?;
                }
                Value::Bool(b) => {
                    sheet.write_boolean(r, c, *b)?;
                }
                v => {
                    sheet.write_string(r, c, cell_text(v))?;
                }
            }
        }
    }
    book.save(out)?;
    Ok(())
}

/// Element → object: attributes as "@name", children by name (repeated → list), text as "#text";
/// an element with only text is that text.
fn read_xml(text: &str) -> anyhow::Result<Value> {
    fn node(n: roxmltree::Node) -> Value {
        let mut m = Map::new();
        for a in n.attributes() {
            m.insert(
                format!("@{}", a.name()),
                Value::String(a.value().to_string()),
            );
        }
        let mut text = String::new();
        for c in n.children() {
            if c.is_element() {
                let v = node(c);
                match m.get_mut(c.tag_name().name()) {
                    Some(Value::Array(a)) => a.push(v),
                    Some(prev) => *prev = Value::Array(vec![prev.take(), v]),
                    None => {
                        m.insert(c.tag_name().name().to_string(), v);
                    }
                }
            } else if let Some(t) = c.text() {
                text.push_str(t);
            }
        }
        let text = text.trim();
        if m.is_empty() {
            return Value::String(text.to_string());
        }
        if !text.is_empty() {
            m.insert("#text".into(), Value::String(text.to_string()));
        }
        Value::Object(m)
    }
    let doc = roxmltree::Document::parse(text)?;
    let root = doc.root_element();
    Ok(Value::Object(Map::from_iter([(
        root.tag_name().name().to_string(),
        node(root),
    )])))
}

fn write_xml(value: &Value) -> String {
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    fn tag(k: &str) -> String {
        let t: String = k
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || "-_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if t.starts_with(|c: char| c.is_alphabetic() || c == '_') {
            t
        } else {
            format!("_{t}")
        }
    }
    fn el(name: &str, v: &Value, depth: usize, out: &mut String) {
        let pad = "  ".repeat(depth);
        match v {
            Value::Array(items) => items.iter().for_each(|i| el(name, i, depth, out)),
            Value::Object(m) => {
                let attrs: String = m
                    .iter()
                    .filter_map(|(k, v)| {
                        k.strip_prefix('@')
                            .map(|a| format!(" {}=\"{}\"", tag(a), esc(&cell_text(v))))
                    })
                    .collect();
                let kids: Vec<_> = m
                    .iter()
                    .filter(|(k, _)| !k.starts_with('@') && *k != "#text")
                    .collect();
                let text = m.get("#text").map(cell_text).unwrap_or_default();
                if kids.is_empty() {
                    out.push_str(&format!("{pad}<{name}{attrs}>{}</{name}>\n", esc(&text)));
                } else {
                    out.push_str(&format!("{pad}<{name}{attrs}>{}\n", esc(&text)));
                    for (k, v) in kids {
                        el(&tag(k), v, depth + 1, out);
                    }
                    out.push_str(&format!("{pad}</{name}>\n"));
                }
            }
            v => out.push_str(&format!("{pad}<{name}>{}</{name}>\n", esc(&cell_text(v)))),
        }
    }
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    match value {
        Value::Object(m) if m.len() == 1 && !m.values().next().unwrap().is_array() => {
            let (k, v) = m.iter().next().unwrap();
            el(&tag(k), v, 0, &mut out);
        }
        Value::Array(_) => {
            out.push_str("<root>\n");
            el("item", value, 1, &mut out);
            out.push_str("</root>\n");
        }
        v => el("root", v, 0, &mut out),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_tables_and_trees() {
        let dir = std::env::temp_dir().join(format!("westron-data-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let csv = dir.join("a.csv");
        std::fs::write(&csv, "name,age\nAda,36\n\"Lin, Y\",7\n").unwrap();
        let v = read(&csv, "csv").unwrap();
        assert_eq!(v[1]["name"], "Lin, Y");
        for to in ["json", "jsonl", "yaml", "toml", "xml", "tsv", "xlsx"] {
            let out = dir.join(format!("a.{to}"));
            write(&v, &out, to).unwrap();
            let back = read(&out, to).unwrap();
            let back = write_delimited(&back, b',').unwrap();
            assert!(
                back.contains("Lin, Y") && back.contains("Ada"),
                "{to}: {back}"
            );
        }
        let xml = dir.join("b.xml");
        std::fs::write(
            &xml,
            r#"<cat id="1"><item>a &amp; b</item><item>c</item></cat>"#,
        )
        .unwrap();
        let v = read(&xml, "xml").unwrap();
        assert_eq!(v["cat"]["@id"], "1");
        assert_eq!(v["cat"]["item"][0], "a & b");
        assert!(write_xml(&v).contains(r#"<cat id="1">"#));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
