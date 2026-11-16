// swift-tools-version:5.9
import PackageDescription

// libheadroom_core.a comes from core/ (Rust). scripts/bundle.sh builds it and passes its folder
// to the linker with -Xlinker -L, so this file carries no paths.
let package = Package(
    name: "Headroom",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "CHeadroom", path: "Sources/CHeadroom"),
        .executableTarget(
            name: "Headroom",
            dependencies: ["CHeadroom"],
            linkerSettings: [
                .linkedFramework("CoreServices"),
            ]
        ),
    ]
)
