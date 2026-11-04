//! Names the Swift product an iOS build needs, so the app's Swift framework
//! holds only the enabled plugins.
//!
//! All of the plugins' Swift used to be one target, and every app shipped all
//! of it: location, camera, notification and StoreKit calls included, which
//! App Store Connect rejects (ITMS-90683) without a usage string for each.
//!
//! `dx` builds one Swift product per `#[manganis::ffi]` block, named after the
//! block's first type, in a copy of the package in a directory named after
//! that type in lower case, and installs the first dynamic library it finds as
//! `DioxusSwiftPlugins.framework`. This script writes one more block, declaring
//! only a marker type whose name lists the enabled plugins
//! (`G3NativePluginsBackButtonClipboard`). `src/ios/Package.swift` reads the
//! list back from its directory name and builds that product as the only
//! dynamic library, holding exactly those plugins; the plugins' own products
//! are static and empty, so `dx` passes over them.
//!
//! The token list must match `plugins` in `src/ios/Package.swift`; a source
//! test in `src/lib.rs` checks that it does.

use std::{env, fs, path::Path};

/// Cargo feature and Swift product token for each plugin with Swift code, in
/// the order the product name lists them.
const SWIFT_PLUGINS: &[(&str, &str)] = &[
    ("app-info", "AppInfo"),
    ("auth", "Auth"),
    ("back-button", "BackButton"),
    ("camera-microphone", "CameraMicrophone"),
    ("clipboard", "Clipboard"),
    ("deep-links", "DeepLinks"),
    ("external-url", "ExternalUrl"),
    ("geolocation", "Geolocation"),
    ("in-app-purchases", "InAppPurchases"),
    ("media", "Media"),
    ("notifications", "Notifications"),
    ("push-notifications", "PushNotifications"),
    ("storage", "Storage"),
    ("updater", "Updater"),
];

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    let mut product = String::from("G3NativePlugins");
    let mut any = false;
    for (feature, token) in SWIFT_PLUGINS {
        let variable = format!("CARGO_FEATURE_{}", feature.to_uppercase().replace('-', "_"));
        if env::var_os(variable).is_some() {
            product.push_str(token);
            any = true;
        }
    }

    // No Swift plugin, no Swift to build.
    let generated = if any {
        format!(
            r#"#[manganis::ffi("src/ios")]
unsafe extern "Swift" {{
    pub type {product};
}}

// Only the name matters: the Swift class is never looked up.
const _: fn() -> Result<{product}, &'static str> = {product}::new;
"#
        )
    } else {
        String::new()
    };

    let out_dir = env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts");
    fs::write(Path::new(&out_dir).join("swift_product.rs"), generated)
        .expect("the build script can write to OUT_DIR");
}
