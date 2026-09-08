#[cfg(target_os = "android")]
#[manganis::ffi("src/android/media")]
unsafe extern "Kotlin" {
    pub type MediaPlugin;
}
#[cfg(target_os = "android")]
use jni::{
    JavaVM,
    objects::{GlobalRef, JClass, JObject, JValue},
};
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type MediaPlugin;
    pub fn prepareFromRust(this: &MediaPlugin);
    pub fn enterPictureInPictureFromRust(this: &MediaPlugin, width: i32, height: i32);
    pub fn setOrientationFromRust(this: &MediaPlugin, orientation: String);
    pub fn setPlaybackActiveFromRust(this: &MediaPlugin, active: bool, title: String);
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
    plugin: Option<GlobalRef>,
    vm: Option<JavaVM>,
}
#[cfg(target_os = "ios")]
pub struct Media {
    plugin: Option<MediaPlugin>,
}
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
pub struct Media;
#[cfg(target_os = "android")]
impl Media {
    pub(crate) fn new() -> Self {
        Self {
            plugin: None,
            vm: None,
        }
    }
    fn get_plugin(&mut self) -> Result<GlobalRef, String> {
        if self.plugin.is_none() {
            let android = ndk_context::android_context();
            let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }
                .map_err(|error| format!("Failed to access Android VM: {error}"))?;
            let mut env = vm
                .attach_current_thread_permanently()
                .map_err(|error| format!("Failed to attach media plugin thread: {error}"))?;
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
                .new_string(MEDIA_PLUGIN_CLASS)
                .map_err(|error| format!("Failed to create media plugin class name: {error}"))?;
            let name_object = JObject::from(name);
            let class_object = env
                .call_method(
                    loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&name_object)],
                )
                .and_then(|value| value.l())
                .map_err(|error| format!("Failed to load MediaPlugin: {error}"))?;
            let class = JClass::from(class_object);
            let instance = env
                .new_object(
                    class,
                    "(Landroid/app/Activity;)V",
                    &[JValue::Object(&activity)],
                )
                .map_err(|error| format!("Failed to create MediaPlugin: {error}"))?;
            self.plugin = Some(
                env.new_global_ref(instance)
                    .map_err(|error| format!("Failed to retain MediaPlugin: {error}"))?,
            );
            self.vm = Some(vm);
            std::mem::forget(activity);
        }
        Ok(self.plugin.as_ref().unwrap().clone())
    }
    fn env(&self) -> Result<jni::JNIEnv<'_>, String> {
        let vm = self
            .vm
            .as_ref()
            .ok_or_else(|| "Media plugin VM is not initialized".to_string())?;
        vm.attach_current_thread_permanently()
            .map_err(|error| format!("Failed to attach media plugin thread: {error}"))
    }
    pub fn prepare(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let mut env = self.env()?;
        env.call_method(plugin.as_obj(), "prepareFromRust", "()V", &[])
            .map_err(|error| format!("Failed to prepare media plugin: {error}"))?;
        Ok(())
    }
    pub fn enter_picture_in_picture(&mut self, width: i32, height: i32) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let mut env = self.env()?;
        env.call_method(
            plugin.as_obj(),
            "enterPictureInPictureFromRust",
            "(II)V",
            &[JValue::Int(width), JValue::Int(height)],
        )
        .map_err(|error| format!("Failed to enter picture-in-picture: {error}"))?;
        Ok(())
    }
    pub fn set_orientation(&mut self, orientation: impl Into<String>) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let mut env = self.env()?;
        let orientation = env
            .new_string(orientation.into())
            .map_err(|error| format!("Failed to prepare orientation: {error}"))?;
        let orientation = JObject::from(orientation);
        env.call_method(
            plugin.as_obj(),
            "setOrientationFromRust",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&orientation)],
        )
        .map_err(|error| format!("Failed to set orientation: {error}"))?;
        Ok(())
    }
    pub fn set_playback_active(
        &mut self,
        active: bool,
        title: impl Into<String>,
    ) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let mut env = self.env()?;
        let title = env
            .new_string(title.into())
            .map_err(|error| format!("Failed to prepare playback title: {error}"))?;
        let title = JObject::from(title);
        env.call_method(
            plugin.as_obj(),
            "setPlaybackActiveFromRust",
            "(ZLjava/lang/String;)V",
            &[JValue::Bool(active.into()), JValue::Object(&title)],
        )
        .map_err(|error| format!("Failed to update background playback: {error}"))?;
        Ok(())
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
        enterPictureInPictureFromRust(plugin, width, height)?;
        Ok(())
    }
    pub fn set_orientation(&mut self, orientation: impl Into<String>) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        setOrientationFromRust(plugin, orientation.into())?;
        Ok(())
    }
    pub fn set_playback_active(
        &mut self,
        active: bool,
        title: impl Into<String>,
    ) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        setPlaybackActiveFromRust(plugin, active, title.into())?;
        Ok(())
    }
}
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
impl Media {
    pub(crate) fn new() -> Self {
        Self
    }
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    pub fn enter_picture_in_picture(&mut self, _width: i32, _height: i32) -> Result<(), String> {
        Ok(())
    }
    pub fn set_orientation(&mut self, _orientation: impl Into<String>) -> Result<(), String> {
        Ok(())
    }
    pub fn set_playback_active(
        &mut self,
        _active: bool,
        _title: impl Into<String>,
    ) -> Result<(), String> {
        Ok(())
    }
}
