# Contributing

Thanks for helping. Westron is a small project, so the process is short.

## Build and run

You need Rust 1.92 or newer and macOS 14 or newer.

```sh
cargo run                  # the window, with a debug build
cargo run -- engines       # the command line
cargo test
scripts/bundle.sh          # release build installed as /Applications/Westron.app
```

## Before a pull request

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check           # cargo install cargo-deny
```

The pull request check runs these, and also checks:

- **Branch name**: [Conventional Branch](https://conventionalbranch.org/): `feat/`, `fix/`,
  `hotfix/` or `chore/` + lower-case words joined by hyphens (`feat/table-resize`), or
  `release/<version>`. Documentation changes go on `chore/`. Rule: `scripts/check-branch-name.sh`.
- **Commit messages and the PR title**: [Conventional Commits](https://www.conventionalcommits.org/),
  `feat: …`, `fix: …`, `docs: …`, at most 100 characters in the subject. Rule: `committed.toml`.
  The body says why, with any measurement that led to the change.
- **No AI attribution**: no AI tool as co-author (`Co-authored-by:`), no `Claude-Session:`
  trailer, no "Generated with …" line in the PR body.
- **No secrets** in the new commits (gitleaks).
- **Dependencies**: no known vulnerability (RustSec), only the licences in `deny.toml`.

Clippy and the tests run on macOS and are skipped while the pull request is a draft.

- One change per pull request, with a test when it changes behaviour.
- Code follows `cargo fmt`'s defaults. Keep code comments to a line or two; the reasoning goes in
  the commit message.
- Add a line under `Unreleased` in `CHANGELOG.md` for anything a user would notice.

## Where things are

| Path | What |
|---|---|
| `src/formats.rs` | Every format Westron knows |
| `src/route.rs` | The cheapest chain of engines from one format to another |
| `src/engine/` | Engines: `image` (ImageIO, `image` crate), `pdf` (CoreGraphics), `archive` (bsdtar, ditto), `native` (textutil, afconvert, avconvert), `data` (JSON, YAML, TOML, XML, CSV, sheets), `text` (subtitles, Markdown), `print` (rich text to PDF, PDF text to DOCX), `tools` (FFmpeg, LibreOffice, Pandoc, calibre, Ghostscript) |
| `src/jobs.rs` | One conversion: route, scratch folder, results next to the original |
| `src/cli.rs` | The `westron` command |
| `src/app.rs`, `src/donut.rs` | The window and the ring at the cursor |
| `src/mac.rs` | AppKit: the global ⌥-drag watch and the menu bar item |
| `src/mcp.rs` | MCP server |
| `src/mem.rs` | Memory settings |
| `skills/westron/` | The skill served over MCP; update it when an MCP tool changes |
| `vendor/` | Vendored GPUI crates (Apache-2.0) |

A new engine is one type implementing `engine::Engine` with its edges; the route search picks it up.

## Releases

The maintainer bumps the version in `Cargo.toml`, moves `Unreleased` in `CHANGELOG.md` under the
new version, and pushes a `v*` tag. The release workflow builds and publishes the app.
