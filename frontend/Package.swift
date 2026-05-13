// swift-tools-version: 5.10

import Foundation
import PackageDescription

// Resolve the Rust target/debug directory relative to this manifest file so
// that any build tool (swift build, swift run, swift-bundler) can find the dylib
// without needing extra -Xlinker flags on the command line.
let repoRoot = URL(fileURLWithPath: #file)
    .deletingLastPathComponent()   // frontend/
    .deletingLastPathComponent()   // repo root
    .standardized
let rustDebug = repoRoot.appendingPathComponent("target/debug").path

let package = Package(
    name: "frontend",
    platforms: [.macOS(.v13)],
    dependencies: [
        .package(
            url: "https://github.com/moreSwift/swift-cross-ui",
            .upToNextMinor(from: "0.2.1")
        ),
    ],
    targets: [
        // C module wrapping the UniFFI-generated header.
        .target(
            name: "passwordFFI",
            path: "Sources/passwordFFI",
            publicHeadersPath: "include",
            linkerSettings: [
                .linkedLibrary("password"),
            ]
        ),

        .executableTarget(
            name: "frontend",
            dependencies: [
                .product(name: "SwiftCrossUI", package: "swift-cross-ui"),
                .product(name: "DefaultBackend", package: "swift-cross-ui"),
                "passwordFFI",
            ],
            path: "Sources/frontend",
            swiftSettings: [
                // Pass the Rust dylib search path and rpath through to the linker.
                .unsafeFlags([
                    "-Xlinker", "-L\(rustDebug)",
                    "-Xlinker", "-rpath",
                    "-Xlinker", rustDebug,
                ]),
            ]
        ),
    ]
)
