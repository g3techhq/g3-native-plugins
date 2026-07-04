// swift-tools-version: 5.7
import PackageDescription

let package = Package(
    name: "ExternalUrlPlugin",
    platforms: [.iOS("15")],
    products: [
        .library(name: "ExternalUrlPlugin", targets: ["ExternalUrlPlugin"]),
    ],
    targets: [
        .target(
            name: "ExternalUrlPlugin",
            path: "Sources"
        ),
    ]
)
