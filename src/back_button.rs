#[cfg(target_os = "android")]
#[manganis::ffi("src/android/back_button")]
unsafe extern "Kotlin" {
    pub type BackButtonPlugin;
}
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type BackButtonPlugin;
    pub fn setInterceptingFromRust(this: &BackButtonPlugin, intercepting: bool);
    pub fn fallThroughFromRust(this: &BackButtonPlugin);
}
#[cfg(target_os = "android")]
use jni::{
    JavaVM,
    objects::{GlobalRef, JClass, JObject, JValue},
};
#[cfg(target_os = "android")]
const BACK_BUTTON_PLUGIN_CLASS: &str = "dev.dioxus.g3_native_plugins.back_button.BackButtonPlugin";
/// DOM event dispatched on `window` for an intercepted system-back request.
///
/// The event is cancelable so higher-priority UI such as a dialog or sheet can
/// claim it with `Event::preventDefault` before a router integration handles
/// it. [`g3-route-transitions`](https://docs.rs/g3-route-transitions) uses this
/// contract for its optional native-back integration.
pub const NATIVE_BACK_EVENT: &str = "g3nativeback";
/// The system back gesture, routed into the app's own history.
///
/// "Back" here is the event, not the button that produced it. Android raises it
/// from the system back gesture or a hardware key; iOS raises it from the swipe
/// in from the left screen edge. Neither reaches a web app hosted in a WebView
/// on its own — Android delivers back to the Activity rather than the WebView,
/// so the app exits on the first press however it is written, and iOS gives the
/// edge swipe meaning only inside a `UINavigationController`, which a bare
/// WebView is not. Intercepting dispatches a `g3nativeback` event on `window`
/// instead, for the app to act on.
///
/// It is an event rather than a direct `history.back()` because the Rust binary
/// runs outside the WebView here and the router keeps its history there — the
/// WebView's own history is not the app's, so going back on it navigates
/// nothing. The event crosses back over the same bridge the app already uses to
/// talk to the page, and it acts on the history it really has.
///
/// `g3nativeback` is a cancelable event. UI layers that need first refusal
/// (dialogs, sheets, fullscreen players) can call `preventDefault()` before a
/// router integration handles the request.
///
/// Interception is off until asked for. Only the app knows whether there is
/// anywhere to go back to, and a permanently enabled handler would leave the
/// user unable to leave on Android and would swallow edge swipes on iOS.
///
/// On the web the browser owns back, and macOS has no equivalent gesture, so
/// both are no-ops and callers need no cfg of their own.
#[cfg(target_os = "android")]
pub struct BackButton {
    plugin: Option<GlobalRef>,
    intercepting: bool,
}
#[cfg(target_os = "ios")]
/// The iOS left-edge back-swipe bridge.
pub struct BackButton {
    plugin: Option<BackButtonPlugin>,
    intercepting: bool,
}
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
/// An inert back-button facade for targets without a native back gesture.
pub struct BackButton {
    intercepting: bool,
}
#[cfg(target_os = "android")]
impl BackButton {
    /// Create an unprepared, non-intercepting system-back plugin.
    ///
    /// Most apps obtain this through [`crate::NativePluginsProvider`]. The
    /// public constructor also lets integration crates own the plugin when an
    /// app does not otherwise need a native-plugin provider.
    pub fn new() -> Self {
        Self {
            plugin: None,
            intercepting: false,
        }
    }
    fn get_plugin(&mut self) -> Result<&GlobalRef, String> {
        if self.plugin.is_none() {
            let android = ndk_context::android_context();
            let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }
                .map_err(|error| format!("Failed to access Android VM: {error}"))?;
            let mut env = vm
                .attach_current_thread_permanently()
                .map_err(|error| format!("Failed to attach plugin thread: {error}"))?;
            let activity = unsafe { JObject::from_raw(android.context().cast()) };
            let loader = env
                .call_method(
                    &activity,
                    "getClassLoader",
                    "()Ljava/lang/ClassLoader;",
                    &[],
                )
                .and_then(|value| value.l())
                .map_err(|error| format!("Failed to obtain app class loader: {error}"))?;
            let name = env
                .new_string(BACK_BUTTON_PLUGIN_CLASS)
                .map_err(|error| format!("Failed to create plugin class name: {error}"))?;
            let name_object = JObject::from(name);
            let class_object = env
                .call_method(
                    loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&name_object)],
                )
                .and_then(|value| value.l())
                .map_err(|error| format!("Failed to load BackButtonPlugin: {error}"))?;
            let class = JClass::from(class_object);
            let instance = env
                .new_object(
                    class,
                    "(Landroid/app/Activity;)V",
                    &[JValue::Object(&activity)],
                )
                .map_err(|error| format!("Failed to create BackButtonPlugin: {error}"))?;
            self.plugin = Some(
                env.new_global_ref(instance)
                    .map_err(|error| format!("Failed to retain BackButtonPlugin: {error}"))?,
            );
            std::mem::forget(activity);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Initializes the Android bridge without enabling interception.
    pub fn prepare(&mut self) -> Result<(), String> {
        self.get_plugin()?;
        Ok(())
    }
    /// Whether the system back press is taken by the app.
    ///
    /// Call with `false` wherever back should leave the app, so the press keeps
    /// its usual meaning at the root of the navigation stack.
    pub fn set_intercepting(&mut self, intercepting: bool) -> Result<(), String> {
        if self.intercepting == intercepting {
            return Ok(());
        }
        let plugin = self.get_plugin()?;
        let android = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }
            .map_err(|error| format!("Failed to access Android VM: {error}"))?;
        let mut env = vm
            .attach_current_thread_permanently()
            .map_err(|error| format!("Failed to attach plugin thread: {error}"))?;
        env.call_method(
            plugin.as_obj(),
            "setInterceptingFromRust",
            "(Z)V",
            &[JValue::Bool(intercepting.into())],
        )
        .map_err(|error| format!("Failed to update Back interception: {error}"))?;
        self.intercepting = intercepting;
        Ok(())
    }
    /// Pass one intercepted press to the next Android Back handler.
    ///
    /// Integration crates use this as a race-safe fallback when a callback was
    /// enabled for transient UI but neither that UI nor router history handles
    /// the press. The native callback is disabled only for the redispatch, so
    /// it cannot recursively receive its own event.
    pub fn fall_through(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let android = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }
            .map_err(|error| format!("Failed to access Android VM: {error}"))?;
        let mut env = vm
            .attach_current_thread_permanently()
            .map_err(|error| format!("Failed to attach plugin thread: {error}"))?;
        env.call_method(plugin.as_obj(), "fallThroughFromRust", "()V", &[])
            .map_err(|error| format!("Failed to pass Back to Android: {error}"))?;
        Ok(())
    }
    /// Returns whether system-back interception is enabled.
    pub fn is_intercepting(&self) -> bool {
        self.intercepting
    }
}
#[cfg(target_os = "ios")]
impl BackButton {
    /// Create an unprepared, non-intercepting system-back plugin.
    ///
    /// Most apps obtain this through [`crate::NativePluginsProvider`]. The
    /// public constructor also lets integration crates own the plugin when an
    /// app does not otherwise need a native-plugin provider.
    pub fn new() -> Self {
        Self {
            plugin: None,
            intercepting: false,
        }
    }
    fn get_plugin(&mut self) -> Result<&BackButtonPlugin, String> {
        if self.plugin.is_none() {
            self.plugin = Some(
                BackButtonPlugin::new()
                    .map_err(|error| format!("Failed to create BackButtonPlugin: {error:?}"))?,
            );
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Initializes the iOS gesture bridge without enabling interception.
    pub fn prepare(&mut self) -> Result<(), String> {
        self.get_plugin()?;
        Ok(())
    }
    /// Whether the left-edge back swipe is taken by the app.
    ///
    /// Call with `false` wherever back should mean nothing, so an edge swipe at
    /// the root of the navigation stack keeps its usual inert behavior rather
    /// than being consumed by a recognizer with nowhere to go.
    pub fn set_intercepting(&mut self, intercepting: bool) -> Result<(), String> {
        if self.intercepting == intercepting {
            return Ok(());
        }
        let plugin = self.get_plugin()?;
        setInterceptingFromRust(plugin, intercepting)?;
        self.intercepting = intercepting;
        Ok(())
    }
    /// Pass one intercepted gesture back to the platform.
    ///
    /// Android's counterpart hands the press to the next Back handler, which
    /// closes the app. iOS has no such default — an edge swipe with nowhere to
    /// go is inert, and an app may not exit itself — so the gesture is dropped.
    /// The call stays available so integration crates need no cfg of their own.
    pub fn fall_through(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        fallThroughFromRust(plugin)?;
        Ok(())
    }
    /// Returns whether left-edge swipe interception is enabled.
    pub fn is_intercepting(&self) -> bool {
        self.intercepting
    }
}
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
impl BackButton {
    /// Create the inert system-back facade used where nothing raises the event.
    pub fn new() -> Self {
        Self {
            intercepting: false,
        }
    }
    /// No-op preparation for targets without a native back gesture.
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Recorded but inert: the browser owns back on the web, and macOS has no
    /// system back gesture to take.
    pub fn set_intercepting(&mut self, intercepting: bool) -> Result<(), String> {
        self.intercepting = intercepting;
        Ok(())
    }
    /// No-op counterpart to Android's one-press fallthrough.
    pub fn fall_through(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Returns the requested interception state.
    pub fn is_intercepting(&self) -> bool {
        self.intercepting
    }
}
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
))]
impl Default for BackButton {
    fn default() -> Self {
        Self::new()
    }
}
