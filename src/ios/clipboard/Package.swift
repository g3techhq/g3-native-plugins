// swift-tools-version: 5.7
import PackageDescription

let package = Package(
    name: "ClipboardPlugin",
    platforms: [.iOS("15")],
    products: [
        .library(name: "ClipboardPlugin", targets: ["ClipboardPlugin"]),
    ],
    targets: [
        .target(
            name: "ClipboardPlugin",
            path: "Sources"
        ),
    ]
)
