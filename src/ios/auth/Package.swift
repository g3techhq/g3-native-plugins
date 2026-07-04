// swift-tools-version: 5.7
import PackageDescription

let package = Package(
    name: "AuthPlugin",
    platforms: [.iOS("15")],
    products: [
        .library(name: "AuthPlugin", targets: ["AuthPlugin"]),
    ],
    targets: [
        .target(
            name: "AuthPlugin",
            path: "Sources"
        ),
    ]
)
