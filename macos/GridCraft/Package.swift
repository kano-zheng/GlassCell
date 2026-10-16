// swift-tools-version: 6.2
// The GridCraft Mac app. `macos/build-app.sh` builds it into GridCraft.app (the bundle carries
// the document types DocumentGroup needs); `swift run grid-snapshot` renders a grid offscreen.
import PackageDescription

let swift6: [SwiftSetting] = [.swiftLanguageMode(.v6)]

let package = Package(
    name: "GridCraft",
    platforms: [.macOS(.v26)],
    dependencies: [.package(path: "../GridCraftEngine")],
    targets: [
        .target(name: "GridCraftUI", dependencies: [.product(name: "GridCraftEngine", package: "GridCraftEngine")], swiftSettings: swift6),
        .executableTarget(name: "GridCraft", dependencies: ["GridCraftUI"], swiftSettings: swift6),
        .executableTarget(name: "grid-snapshot", dependencies: ["GridCraftUI"], swiftSettings: swift6),
        .testTarget(name: "GridCraftUITests", dependencies: ["GridCraftUI"], swiftSettings: swift6),
    ]
)
