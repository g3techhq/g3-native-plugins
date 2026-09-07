# g3-native-plugins

[![CI](https://github.com/g3techhq/g3-native-plugins/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/g3techhq/g3-native-plugins/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/g3-native-plugins.svg)](https://crates.io/crates/g3-native-plugins)
[![docs.rs](https://docs.rs/g3-native-plugins/badge.svg)](https://docs.rs/g3-native-plugins)
[![License](https://img.shields.io/crates/l/g3-native-plugins.svg)](#license)

Dioxus wrappers for native clipboard/share, Apple and Google auth hooks, external URLs, and deep-link metadata helpers.

The Cargo package is `g3-native-plugins`; the Rust crate name is `g3_native_plugins`.

## Platform Support

Coverage is not uniform. Where a plugin has no implementation for the target,
the calls compile to inert no-ops rather than failing the build — so a missing
platform shows up as nothing happening at runtime, not as a compile error.

| Feature | Android | iOS / macOS | Web | Notes |
| --- | --- | --- | --- | --- |
| `clipboard` | yes | yes | yes | Copy plus a native share sheet on mobile. |
| `auth` | yes | yes | no | Google Sign-In on Android, Sign in with Apple on iOS. |
| `external-url` | yes | yes | yes | Opens the system browser. |
| `back-button` | yes | n/a | no | iOS has no hardware back button; use an interactive swipe-back instead. |
| `media` | yes | **not yet** | no | Background playback, lock-screen controls, and Now Playing metadata. The iOS side (AVAudioSession, MPNowPlayingInfoCenter, MPRemoteCommandCenter) is unimplemented. |

`deep_links` is pure metadata generation and builds everywhere, including on
the server.

## Install

Enable only the plugins your app uses:

```toml
[dependencies]
g3-native-plugins = { version = "0.1", features = ["clipboard", "auth", "external-url"] }
```

## Provide Plugins

Create the provider once near your app root. The provider lazily constructs each native plugin the first time it is used.

```rust,ignore
use dioxus::prelude::*;
use g3_native_plugins::NativePluginsProvider;

#[component]
fn App() -> Element {
    rsx! { NativePluginsProvider { Outlet::<Route> {} } }
}
```

## Clipboard and Share

```rust,ignore
use dioxus::prelude::*;
use g3_native_plugins::NativePlugins;

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
g3_native_plugins::ios_app_site_association_route! {
    team_id: "TEAMID",
    bundle_id: "com.example.App",
    paths: ["/games/*/join"],
}

g3_native_plugins::android_asset_links_route! {
    package_name: "com.example.app",
    sha256_cert_fingerprints: ["AA:BB:CC"],
}
```

## System Back Button

Android delivers back to the Activity, never to the WebView, so a web app
hosted this way exits on the first press however it is written. The plugin
takes the press and dispatches the cancelable `g3nativeback` event on the
WebView's `window`. It deliberately does not call browser `history.back()`:
Dioxus keeps native-app route history in Rust rather than WebView history.

`g3-route-transitions` provides the standard integration. Enable its
`native-back` feature and call `use_native_back_navigation::<Route>()` once in
a routed layout; it manages interception and invokes the animated Dioxus pop.
No application-specific event bridge is needed.

Interception is off until asked for, because only the app knows whether there
is anywhere to go back to. Leaving it on at the root of the stack means the
user cannot leave. Higher-level integrations can call `fall_through()` to pass
one intercepted press to Android's next Back handler without recursion.

For lower-level use without `g3-route-transitions`, interception remains
available directly:

```rust,ignore
use dioxus::prelude::*;
use g3_native_plugins::NativePlugins;

let mut plugins = use_context::<NativePlugins>();
let navigator = use_navigator();

// Re-run wherever the route changes.
use_effect(move || {
    let _ = plugins.back_button.write().set_intercepting(navigator.can_go_back());
});
```

iOS has no system back press and on the web the browser owns it, so both are
inert — callers need no `cfg` of their own.

The Android implementation resolves its Kotlin bridge through the Activity's
application class loader. This matters because Dioxus effects run on a native
thread, where JNI's default `FindClass` otherwise sees only the system loader.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
