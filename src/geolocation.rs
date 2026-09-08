use crate::permissions::PermissionState;
use serde::{Deserialize, Serialize};
/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/geolocation")]
unsafe extern "Kotlin" {
    pub type GeolocationPlugin;
}
#[cfg(target_os = "android")]
const GEOLOCATION_CLASS: &str = "dev.dioxus.g3_native_plugins.geolocation.GeolocationPlugin";
#[cfg(target_os = "android")]
type GeolocationHandle = crate::android_bridge::AndroidPlugin;
#[cfg(target_os = "ios")]
type GeolocationHandle = GeolocationPlugin;
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type GeolocationPlugin;
    pub fn startPositionRequestFromRust(this: &GeolocationPlugin, optionsJson: String);
    pub fn getLocationStateFromRust(this: &GeolocationPlugin) -> Option<String>;
    pub fn takePositionFromRust(this: &GeolocationPlugin) -> Option<String>;
    pub fn checkPermissionsFromRust(this: &GeolocationPlugin) -> Option<String>;
    pub fn requestPermissionsFromRust(this: &GeolocationPlugin);
}
/// Location permission, split by precision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    /// Precise location. `ACCESS_FINE_LOCATION` on Android; on iOS, location
    /// access at all.
    pub location: PermissionState,
    /// Approximate location. `ACCESS_COARSE_LOCATION` on Android, which the
    /// user can grant alone on Android 12+. Always equal to
    /// [`location`](PermissionStatus::location) on iOS, which does not split
    /// the grant this way.
    pub coarse_location: PermissionState,
}
/// How hard to work for a fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionOptions {
    /// Ask for GPS-grade accuracy rather than a cheap network fix. Ignored on
    /// Android 12+ when the user granted only approximate location.
    pub enable_high_accuracy: bool,
    /// How long to wait for a fix, in milliseconds, before giving up.
    pub timeout: u32,
    /// How stale a cached fix may be, in milliseconds, and still be returned
    /// immediately. Zero forces a fresh reading.
    pub maximum_age: u32,
}
impl Default for PositionOptions {
    fn default() -> Self {
        Self {
            enable_high_accuracy: false,
            timeout: 10_000,
            maximum_age: 0,
        }
    }
}
/// Where the device is, in the shape the web Geolocation API uses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coordinates {
    /// Latitude in decimal degrees.
    pub latitude: f64,
    /// Longitude in decimal degrees.
    pub longitude: f64,
    /// Radius of uncertainty around the coordinates, in meters.
    pub accuracy: f64,
    /// Uncertainty of [`altitude`](Coordinates::altitude), in meters, where the
    /// platform reports one. Android only supplies it from API 26.
    pub altitude_accuracy: Option<f64>,
    /// Height above the WGS 84 ellipsoid, in meters, where available.
    pub altitude: Option<f64>,
    /// Ground speed in meters per second, where available.
    pub speed: Option<f64>,
    /// Direction of travel in degrees clockwise from true north, where
    /// available.
    pub heading: Option<f64>,
}
/// A fix, and when it was taken.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// Milliseconds since the Unix epoch, as the platform reported them.
    pub timestamp: u64,
    /// The coordinates themselves.
    pub coords: Coordinates,
}
/// What the plugin is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LocationState {
    /// Not looking for a fix.
    #[default]
    Idle,
    /// A request is in flight; keep polling
    /// `Geolocation::poll_position`.
    Locating,
}
/// The device's location.
///
/// Getting a fix is asynchronous everywhere and can take seconds, so this
/// starts a request and polls for the answer rather than blocking. That follows
/// [`crate::Auth`], and for the same reason: the call arrives on a Dioxus
/// effect thread, and blocking it would stall the app for as long as the fix
/// takes. On iOS blocking would not even work — `CLLocationManager` delivers on
/// the main queue, which a blocked worker thread is not.
///
/// ```rust,ignore
/// let mut plugins = use_context::<NativePlugins>();
///
/// // Ask once.
/// plugins.geolocation.write().start_position_request(PositionOptions::default())?;
///
/// // Then poll wherever the UI updates.
/// if let Ok(Some(position)) = plugins.geolocation.write().poll_position() {
///     println!("{}, {}", position.coords.latitude, position.coords.longitude);
/// }
/// ```
///
/// Permissions are declared in `Dioxus.toml`, not here — the Dioxus CLI maps
/// `[permissions] location = { precision = "fine", description = "..." }` to
/// `ACCESS_FINE_LOCATION` and `NSLocationWhenInUseUsageDescription`. This
/// plugin only asks the user at runtime, which Android additionally requires.
///
/// The web build is inert rather than wrapping the browser Geolocation API: a
/// page can call `navigator.geolocation` itself, and doing it here would put a
/// permission prompt behind an API shaped for native asynchrony. macOS is inert
/// too. Callers need no cfg of their own.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct Geolocation {
    plugin: Option<GeolocationHandle>,
}
/// The inert location facade: see the native [`Geolocation`] for the contract.
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
pub struct Geolocation;
#[cfg(any(target_os = "android", target_os = "ios"))]
impl Geolocation {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&GeolocationHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = GeolocationHandle::new(GEOLOCATION_CLASS)?;
            #[cfg(target_os = "ios")]
            let created = GeolocationPlugin::new()
                .map_err(|error| format!("Failed to create GeolocationPlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Construct the native plugin without asking for anything yet.
    pub fn prepare(&mut self) -> Result<(), String> {
        self.get_plugin()?;
        Ok(())
    }
    /// Whether the app may read location right now.
    ///
    /// Cheap on both platforms, so this is the thing to poll after
    /// [`request_permissions`](Geolocation::request_permissions) rather than
    /// waiting on a callback.
    pub fn check_permissions(&mut self) -> Result<PermissionStatus, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let reported = plugin.call_string("checkPermissionsFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = checkPermissionsFromRust(plugin)?;
        let json =
            reported.ok_or_else(|| "Geolocation permissions were not reported".to_string())?;
        serde_json::from_str(&json)
            .map_err(|error| format!("Failed to read permission status: {error}"))
    }
    /// Show the system permission prompt.
    ///
    /// Returns as soon as the prompt is raised; the answer arrives through
    /// [`check_permissions`](Geolocation::check_permissions). Both platforms
    /// only ever prompt once per install, so a `Denied` result stays denied
    /// until the user changes it in Settings.
    pub fn request_permissions(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("requestPermissionsFromRust")?;
        #[cfg(target_os = "ios")]
        requestPermissionsFromRust(plugin)?;
        Ok(())
    }
    /// Start looking for a fix.
    ///
    /// A cached fix no older than
    /// [`maximum_age`](PositionOptions::maximum_age) resolves the request
    /// immediately; otherwise the platform is asked for a fresh one. Starting a
    /// second request while one is in flight replaces it.
    pub fn start_position_request(&mut self, options: PositionOptions) -> Result<(), String> {
        let options = serde_json::to_string(&options)
            .map_err(|error| format!("Failed to encode position options: {error}"))?;
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit_str("startPositionRequestFromRust", &options)?;
        #[cfg(target_os = "ios")]
        startPositionRequestFromRust(plugin, options)?;
        Ok(())
    }
    /// Whether a request is still in flight.
    pub fn location_state(&mut self) -> LocationState {
        let Ok(plugin) = self.get_plugin() else {
            return LocationState::Idle;
        };
        #[cfg(target_os = "android")]
        let state = plugin
            .call_string("getLocationStateFromRust")
            .ok()
            .flatten();
        #[cfg(target_os = "ios")]
        let state = getLocationStateFromRust(plugin).ok().flatten();
        match state.as_deref() {
            Some("locating") => LocationState::Locating,
            _ => LocationState::Idle,
        }
    }
    /// Take the fix once it arrives.
    ///
    /// `Ok(None)` while the request is still running or when nothing was asked
    /// for. The position is handed over once and then cleared, so a second poll
    /// returns `Ok(None)` rather than the same fix again. A failed or timed-out
    /// request surfaces as `Err`, and also clears.
    pub fn poll_position(&mut self) -> Result<Option<Position>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let taken = plugin.call_string("takePositionFromRust")?;
        #[cfg(target_os = "ios")]
        let taken = takePositionFromRust(plugin)?;
        let Some(json) = taken else {
            return Ok(None);
        };
        let value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|error| format!("Failed to read position: {error}"))?;
        if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
            return Err(error.to_string());
        }
        let position = serde_json::from_value(value)
            .map_err(|error| format!("Failed to read position: {error}"))?;
        Ok(Some(position))
    }
}
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
impl Geolocation {
    pub(crate) fn new() -> Self {
        Self
    }
    /// No-op: nothing native to construct on these targets.
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Always [`PermissionState::Prompt`]: nothing here grants location.
    pub fn check_permissions(&mut self) -> Result<PermissionStatus, String> {
        Ok(PermissionStatus::default())
    }
    /// No-op: use the browser's own Geolocation API on the web.
    pub fn request_permissions(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op: no request is ever started on these targets.
    pub fn start_position_request(&mut self, _options: PositionOptions) -> Result<(), String> {
        Ok(())
    }
    /// Always [`LocationState::Idle`].
    pub fn location_state(&mut self) -> LocationState {
        LocationState::Idle
    }
    /// Always `None`: nothing on these targets produces a fix.
    pub fn poll_position(&mut self) -> Result<Option<Position>, String> {
        Ok(None)
    }
}
