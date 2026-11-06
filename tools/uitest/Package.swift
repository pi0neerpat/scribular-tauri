// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "uitest",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "UITestCore", targets: ["UITestCore"]),
        .executable(name: "uitest", targets: ["uitest"]),
    ],
    targets: [
        .target(name: "UITestCore"),
        .executableTarget(name: "uitest", dependencies: ["UITestCore"]),
        .testTarget(name: "UITestCoreTests", dependencies: ["UITestCore"]),
    ]
)
