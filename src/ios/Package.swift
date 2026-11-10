// swift-tools-version: 5.7
import PackageDescription

// Only the enabled plugins' Swift is compiled into an app.
//
// App Store Connect rejects a binary that references a permission API (location,
// camera, notifications...) without the matching usage string (ITMS-90683), so a
// plugin an app has not enabled must not be in its `DioxusSwiftPlugins.framework`
// at all.
//
// `dx` builds one product per `#[manganis::ffi]` block, each in a copy of this
// package named after the product in lower case, and installs the first dynamic
// library it finds. The crate's `build.rs` adds a block whose product is named
// after the enabled features: `G3NativePlugins` followed by each token below, in
// this order (`G3NativePluginsBackButtonClipboard` for those two). In that copy
// this manifest reads the tokens back from its directory name and declares that
// product as the only dynamic library, compiling only their sources. Each
// plugin's own product (`ClipboardPlugin`...) is static and empty, so `dx`
// builds it quickly and passes over it.
//
// Built anywhere else (`swift build` here, Xcode), the directory name does not
// start with the product prefix: the `G3NativePlugins` product holds every
// plugin.
//
// Keep `plugins` in step with `SWIFT_PLUGINS` in `build.rs`; a source test in
// `src/lib.rs` checks the two lists match.

struct Plugin {
    let token: String
    let sources: [String]
    let frameworks: [String]
    /// Plugins whose sources this one uses, as its Cargo feature turns them on.
    var requires: [String] = []
}

let plugins: [Plugin] = [
    Plugin(token: "AppInfo", sources: ["AppInfoPlugin.swift"], frameworks: ["Foundation"]),
    Plugin(token: "Auth", sources: ["AuthPlugin.swift"], frameworks: ["AuthenticationServices", "UIKit"]),
    Plugin(token: "BackButton", sources: ["BackButtonPlugin.swift"], frameworks: ["UIKit", "WebKit"]),
    Plugin(token: "CameraMicrophone", sources: ["CameraMicrophonePlugin.swift"], frameworks: ["AVFoundation", "UIKit"]),
    Plugin(token: "Clipboard", sources: ["ClipboardPlugin.swift"], frameworks: ["UIKit"]),
    Plugin(token: "DeepLinks", sources: ["DeepLinksPlugin.swift"], frameworks: ["UIKit"]),
    Plugin(token: "ExternalUrl", sources: ["ExternalUrlPlugin.swift"], frameworks: ["UIKit"]),
    Plugin(token: "Geolocation", sources: ["GeolocationPlugin.swift"], frameworks: ["CoreLocation"]),
    Plugin(token: "InAppPurchases", sources: ["InAppPurchasesPlugin.swift", "StoreCatalogue.swift"], frameworks: ["StoreKit"]),
    Plugin(token: "Media", sources: ["MediaPlugin.swift"], frameworks: ["AVFoundation", "MediaPlayer", "UIKit", "WebKit"]),
    Plugin(token: "Notifications", sources: ["NotificationsPlugin.swift", "NotificationCenterRelay.swift"], frameworks: ["UIKit", "UserNotifications"]),
    Plugin(token: "PushNotifications", sources: ["PushNotificationsPlugin.swift"], frameworks: ["UIKit", "UserNotifications"], requires: ["Notifications"]),
    Plugin(token: "Storage", sources: ["StoragePlugin.swift"], frameworks: ["Security"]),
    Plugin(token: "Updater", sources: ["UpdaterPlugin.swift"], frameworks: ["Foundation", "UIKit"]),
]

let productPrefix = "G3NativePlugins"

/// The product named by the directory `dx` copied this package into, and the
/// plugins it holds (with those they require), or nil when `dx` did not copy it.
func selectedPlugins() -> (product: String, plugins: [Plugin])? {
    let directory = Context.packageDirectory.split(separator: "/").last.map(String.init) ?? ""
    var rest = Substring(directory.lowercased())
    guard rest.hasPrefix(productPrefix.lowercased()) else { return nil }
    rest = rest.dropFirst(productPrefix.count)

    var selected: [Plugin] = []
    for plugin in plugins where rest.hasPrefix(plugin.token.lowercased()) {
        selected.append(plugin)
        rest = rest.dropFirst(plugin.token.count)
    }
    guard rest.isEmpty, !selected.isEmpty else {
        // Fail the build rather than guess: compiling too much is what this
        // manifest exists to prevent.
        fatalError("g3-native-plugins: unknown Swift plugins in \(directory); update Package.swift and build.rs together")
    }
    let product = productPrefix + selected.map(\.token).joined()
    let required = Set(selected.flatMap(\.requires))
    let compiled = plugins.filter { plugin in
        required.contains(plugin.token) || selected.contains { $0.token == plugin.token }
    }
    return (product, compiled)
}

func linkerSettings(for selected: [Plugin]) -> [LinkerSetting] {
    var frameworks = ["Foundation"]
    for framework in selected.flatMap(\.frameworks) where !frameworks.contains(framework) {
        frameworks.append(framework)
    }
    return frameworks.sorted().map { framework in
        switch framework {
        case "MediaPlayer", "UIKit":
            return .linkedFramework(framework, .when(platforms: [.iOS]))
        default:
            return .linkedFramework(framework)
        }
    }
}

// Spelled through a constant: `dx` rewrites the text `type: .static` in its copy
// to `.dynamic`, and these must stay static for `dx` to pass over them.
let staticLibrary: Product.Library.LibraryType = .static

// One per plugin's `#[manganis::ffi]` block, which `dx` asks for by name.
// They hold nothing: the plugins' Swift is in the product above.
let pluginProducts = plugins.map { plugin in
    Product.library(name: "\(plugin.token)Plugin", type: staticLibrary, targets: ["PluginBinding"])
}
let pluginBinding = Target.target(name: "PluginBinding", path: "PluginBinding")

let products: [Product]
let target: Target
if let (product, selected) = selectedPlugins() {
    products = [.library(name: product, type: .dynamic, targets: ["DioxusNativePlugins"])] + pluginProducts
    target = .target(
        name: "DioxusNativePlugins",
        path: "Sources",
        exclude: plugins.flatMap(\.sources).filter { !selected.flatMap(\.sources).contains($0) },
        sources: selected.flatMap(\.sources),
        linkerSettings: linkerSettings(for: selected)
    )
} else {
    products = [.library(name: productPrefix, type: staticLibrary, targets: ["DioxusNativePlugins"])] + pluginProducts
    target = .target(
        name: "DioxusNativePlugins",
        path: "Sources",
        linkerSettings: linkerSettings(for: plugins)
    )
}

let package = Package(
    name: "DioxusNativePlugins",
    platforms: [.iOS("15")],
    products: products,
    targets: [target, pluginBinding]
)
