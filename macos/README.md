# GridCraft for Mac

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
- `build-app.sh`: builds `target/mac/GridCraft.app` (Info.plist with the xlsx/csv/tsv document
  types, ad-hoc signed).

## Building and testing

Needs Xcode 27 (Swift 6) and rustup (the script uses rustup's toolchain and adds the
`aarch64-apple-darwin` / `x86_64-apple-darwin` targets).

```sh
macos/build-engine.sh                     # after any change to the engine or crates/ffi
(cd macos/GridCraftEngine && swift test)
(cd macos/GridCraft && swift test)
SKIP_ENGINE=1 macos/build-app.sh && open target/mac/GridCraft.app

# Look at the grid without a window, and time it (a 100,000-row sheet, 60 scrolling frames):
(cd macos/GridCraft && swift run -c release grid-snapshot --sample sales out.png)
(cd macos/GridCraft && swift run -c release grid-snapshot --rows 100000 --frames 60 --scroll 0,800000 out.png)
```

`PROFILE=dev` builds the engine unoptimised (faster to build, slow to run); `ARCHS=arm64` builds
this Mac's architecture only.

## Rules

- Swift 6 language mode, strict concurrency. One `Engine` per document, used from one actor.
- No force unwraps, `try!`, `as!`, `fatalError` or `precondition` in handwritten code. The generated
  bindings are exempt, like UniFFI's generated Rust.
- The UI changes the workbook only through `Engine.execute(id:params:)`.
- Never change SwiftUI state (`@State`, `@Published`) from inside AppKit drawing (`draw(_:)`,
  layout): AppKit throws mid-display and the app goes down. Report after the pass (`Task { @MainActor … }`).
