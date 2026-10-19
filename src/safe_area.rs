//! The system bars' insets, for pages drawn edge to edge, and hiding the bars.
//!
//! Android 15 draws every app edge to edge: the status bar, a camera cutout
//! and the gesture or button bar sit over the page. CSS has
//! `env(safe-area-inset-*)` for exactly this, but Android WebViews before
//! Chrome's edge-to-edge support report all four as 0, so a header slides
//! under the status bar and a tab bar under the gesture handle. This reads
//! the insets from the window and publishes them as `--g3-safe-area-top`,
//! `-right`, `-bottom` and `-left` on the root element, which g3-ui prefers
//! over `env()`. iOS's WKWebView and browsers already report `env()`
//! correctly, so there this is inert.
//!
//! [`NativePluginsProvider`](crate::NativePluginsProvider) starts it, so an
//! app enabling the feature needs no code.

/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. Android calls go through [`crate::android_bridge`].
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/safe_area")]
unsafe extern "Kotlin" {
    pub type SafeAreaPlugin;
}
#[cfg(target_os = "android")]
const SAFE_AREA_CLASS: &str = "dev.dioxus.g3_native_plugins.safe_area.SafeAreaPlugin";

/// Publishes system-bar insets to the page, and hides or shows the bars.
pub struct SafeArea {
    #[cfg(target_os = "android")]
    plugin: Option<crate::android_bridge::AndroidPlugin>,
}

impl SafeArea {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(target_os = "android")]
            plugin: None,
        }
    }

    /// Starts publishing the insets, now and whenever they change (rotation,
    /// a cutout, the bars hidden or shown). Safe to call again: the page gets
    /// the current values, which is what a reloaded page needs.
    pub fn start(&mut self) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.plugin()?.call_unit("startFromRust")
        }
        #[cfg(not(target_os = "android"))]
        Ok(())
    }

    /// Hides the status and navigation bars (`true`) or shows them. Hidden, a
    /// swipe from the edge shows them for a moment, and the published insets
    /// drop to 0, so the page fills the screen. Inert outside Android.
    pub fn set_bars_hidden(&mut self, hidden: bool) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.plugin()?
                .call_unit_bool("setBarsHiddenFromRust", hidden)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = hidden;
            Ok(())
        }
    }

    #[cfg(target_os = "android")]
    fn plugin(&mut self) -> Result<&crate::android_bridge::AndroidPlugin, String> {
        if self.plugin.is_none() {
            self.plugin = Some(crate::android_bridge::AndroidPlugin::new(SAFE_AREA_CLASS)?);
        }
        Ok(self.plugin.as_ref().expect("created above"))
    }
}

impl Default for SafeArea {
    fn default() -> Self {
        Self::new()
    }
}
