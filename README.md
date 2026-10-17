<h1 align="center">GlassCell</h1>

<p align="center">
  <b>A native Mac spreadsheet in Liquid Glass, built on the GridCraft engine.</b>
</p>

<p align="center">
  SwiftUI and AppKit on macOS 26, over a clean-room, open-source spreadsheet engine written in Rust.
</p>

GlassCell is a macOS app for spreadsheets: it opens and saves Excel workbooks (`.xlsx`), CSV and
TSV files, and works with formulas, tables, filters, conditional formatting, charts and pictures.
The spreadsheet itself, from the formula language to the file formats, is
[GridCraft](https://github.com/storytold/gridcraft)'s Rust engine. GlassCell replaces GridCraft's
cross-platform interface with a Mac one.

> [!NOTE]
> GlassCell is based on GridCraft by the ArtCraft team. It is an independent project, not made
> or endorsed by the GridCraft or ArtCraft teams. It is early software: keep copies of workbooks
> that matter.

## What it looks like

- **A Mac window, not a ribbon.** A customizable toolbar, a menu bar holding every command, a
  command palette (⇧⌘P), and an inspector for cell, sheet and object formatting.
- **Liquid Glass throughout.** The toolbar is one long glass capsule beside the document's name. The
  sheet scrolls up under the toolbar, the formula bar and the column headers, which frost it. The
  inspector floats over the grid on a pane of glass. At the bottom, the sheet tabs, a horizontal
  scroller and the selection's Sum / Count / Average float over the cells.
- **A grid drawn for the Mac.** A custom AppKit view (Core Text, the engine's layout) keeps large
  sheets smooth. It has frozen panes, filter menus, AutoComplete and function hints.
- **The platform's features.** Document windows with autosave and versions, App Intents for
  Shortcuts and Siri, a Quick Look preview, Spotlight search of workbook contents, sharing, and
  menus in ten languages (partly translated so far).
- **Drivable by agents.** Everything is an engine command, reachable from a local JSON-lines
  control channel (`GlassCell --control 7979`, see [`macos/README.md`](macos/README.md)).

## Building

GlassCell needs macOS 26, Xcode 27 and [rustup](https://rustup.rs).

```sh
macos/build-engine.sh                       # the Rust engine and its Swift bindings
(cd macos/GridCraft && swift test)          # the app's tests
SKIP_ENGINE=1 macos/build-xcode.sh          # target/mac/GlassCell.app (needs XcodeGen)
open target/mac/GlassCell.app
```

`macos/build-app.sh` is a quicker SwiftPM-only build without App Intents or Quick Look. Building
for this Mac only: prefix the commands with `ARCHS=arm64`. Engine checks:
`cargo xtask ci` (format, lints, tests, layering, the WebAssembly build).

## What's where

| Path | What |
|---|---|
| `macos/` | GlassCell: the SwiftUI app (`GridCraft/` package, named for the engine), its tests and build scripts |
| `crates/ffi` | The bridge from Swift to the engine (UniFFI) |
| `crates/*` | The GridCraft engine: formulas, functions, file formats, charts, commands; shared with upstream |
| `crates/ui-egui`, `apps/` | GridCraft's own cross-platform interface, kept as the behaviour reference. GlassCell does not ship it |
| `docs/mac-port.md` | GlassCell's plan, design decisions and progress |
| `docs/` | GridCraft's engine documentation |

## Principles

GlassCell keeps GridCraft's rules:

- **Clean-room.** Excel may be observed from the outside only. No Microsoft code, file, icon,
  template or artwork is in this repository, and none may be added.
- **Never crash.** No panics in the engine, no force-unwraps in the app.
- **Everything is a command.** The app changes a workbook only through the engine's commands.

The full rules for contributors (people and agents) are in [`AGENTS.md`](AGENTS.md).

## License

MIT OR Apache-2.0, as GridCraft. See [`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)
and [`NOTICE`](NOTICE). GridCraft and ArtCraft are names of their owners. Microsoft and Excel are
trademarks of Microsoft Corporation. GlassCell is not affiliated with any of them.
