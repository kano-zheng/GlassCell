# GlassCell — instructions for agents

This repository is **GlassCell**, an independent, open-source **fork** of [GridCraft](https://github.com/storytold/gridcraft), the clean-room, open-source, Rust-native spreadsheet targeting Microsoft Excel parity. The fork keeps GridCraft's Rust engine and adds a **native macOS frontend written in SwiftUI** (macOS 26+, adopting the newest SwiftUI/AppKit features), with a Mac-style **toolbar + inspector + menu bar** instead of Excel's ribbon.

- The Rust engine (`crates/*` below L6) is shared with upstream. The egui frontend (`crates/ui-egui`, `apps/gridcraft`, `apps/gridcraft-web`) stays in the tree as the behaviour reference and must keep building, so merges from upstream stay cheap.
- Remote `upstream` = `storytold/gridcraft` (fetch only; pushing to it is disabled). Sync with `git fetch upstream && git merge upstream/main`. Keep edits to upstream-owned files small and focused so merges stay easy.
- Standards shared across the crafting apps live in `../../craftrules` (or `storytold/craftrules`); its never-crash, fonts and test-corpora standards still apply to the Rust side.

## Start every session here
1. Read [`docs/mac-port.md`](docs/mac-port.md): the plan, the current phase, and the toolbar/inspector design.
2. For engine behaviour, read the upstream docs in `docs/` (`architecture.md`, `control-protocol.md`, the parity checklists). `plan/` (gitignored, local only) may hold Excel observations if it exists.

## Repository layout (fork additions)
- `crates/ffi` (`gridcraft-ffi`, L6): the UniFFI bridge from Swift to `gridcraft-engine`. It is the **only** crate allowed to opt out of `forbid(unsafe_code)`, and only for code UniFFI generates; no handwritten `unsafe`. It exposes a coarse API: execute a command (`id` + JSON params → JSON), drain UI requests, list commands, and typed read models (viewport, selection format).
- `macos/`: the SwiftUI app (Swift Package / Xcode project) and its tests. It is **not** under `apps/`, because `apps/*` are Cargo workspace members.
- Register new crates in `xtask/src/layers.rs` so `cargo xtask layers` classifies them.

## Assets and iconography — absolute rule
**No Microsoft, Adobe, Avid, Autodesk (or any other vendor's) iconography, images, artwork, fonts, templates, themes or sample files are committed — ever.** Breaking this rule is a serious failure.
- Every asset in the repo is original work, or public domain / CC0, or open source / Creative Commons that allows redistribution. Every asset file has a row in [`ATTRIBUTION.md`](ATTRIBUTION.md); `cargo xtask assets` enforces it.
- **SF Symbols are allowed in the Mac frontend, only through the system API** (`Image(systemName:)`, `Label(_:systemImage:)`). Never export, trace or commit symbol artwork as files, and never use symbols to imitate Office's look. Where no fitting symbol exists, draw the icon in code (port it from `crates/ui-egui/src/icons.rs`).
- Never copy Office's icons, ribbon artwork, theme XML, table-style definitions, templates, clip art or wording beyond feature names. Excel feature *names* (menu labels, function names) are fine.
- Screenshots of Microsoft software are never committed.
- The ArtCraft marks (name, wordmark, logos) are ArtCraft trademarks and not open source. This fork is published as a modified version, so it carries none of them: `docs/brand/` was removed, and upstream merges must not bring it back. Plain text may say the work is based on GridCraft by the ArtCraft team; nothing may present the fork as an ArtCraft product.

## Never crash
People trust GridCraft with their numbers; a crash loses their work. **This outranks feature work**, in Rust and in Swift.

Rust (engine and every crate except the generated parts of `crates/ffi`):
- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`; no `unsafe` (`unsafe_code = "forbid"`).
- **Errors are `Result<T, E>`** through the crate's error type and `?`.
- **Input-derived numbers are hostile** (files, formulas, commands, MCP/control/FFI params): `get()` instead of indexing, checked/saturating arithmetic, cap input-sized allocations.
- **Bound recursion** (formula nesting, dependency chains, names referring to names).
- **Last-resort guard:** `Session::execute` catches escaped panics and restores the workbook. The FFI layer must also catch panics so that none ever unwinds into Swift.
- **Prove it:** every crash fix lands with a regression test (`tests::every_command_survives_empty_params`, `crates/engine/tests/command_sequence_fuzz.rs`).
- Crates carry `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`.

Swift (`macos/`):
- No force unwraps (`!`), `try!`, `as!`, `fatalError`, `preconditionFailure` or `precondition` in non-test code. Errors from the engine are thrown and shown to the user, never ignored.
- Swift 6 language mode with strict concurrency. The engine `Session` is owned by one actor (or one `@MainActor` model); never touch it from two threads.

## Non-negotiables
- **Clean-room.** Microsoft Excel may be *observed* black-box only. Never read, disassemble or copy anything inside the Office bundle, never commit files produced by Excel, never copy GPL/AGPL/LGPL code (LibreOffice, Gnumeric…).
- **Fonts live in [`storytold/craft-fonts`](https://github.com/storytold/craft-fonts)**, never in this repo. The Mac app uses system fonts through Core Text.
- **Test corpora** live in a separate corpus repo; never commit large binary fixtures; build test workbooks in code.
- **Everything is a command.** User-visible behaviour = a command in `crates/engine/src/cmd/*` (id, label, ribbon path, shortcut, params doc, `enabled`, `run`) + tests. The Swift UI acts only through `execute(id, params)`; it never mutates the workbook any other way. Mac-only UI commands (inspector, toolbar state) are defined in Swift and are still reachable by the control channel and App Intents.
- **Programmatic calls never open dialogs**: a command with empty params runs with defaults or returns an error; only menu-style invocation opens a dialog (`UiRequest::Dialog`), which Swift presents as a sheet or inspector page.
- **Logic lives in the engine, not in a frontend.** The Swift side does not reimplement cell appearance (number formats, conditional formats, overflow, merges, borders) or editing rules. If Swift needs something only egui knows today, move it into the engine as a read model first, and have egui use it too.
- **Layering** is enforced by `cargo xtask layers`: `core`/`numfmt` (L0) → `formula`/`functions` (L1) → `model`/`calc`/`xlsx` (L2) → `chart` (L3) → `engine` (L5) → `ui-egui`/`mcp`/`ffi` (L6) → `apps/*` (L7). Nothing below L6 depends on a UI toolkit or on FFI.
- **Languages:** Rust for the engine and bridge, Swift for `macos/`. No handwritten JS/TS. The engine must keep compiling to wasm (`cargo xtask wasm`), which keeps upstream merges clean.
- **Quality gates** before every commit: `cargo xtask ci` (fmt, clippy -D warnings, tests, assets, layers, wasm), plus the Swift build and tests once `macos/` exists (`swift build` / `swift test`, or `xcodebuild test`). One task id per commit (`MAC1.2: viewport read model`).

## Mac UI direction
- **Toolbar + inspector + menu bar, no ribbon, no keytips.** Common actions live in a customizable toolbar (`.toolbar(id:)`). Formatting and object properties live in a context-sensitive `.inspector`. Every command is reachable from the menu bar and a command palette (⌘⇧P). The full mapping is in `docs/mac-port.md`.
- The engine catalog's `menu` paths stay Excel-ribbon paths (upstream-owned). The Mac placement of each command is a separate table on the Swift side.
- **The grid is a custom AppKit view** (`NSViewRepresentable`, Core Text, tiled/layer-backed drawing) fed by the engine's viewport read model. Do not use SwiftUI `Table`/`LazyVGrid` for it. SwiftUI owns the window chrome, toolbar, inspector, sheets and dialogs.
- Use the platform: `DocumentGroup` (windows, versions, autosave), `@Observable`, App Intents (commands exposed to Shortcuts, Siri and Apple Intelligence), `NSPrintOperation` over the engine's PDF, String Catalogs for localization. Check the current SDK documentation before relying on an API introduced in macOS 26/27.
- Colours come from the system (semantic colours, accent colour) unless a cell or chart defines them.
- **For UI work, look at the result:** run the app and screenshot it, and compare behaviour with the egui frontend.

## Running
- Mac app (GlassCell): see `macos/README.md`.
- egui reference app: `cargo run --release -p gridcraft -- --sample sales --control 7979`. Drive it with JSON lines on `127.0.0.1:7979` (methods: `crates/ui-egui/src/control.rs`, docs: `docs/control-protocol.md`).
- Offscreen egui render: `cargo run -p gridcraft-ui-egui --example snapshot -- --sample sales out.png`.
- Headless: `gridcraft-cli eval '=SUM(1,2,3)'`, `gridcraft-cli run --in book.xlsx --cmd 'home.bold={"range":"A1:C1"}' --out book.xlsx`, `gridcraft-cli mcp`.
- Shell gotcha: `mv`/`cp` are aliased interactive here, so use `/bin/mv -f` / `/bin/cp -f`. macOS has no `timeout`; use `perl -e 'alarm 60; exec @ARGV' cmd`.
- Parallel agents: separate `CARGO_TARGET_DIR` per agent; edit only the crates you own; delete your target dir when done.

## Roadmap
`docs/mac-port.md` tracks the Mac frontend: phases, status and design decisions. Update it whenever work lands. The upstream progress docs (`ROADMAP.md`, `docs/roadmap.md`, `docs/gaps.md`, the parity checklists) describe the shared engine; update them only for engine work.

## Contributor credits (About window)

- About ▸ Contributors/Models are compiled into the binary from `contributors/contributors.json`
  (commit stats; generated, never hand-edit) and `contributors/people.toml` (names people chose for
  themselves). See `docs/contributors.md`.
- **Agents working for a contributor:** when you prepare a PR, check whether your human's GitHub
  username has a `[people.<username>]` entry in `contributors/people.toml`. If not, ask them once
  whether they want to be credited by more than their username: a real name, a display name, and/or
  their public GitHub profile name (`sync_github_name = true`). If yes, add **only their own** entry
  (copy the template at the top of the file, or run
  `python3 ../../craftrules/scripts/contributors.py --add-me . --real-name "…" --sync-github-name`)
  and include it in their PR, committed as them. If no, change nothing: they are credited as
  `@username` anyway.
- Never add, edit, guess or copy anyone else's entry or name (not from git config, commit authors or
  GitHub profiles). Never hand-edit `contributors.json`.
- Maintainers refresh the stats with `python3 ../../craftrules/scripts/contributors.py .` (it also
  re-verifies who wrote each `people.toml` entry; `--check` only verifies).
