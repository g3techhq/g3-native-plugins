# g3-native-plugins

[![CI](https://github.com/g3techhq/g3-native-plugins/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/g3techhq/g3-native-plugins/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/g3-native-plugins.svg)](https://crates.io/crates/g3-native-plugins)
[![docs.rs](https://docs.rs/g3-native-plugins/badge.svg)](https://docs.rs/g3-native-plugins)
[![License](https://img.shields.io/crates/l/g3-native-plugins.svg)](#license)

Dioxus wrappers for native clipboard/share, Apple and Google auth hooks, external URLs, notifications and push, updating the installed app, and deep-link metadata helpers.

The Cargo package is `g3-native-plugins`; the Rust crate name is `g3_native_plugins`.

## Platform Support

Coverage is not uniform. Where a plugin has no implementation for the target,
the calls compile to inert no-ops rather than failing the build — so a missing
platform shows up as nothing happening at runtime, not as a compile error.

| Feature | Android | iOS / macOS | Web | Notes |
| --- | --- | --- | --- | --- |
| `app-info` | yes | iOS only | no | Installed package identifier, version and native build. Returns `None` on macOS/web/server. No provider is needed. |
| `appearance` | yes | not needed | not needed | The system's dark-mode setting, which an Android WebView reports wrongly, and the system-bar icon color for an edge-to-edge page. Elsewhere `None` and inert: the page's own media query is right. |
| `camera-microphone` | yes | iOS only | n/a | Permission state, prompting, and the Settings escape hatch around `getUserMedia`. Not a capture API. |
| `clipboard` | yes | iOS only | yes | Copy plus a native share sheet on mobile. macOS is inert. |
| `auth` | yes | iOS only | no | Google Sign-In on Android (optionally bound to a server nonce), Sign in with Apple on iOS. The Google calls are Android-only: `start_google_auth_with_nonce` is inert on iOS and macOS, which have no native Google flow. macOS is inert. |
| `external-url` | yes | iOS only | yes | Opens the system browser. macOS is inert. |
| `geolocation` | yes | iOS only | n/a | Start-and-poll position requests and the runtime permission prompt. Web is inert on purpose: use `navigator.geolocation`. |
| `back-button` | yes | iOS only | no | The back *event*, whatever raises it: Android's system back, an iOS left-edge swipe. |
| `deep-links` | yes | iOS only | n/a | Receives the URL a universal link, App Link, or custom scheme opened the app with. |
| `in-app-purchases` | yes | iOS only | n/a | Products, purchases, subscriptions, and restore. Play Billing and StoreKit 2. |
| `safe-area` | yes | inert: use `env()` | inert: use `env()` | The status bar, cutout and gesture-bar insets of an edge-to-edge page, as CSS variables g3-ui reads; and hiding the system bars. Started by the provider. The variables are set only on Android; iOS and browsers leave them unset and report `env(safe-area-inset-*)` correctly. |
| `storage` | yes | iOS only | yes\* | Key-value storage. Keystore AES-GCM / Keychain. \*Web is `localStorage` and **not** encrypted. |
| `media` | yes | iOS only | no | Background playback, lock-screen controls, and Now Playing metadata. |
| `notifications` | yes | iOS only | no | Local notifications: now or scheduled, buttons and replies, Android channels. |
| `push-notifications` | yes | iOS only | no | Remote push: FCM on Android, APNs on iOS. Turns on `notifications`. |
| `updater` | yes | iOS only | no | Signed updates of the installed app, from a Tauri-style update server, or the store page for a store build. Android installs the APK with `updater-install`; Windows, macOS and Linux install too (they have no `NativePlugins`: use `Updater::new()`); iOS opens an ad hoc or enterprise install link. See [App updates](#app-updates). |

The `deep_links` module's metadata builders are not feature-gated and build
everywhere, including on the server. Only receiving a link needs the
`deep-links` feature.

On iOS only the enabled plugins' Swift is compiled into the app's
`DioxusSwiftPlugins.framework`, so the binary references only the permission
APIs of the features the app turned on, and App Store Connect asks only for
their usage strings (ITMS-90683): `geolocation` needs a location description
and `camera-microphone` camera and microphone ones (`[permissions]` in
Dioxus.toml). The crate's `build.rs` names the Swift product after the enabled
features and `src/ios/Package.swift` compiles only those sources; nothing is
needed in the app. `scripts/check-swift-subset.sh` builds a feature set the
way `dx` does and checks the framework.

## Install

### Installed app version

Enable `app-info` and call `g3_native_plugins::installed_app_info()` from a mobile
effect or handler. It returns `Result<Option<AppInfo>, String>` with `identifier`,
`version`, and `build` from the installed package. Keep the build as text: Android
uses its full `longVersionCode`, while iOS build numbers may have numeric parts.
Other targets return `Ok(None)`. Use this metadata for store-update checks.

Enable only the plugins your app uses:

```toml
[dependencies]
g3-native-plugins = { version = "0.4", features = ["clipboard", "auth", "external-url"] }
```

`main` builds against Dioxus `=0.8.0-alpha.3` from the
[Gateway3B fork](https://github.com/Gateway3B/dioxus) (upstream `main` plus the
g3 commits), and nothing from it is published until Dioxus 0.8.0 is: a
crates.io release that only builds with a `[patch]` would be broken for
everyone else. The 0.4 releases on crates.io stay on Dioxus 0.7.9. To use
`main`, pin Dioxus the same way and patch both into your app's workspace root:

```toml
[dependencies]
dioxus = "=0.8.0-alpha.3"
g3-native-plugins = { version = "0.4", features = ["clipboard", "auth", "external-url"] }

[patch.crates-io]
dioxus = { git = "https://github.com/Gateway3B/dioxus", branch = "main" }
dioxus-signals = { git = "https://github.com/Gateway3B/dioxus", branch = "main" }
manganis = { git = "https://github.com/Gateway3B/dioxus", branch = "main" }
g3-native-plugins = { git = "https://github.com/g3techhq/g3-native-plugins", branch = "main" }
```

Build it with the fork's CLI, since `dx` and the Dioxus crates must match:
`cargo install --git https://github.com/Gateway3B/dioxus --branch main dioxus-cli --locked`.

### iOS privacy manifest

The Swift package ships no `PrivacyInfo.xcprivacy`, because its sources call no
required-reason API: no `UserDefaults`, file-timestamp, boot-time or disk-space
API. The app's own manifest covers what it links. Two things to put there:

- **File timestamps.** Rust's `std::fs` calls `stat`, one of Apple's
  file-timestamp APIs, whenever a file's metadata is read; `updater` does that
  for what it downloads. Declare `NSPrivacyAccessedAPICategoryFileTimestamp`
  with reason `C617.1` (files inside the app's own container).
- **Collected data.** What an enabled plugin hands the app, for the manifest's
  `NSPrivacyCollectedDataTypes` and the App Store privacy label, if the app
  sends it to its server: precise location (`geolocation`), purchase history
  (`in-app-purchases`), user ID and email (`auth`, from Apple's identity
  token), and a device ID (`push-notifications`, the APNs token).

The usage-description strings come from `Dioxus.toml`'s `[permissions]`; see
the camera, microphone and geolocation sections.

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

#[cfg(target_os = "ios")]
plugins.auth.write().start_apple_auth(server_state, server_nonce)?;

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
Android yields the Google ID token directly. Apple requires a fresh state and
nonce issued and stored by your server before every request. Its result JSON
contains `identity_token`, `authorization_code`, returned `state`, and, on the
first authorization, any email and display name Apple supplied. Consume the
stored challenge once, compare state, validate the token nonce, and exchange
the authorization code with Apple before creating a local session.

`start_google_auth` takes the **web** client id from your own Google Cloud
project — the one ending `.apps.googleusercontent.com`, not the Android client
id. It is a parameter rather than a crate constant because it names one
project: Credential Manager checks it against an Android client registered in
that same project for your package name and signing certificate, so an id
belonging to another app cannot work however well formed it is. A wrong or
unregistered id fails exactly like a missing account — no credential, no
explanation — so check the registration first when sign-in returns `None`
unexpectedly.

To close the replay window on a Google ID token, issue a fresh nonce on the
server, store it with the sign-in attempt, and start with
`start_google_auth_with_nonce(GOOGLE_SERVER_CLIENT_ID, &nonce)`. Credential
Manager writes the nonce into the token's `nonce` claim; the server checks it
and consumes it once, as for Apple. `start_google_auth` is unchanged, for a
server that does not issue nonces. The nonce variant is Android-only and does
nothing on iOS and macOS.

## Appearance

An Android WebView tells the page `prefers-color-scheme: light` whenever the
app's theme is a light one, which it is in a `dx` build, whatever the phone
is set to. `dark()` asks the system instead, and answers `None` wherever the
page already knows. Make the page follow it with `color-scheme`, which is
what CSS `light-dark()` resolves against:

```rust,ignore
let mut plugins = use_context::<NativePlugins>();
if let Some(dark) = plugins.appearance.write().dark() {
    // e.g. set `color-scheme: dark` or `light` on the root element
}
```

On Android 15 the app is drawn edge to edge, so the page sits under the
status and navigation bars while their icons still follow the Activity's
theme. Tell the system which icons the page needs, now and whenever the
scheme the page draws changes:

```rust,ignore
// Dark icons on a light page, light icons on a dark one.
let _ = plugins.appearance.write().set_dark_bar_icons(!page_is_dark);
```

## Safe Area

Android 15 draws every app edge to edge, so the status bar, a camera cutout
and the gesture or button bar sit over the page. `env(safe-area-inset-*)` is
how a page avoids them, but Android WebViews before Chrome's edge-to-edge
support report all four as 0. With `safe-area` enabled, the provider reads the
insets from the window and keeps `--g3-safe-area-top`, `-right`, `-bottom`
and `-left` up to date on the root element, in CSS pixels. g3-ui uses those
ahead of `env()`, so its header, tab bar, sheets and toasts clear the bars
with no code in the app. iOS and browsers report `env()` correctly already,
so the feature is inert there: **the `--g3-safe-area-*` variables are never
set on iOS or the web**, and `set_bars_hidden` does nothing. Read them with
`env()` as the fallback, as g3-ui does, so one rule works everywhere:

```css
.my-footer {
  padding-bottom: var(--g3-safe-area-bottom, env(safe-area-inset-bottom, 0px));
}
```

Hidden bars report no inset, so hiding them gives the page the whole screen:

```rust,ignore
let mut plugins = use_context::<NativePlugins>();
// A swipe from the edge shows the bars for a moment.
plugins.safe_area.write().set_bars_hidden(true)?;
```

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

### Clearing it when an account is deleted

Call `g3_native_plugins::clear_storage()` once the server has deleted the
account (and wherever else the app forgets a user). It needs no provider
context, so it works straight from the handler that confirms the deletion:

```rust,ignore
// After the delete-account server function succeeds:
#[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
if let Err(error) = g3_native_plugins::clear_storage() {
    tracing::warn!("could not clear local storage after account deletion: {error}");
}
```

On iOS this matters more than it looks: **Keychain items outlive the app**, so
values left behind come back after a reinstall. On the web it clears
`localStorage` for the whole origin. With a context at hand,
`plugins.storage.write().clear()` does the same.

The type is `KeyValueStore`, not `Storage`, because `dioxus::prelude` already
exports a `Storage` trait — an app glob-importing both would otherwise find the
name ambiguous and fail to compile.

Synchronous, unlike most plugins here, because both stores are: a Keychain
lookup and a `SharedPreferences` read are ordinary in-process work and there is
nothing to poll for.

For a small retry checkpoint that must be written before acknowledging external
work, use `set_durable(key, value)`. Android waits for the encrypted preferences
commit and reports disk-write failure; iOS uses the Keychain write result. This
can block the caller. The method returns an error on web/macOS, so it cannot
silently persist a private checkpoint in browser storage. Ordinary `set` keeps
Android's asynchronous disk write. Use `remove_durable(key)` for the matching
encrypted deletion checkpoint; Android waits for its commit and iOS uses the
Keychain deletion result. It also refuses web/macOS calls. Ordinary `remove`
keeps its asynchronous Android behavior. No storage API protects against app removal,
device loss or unavailable device keys.

Values are held by the platform's own secret store — the Keychain on iOS, and
on Android AES-GCM under a key generated in the hardware-backed Keystore, which
will perform operations with it but never export it. That is what
`EncryptedSharedPreferences` does underneath; doing it directly avoids pulling
Tink in behind a library Google no longer maintains, and this module has no
dependencies at all.

An iOS Keychain app must be signed with an application identifier supplied by
its provisioning profile. A Dioxus 0.7.9 `dx serve --ios` simulator bundle is
ad-hoc signed without that identifier, so Keychain calls fail with
`errSecMissingEntitlement` (`-34018`) even though the same API is available in
the simulator. Validate storage in a provisioned device build. Adding a
Keychain access group to an ad-hoc simulator bundle after it is built is not a
substitute for provisioning and can make iOS reject the app at launch.

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

### Leaving it out of Android backups

Android's Auto Backup copies an app's preferences to the user's Google account
and restores them on a new install, but never the Keystore key the values are
encrypted with. A restored store therefore cannot be decrypted, and every
`get()` fails. Exclude it: the plugin ships the rules as resources, and the app
points its `<application>` at them, for example in the manifest that Dioxus.toml's
`[android] manifest` merges:

```xml
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <application
        android:fullBackupContent="@xml/g3_native_plugins_backup_rules"
        android:dataExtractionRules="@xml/g3_native_plugins_data_extraction_rules" />
</manifest>
```

An app that has its own rules adds the same line to each of them instead:
`<exclude domain="sharedpref" path="g3_native_plugins.storage.xml" />`
(inside both `<cloud-backup>` and `<device-transfer>` in data-extraction
rules). The library does not set the attributes itself, because only one
set of rules can apply to an app and it would collide with the app's own.

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

Apps with accounts should use `start_purchase_for_account(product, offer, account_token)`
and `poll_attributed_entitlements()`. Supply a stable opaque UUID issued by your
backend for the reviewed app account; use neither an email nor a raw database ID.
iOS sends it as StoreKit's `appAccountToken`, Android as Play's obfuscated account
ID. Both attributed and ordinary polling consume the same slot; choose one.
Older purchases have `app_account_token: None`; never assign them automatically
to whoever is currently signed in. Confirm attribution against Apple's/Google's
server response and your original ownership records before granting or finishing.
Transfers and legacy assistance are host policy. `Debug` output redacts purchase
proofs, transaction IDs and account tokens; serialization preserves backend proofs.
The testbed buy control uses a fixed synthetic UUID, not a production account.

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
8.2.1 (Google requires 8 or later for apps updated after 2026-08-31); check the
deadline before shipping, because they move it.

On iOS a consumable is never among StoreKit's current entitlements, so an
unfinished one is added to the batch with `needs_finishing` set, and leaves it
once finished — the same as an unconsumed purchase on Play.

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
as designed rather than the `onNewIntent` path. The Dioxus fork's generated
manifest already declares `singleTask`; CLI 0.7.9 had no `launch_mode` key, so
an app still on it needs `[android.raw]` manifest injection.

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

Android holds CPU and Wi-Fi wake locks while playback is active, including
buffering. Pause releases the locks immediately, even while the foreground
service keeps the media controls available during its grace period. Clearing
playback or destroying the service also releases them.

Two things iOS can only ask for, not enforce, so the host app has to allow them:
picture-in-picture needs the `WKWebView` built with
`allowsPictureInPictureMediaPlayback` (the configuration is immutable once the
WebView exists), and `set_orientation` is honored only within the orientations
the `Info.plist` and root view controller permit. The `enter_picture_in_picture`
dimensions are ignored on iOS, where AVKit takes the aspect ratio from the video
track itself; they are kept in the signature for a call site shared with
Android.

## Notifications

```rust,ignore
use g3_native_plugins::{Notification, NotificationEvent, PermissionState, Schedule};

fn main() {
    // Early: a tap that launched the app is delivered during launch, and only
    // to a listener already in place.
    let _ = g3_native_plugins::Notifications::new().prepare();
    dioxus::launch(App);
}

let mut plugins = use_context::<NativePlugins>();

if plugins.notifications.write().check_permissions()? != PermissionState::Granted {
    plugins.notifications.write().request_permissions()?;
}

plugins.notifications.write().show(&Notification::new(1, "Round saved").body("18 holes, 72"))?;
plugins.notifications.write().show(
    &Notification::new(2, "Tee time in 30 minutes")
        .extra("route", "/games/7")
        .schedule(Schedule::At { at_ms: tee_time_ms - 1_800_000, allow_while_idle: true }),
)?;

// Wherever the app polls:
while let Ok(Some(event)) = plugins.notifications.write().take_event() {
    if let NotificationEvent::Action { action_id, notification, .. } = event {
        // action_id is "tap" for the notification itself, else a button id.
    }
}
```

The counterpart of Tauri's notification plugin: permission, immediate and
scheduled notifications (once, or every N minutes/hours/days/weeks), pending
and delivered lists, buttons with optional text replies (`ActionType`), and
Android channels. Taps and foreground deliveries are queued natively and
polled, the same contract as deep links, so nothing is lost for want of a
listener.

Things each platform imposes:

- **Android 13+** asks for `POST_NOTIFICATIONS` at run time, which is what
  `request_permissions()` raises. The plugin's manifest declares it.
- **Android 8+** takes sound and importance from the *channel*, not the
  notification. Create channels with `create_channel` and post to them with
  `Notification::channel`. A `default` channel is created on first use.
- **Android scheduling** uses `AlarmManager`, re-armed after reboot and app
  update. Alarms are exact only if the app holds `SCHEDULE_EXACT_ALARM`, which
  this crate deliberately does not declare — Play restricts it to alarm and
  calendar apps. Without it Android may deliver a few minutes late.
- **Android's status-bar icon** must be a white silhouette. Name a drawable
  with `Notification::icon`; the app icon fallback renders as a white square.
- **iOS** refuses repeats more often than once a minute, and has no channels —
  the channel calls do nothing there.
- The plugin sets the host Activity's launch mode to `singleTask`, as the
  `media` feature does, because a notification tap otherwise starts a second
  Activity, which wry cannot host.

## Push Notifications

```rust,ignore
use g3_native_plugins::{FirebaseOptions, PushEvent, PushNotifications};

fn main() {
    let _ = g3_native_plugins::Notifications::new().prepare();
    let _ = PushNotifications::new().prepare();
    dioxus::launch(App);
}

// From the Firebase console, or the app's google-services.json.
let firebase = FirebaseOptions {
    application_id: "1:1234567890:android:abc123".into(),
    api_key: "AIza...".into(),
    project_id: "my-project".into(),
    sender_id: "1234567890".into(),
};

plugins.notifications.write().request_permissions()?;
plugins.push_notifications.write().register(Some(&firebase))?;

while let Ok(Some(event)) = plugins.push_notifications.write().take_event() {
    match event {
        PushEvent::Token(token) => { /* send token.service + token.token to your server */ }
        PushEvent::Message { data, title, body } => { /* arrived in the foreground, or data-only */ }
        PushEvent::Opened { data, .. } => { /* the user tapped a push */ }
        PushEvent::RegistrationFailed(error) => { /* usually a missing entitlement */ }
    }
}
```

Remote push through Firebase Cloud Messaging on Android and APNs on iOS. The
token is tagged with the service it belongs to, so the server knows which API
to send through.

- **Android** needs a Firebase project with the app registered. Firebase is
  normally configured by the `google-services` Gradle plugin, which a
  generated Dioxus Gradle project cannot run, so the four values are passed to
  `register()` and Firebase is initialized from them at run time. The
  `firebase-messaging` dependency lives in its own Gradle module, which is why
  push is a separate feature: an app with only local notifications does not
  carry Firebase.
- **iOS** needs the Push Notifications capability: an `aps-environment`
  entitlement from a provisioning profile that has push enabled, plus the
  `remote-notification` background mode for data-only pushes. Without the
  entitlement, APNs refuses and the refusal arrives as `RegistrationFailed`.
  The token callbacks are added to the app delegate at run time, the same
  technique as deep links, and only where the delegate has none of its own.
- **Neither platform displays a push that arrives while the app is in the
  foreground** — Android never does, and iOS is told not to so the two match.
  It arrives as `PushEvent::Message`; show it with `Notifications::show` if it
  should be seen.
- Seeing a push needs the notification permission; receiving a data-only push
  does not.

## App updates

The `updater` feature updates the installed app the way Tauri's updater does:
it asks an update server for the latest release, downloads the build for this
platform, checks it against a minisign public key compiled into the app, and
installs it: a sideloaded APK, a desktop installer, an ad hoc or enterprise
iOS build. A build from a store, which may not install itself, still learns
that an update is out: its release entry names the store page, and
`install()` opens it. An app that uses g3-brand already gets store-update prompts
from its catalog, with snooze and required updates; use this store mode only
for an app without it.

| Platform | Release file | Installed by | What the user sees |
| --- | --- | --- | --- |
| Android | `.apk` (with `updater-install`) | `PackageInstaller` | Nothing on Android 12+ once the app may install apps; otherwise the system's install dialog |
| Any, from a store | The store page (Google Play, the App Store, TestFlight, the Microsoft Store, Flathub) | The store | The store opens on the app |
| Windows | NSIS `-setup.exe` or `.msi`, bare or in dx's `.zip` | The installer, silently, after the app exits | The app closes and reopens; UAC for a per-machine install |
| macOS | `.app.tar.gz` | The `.app` is replaced in place | The app closes and reopens |
| Linux | `.AppImage` (bare or `.tar.gz`), or `.deb` | Renaming over `$APPIMAGE`; `pkexec dpkg -i` | The app closes and reopens; a password prompt for a `.deb` |
| iOS | An HTTPS `manifest.plist`, or an App Store / TestFlight link | iOS, through `itms-services://` | iOS's install prompt, or the store |
| Web | — | — | Inert: a deploy already is the update |

### Setup

```toml
# A sideloaded Android build: add `updater-install`, which lets it install
# the APK. A Play build leaves it off and never declares that permission.
g3-native-plugins = { version = "0.4", features = ["updater", "updater-install"] }
```

```rust,ignore
use g3_native_plugins::{Updater, UpdaterConfig, UpdaterState};

// The public half of the release key. Not a secret.
const UPDATER_PUBLIC_KEY: &str = "RWQ...";

fn main() {
    let _ = Updater::new().configure(
        UpdaterConfig::new(UPDATER_PUBLIC_KEY, env!("CARGO_PKG_VERSION"))
            .endpoint("https://updates.example.com/latest.json"),
    );
    dioxus::launch(App);
}

// From a handler or effect, then poll `state()` (a timer, or each render):
let mut updater = Updater::new();
updater.start_check()?;
match updater.state() {
    UpdaterState::Available(update) => { /* offer it, then */ updater.start_download()? }
    UpdaterState::Downloading { progress, .. } => { /* progress.bytes_done / bytes_total */ }
    UpdaterState::Ready(update) => { /* offer to install, then */ updater.install()? }
    UpdaterState::NeedsPermission(_) => { /* Android settings are open; install() again on return */ }
    UpdaterState::Installing(_) => { /* the system has it */ }
    UpdaterState::Failed(error) => { /* nothing was installed */ }
    _ => {}
}
```

Every `Updater` handle shares one process-wide state, so `Updater::new()` and
`NativePlugins::updater` are the same updater. Windows and Linux have no
`NativePlugins`; use `Updater::new()` there.

`install()` on Windows, macOS and Linux **ends the process** once the new
version is on its way: save what needs saving before calling it.

`current_version` is semver, and only a newer release is offered. `dx` derives
every platform's version from the crate version (Android's `versionCode` is
`major * 1000000 + minor * 1000 + patch`), so `env!("CARGO_PKG_VERSION")`
matches what is installed.

### Publishing a release

1. **A key pair, once.** `minisign -G -p updater.pub -s updater.key`, or
   `tauri signer generate`. Compile the public key into the app; keep the
   secret key out of the repository (a CI secret).
2. **The builds.** `dx bundle --package-types nsis --package-types updater`
   (Windows; likewise `macos`, `appimage` or `deb`) writes the update archives
   to `bundle/updater/`. For Android, the release `.apk`; for iOS, an ad hoc
   or enterprise `.ipa` with its `manifest.plist`.
3. **Sign each file** the app downloads: `minisign -S -s updater.key -m
   Tawny_1.1.0_x64-setup.zip` writes a `.minisig` beside it. On iOS, sign the
   `manifest.plist`, not the IPA: the app checks the manifest and iOS fetches
   the IPA it names.
4. **Publish the release JSON** at the endpoint, with each `.minisig`'s
   contents as its `signature`:

```json
{
  "version": "1.1.0",
  "notes": "Faster startup.",
  "pub_date": "2026-11-10T12:00:00Z",
  "platforms": {
    "android": { "url": "https://updates.example.com/1.1.0/Tawny.apk", "signature": "untrusted comment: ..." },
    "windows-x86_64-nsis": { "url": "https://updates.example.com/1.1.0/Tawny_1.1.0_x64-setup.zip", "signature": "..." },
    "darwin-aarch64": { "url": "https://updates.example.com/1.1.0/Tawny_1.1.0.app.tar.gz", "signature": "..." },
    "linux-x86_64-appimage": { "url": "https://updates.example.com/1.1.0/Tawny_1.1.0_amd64.AppImage.tar.gz", "signature": "..." },
    "ios": { "url": "https://updates.example.com/1.1.0/manifest.plist", "signature": "..." }
  }
}
```

A build looks itself up as `{target}-{arch}-{installer}`, then
`{target}-{installer}`, then `{target}-{arch}`, then `{target}`, and a store
entry (`{target}-store`) last. Targets are Tauri's (`android`, `ios`,
`windows`, `darwin`, `linux`), architectures `aarch64`, `x86_64`, `armv7`,
`i686`, and installers `apk`, `ipa`, `nsis`, `msi`, `app`, `appimage`, `deb`
and `store`. A store entry needs only a `url`: nothing is downloaded, so
nothing is signed. An Android build without `updater-install` looks only for
`android-store`, never at an APK; publish one entry of each and a sideloaded
build and a Play build of the same app each find theirs.
A server that picks the build itself can answer with a top-level `url` and
`signature` instead of `platforms`, or `204 No Content` for "nothing new".
Endpoints may use `{{target}}`, `{{arch}}` and `{{current_version}}`.
Signatures from `minisign` and from `tauri signer sign` (base64-wrapped) both
work.

### Per platform

- **Android.** `updater-install` adds a Kotlin module declaring
  `REQUEST_INSTALL_PACKAGES`, which Google Play restricts: **do not enable
  `updater-install` in a build that goes to Play.** `updater` alone declares
  nothing beyond `INTERNET`, and opens the Play page. The first install asks the user to let the app install apps
  (`NeedsPermission` opens that screen). From Android 12, an app updating
  itself skips the confirmation dialog when it targets a recent SDK (34 or
  higher on Android 16; the floor rises each release) and declares `UPDATE_PACKAGES_WITHOUT_USER_ACTION`, which the
  plugin does; the system may still ask, and then shows its dialog. Android
  refuses an APK signed with a different key or with a lower `versionCode`,
  so every release must be signed with the same key. A cancelled dialog goes
  back to `Ready`.
- **Windows.** A hidden PowerShell waits for the app to exit, runs the
  installer (`/S /D=<this app's folder>` for NSIS, since dx's installer does
  not remember its folder; `msiexec /i … /passive` for MSI), then starts the
  app again. A per-machine install shows UAC. Set
  `webview_install_mode = { EmbedBootstrapper = { silent = true } }` under
  `[bundle.windows]` in Dioxus.toml: dx's default embeds Microsoft's 183 MB
  offline WebView2 installer and runs it with a window on every install, so
  each update would download it and interrupt the silent install.
- **macOS.** The running `.app` is moved aside, the new one renamed into its
  place, and the old one deleted; if the swap fails the old one goes back.
  The app needs write access to its own folder, which an administrator has
  in `/Applications`; there is no elevation prompt.
- **Linux.** An AppImage replaces the file `$APPIMAGE` names. A `.deb` goes
  through `pkexec dpkg -i`, which asks for an administrator's password.
- **iOS.** No app can install an IPA, but iOS installs an ad hoc or
  enterprise build from an `itms-services://` link to its `manifest.plist`,
  which `install()` opens after checking the manifest's signature; the
  device must be in the build's provisioning profile. A release may instead
  name an App Store or TestFlight link, which is opened as is: there is
  nothing to download, so nothing is signed. iOS reports nothing back, so the
  state returns to `Ready` in case the user comes back without installing.
  EU Web Distribution is not supported.

### What is checked

- The release JSON is not signed and is not trusted: it only says where to
  look. Its version must be newer than `current_version`.
- Every downloaded file must match its minisign signature from the
  configured key before anything opens it; a file that does not is deleted.
  An entry without a signature is refused before it is downloaded (iOS store
  links excepted, since nothing is downloaded).
- Each platform then applies its own checks on top: Android the APK's
  signing key and `versionCode`, macOS Gatekeeper, iOS the provisioning
  profile.
- Downloads go to the user's cache folder (`%LOCALAPPDATA%`,
  `~/Library/Caches`, `$XDG_CACHE_HOME`; the app's cache on mobile), never a
  shared temporary folder, and are removed at the next `configure`.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
