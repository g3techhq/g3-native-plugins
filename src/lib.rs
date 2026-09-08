//! Dioxus wrappers around native platform capabilities: clipboard and share
//! sheets, Apple and Google sign-in, opening external URLs, system back
//! handling, background media playback, and receiving the URL a deep link
//! opened the app with - plus helpers for the deep-link metadata files iOS
//! and Android expect a server to host.
//!
//! Every plugin sits behind a feature flag, so an app compiles in only what
//! it uses. Platform coverage is not uniform; see the support matrix in the
//! README. Where a plugin has no implementation for the current target it
//! compiles to an inert no-op rather than failing to build, which keeps the
//! same call sites working on web, desktop, and mobile.
#![allow(non_snake_case)]
#![warn(missing_docs)]
#[cfg(all(
    target_os = "android",
    any(
        feature = "auth",
        feature = "camera-microphone",
        feature = "clipboard",
        feature = "deep-links",
        feature = "external-url",
        feature = "geolocation",
        feature = "in-app-purchases",
        feature = "storage"
    )
))]
mod android_bridge;
/// Both halves of deep linking: builders for the `apple-app-site-association`
/// and `assetlinks.json` files that make universal links and App Links resolve
/// to the app, and — behind the `deep-links` feature — the plugin that receives
/// the URL once one does.
pub mod deep_links;
/// The permission states plugins here report, in one place so an app can treat
/// a refusal the same way whatever was refused.
pub mod permissions;
#[cfg(all(
    feature = "deep-links",
    any(
        target_arch = "wasm32",
        target_os = "android",
        target_os = "ios",
        target_os = "macos"
    )
))]
#[allow(unused_imports)]
pub use deep_links::DeepLinks;
#[allow(unused_imports)]
pub use permissions::PermissionState;
cfg_if::cfg_if! {
    if #[cfg(feature = "clipboard")] { mod clipboard; #[allow(unused_imports)] pub use
    clipboard::*; }
}
cfg_if::cfg_if! {
    if #[cfg(feature = "auth")] { mod auth; #[allow(unused_imports)] pub use auth::*; }
}
cfg_if::cfg_if! {
    if #[cfg(feature = "back-button")] { mod back_button; #[allow(unused_imports)] pub
    use back_button::*; }
}
cfg_if::cfg_if! {
    if #[cfg(feature = "camera-microphone")] { mod camera_microphone;
    #[allow(unused_imports)] pub use camera_microphone::*; }
}
cfg_if::cfg_if! {
    if #[cfg(feature = "external-url")] { mod external_url; #[allow(unused_imports)] pub
    use external_url::*; }
}
cfg_if::cfg_if! {
    if #[cfg(feature = "geolocation")] { mod geolocation; #[allow(unused_imports)] pub
    use geolocation::*; }
}
cfg_if::cfg_if! {
    if #[cfg(feature = "in-app-purchases")] { mod in_app_purchases;
    #[allow(unused_imports)] pub use in_app_purchases::*; }
}
cfg_if::cfg_if! {
    if #[cfg(feature = "media")] { mod media; #[allow(unused_imports)] pub use media::*;
    }
}
cfg_if::cfg_if! {
    if #[cfg(feature = "storage")] { mod storage; #[allow(unused_imports)] pub use
    storage::*; }
}
use dioxus::prelude::*;
#[cfg(all(
    any(
        feature = "auth",
        feature = "back-button",
        feature = "camera-microphone",
        feature = "clipboard",
        feature = "deep-links",
        feature = "external-url",
        feature = "geolocation",
        feature = "in-app-purchases",
        feature = "media",
        feature = "storage"
    ),
    any(
        target_arch = "wasm32",
        target_os = "android",
        target_os = "ios",
        target_os = "macos"
    )
))]
use dioxus_signals::Signal;
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
))]
#[derive(Clone, Copy)]
pub struct NativePlugins {
    #[cfg(all(
        feature = "auth",
        any(target_os = "android", target_os = "ios", target_os = "macos")
    ))]
    pub auth: Signal<Auth>,
    /// Available on every target: the non-Android builds are inert, so callers
    /// need no cfg of their own.
    #[cfg(feature = "back-button")]
    pub back_button: Signal<BackButton>,
    /// Available on every target: the web and macOS builds are inert, so
    /// callers need no cfg of their own.
    #[cfg(feature = "camera-microphone")]
    pub camera_microphone: Signal<CameraMicrophone>,
    #[cfg(feature = "clipboard")]
    pub clipboard: Signal<Clipboard>,
    /// Available on every target: the web and macOS builds are inert, so
    /// callers need no cfg of their own.
    #[cfg(feature = "deep-links")]
    pub deep_links: Signal<DeepLinks>,
    #[cfg(all(
        feature = "external-url",
        any(target_arch = "wasm32", target_os = "android", target_os = "ios")
    ))]
    pub external_url: Signal<ExternalUrl>,
    /// Available on every target: the web and macOS builds are inert, so
    /// callers need no cfg of their own.
    #[cfg(feature = "geolocation")]
    pub geolocation: Signal<Geolocation>,
    /// Available on every target: the web and macOS builds are inert, so
    /// callers need no cfg of their own.
    #[cfg(feature = "in-app-purchases")]
    pub in_app_purchases: Signal<InAppPurchases>,
    #[cfg(feature = "media")]
    pub media: Signal<Media>,
    /// Available on every target, but unlike the others the macOS build reports
    /// an error rather than doing nothing: a store that silently discards is
    /// worse than one that says it cannot help.
    #[cfg(feature = "storage")]
    pub storage: Signal<KeyValueStore>,
}
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
))]
impl NativePlugins {
    pub fn new() -> Self {
        Self {
            #[cfg(all(
                feature = "auth",
                any(target_os = "android", target_os = "ios", target_os = "macos")
            ))]
            auth: Signal::new(Auth::new()),
            #[cfg(feature = "back-button")]
            back_button: Signal::new(BackButton::new()),
            #[cfg(feature = "camera-microphone")]
            camera_microphone: Signal::new(CameraMicrophone::new()),
            #[cfg(feature = "clipboard")]
            clipboard: Signal::new(Clipboard::new()),
            #[cfg(feature = "deep-links")]
            deep_links: Signal::new(DeepLinks::new()),
            #[cfg(all(
                feature = "external-url",
                any(target_arch = "wasm32", target_os = "android", target_os = "ios")
            ))]
            external_url: Signal::new(ExternalUrl::new()),
            #[cfg(feature = "geolocation")]
            geolocation: Signal::new(Geolocation::new()),
            #[cfg(feature = "in-app-purchases")]
            in_app_purchases: Signal::new(InAppPurchases::new()),
            #[cfg(feature = "media")]
            media: Signal::new(Media::new()),
            #[cfg(feature = "storage")]
            storage: Signal::new(KeyValueStore::new()),
        }
    }
}
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
))]
impl Default for NativePlugins {
    fn default() -> Self {
        Self::new()
    }
}
#[component]
pub fn NativePluginsProvider(children: Element) -> Element {
    #[cfg(any(
        target_arch = "wasm32",
        target_os = "android",
        target_os = "ios",
        target_os = "macos"
    ))]
    use_context_provider(NativePlugins::default);
    rsx! {
        {children}
    }
}
#[cfg(test)]
mod tests {
    fn production_source(source: &str) -> &str {
        source
            .split("#[cfg(test)]")
            .next()
            .expect("source should split before tests")
    }
    #[test]
    fn back_button_interception_is_opt_in() {
        let source = production_source(include_str!("back_button.rs"));
        assert!(source.contains("intercepting: false"));
        assert!(source.contains("pub fn set_intercepting"));
        let kotlin = include_str!(
            "android/back_button/src/main/kotlin/dev/dioxus/g3_native_plugins/back_button/BackButtonPlugin.kt",
        );
        assert!(kotlin.contains("g3nativeback"));
        assert!(kotlin.contains("cancelable: true"));
        assert!(kotlin.contains("evaluateJavascript(BACK_EVENT_SCRIPT"));
        assert!(kotlin.contains("OnBackPressedCallback(false)"));
        assert!(kotlin.contains("fun fallThroughFromRust()"));
        assert!(kotlin.contains("current?.isEnabled = false"));
        assert!(source.contains("pub fn fall_through"));
        assert!(source.contains("pub const NATIVE_BACK_EVENT"));
    }
    #[test]
    fn ios_back_button_raises_the_same_event_from_an_edge_swipe() {
        let source = production_source(include_str!("back_button.rs"));
        let swift = include_str!("ios/Sources/BackButtonPlugin.swift");
        assert!(source.contains("#[manganis::ffi(\"src/ios\")]"));
        assert!(source.contains(
            "pub fn setInterceptingFromRust(this: &BackButtonPlugin, intercepting: bool);",
        ),);
        assert!(source.contains("pub fn fallThroughFromRust(this: &BackButtonPlugin);"));
        assert!(swift.contains("UIScreenEdgePanGestureRecognizer"));
        assert!(swift.contains("created.edges = .left"));
        // The same event contract as Android, so one web listener serves both.
        assert!(swift.contains("g3nativeback"));
        assert!(swift.contains("cancelable: true"));
        assert!(swift.contains("webView.allowsBackForwardNavigationGestures = false"));
        assert!(swift.contains("panGestureRecognizer.require(toFail: created)"));
        assert!(swift.contains("created.isEnabled = false"));
    }
    #[test]
    fn deep_links_are_collected_from_both_launch_paths() {
        let source = production_source(include_str!("deep_links.rs"));
        let kotlin = include_str!(
            "android/deep_links/src/main/kotlin/dev/dioxus/g3_native_plugins/deep_links/DeepLinksPlugin.kt",
        );
        let swift = include_str!("ios/Sources/DeepLinksPlugin.swift");
        assert!(
            source.contains("pub fn takeLinkFromRust(this: &DeepLinksPlugin) -> Option<String>;"),
        );
        assert!(source.contains("pub fn take_link(&mut self) -> Result<Option<String>, String>"),);
        // Public so an app can prepare ahead of the provider, which is what
        // makes an iOS cold-start link catchable at all.
        assert!(source.contains("pub fn new() -> Self"));
        // Android: cold start from the launch Intent, read once; warm links
        // through the listener, since onNewIntent cannot be overridden here.
        assert!(kotlin.contains("Intent.ACTION_VIEW"));
        assert!(kotlin.contains("enqueue(activity.intent)"));
        assert!(kotlin.contains("launchIntentConsumed = true"));
        assert!(kotlin.contains("addOnNewIntentListener"));
        // Static, so a link is not stranded on a discarded plugin instance.
        assert!(kotlin.contains("companion object"));
        // iOS: cold start from the launch options, warm links from delegate
        // methods added to the host's delegate class at runtime.
        assert!(swift.contains("didFinishLaunchingNotification"));
        assert!(swift.contains("LaunchOptionsKey.userActivityDictionary"));
        assert!(swift.contains("class_addMethod"));
        assert!(swift.contains("application(_:continue:restorationHandler:)"));
        assert!(swift.contains("application(_:open:options:)"));
        // A cold-start link is reported through both paths; only one survives.
        assert!(swift.contains("duplicateWindow"));
    }
    #[test]
    fn android_plugins_resolve_classes_through_the_app_loader() {
        // Learned the hard way on a device: JNI's FindClass resolves against
        // the calling thread's loader, and a thread attached from native code
        // gets the system loader, which has never heard of the app's classes.
        // The macro's generated bindings use FindClass, so every Android call
        // here goes through the bridge instead, which asks the Activity.
        let bridge = include_str!("android_bridge.rs");
        assert!(bridge.contains("getClassLoader"));
        assert!(bridge.contains("loadClass"));
        for source in [
            include_str!("auth.rs"),
            include_str!("camera_microphone.rs"),
            include_str!("clipboard.rs"),
            include_str!("deep_links.rs"),
            include_str!("external_url.rs"),
            include_str!("geolocation.rs"),
            include_str!("in_app_purchases.rs"),
            include_str!("storage.rs"),
        ] {
            let production = production_source(source);
            let android = production
                .split("unsafe extern \"Kotlin\" {")
                .nth(1)
                .expect("each plugin declares a Kotlin block")
                .split('}')
                .next()
                .expect("the Kotlin block should be closed");
            // Only the type, so `dx` still builds the module. A function
            // declared here would generate a FindClass binding and crash the
            // moment a Dioxus effect called it.
            assert!(
                !android.contains("pub fn "),
                "a Kotlin block still declares functions: {android}",
            );
            assert!(production.contains("crate::android_bridge::AndroidPlugin"));
        }
    }
    #[test]
    fn storage_encrypts_values_and_refuses_to_pretend_on_macos() {
        let source = production_source(include_str!("storage.rs"));
        let kotlin = include_str!(
            "android/storage/src/main/kotlin/dev/dioxus/g3_native_plugins/storage/StoragePlugin.kt",
        );
        let swift = include_str!("ios/Sources/StoragePlugin.swift");
        let gradle = include_str!("android/storage/build.gradle.kts");
        assert!(source.contains("pub fn get(&mut self, key: &str)"));
        assert!(source.contains("pub fn set(&mut self, key: &str, value: &str)"));
        assert!(source.contains("pub fn keys(&mut self)"));
        // A store that silently discards is worse than one that says it cannot
        // help, so this plugin alone does not compile to an inert no-op.
        assert!(source.contains("Secure storage is not implemented on macOS"));
        // Android: Keystore-held AES-GCM, and no Tink dependency behind an
        // unmaintained library to get it.
        assert!(kotlin.contains("AndroidKeyStore"));
        assert!(kotlin.contains("AES/GCM/NoPadding"));
        // No dependency at all, so nothing to argue about. The comment in that
        // file names security-crypto to explain why, hence matching on the
        // declaration form rather than the word.
        assert!(!gradle.contains("implementation("));
        // The cipher picks the nonce: a reused GCM nonce leaks the plaintext.
        assert!(kotlin.contains("val nonce = cipher.iv"));
        // kotlin.error() throws, so a helper of that name would be a live trap.
        assert!(!kotlin.contains(" error("));
        assert!(kotlin.contains("errorJson("));
        // iOS: keychain, readable in the background after one unlock.
        assert!(swift.contains("kSecClassGenericPassword"));
        assert!(swift.contains("kSecAttrAccessibleAfterFirstUnlock"));
        // The keychain has no upsert; update first, then add.
        assert!(swift.contains("SecItemUpdate"));
        assert!(swift.contains("SecItemAdd"));
        assert!(swift.contains("errSecItemNotFound"));
    }
    #[test]
    fn camera_microphone_leaves_the_webview_permission_bridge_to_wry() {
        let source = production_source(include_str!("camera_microphone.rs"));
        let kotlin = include_str!(
            "android/camera_microphone/src/main/kotlin/dev/dioxus/g3_native_plugins/camera_microphone/CameraMicrophonePlugin.kt",
        );
        let swift = include_str!("ios/Sources/CameraMicrophonePlugin.swift");
        assert!(source.contains("pub fn check_permissions"));
        assert!(source.contains("pub fn open_settings"));
        assert!(source.contains("pub struct CapturePermissions"));
        // wry already answers the WebView's own permission callbacks. Taking
        // either delegate over would break its file chooser and JS dialogs, so
        // neither native side may install one. The prose above each plugin says
        // as much, so these look for the code, not the words.
        assert!(!kotlin.contains("setWebChromeClient"));
        assert!(!kotlin.contains("override fun onPermissionRequest"));
        assert!(!swift.contains("uiDelegate ="));
        assert!(!swift.contains("func webView("));
        // What is left is the part the page cannot reach.
        assert!(kotlin.contains("ACTION_APPLICATION_DETAILS_SETTINGS"));
        assert!(kotlin.contains("shouldShowRequestPermissionRationale"));
        assert!(swift.contains("openSettingsURLString"));
        assert!(swift.contains("AVCaptureDevice.authorizationStatus"));
        assert!(swift.contains("AVCaptureDevice.requestAccess"));
        // A .playback session has no input, so a shared session needs moving.
        assert!(swift.contains("playAndRecord"));
        assert!(swift.contains("defaultToSpeaker"));
    }
    #[test]
    fn permission_state_is_shared_rather_than_duplicated_per_plugin() {
        // Both plugins glob-export at the crate root, so a second definition
        // would collide there rather than merely repeating itself.
        let permissions = include_str!("permissions.rs");
        let geolocation = production_source(include_str!("geolocation.rs"));
        let capture = production_source(include_str!("camera_microphone.rs"));
        assert!(permissions.contains("pub enum PermissionState"));
        assert!(!geolocation.contains("pub enum PermissionState"));
        assert!(!capture.contains("pub enum PermissionState"));
        assert!(geolocation.contains("use crate::permissions::PermissionState;"));
        assert!(capture.contains("use crate::permissions::PermissionState;"));
    }
    #[test]
    fn geolocation_starts_and_polls_rather_than_blocking() {
        let source = production_source(include_str!("geolocation.rs"));
        let kotlin = include_str!(
            "android/geolocation/src/main/kotlin/dev/dioxus/g3_native_plugins/geolocation/GeolocationPlugin.kt",
        );
        let swift = include_str!("ios/Sources/GeolocationPlugin.swift");
        let gradle = include_str!("android/geolocation/build.gradle.kts");
        assert!(source.contains("pub fn start_position_request"));
        assert!(
            source.contains("pub fn poll_position(&mut self) -> Result<Option<Position>, String>"),
        );
        assert!(source.contains("pub fn check_permissions"));
        assert!(source.contains("pub struct Position"));
        assert!(source.contains("pub struct Coordinates"));
        // The reference example blocks the caller on both platforms; this one
        // must not, because the call arrives on a Dioxus effect thread.
        assert!(!kotlin.contains("CountDownLatch"));
        assert!(!swift.contains("DispatchSemaphore"));
        assert!(!swift.contains("RunLoop.current.run"));
        // A library does not get to force Play Services on its consumers, so
        // no dependency may name the coordinate the fused provider ships under.
        assert!(!gradle.contains("com.google.android.gms"));
        assert!(!kotlin.contains("com.google.android.gms"));
        assert!(kotlin.contains("LocationManager"));
        assert!(kotlin.contains("requestLocationUpdates"));
        assert!(kotlin.contains("prompt-with-rationale"));
        assert!(swift.contains("CLLocationManagerDelegate"));
        assert!(swift.contains("manager.requestLocation()"));
        assert!(swift.contains("locationManagerDidChangeAuthorization"));
        // Negative CoreLocation accuracies mean invalid, not precise.
        assert!(swift.contains("location.verticalAccuracy >= 0"));
    }
    #[test]
    fn in_app_purchases_start_and_poll_on_both_stores() {
        let source = production_source(include_str!("in_app_purchases.rs"));
        let kotlin = include_str!(
            "android/in_app_purchases/src/main/kotlin/dev/dioxus/g3_native_plugins/in_app_purchases/InAppPurchasesPlugin.kt",
        );
        let catalogue = include_str!(
            "android/in_app_purchases/src/main/kotlin/dev/dioxus/g3_native_plugins/in_app_purchases/Catalogue.kt",
        );
        let swift = include_str!("ios/Sources/InAppPurchasesPlugin.swift");
        let store = include_str!("ios/Sources/StoreCatalogue.swift");
        let gradle = include_str!("android/in_app_purchases/build.gradle.kts");
        assert!(source.contains("pub fn start_purchase"));
        assert!(source.contains("pub fn poll_entitlements"));
        assert!(source.contains("pub fn finish(&mut self, transaction_id: &str, consume: bool)"),);
        assert!(source.contains("Subscription"));
        assert!(source.contains("pub struct SubscriptionOffer"));
        assert!(source.contains("pub will_auto_renew: bool"));
        // Play billing has no substitute, so it is a real dependency rather
        // than compileOnly like the androidx ones elsewhere.
        assert!(gradle.contains("com.android.billingclient:billing"));
        assert!(gradle.contains("implementation("));
        // A purchase left unacknowledged for three days is refunded by Play.
        assert!(kotlin.contains("acknowledgePurchase"));
        assert!(kotlin.contains("consumeAsync"));
        assert!(kotlin.contains("needsFinishing"));
        // Subscriptions cannot be bought without naming an offer token.
        assert!(kotlin.contains("setOfferToken"));
        assert!(kotlin.contains("queryPurchasesAsync"));
        assert!(catalogue.contains("subscriptionOfferDetails"));
        // StoreKit 2 is async Swift, which cannot cross the FFI boundary.
        assert!(swift.contains("Transaction.updates"));
        assert!(swift.contains("Transaction.currentEntitlements"));
        assert!(swift.contains("Transaction.unfinished"));
        assert!(swift.contains("AppStore.sync()"));
        assert!(swift.contains("jwsRepresentation"));
        assert!(swift.contains("renewal.willAutoRenew"));
        assert!(store.contains("nonRenewable"));
    }
    #[test]
    fn ios_media_plugin_owns_background_audio_and_lock_screen_controls() {
        let source = production_source(include_str!("media.rs"));
        let swift = include_str!("ios/Sources/MediaPlugin.swift");
        assert!(source.contains("#[manganis::ffi(\"src/ios\")]"));
        assert!(source.contains(
            "pub fn setPlaybackActiveFromRust(this: &MediaPlugin, active: bool, title: String);",
        ),);
        assert!(swift.contains("AVAudioSession.sharedInstance().setCategory(.playback"));
        assert!(swift.contains("MPNowPlayingInfoCenter"));
        assert!(swift.contains("MPRemoteCommandCenter.shared()"));
        assert!(swift.contains("beginReceivingRemoteControlEvents"));
        assert!(swift.contains("notifyOthersOnDeactivation"));
        assert!(swift.contains("webkitSetPresentationMode"));
        assert!(swift.contains("requestGeometryUpdate"));
        // Same duplicate-event guard as Android, for the same audible reason.
        assert!(swift.contains("if !wasActive || titleChanged"));
        assert!(swift.contains("__tawnyPlaybackIntent"));
    }
    #[test]
    fn native_plugins_provider_owns_plugin_construction() {
        let source = production_source(include_str!("lib.rs"));
        let provider_start = source
            .find("pub fn NativePluginsProvider(children: Element) -> Element")
            .expect("provider component should be exported");
        let provider_prefix = &source[..provider_start];
        assert!(source.contains("pub struct NativePlugins"));
        assert!(source.contains("pub fn NativePluginsProvider(children: Element) -> Element"),);
        assert!(
            !provider_prefix
                .trim_end()
                .ends_with("target_os = \"macos\"\n))]\n#[component]"),
        );
        assert!(source.contains("use_context_provider(NativePlugins::default)"));
        assert!(source.contains("{children}"));
        assert!(source.contains("pub auth: Signal<Auth>"));
        assert!(source.contains("pub camera_microphone: Signal<CameraMicrophone>"));
        assert!(source.contains("pub clipboard: Signal<Clipboard>"));
        assert!(source.contains("pub deep_links: Signal<DeepLinks>"));
        assert!(source.contains("pub external_url: Signal<ExternalUrl>"));
        assert!(source.contains("pub geolocation: Signal<Geolocation>"));
        assert!(source.contains("pub in_app_purchases: Signal<InAppPurchases>"));
        assert!(source.contains("pub media: Signal<Media>"));
        assert!(source.contains("pub storage: Signal<KeyValueStore>"));
        assert!(source.contains("auth: Signal::new(Auth::new())"));
        assert!(source.contains("camera_microphone: Signal::new(CameraMicrophone::new())"));
        assert!(source.contains("clipboard: Signal::new(Clipboard::new())"));
        assert!(source.contains("deep_links: Signal::new(DeepLinks::new())"));
        assert!(source.contains("external_url: Signal::new(ExternalUrl::new())"));
        assert!(source.contains("geolocation: Signal::new(Geolocation::new())"));
        assert!(source.contains("in_app_purchases: Signal::new(InAppPurchases::new())"));
        assert!(source.contains("media: Signal::new(Media::new())"));
        assert!(source.contains("storage: Signal::new(KeyValueStore::new())"));
        assert!(source.contains("impl Default for NativePlugins"));
        assert!(source.contains("Self::new()"));
    }
    #[test]
    fn ios_clipboard_plugin_supports_copy_and_scene_safe_share() {
        let swift = include_str!("ios/Sources/ClipboardPlugin.swift");
        let rust = include_str!("clipboard.rs");
        assert!(rust.contains("pub fn copy_to_clipboard(&mut self, text: String)"));
        assert!(rust.contains(
            "pub fn copyToClipboardFromRust(this: &ClipboardPlugin, text: String) -> String;",
        ),);
        assert!(swift.contains("public func copyToClipboardFromRust(_ text: String) -> String",),);
        assert!(swift.contains("UIPasteboard.general.string = text"));
        assert!(swift.contains("NSLog(\"[ClipboardPlugin]"));
        assert!(swift.contains("connectedScenes"));
        assert!(swift.contains("popover.sourceView = vc.view"));
        assert!(!swift.contains("keyWindow"));
    }
    #[test]
    fn ios_plugins_share_one_swift_package_for_current_dx() {
        let manifest = include_str!("ios/Package.swift");
        let auth = include_str!("auth.rs");
        let swift_auth = include_str!("ios/Sources/AuthPlugin.swift");
        let clipboard = include_str!("clipboard.rs");
        let external_url = include_str!("external_url.rs");
        assert!(manifest.contains("name: \"DioxusNativePlugins\""));
        assert!(manifest.contains(".library(name: \"AuthPlugin\""));
        assert!(manifest.contains(".library(name: \"BackButtonPlugin\""));
        assert!(manifest.contains(".library(name: \"CameraMicrophonePlugin\""));
        assert!(manifest.contains(".library(name: \"ClipboardPlugin\""));
        assert!(manifest.contains(".library(name: \"DeepLinksPlugin\""));
        assert!(manifest.contains(".library(name: \"ExternalUrlPlugin\""));
        assert!(manifest.contains(".library(name: \"GeolocationPlugin\""));
        assert!(manifest.contains(".library(name: \"InAppPurchasesPlugin\""));
        assert!(manifest.contains(".linkedFramework(\"StoreKit\")"));
        assert!(manifest.contains(".linkedFramework(\"CoreLocation\")"));
        assert!(manifest.contains(".library(name: \"MediaPlugin\""));
        assert!(manifest.contains(".library(name: \"StoragePlugin\""));
        assert!(manifest.contains(".linkedFramework(\"Security\")"));
        assert!(manifest.contains(".linkedFramework(\"AuthenticationServices\")"));
        assert!(manifest.contains(".linkedFramework(\"AVFoundation\")"));
        assert!(manifest.contains(".linkedFramework(\"MediaPlayer\""));
        assert!(manifest.contains(".linkedFramework(\"WebKit\")"));
        assert!(auth.contains("#[manganis::ffi(\"src/ios\")]"));
        assert!(
            auth.contains("pub fn startAppleAuthFromRust(this: &AuthPlugin) -> Option<String>;",),
        );
        assert!(auth.contains("pub fn getAuthState(this: &AuthPlugin) -> Option<String>;"),);
        assert!(!auth.contains("signOutFromRust"));
        assert!(swift_auth.contains("ASAuthorizationAppleIDProvider"));
        assert!(swift_auth.contains("public func getAuthState() -> String?"));
        assert!(swift_auth.contains("identity_token"));
        assert!(clipboard.contains("#[manganis::ffi(\"src/ios\")]"));
        assert!(external_url.contains("#[manganis::ffi(\"src/ios\")]"));
    }
    #[test]
    fn android_auth_plugin_exposes_google_sign_in() {
        let auth = include_str!("auth.rs");
        let kotlin_auth = include_str!(
            "android/auth/src/main/kotlin/dev/dioxus/g3_native_plugins/auth/AuthPlugin.kt",
        );
        assert!(auth.contains("#[manganis::ffi(\"src/android/auth\")]"));
        assert!(auth.contains("unsafe extern \"Kotlin\""));
        let android = auth
            .split("unsafe extern \"Kotlin\" {")
            .nth(1)
            .expect("auth declares its Kotlin module")
            .split('}')
            .next()
            .expect("the Kotlin block should be closed");
        assert!(!android.contains("pub fn "));
        assert!(auth.contains("type AuthHandle = crate::android_bridge::AndroidPlugin"));
        // The client id is the caller's, not the crate's. It names one Google
        // Cloud project and is checked against an Android client registered
        // there for the app's package and certificate, so a library carrying
        // its own could only ever authenticate the app it shipped with.
        assert!(auth.contains(
            "pub fn start_google_auth(&mut self, server_client_id: &str) -> Result<(), String>",
        ),);
        assert!(
            auth.contains("pub fn poll_auth_result(&mut self) -> Result<Option<String>, String>",)
        );
        assert!(auth.contains("pub fn is_auth_awaiting(&mut self) -> bool"));
        assert!(kotlin_auth.contains("fun startGoogleAuthFromRust(serverClientId: String)"));
        assert!(kotlin_auth.contains("fun getPendingResult(): String?"));
        assert!(kotlin_auth.contains("fun getAuthState(): String"));
        assert!(kotlin_auth.contains("setServerClientId(serverClientId)"));
        assert!(!kotlin_auth.contains("apps.googleusercontent.com"));
        assert!(!kotlin_auth.contains("CountDownLatch"));
        assert!(!kotlin_auth.contains("TimeUnit"));
        assert!(kotlin_auth.contains("GoogleIdTokenCredential.createFrom"));
    }
    #[test]
    fn android_media_plugin_owns_mobile_playback_capabilities() {
        let rust = include_str!("media.rs");
        let kotlin = include_str!(
            "android/media/src/main/kotlin/dev/dioxus/g3_native_plugins/media/MediaPlugin.kt",
        );
        let service = include_str!(
            "android/media/src/main/kotlin/dev/dioxus/g3_native_plugins/media/PlaybackService.kt",
        );
        let manifest = include_str!("android/media/src/main/AndroidManifest.xml");
        assert!(rust.contains("getClassLoader"));
        assert!(rust.contains("enter_picture_in_picture"));
        assert!(rust.contains("set_orientation"));
        assert!(rust.contains("set_playback_active"));
        assert!(kotlin.contains("enterPictureInPictureMode"));
        assert!(kotlin.contains("setActions(pipActions())"));
        assert!(kotlin.contains("COMMAND_REWIND"));
        assert!(kotlin.contains("COMMAND_FORWARD"));
        assert!(kotlin.contains("dataset.androidPip"));
        assert!(kotlin.contains("tawnynativepictureinpicturechange"));
        assert!(kotlin.contains("tawnynativeplaybackresume"));
        assert!(kotlin.contains("Application.ActivityLifecycleCallbacks"));
        assert!(kotlin.contains("if (!wasActive || titleChanged)"));
        assert!(!kotlin.contains("setSourceRectHint"));
        assert!(kotlin.contains("SCREEN_ORIENTATION_SENSOR_LANDSCAPE"));
        assert!(kotlin.contains("--android-status-bar-inset"));
        assert!(service.contains("startForeground"));
        assert!(manifest.contains("android:supportsPictureInPicture=\"true\""));
        assert!(manifest.contains("foregroundServiceType=\"mediaPlayback\""));
    }
    #[test]
    fn plugin_wrapper_constructors_stay_crate_private_and_lazy() {
        for (source, plugin_type, handle_type) in [
            (
                production_source(include_str!("auth.rs")),
                "AuthPlugin",
                "AuthHandle",
            ),
            (
                production_source(include_str!("clipboard.rs")),
                "ClipboardPlugin",
                "ClipboardHandle",
            ),
            (
                production_source(include_str!("external_url.rs")),
                "ExternalUrlPlugin",
                "ExternalUrlHandle",
            ),
        ] {
            assert!(source.contains("pub(crate) fn new() -> Self"));
            assert!(!source.contains("pub fn new() -> Self"));
            assert!(source.contains(&format!("plugin: Option<{handle_type}>")));
            assert!(source.contains("Self { plugin: None }"));
            assert!(source.contains("self.plugin = Some("));
            assert!(source.contains(&format!("{plugin_type}::new()")));
            assert!(!source.contains(&format!("plugin: Result<{plugin_type}, String>")));
        }
    }
}
