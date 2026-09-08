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
| `camera-microphone` | yes | iOS only | n/a | Permission state, prompting, and the Settings escape hatch around `getUserMedia`. Not a capture API. |
| `clipboard` | yes | yes | yes | Copy plus a native share sheet on mobile. |
| `auth` | yes | yes | no | Google Sign-In on Android, Sign in with Apple on iOS. |
| `external-url` | yes | yes | yes | Opens the system browser. |
| `geolocation` | yes | iOS only | n/a | Start-and-poll position requests and the runtime permission prompt. Web is inert on purpose: use `navigator.geolocation`. |
| `back-button` | yes | iOS only | no | The back *event*, whatever raises it: Android's system back, an iOS left-edge swipe. |
| `deep-links` | yes | iOS only | n/a | Receives the URL a universal link, App Link, or custom scheme opened the app with. |
| `in-app-purchases` | yes | iOS only | n/a | Products, purchases, subscriptions, and restore. Play Billing and StoreKit 2. |
| `storage` | yes | iOS only | yes\* | Key-value storage. Keystore AES-GCM / Keychain. \*Web is `localStorage` and **not** encrypted. |
| `media` | yes | iOS only | no | Background playback, lock-screen controls, and Now Playing metadata. |

The `deep_links` module's metadata builders are not feature-gated and build
everywhere, including on the server. Only receiving a link needs the
`deep-links` feature.

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
// The web client id from your own Google Cloud project.
plugins.auth.write().start_google_auth(GOOGLE_SERVER_CLIENT_ID)?;

#[cfg(any(target_os = "ios", target_os = "macos"))]
plugins.auth.write().start_apple_auth()?;

// Both calls return as soon as the native account UI has been requested.
if let Some(credential) = plugins.auth.write().poll_auth_result()? {
    // Send the Google ID token or Apple credential JSON to your server.
} else if !plugins.auth.write().is_auth_awaiting() {
    // The flow ended without a credential: cancellation or refusal.
}
```

Sign-in is asynchronous on both platforms. Call `poll_auth_result()` from your
UI flow until a credential arrives or `is_auth_awaiting()` becomes false. A
successful Android poll consumes the credential, so it is returned only once.
Android yields the Google ID token directly; Apple yields JSON containing
`identity_token` and, on the first authorization, any email and display name
Apple supplied.

`start_google_auth` takes the **web** client id from your own Google Cloud
project — the one ending `.apps.googleusercontent.com`, not the Android client
id. It is a parameter rather than a crate constant because it names one
project: Credential Manager checks it against an Android client registered in
that same project for your package name and signing certificate, so an id
belonging to another app cannot work however well formed it is. A wrong or
unregistered id fails exactly like a missing account — no credential, no
explanation — so check the registration first when sign-in returns `None`
unexpectedly.

## External URLs

```rust,ignore
let mut plugins = use_context::<NativePlugins>();
plugins.external_url.write().open("https://example.com")?;
```

## Storage

```rust,ignore
let mut plugins = use_context::<NativePlugins>();

plugins.storage.write().set("session", &token)?;
let token = plugins.storage.write().get("session")?;
plugins.storage.write().remove("session")?;
```

The type is `KeyValueStore`, not `Storage`, because `dioxus::prelude` already
exports a `Storage` trait — an app glob-importing both would otherwise find the
name ambiguous and fail to compile.

Synchronous, unlike most plugins here, because both stores are: a Keychain
lookup and a `SharedPreferences` read are ordinary in-process work and there is
nothing to poll for.

Values are held by the platform's own secret store — the Keychain on iOS, and
on Android AES-GCM under a key generated in the hardware-backed Keystore, which
will perform operations with it but never export it. That is what
`EncryptedSharedPreferences` does underneath; doing it directly avoids pulling
Tink in behind a library Google no longer maintains, and this module has no
dependencies at all.

Three things to know:

- **Key names are stored in the clear** on both platforms. Only values are
  encrypted, so a key name is a label, not a hiding place.
- **The web build encrypts nothing.** `localStorage` is plain text readable by
  any script on the origin. It is there so one call site works everywhere, not
  because it is equivalent — anything that would matter if it leaked does not
  belong in a web build of this.
- **macOS returns an error from every call**, and is the one plugin here that
  does not compile to an inert no-op. A `set` that silently discarded and a
  `get` that always answered `None` would look like a working cache while
  losing data.

On Android a value can stop decrypting if the user adds or removes a device
lock, which can invalidate the Keystore key. That surfaces as an error from
`get()` rather than as a missing key, so the app can tell "signed out" from
"something went wrong".

## Camera and Microphone

**Capture is ordinary web code.** Call `navigator.mediaDevices.getUserMedia`
from the page and it works on both platforms — wry already bridges the WebView
permission callbacks. Its `WebChromeClient` asks Android for `CAMERA` and
`RECORD_AUDIO` when the page requests capture, and its `WKUIDelegate` grants
WebKit's request so iOS raises the system prompt. This plugin does not touch
either, and neither should you: replacing those delegates takes wry's file
chooser and JS dialogs with it.

What actually makes capture possible is declaring the permissions, which the
Dioxus CLI writes into the manifest and Info.plist:

```toml
[permissions]
camera = { description = "Show your swing to your partner" }
microphone = { description = "Talk to your partner" }

# Required on Android for audio capture, and easy to miss.
[android.permissions]
"android.permission.MODIFY_AUDIO_SETTINGS" = { description = "Audio capture" }
```

Without those, Android refuses before any prompt appears and iOS terminates the
app outright. If `getUserMedia` is failing, check this first — it is almost
always the answer.

**That last line is not optional, and its absence fails in a confusing way.**
wry's `WebChromeClient` asks Android for `MODIFY_AUDIO_SETTINGS` *alongside*
`RECORD_AUDIO` when a page requests audio. Requesting a permission the manifest
does not declare is denied instantly, and wry then denies the whole request — so
`getUserMedia` fails with `NotAllowedError` even though `RECORD_AUDIO` shows as
granted and the app never sees a prompt. It is an install-time permission, so no
amount of asking at runtime substitutes for the manifest entry. Verified on an
API 35 emulator: without the line, `NotAllowedError`; with it, two tracks.

The plugin covers what JavaScript cannot reach:

```rust,ignore
use g3_native_plugins::{NativePlugins, PermissionState};

let mut plugins = use_context::<NativePlugins>();

let state = plugins.camera_microphone.write().check_permissions()?;
match state.camera {
    // Ask at a moment you chose, not mid-stream.
    PermissionState::Prompt | PermissionState::PromptWithRationale => {
        plugins.camera_microphone.write().request_camera()?;
    }
    // Neither platform will prompt again. Settings is the only way back.
    PermissionState::Denied => plugins.camera_microphone.write().open_settings()?,
    PermissionState::Granted => { /* start the stream from JS */ }
}
```

Prompt results arrive on a callback a library cannot hook, so poll
`check_permissions()` after requesting, the same as geolocation.

One interaction worth knowing: if the app also uses the `media` feature, that
plugin claims a `.playback` audio session, which has **no input** — a microphone
opened under it returns silence. Call `set_capturing(true)` around capture to
move the session to `.playAndRecord`, and `set_capturing(false)` after. It is a
no-op on Android and unnecessary on iOS for an app that only captures, since
WebKit configures the session itself when nothing else has claimed it.

## In-App Purchases

```rust,ignore
let mut plugins = use_context::<NativePlugins>();

// At startup. Not at the point of sale — see below.
plugins.in_app_purchases.write().prepare()?;

plugins.in_app_purchases.write()
    .start_products_request(&["pro.monthly".to_string()])?;
if let Ok(Some(products)) = plugins.in_app_purchases.write().poll_products() { /* show */ }

plugins.in_app_purchases.write().start_purchase("pro.monthly", None)?;
if let Ok(Some(entitlements)) = plugins.in_app_purchases.write().poll_entitlements() {
    for entitlement in entitlements {
        // Verify entitlement.purchase_token on your server, then grant.
        if entitlement.needs_finishing {
            plugins.in_app_purchases.write().finish(&entitlement.transaction_id, false)?;
        }
    }
}
```

`poll_entitlements()` is the only place ownership arrives from, whether it came
from a purchase, a restore, or the background listener. Poll it for the life of
the app, not only around a purchase: both stores deliver transactions that
completed while the app was closed — an interrupted payment, a family-shared
buy, a renewal, a purchase a parent approved hours later — and only a listener
already running catches them. That is why `prepare()` belongs at startup.

**Finish every purchase.** `finish()` is the store's record that the user got
what they paid for. Android refunds anything left unacknowledged for three
days. Pass `consume: true` only for a consumable, so it can be bought again.

**Verify server-side.** `purchase_token` carries the signed JWS on iOS and the
purchase token on Android, both meant to be checked against Apple's or Google's
servers. Everything else on an `Entitlement` is display state read off a device,
and a device is not a thing to take financial claims from. This crate
deliberately does no verification.

Three places the platforms genuinely differ, none of which are papered over:

| | iOS | Android |
| --- | --- | --- |
| `expires_at_ms` | filled in | **always `None`** — Play does not tell the device, so subscription expiry has to come from the Play Developer API server-side |
| `ProductKind` | real, from StoreKit | one-time products always report `NonConsumable`; Play doesn't distinguish, the app decides by passing `consume` |
| `offer_id` | ignored — StoreKit applies an introductory offer itself, and a promotional one needs a signature this crate can't produce | selects a base plan or offer; a subscription can't be bought without one |

Play Billing is a real `implementation` dependency, unlike the `compileOnly`
androidx ones elsewhere here: the host app already carries androidx but has no
reason to carry billing, and unlike the location case there is no alternative
to weigh — Play billing is only reachable through that library. It is pinned to
7.1.1, the floor Google currently requires; check the deadline before shipping,
because they move it.

## Geolocation

Declare the permission in `Dioxus.toml` — the Dioxus CLI maps it to
`ACCESS_FINE_LOCATION` and `NSLocationWhenInUseUsageDescription`:

```toml
[permissions]
location = { precision = "fine", description = "Find courses near you" }
```

Then ask at runtime, which Android requires separately from declaring it:

```rust,ignore
use g3_native_plugins::{NativePlugins, PermissionState, PositionOptions};

let mut plugins = use_context::<NativePlugins>();

// Prompt, then poll check_permissions for the answer.
plugins.geolocation.write().request_permissions()?;

// Ask once...
plugins.geolocation.write().start_position_request(PositionOptions::default())?;

// ...and poll wherever the UI updates.
if let Ok(Some(position)) = plugins.geolocation.write().poll_position() {
    println!("{}, {}", position.coords.latitude, position.coords.longitude);
}
```

Requests are started and polled rather than awaited, because a fix takes
seconds and arrives on a platform callback. Blocking the caller would stall a
Dioxus effect thread for the duration, and on iOS it would not work at all:
`CLLocationManager` delivers on the main queue, which a blocked worker thread
is not. Permission results are polled for the same reason — the answer comes
back through `onRequestPermissionsResult`, an Activity callback a library
cannot override from outside, so `check_permissions()` is the way to read it.

Android fixes come from the platform `LocationManager`, not Google's fused
provider. The fused provider gives better results for less battery, but only by
forcing `play-services-location` on everyone depending on this crate, which is
not a trade a library should make for its consumers. An app that wants it can
still use it directly.

The web build is inert rather than wrapping `navigator.geolocation`: a page can
call that itself, and routing it through here would put a browser permission
prompt behind an API shaped for native asynchrony. macOS is inert too.

## Deep Links

Deep linking takes three pieces, and this crate is two of them.

1. **Declaring the links.** The Dioxus CLI already does this — `[deep_links]` in
   `Dioxus.toml` writes `CFBundleURLTypes` and the associated-domains entitlement
   on iOS, and the intent filters on Android. Nothing here duplicates it.
2. **Proving the claim.** Both platforms fetch a file from the site the app says
   it owns. The `deep_links` builders below generate them.
3. **Receiving the URL.** The `deep-links` feature. Neither platform hands the
   link to the WebView, so without this the app opens and the URL is lost.

### Receiving

```rust,ignore
use g3_native_plugins::DeepLinks;

fn main() {
    // As early as possible: see the cold-start note below.
    let mut deep_links = DeepLinks::new();
    let _ = deep_links.prepare();
    dioxus::launch(App);
}
```

Then drain the queue wherever routing decisions are made:

```rust,ignore
let mut plugins = use_context::<NativePlugins>();

use_future(move || async move {
    loop {
        while let Ok(Some(url)) = plugins.deep_links.write().take_link() {
            // Route on the URL however the app wants to.
        }
        gloo_timers::future::TimeoutFuture::new(200).await;
    }
});
```

Links are queued and polled rather than pushed at a listener, which is the
opposite of the back-button contract above and deliberately so: a back press is
a live gesture that is meaningless once missed, while a deep link is a value
that arrives before the app has rendered anything able to receive it. The queue
lives on the native side and is shared across plugin instances, so preparing
early and reading later from the provider both see the same links.

**Cold start on iOS is the one timing-sensitive case.** A link that starts the
app is delivered before any plugin exists, so it is recovered from the launch
options via `UIApplication.didFinishLaunchingNotification` — which only works if
`prepare()` ran before launching finished. Preparing from a component effect is
usually too late for that one case; preparing in `main` is not. Android has no
such constraint: the launch Intent stays readable on the Activity, so timing
does not matter there.

**Android needs `singleTask` or `singleTop`** as the host Activity's launch
mode for links to arrive while the app is already running. Under the default
`standard` mode Android starts a second Activity rather than calling
`onNewIntent`, and the link shows up as that Activity's launch Intent instead.
Observed on an emulator: a link sent to a running app restarted it under a new
pid and arrived through the launch-Intent path, which is the fallback working
as designed rather than the `onNewIntent` path. Dioxus.toml has no `launch_mode`
key as of CLI 0.7.9, so setting it needs `[android.raw]` manifest injection.

On the web the browser hands the app its URL directly and the router already
routes it; macOS delivers nothing here. Both are inert, so callers need no
`cfg` of their own.

The iOS implementation adds `application(_:continue:restorationHandler:)` and
`application(_:open:options:)` to the live app delegate's class at runtime,
because the delegate belongs to the windowing layer Dioxus runs on and
implements neither. It only ever adds: a delegate that already implements one
keeps its own, and the plugin logs instead of displacing it.

### Serving the well-known files

The `deep_links` module is always available — including on the server — and
generates the `.well-known` JSON values:

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

## System Back

This is about the back *event*, not the button. What raises it differs by
platform — Android's system back gesture or hardware key, an iOS swipe in from
the left screen edge — and neither reaches a WebView-hosted web app on its own.
Android delivers back to the Activity, never to the WebView, so the app exits on
the first press however it is written. iOS gives the edge swipe meaning only
inside a `UINavigationController`, which a bare `WKWebView` is not, so the swipe
lands on nothing.

Both plugins take the gesture and dispatch the same cancelable `g3nativeback`
event on the WebView's `window`, so one web-side listener serves both. Neither
calls browser `history.back()`: Dioxus keeps native-app route history in Rust
rather than in WebView history.

`g3-route-transitions` provides the standard integration. Enable its
`native-back` feature and call `use_native_back_navigation::<Route>()` once in
a routed layout; it manages interception and invokes the animated Dioxus pop.
No application-specific event bridge is needed.

Interception is off until asked for, because only the app knows whether there
is anywhere to go back to. Leaving it on at the root of the stack means the
user cannot leave on Android, and swallows edge swipes that should do nothing
on iOS. Higher-level integrations can call `fall_through()` to pass one
intercepted press to Android's next Back handler without recursion; on iOS
there is no default handler to pass it to — an app may not exit itself — so the
call is accepted and the gesture dropped.

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

On the web the browser owns back, and macOS has no equivalent gesture, so both
are inert — callers need no `cfg` of their own.

The Android implementation resolves its Kotlin bridge through the Activity's
application class loader. This matters because Dioxus effects run on a native
thread, where JNI's default `FindClass` otherwise sees only the system loader.

The iOS implementation attaches a `UIScreenEdgePanGestureRecognizer` to the
`WKWebView` and requires the WebView's scroll pan to fail before it, the way
UIKit gives a navigation controller's pop gesture priority over content. It
commits on release, past a short distance or a throw, so a stray touch near the
bezel does not navigate. WebKit's own back/forward swipes are turned off for the
same reason the plugin avoids `history.back()`.

## Media

```rust,ignore
let mut plugins = use_context::<NativePlugins>();
plugins.media.write().prepare()?;
plugins.media.write().set_playback_active(true, "Episode 4")?;
plugins.media.write().enter_picture_in_picture(16, 9)?;
plugins.media.write().set_orientation("landscape")?;
```

Both native implementations keep a WebView `<video>` playing once the app stops
being frontmost and give it lock-screen controls, by the means each platform
requires: a foreground service and picture-in-picture actions on Android, a
`.playback` `AVAudioSession` with `MPNowPlayingInfoCenter` metadata and
`MPRemoteCommandCenter` commands on iOS. Controls on either platform drive the
same player element through the same script.

Two things iOS can only ask for, not enforce, so the host app has to allow them:
picture-in-picture needs the `WKWebView` built with
`allowsPictureInPictureMediaPlayback` (the configuration is immutable once the
WebView exists), and `set_orientation` is honored only within the orientations
the `Info.plist` and root view controller permit. The `enter_picture_in_picture`
dimensions are ignored on iOS, where AVKit takes the aspect ratio from the video
track itself; they are kept in the signature for a call site shared with
Android.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
