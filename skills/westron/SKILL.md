---
name: westron
description: Convert files on this Mac between formats (images, PDF, Word documents, spreadsheets and data, audio, video, subtitles, archives, Markdown) without uploading them. Use it whenever a task needs a file in another format, pages of a PDF as images, PDFs merged or split, or an archive packed or unpacked.
---

# Westron

Westron converts local files with the engines on this Mac: macOS's own (ImageIO, CoreGraphics,
`textutil`, `afconvert`, `avconvert`, `bsdtar`, `ditto`), built-in Rust code (data, tables,
subtitles, Markdown), and FFmpeg, LibreOffice, Pandoc, calibre or Ghostscript when installed.
Nothing leaves the computer.

## Converting

`convert(files, to, out_dir?, merge?)`:

- `files`: absolute paths. Several files run in parallel; each one gets its own result line.
- `to`: a format id or an extension: `jpg`, `webp`, `heic`, `pdf`, `png`, `docx`, `xlsx`, `csv`,
  `json`, `yaml`, `mp4`, `m4a`, `mp3`, `srt`, `zip`, `folder` (unpack an archive), …
- `out_dir`: where the results go. Without it they go next to each input.
- `merge: true` with `to: "pdf"` joins the PDFs into one file, in the order given.

```json
{"files": ["/Users/me/Desktop/IMG_0412.HEIC"], "to": "jpg"}
```

The result is `input → output` per file, or `input ✗ error`. Read the output paths from it:

- An existing file is never overwritten: the result may be `photo (2).jpg`.
- A PDF with several pages becomes one image per page: `report-1.png`, `report-2.png`, ….
- Unpacking an archive gives a folder named after it.

## Before converting

When you are not sure a conversion exists, call `formats(of)` with the path or the format. It lists
every target, cheapest first, with the engines on the way (`docx (markdown → textutil)`). A
conversion can take up to three engines; Westron finds the chain.

`engines()` shows which engines can run. A missing one says how to install it
(`brew install ffmpeg`); do not install anything without the user's consent.

`split_pdf(file, out_dir?)` writes one PDF per page.

## Limits

- Tables (CSV, TSV, XLSX, JSON, YAML, TOML, XML) are lists of records: the first row of a sheet or
  CSV is the header. Only the first sheet of a workbook is read. CSV values are text.
- Documents become A4 PDFs through macOS's text system; with LibreOffice installed, Word files go
  through LibreOffice instead, closer to Word's layout. Complex layouts (columns, floating images)
  may differ from the original.
- PDF to DOCX, RTF or TXT carries the text with its fonts and styles and a page break per page;
  pictures, columns and table layout are not carried over. A scanned PDF has no text (no OCR).
- Large videos take time; the call returns when every file is done.

More examples: [references/recipes.md](references/recipes.md).
