#[cfg(target_os = "android")]
#[manganis::ffi("src/android/media")]
unsafe extern "Kotlin" {
    pub type MediaPlugin;
}

#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type MediaPlugin;

    pub fn prepareFromRust(this: &MediaPlugin);

    pub fn enterPictureInPictureFromRust(this: &MediaPlugin, dimensionsJson: String);

    pub fn setOrientationFromRust(this: &MediaPlugin, orientation: String);

    pub fn setPlaybackActiveFromRust(this: &MediaPlugin, playbackJson: String);
}

#[cfg(target_os = "android")]
const MEDIA_PLUGIN_CLASS: &str = "dev.dioxus.g3_native_plugins.media.MediaPlugin";

/// Playback that survives leaving the app, and the controls that go with it.
///
/// A `<video>` inside a WebView is only allowed to keep running while the app
/// is frontmost. Android needs a foreground service to hold it, iOS needs a
/// playback audio session; neither platform shows lock-screen controls unless
/// the app publishes metadata for them. This plugin does that per platform and
/// routes the resulting controls back into the same player element, so one call
/// site drives both.
///
/// The web and macOS builds are inert, so callers need no cfg of their own.
#[cfg(target_os = "android")]
pub struct Media {
    plugin: Option<crate::android_bridge::AndroidPlugin>,
}

#[cfg(target_os = "ios")]
/// Native media-session support for iOS.
pub struct Media {
    plugin: Option<MediaPlugin>,
}

#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
/// An inert media facade for targets without a native media session.
pub struct Media;

#[cfg(target_os = "android")]
impl Media {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }

    fn get_plugin(&mut self) -> Result<&crate::android_bridge::AndroidPlugin, String> {
        if self.plugin.is_none() {
            self.plugin = Some(crate::android_bridge::AndroidPlugin::new(
                MEDIA_PLUGIN_CLASS,
            )?);
        }
        Ok(self.plugin.as_ref().unwrap())
    }

    /// Initializes Android media-session support.
    pub fn prepare(&mut self) -> Result<(), String> {
        // Kept: the Kotlin plugin releases its session and listeners when its
        // Activity is destroyed, so a recreated Activity needs them again.
        // Playback state is not replayed; the page reports it afresh.
        self.get_plugin()?
            .keep_unit("prepareFromRust")
            .map_err(|error| format!("Failed to prepare media plugin: {error}"))
    }

    /// Requests picture-in-picture using the supplied source dimensions.
    pub fn enter_picture_in_picture(&mut self, width: i32, height: i32) -> Result<(), String> {
        self.get_plugin()?
            .call_unit_int_int("enterPictureInPictureFromRust", width, height)
            .map_err(|error| format!("Failed to enter picture-in-picture: {error}"))
    }

    /// Requests a device orientation such as `portrait` or `landscape`. Anything
    /// else, such as `unspecified`, returns to the app's own: on Android, its
    /// manifest's lock on a phone and free rotation on a tablet.
    pub fn set_orientation(&mut self, orientation: impl Into<String>) -> Result<(), String> {
        let orientation = orientation.into();
        self.get_plugin()?
            .call_unit_str("setOrientationFromRust", &orientation)
            .map_err(|error| format!("Failed to set orientation: {error}"))
    }

    /// Hides or restores the status and navigation bars, for fullscreen
    /// playback. A WebView's own fullscreen stops at the WebView's bounds.
    pub fn set_system_bars_hidden(&mut self, hidden: bool) -> Result<(), String> {
        self.get_plugin()?
            .call_unit_bool("setSystemBarsHiddenFromRust", hidden)
            .map_err(|error| format!("Failed to update system bars: {error}"))
    }

    /// Publishes or clears an active background-playback session.
    pub fn set_playback_active(
        &mut self,
        active: bool,
        title: impl Into<String>,
    ) -> Result<(), String> {
        let title = title.into();
        self.get_plugin()?
            .call_unit_bool_str("setPlaybackActiveFromRust", active, &title)
            .map_err(|error| format!("Failed to update background playback: {error}"))
    }

    /// Removes the media session and its controls once the player is gone.
    /// An inactive [`Media::set_playback_active`] only pauses them, so the
    /// system player and a headset can still resume playback.
    pub fn clear_playback(&mut self) -> Result<(), String> {
        self.get_plugin()?
            .call_unit("clearPlaybackFromRust")
            .map_err(|error| format!("Failed to clear background playback: {error}"))
    }
}

#[cfg(target_os = "ios")]
impl Media {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }

    fn get_plugin(&mut self) -> Result<&MediaPlugin, String> {
        if self.plugin.is_none() {
            self.plugin = Some(
                MediaPlugin::new()
                    .map_err(|error| format!("Failed to create MediaPlugin: {error:?}"))?,
            );
        }
        Ok(self.plugin.as_ref().unwrap())
    }

    /// Initializes the iOS audio session and remote-command handlers.
    pub fn prepare(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        prepareFromRust(plugin)?;
        Ok(())
    }

    /// The dimensions are accepted for a call site shared with Android and
    /// ignored here: AVKit sizes the picture-in-picture window from the video
    /// track rather than from a caller-supplied aspect hint.
    pub fn enter_picture_in_picture(&mut self, width: i32, height: i32) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let dimensions = serde_json::to_string(&serde_json::json!({
            "width": width,
            "height": height,
        }))
        .map_err(|error| format!("Failed to encode picture-in-picture dimensions: {error}"))?;
        enterPictureInPictureFromRust(plugin, dimensions)?;
        Ok(())
    }

    /// Requests a device orientation such as `portrait` or `landscape`. Anything
    /// else, such as `unspecified`, returns to the app's own: on Android, its
    /// manifest's lock on a phone and free rotation on a tablet.
    pub fn set_orientation(&mut self, orientation: impl Into<String>) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        setOrientationFromRust(plugin, orientation.into())?;
        Ok(())
    }

    /// WebKit's fullscreen presentation already covers the status bar.
    pub fn set_system_bars_hidden(&mut self, _hidden: bool) -> Result<(), String> {
        Ok(())
    }

    /// Publishes or clears an active background-playback session.
    pub fn set_playback_active(
        &mut self,
        active: bool,
        title: impl Into<String>,
    ) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let playback = serde_json::to_string(&serde_json::json!({
            "active": active,
            "title": title.into(),
        }))
        .map_err(|error| format!("Failed to encode playback state: {error}"))?;
        setPlaybackActiveFromRust(plugin, playback)?;
        Ok(())
    }

    /// iOS already releases its session on an inactive update.
    pub fn clear_playback(&mut self) -> Result<(), String> {
        self.set_playback_active(false, String::new())
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
impl Media {
    pub(crate) fn new() -> Self {
        Self
    }

    /// No-op preparation on targets without native media-session support.
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// No-op picture-in-picture request on unsupported targets.
    pub fn enter_picture_in_picture(&mut self, _width: i32, _height: i32) -> Result<(), String> {
        Ok(())
    }

    /// No-op orientation request on unsupported targets.
    pub fn set_orientation(&mut self, _orientation: impl Into<String>) -> Result<(), String> {
        Ok(())
    }

    /// No-op system-bar request on unsupported targets.
    pub fn set_system_bars_hidden(&mut self, _hidden: bool) -> Result<(), String> {
        Ok(())
    }

    /// No-op playback-state update on unsupported targets.
    pub fn set_playback_active(
        &mut self,
        _active: bool,
        _title: impl Into<String>,
    ) -> Result<(), String> {
        Ok(())
    }

    /// No-op playback clear on unsupported targets.
    pub fn clear_playback(&mut self) -> Result<(), String> {
        Ok(())
    }
}
