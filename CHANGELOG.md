# Changelog

## Unreleased

- PDF to DOCX, RTF and TXT (and through them ODT, DOC, HTML): the text with its fonts and styles, a page break per page.

## 0.1.0

- First version: the ⌥-drag ring at the cursor, the drop window, the menu bar item and the `westron` command.
- Images through macOS ImageIO and the `image` crate; PDF pages to images, merge and split; archives through bsdtar.
- FFmpeg, LibreOffice, Pandoc, calibre and Ghostscript when installed.
- Built-in macOS converters: `textutil` (DOCX, DOC, ODT, RTF, HTML, TXT), `afconvert` (audio), `avconvert` (MOV, MP4, M4V); ZIP through `ditto`.
- Data and tables (JSON, JSON Lines, YAML, TOML, XML, CSV, TSV, XLSX, XLS, ODS), subtitles (SRT, WebVTT, ASS) and Markdown → HTML.
- HTML, RTF, TXT, Markdown and Word files to A4 PDF through macOS's text and print system.
- `westron mcp`: `convert`, `split_pdf`, `formats` and `engines` for AI agents, with the `westron` skill served over MCP.
