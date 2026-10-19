//! The system's light or dark setting, where the web view cannot see it, and
//! the system-bar icon color that has to match the page.
//!
//! An Android WebView does not report the system theme to the page. From
//! target SDK 33 its `prefers-color-scheme` follows the app's own theme
//! instead (`isLightTheme`), and the Activity `dx` generates uses a light
//! AppCompat theme, so the page is told "light" whatever the phone is set to.
//! Asking the system directly is the way round it. Browsers and iOS's
//! WKWebView already report the real setting, so there this answers `None`
//! and leaves the page to its own media query.
//!
//! Changing the system theme recreates the Android Activity (the generated
//! manifest does not handle `uiMode` itself), which restarts the app, so one
//! read at startup stays true for the life of the page.

/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. Android calls go through [`crate::android_bridge`] so
/// they resolve the class through the app's loader on Dioxus worker threads.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/appearance")]
unsafe extern "Kotlin" {
    pub type AppearancePlugin;
}
#[cfg(target_os = "android")]
const APPEARANCE_CLASS: &str = "dev.dioxus.g3_native_plugins.appearance.AppearancePlugin";

/// Reads whether the system is set to dark mode.
pub struct SystemAppearance {
    #[cfg(target_os = "android")]
    plugin: Option<crate::android_bridge::AndroidPlugin>,
}

impl SystemAppearance {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(target_os = "android")]
            plugin: None,
        }
    }

    /// Whether the system is in dark mode, where the web view misreports it.
    ///
    /// `None` means the page's own `prefers-color-scheme` is already right:
    /// on the web and on Apple platforms, and on Android if the system could
    /// not be asked.
    pub fn dark(&mut self) -> Option<bool> {
        #[cfg(target_os = "android")]
        {
            let answer = self.plugin()?.call_string("isDarkFromRust").ok()??;
            Some(answer == "true")
        }
        #[cfg(not(target_os = "android"))]
        None
    }

    /// Makes the status- and navigation-bar icons dark (`true`, for a light
    /// page) or light (`false`, for a dark page).
    ///
    /// On Android 15 the app is drawn edge to edge: the page sits behind the
    /// system bars, but their icons follow the Activity's theme, so a light
    /// page under the light theme `dx` generates can end up with white icons
    /// on it. Call this with the scheme the page actually draws, and again
    /// whenever it changes. Inert elsewhere: iOS picks the status-bar style
    /// from the interface style the web view already follows, and a browser
    /// has no bars to color.
    pub fn set_dark_bar_icons(&mut self, dark: bool) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            let plugin = self
                .plugin()
                .ok_or_else(|| "The appearance plugin is unavailable".to_string())?;
            plugin.call_unit_bool("setDarkBarIconsFromRust", dark)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = dark;
            Ok(())
        }
    }

    #[cfg(target_os = "android")]
    fn plugin(&mut self) -> Option<&crate::android_bridge::AndroidPlugin> {
        if self.plugin.is_none() {
            self.plugin = crate::android_bridge::AndroidPlugin::new(APPEARANCE_CLASS).ok();
        }
        self.plugin.as_ref()
    }
}

impl Default for SystemAppearance {
    fn default() -> Self {
        Self::new()
    }
}
