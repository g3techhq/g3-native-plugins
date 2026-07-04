// swift-tools-version: 5.7
import PackageDescription

let package = Package(
    name: "DioxusNativePlugins",
    platforms: [.iOS("15")],
    products: [
        .library(name: "AuthPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "ClipboardPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "ExternalUrlPlugin", type: .static, targets: ["DioxusNativePlugins"]),
    ],
    targets: [
        .target(
            name: "DioxusNativePlugins",
            path: "Sources",
            linkerSettings: [
                .linkedFramework("AuthenticationServices"),
                .linkedFramework("Foundation"),
                .linkedFramework("UIKit", .when(platforms: [.iOS])),
            ]
        ),
    ]
)
