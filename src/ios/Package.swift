// swift-tools-version: 5.7
import PackageDescription

let package = Package(
    name: "DioxusNativePlugins",
    platforms: [.iOS("15")],
    products: [
        .library(name: "AuthPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "BackButtonPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "CameraMicrophonePlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "ClipboardPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "DeepLinksPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "ExternalUrlPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "GeolocationPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "InAppPurchasesPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "MediaPlugin", type: .static, targets: ["DioxusNativePlugins"]),
        .library(name: "StoragePlugin", type: .static, targets: ["DioxusNativePlugins"]),
    ],
    targets: [
        .target(
            name: "DioxusNativePlugins",
            path: "Sources",
            linkerSettings: [
                .linkedFramework("AVFoundation"),
                .linkedFramework("AuthenticationServices"),
                .linkedFramework("CoreLocation"),
                .linkedFramework("Foundation"),
                .linkedFramework("MediaPlayer", .when(platforms: [.iOS])),
                .linkedFramework("Security"),
                .linkedFramework("StoreKit"),
                .linkedFramework("UIKit", .when(platforms: [.iOS])),
                .linkedFramework("WebKit"),
            ]
        ),
    ]
)
