#![allow(non_snake_case)]

pub mod deep_links;

cfg_if::cfg_if! {
    if #[cfg(feature = "clipboard")] {
        mod clipboard;
        #[allow(unused_imports)]
        pub use clipboard::*;
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "auth")] {
        mod auth;
        #[allow(unused_imports)]
        pub use auth::*;
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "back-button")] {
        mod back_button;
        #[allow(unused_imports)]
        pub use back_button::*;
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "external-url")] {
        mod external_url;
        #[allow(unused_imports)]
        pub use external_url::*;
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "media")] {
        mod media;
        #[allow(unused_imports)]
        pub use media::*;
    }
}

#[cfg(all(
    any(
        feature = "auth",
        feature = "back-button",
        feature = "clipboard",
        feature = "external-url",
        feature = "media"
    ),
    any(
        target_arch = "wasm32",
        target_os = "android",
        target_os = "ios",
        target_os = "macos"
    )
))]
use dioxus_signals::Signal;

use dioxus::prelude::*;

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
    #[cfg(feature = "clipboard")]
    pub clipboard: Signal<Clipboard>,
    #[cfg(all(
        feature = "external-url",
        any(target_arch = "wasm32", target_os = "android", target_os = "ios")
    ))]
    pub external_url: Signal<ExternalUrl>,
    #[cfg(feature = "media")]
    pub media: Signal<Media>,
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
            #[cfg(feature = "clipboard")]
            clipboard: Signal::new(Clipboard::new()),
            #[cfg(all(
                feature = "external-url",
                any(target_arch = "wasm32", target_os = "android", target_os = "ios")
            ))]
            external_url: Signal::new(ExternalUrl::new()),
            #[cfg(feature = "media")]
            media: Signal::new(Media::new()),
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

    rsx! { {children} }
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
        // A callback left enabled at the root of the stack traps the user in
        // the app, so nothing may intercept until the app asks.
        let source = production_source(include_str!("back_button.rs"));
        assert!(source.contains("intercepting: false"));
        assert!(source.contains("pub fn set_intercepting"));
        // The router's history lives in Rust, not in the WebView, so the press
        // has to cross back over the bridge rather than call history.back().
        let kotlin = include_str!(
            "android/back_button/src/main/kotlin/dev/dioxus/dx_native_plugins/back_button/BackButtonPlugin.kt"
        );
        assert!(kotlin.contains("dxnativeback"));
        assert!(kotlin.contains("evaluateJavascript(BACK_EVENT_SCRIPT"));
        assert!(kotlin.contains("OnBackPressedCallback(false)"));
    }

    #[test]
    fn native_plugins_provider_owns_plugin_construction() {
        let source = production_source(include_str!("lib.rs"));

        let provider_start = source
            .find("pub fn NativePluginsProvider(children: Element) -> Element")
            .expect("provider component should be exported");
        let provider_prefix = &source[..provider_start];

        assert!(source.contains("pub struct NativePlugins"));
        assert!(source.contains("pub fn NativePluginsProvider(children: Element) -> Element"));
        assert!(
            !provider_prefix
                .trim_end()
                .ends_with("target_os = \"macos\"\n))]\n#[component]")
        );
        assert!(source.contains("use_context_provider(NativePlugins::default)"));
        assert!(source.contains("rsx! { {children} }"));
        assert!(source.contains("pub auth: Signal<Auth>"));
        assert!(source.contains("pub clipboard: Signal<Clipboard>"));
        assert!(source.contains("pub external_url: Signal<ExternalUrl>"));
        assert!(source.contains("pub media: Signal<Media>"));
        assert!(source.contains("auth: Signal::new(Auth::new())"));
        assert!(source.contains("clipboard: Signal::new(Clipboard::new())"));
        assert!(source.contains("external_url: Signal::new(ExternalUrl::new())"));
        assert!(source.contains("media: Signal::new(Media::new())"));
        assert!(source.contains("impl Default for NativePlugins"));
        assert!(source.contains("Self::new()"));
    }

    #[test]
    fn ios_clipboard_plugin_supports_copy_and_scene_safe_share() {
        let swift = include_str!("ios/Sources/ClipboardPlugin.swift");
        let rust = include_str!("clipboard.rs");

        assert!(rust.contains("pub fn copy_to_clipboard(&mut self, text: String)"));
        assert!(rust.contains(
            "pub fn copyToClipboardFromRust(this: &ClipboardPlugin, text: String) -> String;"
        ));
        assert!(swift.contains("public func copyToClipboardFromRust(_ text: String) -> String"));
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
        assert!(manifest.contains(".library(name: \"ClipboardPlugin\""));
        assert!(manifest.contains(".library(name: \"ExternalUrlPlugin\""));
        assert!(manifest.contains(".linkedFramework(\"AuthenticationServices\")"));
        assert!(auth.contains("#[manganis::ffi(\"src/ios\")]"));
        assert!(
            auth.contains("pub fn startAppleAuthFromRust(this: &AuthPlugin) -> Option<String>;")
        );
        assert!(auth.contains("pub fn getAuthState(this: &AuthPlugin) -> Option<String>;"));
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
            "android/auth/src/main/kotlin/dev/dioxus/dx_native_plugins/auth/AuthPlugin.kt"
        );

        assert!(auth.contains("#[manganis::ffi(\"src/android/auth\")]"));
        assert!(auth.contains("unsafe extern \"Kotlin\""));
        assert!(
            auth.contains("pub fn startGoogleAuthFromRust(this: &AuthPlugin) -> Option<String>;")
        );
        assert!(auth.contains("pub struct AuthResult"));
        assert!(auth.contains("pub credential: Option<String>"));
        assert!(auth.contains("pub fn start_google_auth(&mut self) -> Result<AuthResult, String>"));
        assert!(kotlin_auth.contains("fun startGoogleAuthFromRust(): String?"));
        assert!(kotlin_auth.contains("GoogleIdTokenCredential.createFrom"));
    }

    #[test]
    fn android_media_plugin_owns_mobile_playback_capabilities() {
        let rust = include_str!("media.rs");
        let kotlin = include_str!(
            "android/media/src/main/kotlin/dev/dioxus/dx_native_plugins/media/MediaPlugin.kt"
        );
        let service = include_str!(
            "android/media/src/main/kotlin/dev/dioxus/dx_native_plugins/media/PlaybackService.kt"
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
        assert!(!kotlin.contains("setSourceRectHint"));
        assert!(kotlin.contains("SCREEN_ORIENTATION_SENSOR_LANDSCAPE"));
        assert!(kotlin.contains("--android-status-bar-inset"));
        assert!(service.contains("startForeground"));
        assert!(manifest.contains("android:supportsPictureInPicture=\"true\""));
        assert!(manifest.contains("foregroundServiceType=\"mediaPlayback\""));
    }
    #[test]
    fn plugin_wrapper_constructors_stay_crate_private_and_lazy() {
        for (source, plugin_type) in [
            (production_source(include_str!("auth.rs")), "AuthPlugin"),
            (
                production_source(include_str!("clipboard.rs")),
                "ClipboardPlugin",
            ),
            (
                production_source(include_str!("external_url.rs")),
                "ExternalUrlPlugin",
            ),
        ] {
            assert!(source.contains("pub(crate) fn new() -> Self"));
            assert!(!source.contains("pub fn new() -> Self"));
            assert!(source.contains(&format!("plugin: Option<{plugin_type}>")));
            assert!(source.contains("Self { plugin: None }"));
            assert!(source.contains("self.plugin = Some("));
            assert!(source.contains(&format!("{plugin_type}::new()")));
            assert!(!source.contains(&format!("plugin: Result<{plugin_type}, String>")));
        }
    }
}
