// swift-tools-version: 6.2
// The GridCraft engine for the Mac app: the Rust engine (`crates/ffi`) as an XCFramework, its
// UniFFI bindings, and Swift helpers. Build the framework first: `macos/build-engine.sh`.
import PackageDescription

let package = Package(
    name: "GridCraftEngine",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "GridCraftEngine", targets: ["GridCraftEngine"]),
    ],
    targets: [
        .binaryTarget(name: "GridCraftFFI", path: "GridCraftFFI.xcframework"),
        .target(
            name: "GridCraftEngine",
            dependencies: ["GridCraftFFI"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "GridCraftEngineTests",
            dependencies: ["GridCraftEngine"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
