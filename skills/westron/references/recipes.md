# Recipes

Photos from an iPhone to JPG, into one folder:

```json
{"files": ["/Users/me/Pictures/a.HEIC", "/Users/me/Pictures/b.HEIC"], "to": "jpg", "out_dir": "/Users/me/Desktop/jpg"}
```

Every page of a PDF as PNG (150 DPI):

```json
{"files": ["/Users/me/report.pdf"], "to": "png"}
```

Join scanned PDFs, in this order:

```json
{"files": ["/Users/me/scan1.pdf", "/Users/me/scan2.pdf"], "to": "pdf", "merge": true}
```

A spreadsheet as JSON records (first sheet, first row is the header):

```json
{"files": ["/Users/me/data.xlsx"], "to": "json"}
```

Markdown notes as a PDF (Markdown → HTML → PDF):

```json
{"files": ["/Users/me/notes.md"], "to": "pdf"}
```

Markdown notes as a Word document (Markdown → HTML → DOCX):

```json
{"files": ["/Users/me/notes.md"], "to": "docx"}
```

The sound of a video:

```json
{"files": ["/Users/me/talk.mov"], "to": "m4a"}
```

Unpack an archive, then pack the folder as ZIP:

```json
{"files": ["/Users/me/Downloads/files.7z"], "to": "folder"}
{"files": ["/Users/me/Downloads/files"], "to": "zip"}
```

Subtitles from SRT to WebVTT:

```json
{"files": ["/Users/me/movie.srt"], "to": "vtt"}
```
