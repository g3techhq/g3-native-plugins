# dx-native-plugins

Dioxus wrappers for native clipboard/share, Apple and Google auth hooks, external URLs, and deep-link metadata helpers.

The Cargo package is `dx-native-plugins`; the Rust crate name is `dx_native_plugins`.

## Install

Enable only the plugins your app uses:

```toml
[dependencies]
dx-native-plugins = { version = "0.1", features = ["clipboard", "auth", "external-url"] }
```

## Provide Plugins

Create the provider once near your app root. The provider lazily constructs each native plugin the first time it is used.

```rust,ignore
use dioxus::prelude::*;
use dx_native_plugins::NativePluginsProvider;

#[component]
fn App() -> Element {
    rsx! { NativePluginsProvider { Outlet::<Route> {} } }
}
```

## Clipboard and Share

```rust,ignore
use dioxus::prelude::*;
use dx_native_plugins::NativePlugins;

let mut plugins = use_context::<NativePlugins>();
plugins.clipboard.write().copy_to_clipboard("Invite copied".to_string())?;
plugins.clipboard.write().share("Join my game".to_string())?;
```

The web implementation uses the browser Clipboard and Web Share APIs. Android and iOS use the bundled Manganis FFI plugin sources under `src/android` and `src/ios`.

## Auth

Enable the `auth` feature to expose platform auth helpers:

```rust,ignore
let mut plugins = use_context::<NativePlugins>();

#[cfg(target_os = "android")]
let google = plugins.auth.write().start_google_auth()?;

#[cfg(any(target_os = "ios", target_os = "macos"))]
let apple = plugins.auth.write().start_apple_auth()?;
```

On iOS and macOS, Apple Sign In is asynchronous. Call `is_auth_awaiting()` and `poll_auth_result()` from your UI flow until a credential arrives or the flow ends.

## External URLs

```rust,ignore
let mut plugins = use_context::<NativePlugins>();
plugins.external_url.write().open("https://example.com")?;
```

## Deep-Link Metadata

The `deep_links` module is always available and generates `.well-known` JSON values:

```rust,ignore
dx_native_plugins::ios_app_site_association_route! {
    team_id: "TEAMID",
    bundle_id: "com.example.App",
    paths: ["/games/*/join"],
}

dx_native_plugins::android_asset_links_route! {
    package_name: "com.example.app",
    sha256_cert_fingerprints: ["AA:BB:CC"],
}
```
