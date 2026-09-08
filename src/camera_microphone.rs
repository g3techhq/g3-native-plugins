use crate::permissions::PermissionState;
use serde::{Deserialize, Serialize};
/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/camera_microphone")]
unsafe extern "Kotlin" {
    pub type CameraMicrophonePlugin;
}
#[cfg(target_os = "android")]
const CAMERA_MICROPHONE_CLASS: &str =
    "dev.dioxus.g3_native_plugins.camera_microphone.CameraMicrophonePlugin";
#[cfg(target_os = "android")]
type CameraMicrophoneHandle = crate::android_bridge::AndroidPlugin;
#[cfg(target_os = "ios")]
type CameraMicrophoneHandle = CameraMicrophonePlugin;
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type CameraMicrophonePlugin;
    pub fn checkPermissionsFromRust(this: &CameraMicrophonePlugin) -> Option<String>;
    pub fn requestCameraFromRust(this: &CameraMicrophonePlugin);
    pub fn requestMicrophoneFromRust(this: &CameraMicrophonePlugin);
    pub fn openSettingsFromRust(this: &CameraMicrophonePlugin);
    pub fn setCapturingFromRust(this: &CameraMicrophonePlugin, capturing: bool);
}
/// Whether the app may use the camera and the microphone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePermissions {
    /// `CAMERA` on Android, `NSCameraUsageDescription` on iOS.
    pub camera: PermissionState,
    /// `RECORD_AUDIO` on Android, `NSMicrophoneUsageDescription` on iOS.
    pub microphone: PermissionState,
}
/// The permission state around `getUserMedia`, not a capture API of its own.
///
/// Camera and microphone capture in a Dioxus app is ordinary web code: call
/// `navigator.mediaDevices.getUserMedia` from the page and it works. wry
/// already bridges the WebView's permission callbacks on both platforms — its
/// `WebChromeClient` asks Android for `CAMERA` and `RECORD_AUDIO` when the page
/// requests capture, and its `WKUIDelegate` grants WebKit's request so iOS
/// raises the system prompt. This plugin does not touch either, because
/// replacing those delegates would take the file chooser and JS dialogs with
/// it.
///
/// What it adds is the part JavaScript cannot reach:
///
/// - reading permission state **before** calling `getUserMedia`, so the app can
///   show something better than a failed camera,
/// - prompting at a moment of the app's choosing rather than mid-flow,
/// - opening the app's Settings page, which is the only route back once the
///   user has refused for good,
/// - keeping the iOS audio session usable when the app both plays and records.
///
/// Declare the permissions in `Dioxus.toml`, which is what actually makes
/// capture possible; without these the OS refuses before any prompt appears:
///
/// ```toml
/// [permissions]
/// camera = { description = "Show your swing to your partner" }
/// microphone = { description = "Talk to your partner" }
/// ```
///
/// ```rust,ignore
/// let mut plugins = use_context::<NativePlugins>();
///
/// // Ask before the page calls getUserMedia, so a refusal is not a dead end.
/// let state = plugins.camera_microphone.write().check_permissions()?;
/// match state.camera {
///     PermissionState::Prompt | PermissionState::PromptWithRationale => {
///         plugins.camera_microphone.write().request_camera()?;
///     }
///     PermissionState::Denied => {
///         // Only Settings can undo this.
///         plugins.camera_microphone.write().open_settings()?;
///     }
///     PermissionState::Granted => { /* start the stream from JS */ }
/// }
/// ```
///
/// Prompting is asynchronous and its answer arrives on a callback a library
/// cannot hook, so poll
/// [`check_permissions`](CameraMicrophone::check_permissions) for the result,
/// the same as [`crate::Geolocation`].
///
/// The web build is inert: a browser page already has the Permissions API and
/// its own prompt. macOS is inert too.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct CameraMicrophone {
    plugin: Option<CameraMicrophoneHandle>,
}
/// The inert capture-permission facade: see the native [`CameraMicrophone`].
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
pub struct CameraMicrophone;
#[cfg(any(target_os = "android", target_os = "ios"))]
impl CameraMicrophone {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&CameraMicrophoneHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = CameraMicrophoneHandle::new(CAMERA_MICROPHONE_CLASS)?;
            #[cfg(target_os = "ios")]
            let created = CameraMicrophonePlugin::new()
                .map_err(|error| format!("Failed to create CameraMicrophonePlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Construct the native plugin without asking for anything yet.
    pub fn prepare(&mut self) -> Result<(), String> {
        self.get_plugin()?;
        Ok(())
    }
    /// What the app is allowed to capture right now.
    ///
    /// Cheap on both platforms, so this is the thing to poll after a request
    /// rather than waiting on a callback.
    pub fn check_permissions(&mut self) -> Result<CapturePermissions, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let reported = plugin.call_string("checkPermissionsFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = checkPermissionsFromRust(plugin)?;
        let json = reported.ok_or_else(|| "Capture permissions were not reported".to_string())?;
        serde_json::from_str(&json)
            .map_err(|error| format!("Failed to read capture permissions: {error}"))
    }
    /// Show the system camera prompt.
    ///
    /// Does nothing once the state is [`PermissionState::Denied`]: neither
    /// platform will show the dialog again. Send the user to
    /// [`open_settings`](CameraMicrophone::open_settings) instead.
    pub fn request_camera(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("requestCameraFromRust")?;
        #[cfg(target_os = "ios")]
        requestCameraFromRust(plugin)?;
        Ok(())
    }
    /// Show the system microphone prompt. Same caveat as
    /// [`request_camera`](CameraMicrophone::request_camera).
    pub fn request_microphone(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("requestMicrophoneFromRust")?;
        #[cfg(target_os = "ios")]
        requestMicrophoneFromRust(plugin)?;
        Ok(())
    }
    /// Open this app's page in the Settings app.
    ///
    /// The only way back from a permanent refusal, and not reachable from the
    /// page. Leaves the app, so ask before calling it.
    pub fn open_settings(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("openSettingsFromRust")?;
        #[cfg(target_os = "ios")]
        openSettingsFromRust(plugin)?;
        Ok(())
    }
    /// Tell iOS the app is recording, so playback and capture can share the
    /// audio session.
    ///
    /// Only needed by an app that also uses [`crate::Media`] or otherwise plays
    /// audio: that plugin claims a `.playback` session, which has no input, and
    /// a microphone opened under it gets nothing. This moves the session to
    /// `.playAndRecord` for the duration and back to `.playback` afterwards.
    ///
    /// A no-op on Android, which has no such notion, and unnecessary on iOS for
    /// an app that only captures — WebKit configures the session itself when
    /// nothing else has claimed it.
    pub fn set_capturing(&mut self, capturing: bool) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit_bool("setCapturingFromRust", capturing)?;
        #[cfg(target_os = "ios")]
        setCapturingFromRust(plugin, capturing)?;
        Ok(())
    }
}
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
impl CameraMicrophone {
    pub(crate) fn new() -> Self {
        Self
    }
    /// No-op: nothing native to construct on these targets.
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Always [`PermissionState::Prompt`]: on the web the browser owns this,
    /// and the Permissions API is already available to the page.
    pub fn check_permissions(&mut self) -> Result<CapturePermissions, String> {
        Ok(CapturePermissions::default())
    }
    /// No-op: `getUserMedia` raises the browser's own prompt.
    pub fn request_camera(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op: `getUserMedia` raises the browser's own prompt.
    pub fn request_microphone(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op: there is no app settings page to open.
    pub fn open_settings(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op: no audio session to rearrange.
    pub fn set_capturing(&mut self, _capturing: bool) -> Result<(), String> {
        Ok(())
    }
}
