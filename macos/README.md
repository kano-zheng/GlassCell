# GlassCell (the macOS app)

The native macOS frontend (SwiftUI, macOS 26+) over GridCraft's Rust engine. Plan and status:
[`docs/mac-port.md`](../docs/mac-port.md).

## Layout

- `build-engine.sh`: builds `crates/ffi` (the UniFFI bridge) for arm64 and x86_64 into
  `GridCraftEngine/GridCraftFFI.xcframework` and writes the Swift bindings to
  `GridCraftEngine/Sources/GridCraftEngine/Generated.swift`. Neither is committed.
- `GridCraftEngine/`: Swift package wrapping the engine: the generated bindings plus Swift helpers
  (`CoreTextMeasurer`, the text measurement the engine's viewport layout calls back into).
- `GridCraft/`: the app. `GridCraftUI` holds the document (`SpreadsheetDocument`, a
  `ReferenceFileDocument` over one engine session), the grid (`GridNSView`, drawn by
  `GridRenderer` from the engine's viewport and geometry) and the window (`WorkbookView`);
  `GridCraft` is the `DocumentGroup` app; `grid-snapshot` renders a grid offscreen and times it.
- `build-app.sh`: builds `target/mac/GlassCell.app` with SwiftPM (Info.plist with the xlsx/csv/tsv
  document types, ad-hoc signed): quick, but without App Intents metadata or extensions.
- `project.yml` + `build-xcode.sh`: the full app through Xcode (XcodeGen generates
  `GridCraft.xcodeproj`, not committed): App Intents metadata and the app extensions.

## Building and testing

Needs Xcode 27 (Swift 6) and rustup (the script uses rustup's toolchain and adds the
`aarch64-apple-darwin` / `x86_64-apple-darwin` targets).

```sh
macos/build-engine.sh                     # after any change to the engine or crates/ffi
(cd macos/GridCraftEngine && swift test)
(cd macos/GridCraft && swift test)
SKIP_ENGINE=1 macos/build-app.sh && open target/mac/GlassCell.app

# Look at the grid without a window, and time it (a 100,000-row sheet, 60 scrolling frames):
(cd macos/GridCraft && swift run -c release grid-snapshot --sample sales out.png)
(cd macos/GridCraft && swift run -c release grid-snapshot --rows 100000 --frames 60 --scroll 0,800000 out.png)
```

`PROFILE=dev` builds the engine unoptimised (faster to build, slow to run); `ARCHS=arm64` builds
this Mac's architecture only.

## Localization

`GridCraft/Resources/Localizable.xcstrings` is generated from egui's translation tables:
`cargo xtask mac-strings` after changing them (`--check` verifies it is current).

## Control channel

Agents and scripts drive the app with the JSON-lines protocol of `docs/control-protocol.md`:

```sh
open -n target/mac/GlassCell.app --args --control 7979   # or GRIDCRAFT_CONTROL_PORT=7979
printf '%s\n' '{"id":1,"method":"app.open","params":{"path":"/tmp/book.xlsx"}}' \
  '{"id":2,"method":"cell.get","params":{"cell":"B3"}}' | nc 127.0.0.1 7979
```

It listens on 127.0.0.1 only and is off unless asked for. It acts on the frontmost workbook;
`ui.click`/`ui.drag` take points from the top-left of the window's content, which `ui.inspect`
reports the grid in, and `ui.screenshot` captures only the GridCraft window.

## Rules

- Swift 6 language mode, strict concurrency. One `Engine` per document, used from one actor.
- No force unwraps, `try!`, `as!`, `fatalError` or `precondition` in handwritten code. The generated
  bindings are exempt, like UniFFI's generated Rust.
- The UI changes the workbook only through `Engine.execute(id:params:)`.
- Never change SwiftUI state (`@State`, `@Published`) from inside AppKit drawing (`draw(_:)`,
  layout): AppKit throws mid-display and the app goes down. Report after the pass (`Task { @MainActor … }`).
