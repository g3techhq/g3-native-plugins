# Changelog

All notable changes to `g3-native-plugins` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Breaking.** Native auth now uses one non-blocking start-and-poll shape on
  both platforms. `start_google_auth` and `start_apple_auth` return
  `Result<(), String>`, `poll_auth_result` returns
  `Result<Option<String>, String>`, and `is_auth_awaiting` is available on both.
  Android previously blocked the calling thread on a `CountDownLatch` for up to
  two minutes while Credential Manager's UI was open. `start_google_auth` also
  now takes the caller's Google server client id:
  `start_google_auth(&mut self, server_client_id: &str)`. It was hardcoded in
  the Kotlin plugin, which meant the crate shipped one app's credential and
  could only ever authenticate that app — a client id names a Google Cloud
  project, and Credential Manager checks it against an Android client
  registered there for the calling package and signing certificate. An empty
  id is now rejected up front, because Credential Manager's own failure for a
  bad one is indistinguishable from having no account signed in.

### Fixed

- Android geolocation timed out when the best-looking provider was enabled but
  silent. A provider can report itself enabled and never produce a fix — network
  location with nothing to work from, or an emulator where only GPS is fed — so
  betting a request on one provider turned that into a timeout while another had
  an answer ready. Requests now register on every enabled provider and take the
  first fix; a high-accuracy request still listens to GPS alone, since returning
  a coarse fix because it arrived first is not what was asked for. Found by
  running the test bed against `adb emu geo fix`.
- Android plugins crashed with `ClassNotFoundException` the moment a Dioxus
  effect called them. JNI's `FindClass` resolves against the calling thread's
  class loader, and a thread attached from native code gets the system loader,
  which does not know the app's classes. The `#[manganis::ffi]` macro's
  generated bindings use `FindClass`, so every Android call now goes through a
  new internal `android_bridge` that asks the Activity for its loader — the
  approach `back_button` and `media` already used by hand. Each plugin keeps a
  function-less `extern "Kotlin"` block, which is what makes `dx` build and
  bundle the Kotlin module. Found by running the test bed on a device.
- Renamed the storage plugin's type from `Storage` to `KeyValueStore`.
  `dioxus::prelude` exports a `Storage` trait, so an app glob-importing both
  would have found the name ambiguous and failed to compile. The field on
  `NativePlugins` is still `storage`.

### Added

- A `testbed/` Dioxus app exercising every plugin, built to be driven from adb:
  it self-tests on launch and reports to logcat under a `G3TESTBED` tag. It is
  a standalone package, so the library's own `cargo test` and `cargo package`
  ignore it.

- New `storage` feature: key-value storage encrypted by the platform's own
  secret store — the Keychain on iOS, AES-GCM under a hardware-backed Keystore
  key on Android, `localStorage` (unencrypted) on the web. Calls are
  synchronous because both native stores are. The Android side has no
  dependencies: doing the Keystore work directly avoids Tink and the
  unmaintained `androidx.security:security-crypto`. Key names are stored in the
  clear on both platforms; only values are encrypted. Alone among these
  plugins, the macOS build returns an error from every call instead of
  compiling to an inert no-op, because a store that silently discards is worse
  than one that says it cannot help.
- New `camera-microphone` feature. Deliberately not a capture API: wry already
  bridges the WebView permission callbacks on both platforms, so
  `getUserMedia` works from the page once `[permissions]` in `Dioxus.toml`
  declares camera and microphone, and installing a `WebChromeClient` or
  `WKUIDelegate` here would displace wry's file chooser and JS dialogs. What it
  adds is what the page cannot reach: reading permission state before capture
  is attempted, prompting at a chosen moment, opening the app's Settings page
  after a permanent refusal, and moving the iOS audio session to
  `.playAndRecord` so a microphone still works under the `media` plugin's
  `.playback` session.
- `PermissionState` moved to a new always-compiled `permissions` module and is
  now shared by `geolocation` and `camera-microphone`. Both glob-export at the
  crate root, so a second definition would have collided there.
  `g3_native_plugins::PermissionState` is unchanged.
- New `in-app-purchases` feature covering both stores, including
  subscriptions: products with offers and billing periods, purchases, restore,
  and the transaction listeners that catch purchases completing outside the
  app. StoreKit 2 on iOS, Play Billing 7 on Android. Nothing is verified on
  device — `Entitlement::purchase_token` carries the signed JWS or purchase
  token for server-side checking, and the rest is display state.
  Three platform differences are surfaced rather than hidden: Play does not
  report subscription expiry to the device, does not distinguish consumable
  from non-consumable products, and needs an offer named to buy a subscription
  where StoreKit does not.
- New `geolocation` feature, ported from the Dioxus repo's
  `geolocation-native-plugin` example with two deliberate departures. Requests
  are started and polled instead of blocking the caller, because the call
  arrives on a Dioxus effect thread and because the example's iOS wait spins a
  run loop that `CLLocationManager` never delivers to. And Android fixes come
  from the platform `LocationManager` rather than the fused provider, so the
  crate does not force `play-services-location` on its consumers.
- New `deep-links` feature: receives the URL a universal link, App Link, or
  custom scheme opened the app with, completing the half of deep linking the
  `deep_links` metadata builders and the Dioxus CLI's `[deep_links]` config do
  not cover. Links are queued natively and drained with `take_link()`, because
  one usually arrives before the app has rendered anything able to receive it.
  Android reads the launch Intent and registers a new-intent listener; iOS reads
  the launch options and adds the two delegate callbacks to the host's app
  delegate class at runtime.
- iOS `back-button`: a left-edge swipe recognizer on the `WKWebView` now raises
  the same cancelable `g3nativeback` event as Android, so one web-side listener
  serves both platforms. The plugin is about the back event, not about which
  button produced it.
- iOS `media`: background playback through a `.playback` `AVAudioSession`, lock
  screen and Control Center metadata through `MPNowPlayingInfoCenter`, and
  play/pause and ten-second skip commands through `MPRemoteCommandCenter`,
  wired into the same player element the Android picture-in-picture actions
  drive. Picture-in-picture and orientation requests are also implemented.

### Changed

- `back-button` and `media` are no longer documented as Android-only. iOS
  `fall_through()` is accepted and drops the gesture: iOS has no default back
  handler to pass it to, and an app may not exit itself.

## [0.1.0] - 2026-09-06

Initial release.

- Standardized Android system Back as the cancelable `g3nativeback` DOM event
  so route libraries and higher-priority UI layers can share one event
  contract without app-specific bridge code.
- Made `BackButton::new` public for integration crates that need to own the
  plugin outside `NativePluginsProvider`.
- Added one-press Android fallthrough so a router integration can preserve the
  operating system's normal Back behavior if transient UI and route history
  both decline an intercepted request.
- Feature-gated native plugins: `clipboard` (copy and share sheet), `auth`
  (Sign in with Apple, Google Sign-In), `external-url`, `back-button`, and
  `media` (background playback).
- `deep_links` builders for `apple-app-site-association` and
  `assetlinks.json`, plus route macros that serve them.
- Platform coverage is uneven; see the support matrix in the README. Notably,
  `media` and `back-button` are Android-only.
