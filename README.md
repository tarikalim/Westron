# Westron

A native macOS file converter you use by dragging, written in Rust with [GPUI](https://www.gpui.rs/). Hold <kbd>⌥ Option</kbd>, drag files from Finder, and drop them on a format in the ring that opens at your cursor. The result is saved next to the original.

Westron is a Rust port of [Filee](https://github.com/KnifeLemon/Filee) by Myeonggu Kim (KnifeLemon), MIT licensed. The format list, the route search and the engine edges follow Filee's; the app is rewritten for macOS with a low memory cost as the goal.

The name is Tolkien's *Westron*, the Common Speech of Middle-earth: the language that let the free peoples understand each other.

## How it works

- **The ring at the cursor**: hold ⌥ while you drag files anywhere. The formats those files can become open around the cursor; drop on one. The centre opens the window with every format.
- **The window**: drop files on it, pick a format; conversions run in parallel and show their progress. Click a finished one to show it in Finder.
- **Routes**: engines know direct conversions; Westron finds the cheapest chain of up to three (HEIC → PNG → WebP, PDF → PNG pages → JPG, 7Z → folder → TAR.GZ).
- **Menu bar**: Westron lives in the menu bar; closing the window keeps the ⌥-drag ring working.

## Formats (version 0.1)

| From | To | Engine |
|---|---|---|
| Images: JPG, PNG, HEIC, AVIF, WebP, TIFF, GIF, BMP, PSD, TGA, JPEG 2000, JPEG XL, ICO, ICNS, EXR, HDR, DDS, DICOM, camera RAW, SVG, … | JPG, PNG, HEIC, AVIF, WebP, TIFF, GIF, BMP, PSD, TGA, JPEG 2000, EXR, ICO, ICNS, QOI, PPM, PDF | macOS ImageIO (`sips`), `image` crate |
| PDF | PNG, JPG, TIFF (one image per page); merge, split | CoreGraphics |
| Archives: ZIP, 7Z, RAR, TAR, TAR.GZ/BZ2/XZ, ISO, CAB, JAR, CPIO, GZ, BZ2, … | a folder; a folder → ZIP, 7Z, TAR, TAR.GZ/BZ2/XZ | macOS `bsdtar`; ZIP as Finder's Compress (`ditto`) |
| Word processing: DOCX, DOC, ODT, RTF, HTML, TXT | each other | macOS `textutil` |
| Markdown | HTML, PDF, and through HTML DOCX, DOC, ODT, RTF, TXT | `pulldown-cmark`, `textutil` |
| HTML, RTF, TXT, DOCX, DOC, ODT | PDF (A4) | macOS text and print system |
| Audio: WAV, AIFF, M4A, CAF, FLAC, AAC, MP3, … | WAV, AIFF, M4A (AAC), CAF, FLAC, AAC | macOS `afconvert` |
| Video: MOV, MP4, M4V | each other, M4A | macOS `avconvert` |
| Data and tables: JSON, JSON Lines, YAML, TOML, XML, CSV, TSV, XLSX, XLS, ODS | JSON, JSON Lines, YAML, TOML, XML, CSV, TSV, XLSX | `serde`, `calamine`, `rust_xlsxwriter` |
| Subtitles: SRT, WebVTT, ASS, SSA | SRT, WebVTT, ASS | built in |
| Video, audio | MP4, MOV, MKV, WebM, AVI, GIF, MP3, M4A, FLAC, WAV, OGG, Opus, subtitles, … | FFmpeg¹ |
| Word, Excel, PowerPoint, OpenDocument, Pages, Numbers, Keynote, … | PDF and the editable formats of their kind | LibreOffice¹ |
| Markdown, HTML, DOCX, reStructuredText, LaTeX, Org, AsciiDoc, Typst, … | each other, EPUB, PPTX | Pandoc¹ |
| E-books: EPUB, MOBI, AZW3, FB2, CBZ, … | EPUB, MOBI, AZW3, PDF, … | calibre¹ |
| EPS, PS, AI | PDF; PDF → EPS, PS | Ghostscript¹ |

¹ Uses the program installed on your Mac (Homebrew or the app in /Applications); `westron engines` shows which ones it found.

Word files reach PDF through macOS's text system; with LibreOffice installed, LibreOffice lays them out instead, closer to Word. Filee's other built-in engines (HWP/HWPX, PDF to DOCX, CAD, 3D models, fonts, e-mail) come in later versions.

## Command line

```
westron convert photo.heic --to jpg
westron convert *.png --to webp -o converted
westron convert a.pdf b.pdf --to pdf --merge
westron split book.pdf
westron watch ~/Inbox --to pdf
westron formats photo.heic        # what it can become, and through which engines
westron engines
```

## MCP

`westron mcp` lets AI agents such as Claude Code convert files on your Mac. Tool results are text.

| Tool | For |
|---|---|
| `convert` | Files to a format, in parallel; `merge` joins PDFs |
| `split_pdf` | One PDF per page |
| `formats` | What a file or format can become, and through which engines |
| `engines` | Which engines can run, and how to install the missing ones |

The `westron` skill ([skills/westron](skills/westron/SKILL.md)) explains the tools, the formats and the paths they return. It is served over the MCP Skills extension (`skills/list`, `skills/get`, `resources/read`).

```sh
claude mcp add --scope user westron -- /Applications/Westron.app/Contents/MacOS/westron mcp
```

## Install and run

```sh
scripts/bundle.sh    # release build installed as /Applications/Westron.app, and ~/.local/bin/westron
```

The build is signed only locally (ad hoc). Requires macOS 14 or newer.

## Licence

MIT or Apache-2.0, at your option. Westron is derived from Filee (MIT); see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
